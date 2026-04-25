# 基本設計書 — error（エラーハンドリング方針 / 禁止事項 / Fail Fast 集約）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/basic-design/error.md -->
<!-- 兄弟: ./index.md, ./flows.md, ./security.md, ./ipc-protocol.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。Rust シグネチャが必要な場合はインライン `code` で示す。

## エラーハンドリング方針

本 feature は **3 層（daemon / IPC スキーマ / CLI クライアント）** に跨るため、各層で発生するエラーの処理方針を明確にする。

### daemon 側

| 例外種別 | 処理方針 | 観測経路 |
|---------|---------|--------|
| シングルインスタンス先取り失敗（`flock EWOULDBLOCK` / Named Pipe `FILE_FLAG_FIRST_PIPE_INSTANCE` 失敗） | 起動時 fail fast、`tracing::error!` + `ExitCode::SingleInstanceUnavailable (2)` | stderr ログ |
| `repo.load()` 失敗 | 起動時 fail fast、`tracing::error!` + `ExitCode::SystemError (1)` | stderr ログ |
| 暗号化 vault 検出 | 起動時 fail fast、`tracing::error!` + `ExitCode::EncryptionUnsupported (3)` | stderr ログ |
| ソケット bind / SDDL 設定失敗 | 起動時 fail fast、`tracing::error!` + `ExitCode::SystemError (1)` | stderr ログ |
| ピア UID/SID 検証失敗 | **接続のみ切断**（daemon 継続）、`tracing::warn!` | stderr ログ |
| ハンドシェイクタイムアウト（5 秒以内に Handshake が来ない）| 接続のみ切断、`tracing::warn!` | stderr ログ |
| プロトコルバージョン不一致 | `IpcResponse::ProtocolVersionMismatch { server, client }` 返送 → 接続切断、`tracing::warn!` | stderr ログ + クライアント側の終了コード |
| MessagePack デコード失敗 | 接続のみ切断（daemon 継続、graceful degradation）、`tracing::warn!` | stderr ログ |
| フレーム長 16 MiB 超過 | 接続のみ切断、`tracing::warn!` + バッファ即解放 | stderr ログ |
| `vault.add_record` / `update_record` / `remove_record` の `DomainError` | `IpcResponse::Error(IpcErrorCode::Domain { reason })` 返送、daemon 継続 | クライアント側の `IpcResponse` |
| `vault.find_record` で `None`（存在しない id） | `IpcResponse::Error(IpcErrorCode::NotFound { id })` 返送 | クライアント側の `IpcResponse` |
| `repo.save()` の `PersistenceError` | `IpcResponse::Error(IpcErrorCode::Persistence { reason })` 返送、daemon 継続 | クライアント側の `IpcResponse` |
| その他想定外（`unwrap` / `expect` 経路）| panic_hook で固定文言 stderr 出力 + Rust ランタイム exit 101 | stderr 固定文言 |

### `IpcErrorCode` バリアント詳細

| バリアント | フィールド | 発生箇所 | 設計意図 |
|-----------|-----------|---------|---------|
| `EncryptionUnsupported` | なし | 起動時 vault 検証 / 防御的コードでハンドラ内 | クライアント側 `CliError::EncryptionUnsupported` 経由で終了コード 3 |
| `NotFound { id: RecordId }` | 対象 id | `edit` / `remove` ハンドラで `find_record` が `None` | クライアント側で `MSG-CLI-106` に写像 |
| `InvalidLabel { reason: String }` | 検証失敗の理由（英語短文）| ハンドラで防御的に再検証（クライアント側で検証済みのはずだが念のため） | クライアント側で `MSG-CLI-101` に写像 |
| `Persistence { reason: String }` | `PersistenceError::Display` の文字列化 | `repo.save` 失敗 | クライアント側で `MSG-CLI-107` / `108` に写像（reason 内容で分岐は CLI 側 best-effort、本 feature は粒度を維持） |
| `Domain { reason: String }` | `DomainError::Display` の文字列化 | `vault.add_record` 等の集約整合性エラー | クライアント側で `MSG-CLI-109` に写像 |
| `Internal { reason: String }` | 想定外バグの原因（debug 用、本番では generic 文字列）| ハンドラで予期せぬ状態を検出 | クライアント側で `MSG-CLI-109` に写像 |

**`reason` フィールドの設計規約**:
- **英語短文のみ**（i18n は CLI 側の `presenter::error` で行う、daemon は英語ログ）
- **secret 値・絶対パス・ピア UID 等を含めない**（漏洩経路防止）
- 生成は `format!("{}", err)` で `Display` を呼ぶ。`Debug` は使わない（payload 展開リスク）
- `PersistenceError::Display` / `DomainError::Display` は既存型として secret を含まないことが `cli-vault-commands` で証明済み（`basic-design/security.md` の表）

### CLI クライアント側

`shikomi-cli` の `CliError` に本 feature でバリアントを追加する。

| バリアント | フィールド | 発生箇所 | 写像 `ExitCode` | メッセージ ID |
|-----------|-----------|---------|--------------|--------------|
| `DaemonNotRunning(PathBuf)` | ソケットパス | `IpcVaultRepository::connect` 失敗 | `UserError (1)` | `MSG-CLI-110` |
| `ProtocolVersionMismatch { server, client }` | プロトコルバージョン両者 | ハンドシェイク不一致 | `UserError (1)` | `MSG-CLI-111` |

既存 `CliError::Persistence(PersistenceError)` 経由でも IPC 由来の `PersistenceError::DaemonNotRunning` / `ProtocolVersionMismatch` を捕捉できる構造とする（詳細設計で `PersistenceError` への variant 追加を確定）。

### `PersistenceError` への variant 追加（IPC 由来）

`shikomi-infra::persistence::error::PersistenceError` に本 feature で**追加するバリアント**:

| バリアント | フィールド | 発生箇所 |
|-----------|-----------|---------|
| `DaemonNotRunning(PathBuf)` | ソケットパス | `IpcVaultRepository::connect` で接続失敗（`ECONNREFUSED` / `ENOENT` / `ERROR_FILE_NOT_FOUND` 等） |
| `ProtocolVersionMismatch { server: IpcProtocolVersion, client: IpcProtocolVersion }` | プロトコルバージョン両者 | ハンドシェイク応答が `ProtocolVersionMismatch` |
| `IpcDecode { reason: String }` | デコード失敗の理由 | `rmp_serde::from_slice` 失敗（クライアント側）|
| `IpcEncode { reason: String }` | エンコード失敗の理由 | `rmp_serde::to_vec` 失敗（クライアント側）|
| `IpcIo { reason: String }` | I/O 失敗の理由 | `Framed::send` / `Framed::next` の `tokio::io::Error` |

**設計理由**: `VaultRepository` trait の戻り値型 `Result<_, PersistenceError>` を変えずに IPC 経路のエラーを表現できる。CLI 側は既存の `From<PersistenceError> for CliError` を経由して `CliError::Persistence(...)` でラップ可能。`DaemonNotRunning` / `ProtocolVersionMismatch` は CLI で `match` して個別の終了コード / メッセージ ID に写像する経路を持つ（既存の `Persistence` 一括写像と分けて扱う）。

### Fail Fast 集約

本 feature の Fail Fast 経路を一覧化:

| Fail Fast 検出箇所 | 経路 | 観測 |
|------------------|------|------|
| daemon 起動: シングルインスタンス先取り失敗 | `SingleInstanceLock::acquire` で `flock EWOULDBLOCK` / Named Pipe 失敗 | exit 2 |
| daemon 起動: vault load 失敗 | `repo.load()` の `PersistenceError` | exit 1 |
| daemon 起動: 暗号化 vault | `vault.protection_mode() == Encrypted` | exit 3 |
| daemon 起動: ソケットパーミッション異常 | 親ディレクトリ `0700` 検証失敗 | exit 1 |
| daemon 接続: ピア UID/SID 不一致 | `verify_peer` 失敗 | 接続切断のみ（daemon 継続） |
| daemon 接続: ハンドシェイクタイムアウト | 5 秒以内に Handshake 不達 | 接続切断のみ |
| daemon 接続: プロトコル不一致 | `IpcProtocolVersion` 不一致 | `ProtocolVersionMismatch` 返送 + 切断 |
| daemon 接続: MessagePack 破損 | `rmp_serde::from_slice` 失敗 | 接続切断のみ |
| daemon 接続: フレーム長超過 | `LengthDelimitedCodec` 16 MiB 超過 | 接続切断のみ |
| CLI: daemon 未起動 | `IpcVaultRepository::connect` 失敗 | `CliError::DaemonNotRunning` → 終了コード 1 |
| CLI: プロトコル不一致 | ハンドシェイク受信で `ProtocolVersionMismatch` | `CliError::ProtocolVersionMismatch` → 終了コード 1 |
| CLI: IPC I/O エラー | 接続中の `tokio::io::Error` | `CliError::Persistence` → 終了コード 2 |

**設計原則**: daemon は**接続単位の Fail Fast**を徹底し、daemon プロセス全体は壊さない（graceful degradation、可用性）。クライアント単位のエラーで daemon が落ちると、他クライアントへの影響が広がる。

## 禁止事項（本 feature での実装規約）

### daemon 側

- `Result<T, String>` / `Result<T, Box<dyn Error>>` をモジュール公開 API で使わない（情報欠損）
- `unwrap()` / `expect()` を本番コードパスで使わない（テストコードは許容、ただし `expect("reason")` で理由必須）
- ハンドラ内で `?` で `PersistenceError` を CLI 側まで透過させない。daemon は `IpcResponse::Error(IpcErrorCode::Persistence { reason: format!("{}", e) })` に**明示的に変換**する（reason の secret 含有を制御するため）
- エラーを握り潰さない。`if let Err(_) = ... {}` を無言で通過しない
- **`tracing` マクロに `IpcRequest` / `IpcResponse` 全体を渡さない**（secret 露出経路）。代わりに variant 名のみ（`request.discriminant_name()` 相当）を出す
- **`std::env::set_var` / `std::env::remove_var` を本番コードで呼ばない**（thread-unsafe、テストでも `assert_cmd::Command::env()` を使う）
- **panic hook 内で `info.payload()` / `info.message()` / `info.location()` の値を参照しない**（CLI と同型、`./security.md §panic hook`）
- **panic hook 内で `tracing` マクロを呼ばない**（CLI と同型）
- **`expose_secret()` を `crates/shikomi-core/src/ipc/` / `crates/shikomi-daemon/src/` 内で呼ばない**（CI grep で検証、`./security.md §expose_secret 経路監査`）
- **`unsafe` ブロックを `crates/shikomi-daemon/src/permission/{unix,windows}.rs` 以外に書かない**（OS API 依存箇所のみ allow、`./security.md §unsafe_code の扱い`）
- **`rmp_serde::Raw` / `rmp_serde::RawRef` を使わない**（`shikomi-core::ipc` で構造的に遮断、`tech-stack.md` §2.1 契約）
- **listener から `accept` した stream の `unsafe { Stream::from_raw_fd(...) }` 等の生 fd 経路を使わない**（`tokio::net::UnixListener` / `NamedPipeServer` のセーフ API のみ使用）

### `shikomi-core::ipc` 側

- **`tokio` / `rmp-serde` / `tokio-util` を依存に追加しない**（`shikomi-core` の純粋ドメイン性維持、`tech-stack.md` §4.5）
- `serde::{Serialize, Deserialize}` の derive のみ許可。手動 `Serialize` 実装は `SerializableSecretBytes` の `expose_secret` 不使用契約のために必要な範囲のみ（`./security.md §SecretBytes のシリアライズ契約`）
- **`SerializableSecretBytes` を `shikomi-infra::persistence` から import しない**（永続化経路への誤流入防止、CI grep）
- `IpcRequest` / `IpcResponse` / `IpcProtocolVersion` / `IpcErrorCode` の `#[non_exhaustive]` を**外さない**（後続 feature の非破壊拡張保証）
- バリアントの**削除 / 改名禁止**（プロトコル互換性契約、`./ipc-protocol.md §バージョニングルール`）

### `shikomi-cli::io::ipc_*` 側

- **`expose_secret()` を呼ばない**（CI grep）
- 接続失敗時に panic しない（`Result` を返す）
- `IpcVaultRepository::connect` 内のハンドシェイク失敗は `Err(PersistenceError::ProtocolVersionMismatch)` で返し、上位の `run()` で `CliError::ProtocolVersionMismatch` に写像

## i18n 扱い

daemon 側のログは **英語のみ**（`tracing` で構造化、運用者向け）。CLI クライアント側のエラーメッセージは既存の `presenter::error::render_error` 経由で英語 + 日本語併記（`Locale` 引数）。

`MSG-CLI-110` / `MSG-CLI-111` の追加で本 feature の i18n 対応は完了。daemon の `IpcResponse::Error.reason` は英語短文 → CLI 側で `match` して既存の `MSG-CLI-xxx` に写像（reason をそのまま表示しない、i18n のため）。

## テスト設計への引き継ぎ

本 feature のエラー系挙動について、テスト設計担当（涅マユリ）には以下の観点を伝える:

1. `IpcErrorCode` 全 6 バリアント × `IpcResponse::Error` への埋込（UT、MessagePack round-trip）
2. `PersistenceError` 追加バリアント（`DaemonNotRunning` / `ProtocolVersionMismatch` / `IpcDecode` / `IpcEncode` / `IpcIo`）の `Display` 実装が secret を含まないこと（UT）
3. `CliError::DaemonNotRunning` / `ProtocolVersionMismatch` の `ExitCode` 写像（UT、`From<&CliError> for ExitCode` の追加分）
4. `render_error` の `MSG-CLI-110` / `MSG-CLI-111` 出力テスト（UT、英日 2 locale × 2 メッセージ = 4 パターン）
5. daemon 側 graceful degradation: MessagePack 破損送信 → 接続切断 → daemon プロセス継続（IT、`tokio::io::duplex` で破損バイト列）
6. daemon 側 fail fast: シングルインスタンス先取り失敗 → exit 2（E2E、同じソケットパスで 2 つ目の daemon を spawn）
7. daemon 起動時の暗号化 vault 検出 → exit 3（E2E、フィクスチャ vault 使用）
8. ピア UID 検証: 別ユーザからの接続拒否（E2E、Linux 用 `sudo -u nobody` テストハーネス）
9. CI grep 系（TC-CI-016〜023、`./security.md` に詳細）
10. 接続単位 Fail Fast の独立性: 接続 A の MessagePack 破損が接続 B の応答に影響しないこと（IT、複数接続並行）
11. graceful shutdown 中の in-flight リクエスト完了: SIGTERM 受信後に既存接続の Add リクエストが応答完了するまで daemon が exit しないこと（E2E、シーケンス検証）
12. CLI `--ipc list` と `shikomi list`（直結）の bit 同一出力検証（E2E、同 vault に対する両経路）
