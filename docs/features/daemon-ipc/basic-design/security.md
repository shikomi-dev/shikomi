# 基本設計書 — security（脅威モデル / OWASP / CVE 確認 / 漏洩経路監査 / SecretBytes 契約）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/basic-design/security.md -->
<!-- 兄弟: ./index.md, ./flows.md, ./error.md, ./ipc-protocol.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。Rust シグネチャが必要な場合はインライン `code` で示す。

## 脅威モデル（IPC 層の追加視点）

IPC 層は `docs/architecture/context/threat-model.md` で既に定義された脅威を**新たに増やすものではない**が、以下の経路を設計時点で封じる。本 feature の脅威表は arch threat-model §S 行（Spoofing）/ §T 行（Tampering）/ §I 行（Information Disclosure）/ §D 行（Denial of Service）に整合させる。

| 想定攻撃 / 事故 | 経路 | 保護資産 | 対策 |
|--------------|------|---------|------|
| **別ユーザの悪意あるプロセスから daemon への接続** | 共有マシンで別ユーザが UDS / Named Pipe へ接続を試行 | vault 内容（IPC 経路で得られる平文値）| (1) UDS パーミッション `0600`（owner-only）、(2) Named Pipe SDDL owner-only ACE、(3) **`SO_PEERCRED` / `LOCAL_PEERCRED` / `GetNamedPipeClientProcessId` によるピア UID/SID 検証**（多層防御、`process-model.md` §4.2 認証 (1)、Issue #26 必須） |
| **同ユーザ内の悪意あるプロセスから daemon への接続** | 同 UID で動く別プロセス（マルウェア等）が UDS / Named Pipe へ接続 | vault 内容 | **本 Issue では未対策**（scope-out）。`process-model.md` §4.2 認証 (2) のセッショントークンが後続 Issue（`daemon-session-token`、未起票）で `IpcProtocolVersion::V2` 追加と同時に導入予定。`#[non_exhaustive] enum` の効能で非破壊拡張可能 |
| **stale UDS socket による次起動失敗** | SIGKILL された daemon が残した `daemon.sock` ファイルが原因で次の `bind` が `EADDRINUSE` で失敗、ユーザが手動 `rm` を強制される | 可用性 | **`flock(LOCK_EX \| LOCK_NB)` 獲得後に `unlink` → `bind` の 3 段階順序**（`process-model.md` §4.1 ルール 2 Unix）。`flock` はカーネル自動 release のため stale PID 問題が原理的に発生しない（ペテルギウス指摘 ②、PR #27 で確定） |
| **race condition: 他 daemon のソケット誤削除** | 順序を間違えて先に `unlink` すると、起動中の正常 daemon のソケットを消してしまう | 既存 daemon の継続運用 | **`flock` 獲得を**先に**することで「ロック取得済 = 自分が単一インスタンス確定」を保証してから `unlink`** する設計。順序を逆転すると race-safe でなくなる |
| **MessagePack デコード爆発（DoS）** | 巨大なフレーム長を送りつけて daemon メモリを枯渇 | 可用性 | **`LengthDelimitedCodec::max_frame_length(16 * 1024 * 1024)` で 16 MiB 上限**。超過は該当接続のみ切断 + バッファ即解放、daemon プロセスはクラッシュさせない（`process-model.md` §4.2、graceful degradation） |
| **MessagePack ペイロード破損で daemon クラッシュ** | 無効なバイト列を送ってデコード時 panic を誘発 | 可用性 | `rmp_serde::from_slice` が `Result` を返す（panic しない）。decode 失敗 → 該当接続切断 + `tracing::warn!`、daemon プロセス継続。**panic_hook で `info.payload()` を参照しない**（CLI と同型 fixed-message、§panic hook） |
| **secret 値の `tracing` ログ漏洩** | daemon が `IpcRequest::AddRecord` を `tracing::info!` で構造化出力する際、`SecretBytes` 中身が展開される | Secret 値 | (1) `SecretBytes::Debug` が `[REDACTED]` 固定、(2) `SerializableSecretBytes` の `Debug` も同じ、(3) **`crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/` 配下で `expose_secret()` 呼出 0 件**（CI grep）、(4) `tracing` マクロに `IpcRequest::AddRecord` 全体を渡さない。代わりに `request.discriminant()` 等で variant 名のみログ出力 |
| **secret 値のクライアント側ログ漏洩** | `IpcVaultRepository::save` 等で `tracing::debug!("sending {:?}", request)` 記述で漏れる | Secret 値 | 同上の `Debug` 契約 + `crates/shikomi-cli/src/io/` で `expose_secret` 0 件監査 |
| **fail open（エラー握り潰し）** | daemon ハンドラが `repo.save` 失敗時に `IpcResponse::Edited` を返してしまう | データ整合性 | ハンドラは `Result<IpcResponse, IpcErrorCode>` 相当の型構造で、`save` 失敗時のみ `IpcResponse::Error(...)` を返す経路を**型で強制**。`?` で安易にラップせず明示的に `match` |
| **フレームサイズ偽装（小さな length プレフィックス + 大きな payload）** | 攻撃者がプレフィックスに小さな値を書き、続けて大量バイトを送る | メモリ / CPU | `LengthDelimitedCodec` は length プレフィックスを信用してそのバイト数だけ読む。プレフィックス自体が 16 MiB を超えれば検出される。プレフィックス通りに読むため超過バイトは「次フレームの先頭」として扱われ、その時点で次のフレーム長を再評価 |
| **コネクション枯渇 DoS** | 攻撃者が大量接続を張ったまま放置（同ユーザでハンドシェイク完了して放置） | 可用性 | MVP では同時接続数制限を**設けない**（同ユーザ攻撃面は OS で防げないため別レイヤ対策、後続 Issue）。`tokio` の async タスクは軽量で実用上問題にならない（`nfr.md` §9 の 32 同時接続程度を許容） |
| **IPC 経路の盗聴** | 同マシン内の root / 他ユーザがソケット通信を傍受 | 通信内容 | UDS / Named Pipe は OS のプロセス境界で保護され、ローカル他ユーザ / root 以外には見えない（root は脅威モデル §7.0 で受容）。TLS は localhost 通信に過剰設計のため不採用（`process-model.md` §4.2） |

## panic hook と secret 漏洩経路の遮断

CLI 側の panic hook 設計（`cli-vault-commands` の `basic-design/security.md §panic hook` で確定）を **daemon 側にも同型で適用**する。

**問題**: daemon の panic hook で `tracing::error!(panic = ?info)` 等を呼ぶと、`PanicHookInfo::Debug` が payload の raw 文字列を展開する。`shikomi-infra::SqliteVaultRepository::save` 等が `panic!("bad value: {}", raw_str)` の形で panic すると、`raw_str` に secret が混入する経路が残る。

**本設計の契約（CLI と同型）**:

1. **panic hook は `eprintln!` で固定文言のみを出力する**（`"error: shikomi-daemon internal bug; please report this issue to https://github.com/shikomi-dev/shikomi/issues\n"`）
2. **panic hook 内で `tracing` マクロを一切呼ばない**（`tracing::error!` / `warn!` / `info!` / `debug!` / `trace!` 全て禁止）
3. **`info.payload()` / `info.message()` / `info.location()` の値を文字列化してログ・stderr に流さない**
4. **panic 発生時の終了コード**: Rust ランタイム既定の `101` を許容（`ExitCode::SystemError = 1` への揃えは行わない、`catch_unwind` の `UnwindSafe` 境界コストを避ける）
5. **CI 環境で `RUST_BACKTRACE=0` を推奨**（`detailed-design/future-extensions.md §運用注記` で明記、daemon でも CLI と同じ運用）

**テスト観点**:
- TC-CI: `crates/shikomi-daemon/src/` 配下で `tracing::` マクロの引数に `panic` / `PanicHookInfo` / `info.payload` を含む行が存在しないこと（grep）
- TC-UT: `std::panic::catch_unwind` で意図的 panic を起こし、stderr 出力が固定文言に一致すること

## ピア資格情報検証の設計詳細

**Linux**:

- syscall: `getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &mut ucred, &mut len)` で `struct ucred { pid, uid, gid }` を取得
- daemon の `geteuid()` と `ucred.uid` を比較
- 不一致時のログ: `tracing::warn!(target: "shikomi_daemon::permission", expected_uid = daemon_uid, peer_uid = ucred.uid, "peer credential mismatch")`
- crate: `nix::sys::socket::getsockopt`（`PeerCredentials` option）を採用

**macOS**:

- syscall: `getsockopt(fd, SOL_LOCAL, LOCAL_PEERCRED, &mut xucred, &mut len)` で `struct xucred { cr_uid, cr_groups, ... }` を取得
- 比較ロジックは Linux と同じ
- crate: `nix` v0.29 では macOS 用の薄いラッパーが提供されるか、なければ `libc` 直叩き（`unsafe` ブロックは `permission/unix.rs` 内に閉じる、workspace lint `unsafe_code = deny` のオーバーライドを `permission` モジュールに限定）

**Windows**:

- WinAPI: `GetNamedPipeClientProcessId(handle, &mut pid)` で接続元 PID 取得
- `OpenProcessToken(OpenProcess(pid), TOKEN_QUERY, ...)` でトークン取得
- `GetTokenInformation(token, TokenUser, ...)` で SID 取得
- daemon 自身の SID は `GetCurrentProcessToken` 同経路で取得し比較
- crate: `windows-sys` の `Win32_System_Pipes` / `Win32_System_Threading` / `Win32_Security` features を有効化
- `unsafe` ブロックは `permission/windows.rs` 内に閉じる（既存 `shikomi-infra::permission::windows` の `unsafe_code` allow オーバーライド規約と整合）

**フェイルセーフ**:
- `getsockopt` / `GetNamedPipeClientProcessId` 自体が失敗した場合（kernel API エラー）→ **接続を即切断**（攻撃の可能性、確実に拒否）+ `tracing::warn!`

## シングルインスタンスの race-safe 保証（Unix）

`process-model.md` §4.1 ルール 2 Unix の **3 段階手順**を `lifecycle::single_instance::SingleInstanceLock::acquire` で実装する。

| 段階 | 操作 | エラー時 |
|------|------|--------|
| 1 | lock ファイル `daemon.lock` を `0600` で `open(O_CREAT \| O_RDWR)` → `flock(LOCK_EX \| LOCK_NB)` | `EWOULDBLOCK` → `tracing::error!("another daemon is running; flock acquisition failed")` + `ExitCode::SingleInstanceUnavailable (2)` |
| 2 | 既存 `daemon.sock` を `unlink`（`ENOENT` は無視） | `EACCES` 等 → exit 非 0、`flock` は OS が自動 release |
| 3 | `UnixListener::bind("daemon.sock")` でソケット新規作成 | `EADDRINUSE`（極稀、ロック獲得後の race だが flock の保証下では発生しないはず）/ パーミッション設定失敗 → exit 非 0 |

**race-safe の根拠**:

- ロックを**先に**取ることで「ロック取得済 = 自分が単一インスタンス」が保証される
- 順序を逆転（先に `unlink` → `bind` → `flock`）すると、別の daemon プロセスが `unlink` 直後に `bind` を完了する可能性があり、後続の `flock` で初めてエラー検出 = その時点で正常 daemon のソケットが消えている
- `flock` は POSIX advisory lock。プロセス終了時にカーネルが自動 release するため、`SIGKILL` された daemon が残した lock ファイルでも次の起動は必ず獲得できる（stale PID エッジケース不発生）

**親ディレクトリ `0700` 検証**:

- `bind` 前に `stat($XDG_RUNTIME_DIR/shikomi/)` で `mode & 0o777 == 0o700` を確認 → 不一致なら `tracing::error!` + exit 非 0
- 親ディレクトリ未作成の場合は `mkdir -p $XDG_RUNTIME_DIR/shikomi` で `0700` で作成（`umask` 影響を避けるため明示的に `chmod 0o700`）

## `SecretBytes` のシリアライズ契約

本 feature では IPC 経路で secret 値を運搬する必要があるため、`shikomi-core::SecretBytes` のシリアライズ実装を**慎重に設計**する。

### 前提

- `shikomi-core::SecretBytes` は `Vec<u8>` 系 secret ラッパで、`Debug` で `[REDACTED]` 固定、`Drop` で `zeroize` を呼ぶ
- 本 crate は永続化フォーマット側で `serde_json` 等に流れることを**型で防ぐ**ため、`SecretBytes` 自体は `Serialize` / `Deserialize` を**意図的に実装しない**（`tech-stack.md` §4.5「`secrecy` の `serde` 連携は使わない」と整合）

### 契約: `SerializableSecretBytes` newtype

`shikomi-core::ipc::secret_bytes::SerializableSecretBytes(pub SecretBytes)` を本 feature で導入する。**IPC 経路でのみ秘密を運搬する文脈**を newtype で明示化し、永続化経路への誤流入を型で防ぐ。

| 項目 | 設計 |
|------|------|
| 型定義 | `pub struct SerializableSecretBytes(pub SecretBytes)`（`crates/shikomi-core/src/ipc/secret_bytes.rs`） |
| `Debug` | derive 不可（`SecretBytes` の `Debug` は `[REDACTED]` 固定だが、newtype の derive で透過する場合のみ）。**手動実装で `[REDACTED:SerializableSecretBytes]` 固定** |
| `Serialize` 実装 | 手動実装。`SecretBytes` の内部参照を `expose_secret` 不使用で `serializer.serialize_bytes(&inner_slice)` に渡す経路。`SecretBytes` 側に `pub(crate) fn as_bytes(&self) -> &[u8]` 等の crate-internal アクセサを設け、`shikomi-core::ipc::secret_bytes` のみが呼べるようにする |
| `Deserialize` 実装 | 手動実装。`Vec<u8>` をデシリアライズ → `SecretBytes::from_vec(bytes)` で `SecretBytes` 構築（既存 API の `from_bytes` / `from_vec` を流用、内部で `zeroize` 対応の `Vec<u8>` を保持） |
| 用途 | `IpcRequest::AddRecord.value` / `IpcRequest::EditRecord.value` のフィールド型としてのみ使用 |
| 制約 | **永続化（`shikomi-infra::persistence`）からこの型は import しない**（grep で監査）。`shikomi-core::ipc` 配下の使用に限定 |

### `expose_secret` を呼ばない実装方針

- `SerializableSecretBytes::serialize` 内で `self.0.expose_secret()` を呼ぶ案は**不採用**。理由: CI grep の対象が `crates/shikomi-core/src/ipc/` を含む場合、本 feature の契約「`expose_secret` 呼出 0 件」が崩れる
- 代わりに `SecretBytes` に **`pub(crate) fn as_serialize_slice(&self) -> &[u8]`** を追加（`shikomi-core::secret` 配下、`pub(crate)` で公開範囲を限定）。本メソッドは内部で `expose_secret` を呼ぶが、その呼出は `crates/shikomi-core/src/secret/bytes.rs` 内に閉じる
- `shikomi-core::ipc::secret_bytes::SerializableSecretBytes::serialize` は `as_serialize_slice` を呼ぶのみ。`expose_secret` は出現しない
- **CI grep 監査範囲**: `crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/`。`crates/shikomi-core/src/secret/` は対象外（既存の `Record::text_preview` と同パターン、`expose_secret` を core 内の限定箇所に閉じる）

### `Raw` / `RawRef` 不使用契約

`tech-stack.md` §2.1 IPC シリアライズ行で確定済み: `crates/shikomi-core/src/ipc/` 配下で `rmp_serde::Raw` / `rmp_serde::RawRef` を使用しない（RUSTSEC-2022-0092 unsound 経路への再接触を構造的に遮断）。

**本 feature での具体実装**:

- 全フィールドを `String` / `Vec<u8>` / `RecordId`（UUIDv7 内部表現）/ `RecordKind`（`#[derive(Serialize)]` した enum）/ `RecordLabel`（`String` ラッパ） / `OffsetDateTime`（`time::serde::rfc3339`）等の**型付き値**で表現
- `SerializableSecretBytes` も `Vec<u8>` を経由した型付き表現（MessagePack の `bin` 型に直列化）
- CI grep: `crates/shikomi-core/src/ipc/` で `Raw` / `RawRef` が 0 件であること

## `expose_secret()` 呼び出し経路の監査（CI 契約）

**契約**: 以下のディレクトリ配下で `SecretString::expose_secret()` / `SecretBytes::expose_secret()` の呼び出しを **0 箇所**とする。

- `crates/shikomi-core/src/ipc/` 配下の全 `.rs` ファイル
- `crates/shikomi-cli/src/io/` 配下の全 `.rs` ファイル（特に `ipc_vault_repository.rs` / `ipc_client.rs`）
- `crates/shikomi-daemon/src/` 配下の全 `.rs` ファイル（`main.rs` / `lib.rs` / `lifecycle/` / `ipc/` / `permission/`）

**監査対象から除外**: `crates/shikomi-core/src/secret/`（`SecretBytes::as_serialize_slice` 内の expose 呼出は意図的、許可範囲）、`crates/shikomi-core/src/vault/record.rs`（既存 `text_preview` の expose 呼出、`cli-vault-commands` で確立済み）。

**CI 監査スクリプト**: 既存の `scripts/ci/audit-secret-paths.sh` を拡張し、上記 3 ディレクトリで `expose_secret` が 0 件であることを検査。`tests/` 配下は対象外（テストでのモック比較を許容）。

**TC-CI 採番（テスト設計担当 = マユリへの引き継ぎ）**:

- `expose_secret` grep 契約: `crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/` の 3 領域 × 0 件（**TC-CI-016〜018** 相当）
- `Raw` / `RawRef` 不使用契約: `crates/shikomi-core/src/ipc/` 配下で 0 件（**TC-CI-019** 相当）
- daemon panic hook 監査: `crates/shikomi-daemon/src/` 配下で `info.payload()` / `info.message()` / `info.location()` / panic hook 内 `tracing::*!` 0 件（**TC-CI-020〜022** 相当）

**本書では追加の TC-CI-xxx 定義を行わない**（TC-ID 採番は `test-design/ci.md` に一元化、`cli-vault-commands` の流儀踏襲）。

## OWASP Top 10 対応

本 feature は IPC 層 + daemon プロセスのため、`cli-vault-commands` とは異なる項目が該当する。

| # | カテゴリ | 対応状況 |
|---|---------|---------|
| A01 | Broken Access Control | **対応** — UDS `0600` / Named Pipe owner-only DACL（`process-model.md` §4.2）+ ピア UID/SID 検証（多層防御）。同ユーザ内悪性プロセス対策（セッショントークン）は後続 Issue で `V2` 拡張 |
| A02 | Cryptographic Failures | **対応** — 暗号化未対応のため暗号処理は発生しない。本 feature は IPC 経路の暗号化を**行わない**（OS プロセス境界保護で十分、`process-model.md` §4.2）。暗号化は将来 feature `daemon-vault-encryption` |
| A03 | Injection | **対応** — IPC スキーマは `IpcRequest` / `IpcResponse` の型付き enum。MessagePack デコードで型検証され、生 `String` を SQL / シェルに流さない。SQLite 経路は `shikomi-infra` の既存 `rusqlite` パラメータバインディングを継承 |
| A04 | Insecure Design | **対応** — `IpcServer` / `IpcRequestHandler` / `PeerCredentialVerifier` / `SingleInstanceLock` の責務分離 + Phase 2 移行契約の実体化（trait 越しの差替え）。`#[non_exhaustive] enum` で破壊的変更経路を型で表現 |
| A05 | Security Misconfiguration | **対応** — UDS `0600` / 親ディレクトリ `0700` を起動時 stat 検証で fail fast。Named Pipe SDDL は明示的 owner-only。デフォルト挙動が安全 |
| A06 | Vulnerable Components | **対応** — §依存 crate の CVE 確認結果 を参照。`tokio` `^1.44.2`（RUSTSEC-2025-0023 patched）/ `rmp-serde` `^1.3`（RUSTSEC-2022-0092 patched + Raw 不使用契約）/ `tokio-util` `^0.7`（RustSec クリーン、PR #27 確認済み） |
| A07 | Auth Failures | **対応**（部分） — ピア UID/SID 検証は本 Issue 必須。同ユーザ内悪性プロセス対策のセッショントークンは scope-out（後続 Issue で `V2` 拡張）。マスターパスワード認証は暗号化モード feature で扱う |
| A08 | Data Integrity Failures | **対応** — vault の atomic write は既存 `shikomi-infra` で保証。daemon は `repo.save` 失敗時に `IpcResponse::Error(Persistence)` を返し、クライアントが終了コード非 0 で気付く（fail open 防止） |
| A09 | Logging Failures | **対応** — `tracing` マクロ呼出時に `IpcRequest` 全体を渡さない。secret 値は `SerializableSecretBytes::Debug` の `[REDACTED]` 固定で保護。panic hook で `payload` 非参照（CI grep 監査） |
| A10 | SSRF | 対象外 — daemon / CLI とも HTTP / 外部 URL アクセスを行わない |

## 依存 crate の CVE 確認結果

**確認日時**: 2026-04-25  
**確認方法**: RustSec Advisory Database（<https://rustsec.org/advisories/>）を本 feature で新規導入する全 crate に対して照合。`tokio` / `tokio-util` / `rmp-serde` は工程 0（PR #27 / `tech-stack.md` §2.1）で詳細確認済み、本表は再確認結果のみ記載。  
**確認者**: 設計担当（セル）

| crate | 採用バージョン | 用途 | RustSec advisory | 判定 |
|-------|-------------|------|-----------------|------|
| `tokio` | `^1.44.2` | 多重スレッドランタイム / UDS / Named Pipe / signal / sync | RUSTSEC-2025-0023 patched 列の最新を完全内包（`^1.44.2` で 1.42.0 / 1.43.0 排除）| ✅ クリーン（PR #27 で証跡確定） |
| `tokio-util` | `^0.7` | `LengthDelimitedCodec` | active advisory なし（2026-04-24 時点、PR #27 で `crates/tokio-util/` ディレクトリ空ディレクトリ確認済み）| ✅ クリーン |
| `rmp-serde` | `^1.3` | MessagePack シリアライズ | RUSTSEC-2022-0092（`Raw` / `RawRef` unsound）は v1.1.1 で patched、v1.3 系は完全安全圏。**`Raw` / `RawRef` 不使用契約**を `shikomi-core::ipc` で構造的に遮断 | ✅ クリーン（契約付き） |
| `bytes` | `^1` | `Bytes` 型（`Framed` の `Item` ）| active advisory なし | ✅ クリーン |
| `nix` | `^0.29`（Unix のみ）| `getsockopt(SO_PEERCRED / LOCAL_PEERCRED)` / `flock` | active advisory なし（2026-04-25 時点）| ✅ クリーン |
| `windows-sys` | `^0.59`（Windows のみ）| Named Pipe / SDDL / OpenProcessToken | active advisory なし | ✅ クリーン |

**運用規約**:

- 本 feature の PR が develop にマージされる時点で `cargo-deny check advisories` が pass すること（既存 CI で実行）
- 本表は**静的スナップショット**。`cargo-deny` と Dependabot による継続監査が本線（`tech-stack.md` §2.2）
- 暗号クリティカル crate（`§4.3.2`）は本 feature では新規追加なし。`zeroize` / `secrecy` の既存ピンを継承

## `unsafe_code` の扱い

本 feature は workspace lint `unsafe_code = "deny"` を引き続き尊重するが、以下の領域で **allow オーバーライドを限定許可**する:

| crate | モジュール | 許可理由 |
|-------|-----------|---------|
| `shikomi-daemon` | `permission/unix.rs` | `getsockopt(SO_PEERCRED / LOCAL_PEERCRED)` の `libc` 呼出。`nix` crate でラップされている範囲では `unsafe` 不要、それ以外は本モジュールに閉じる（既存 `shikomi-infra::permission::windows` の同パターン） |
| `shikomi-daemon` | `permission/windows.rs` | `GetNamedPipeClientProcessId` / `OpenProcessToken` / `GetTokenInformation` は `windows-sys` のため `unsafe` 必須。本モジュールに閉じる |

**設計規約**:

- `crates/shikomi-daemon/src/permission/{unix,windows}.rs` の冒頭に `#![allow(unsafe_code)]` を明示
- それ以外のディレクトリ（`ipc/` / `lifecycle/` / `lib.rs` 等）では `unsafe` を**書かない**
- `shikomi-cli::io::ipc_vault_repository` / `ipc_client` は `tokio` のセーフ API のみ使用、`unsafe` 不要

**監査**: `crates/shikomi-daemon/src/` 配下で `unsafe` ブロックが `permission/` 以外に出現しないことを CI grep で監査（**TC-CI-023** 相当、テスト設計でマユリが確定）

## セキュリティに関するテスト責務の分担

| テストレベル | 責務 |
|-------------|------|
| ユニット（UT） | `IpcRequest` / `IpcResponse` の MessagePack round-trip（serialize → deserialize で同値）、`SerializableSecretBytes` の `Debug` が `[REDACTED]` 固定、`SerializableSecretBytes::serialize` が `expose_secret` を呼ばないこと（コードレベル契約、grep でも検証）、ピア検証関数の単独テスト（モック UID / SID 入力） |
| 結合（IT） | daemon プロセス内の `tokio::test` で `tokio::io::duplex` を使った in-memory IPC ラウンドトリップ。各 `IpcRequest` バリアントに対する `IpcResponse` の整合性、暗号化 vault 検出時の起動失敗（`ExitCode::EncryptionUnsupported` 経路）、`SingleInstanceLock` の race-safe 順序検証 |
| E2E | 実 daemon プロセスを `assert_cmd::Command` で spawn し、`shikomi --ipc list` 経由で SQLite 直結版と bit 同一の結果を確認、SIGKILL 後の stale socket での次起動成功、SIGTERM での graceful shutdown、別ユーザからの接続拒否（Linux 用に `sudo -u nobody` 等のテストハーネスが必要、CI で再現可能な範囲のみ） |
| CI（補助） | `expose_secret` 呼出 0 件 grep（§expose_secret 監査）/ `tracing::` マクロへの panic / payload 参照禁止 grep / `unsafe` ブロック `permission/` 限定確認 / `Raw` / `RawRef` 不使用 grep |

テストケース番号の割当は `test-design/` が担当する。本書は設計側からの**要件の明示**に留める。
