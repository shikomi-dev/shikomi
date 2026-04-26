# 要件定義書

<!-- feature: daemon-ipc / Issue #26 (Phase 1: list) / Issue #30 (Phase 1.5: add/edit/remove) -->
<!-- 配置先: docs/features/daemon-ipc/requirements.md -->

## Phase 区分（Issue #30 で確定）

本 feature は IPC 経路の**段階的透過化**を行う。Phase 区分は以下:

| Phase | スコープ | 対応 PR / Issue | 状態 |
|-------|---------|---------------|------|
| Phase 1 | daemon プロセス骨格 / IPC プロトコル定義 / `--ipc list` 透過 | Issue #26 / PR #29 | ✅ 完了（merged at 2026-04-25 ae4df15） |
| **Phase 1.5** | **`--ipc add` / `--ipc edit` / `--ipc remove` 透過化、PR #29 の runtime reject 経路撤去** | **Issue #30** | **🚧 本 Issue で対応** |
| Phase 2 | `--ipc` 既定化（または `--no-ipc` 反転）、daemon 自動起動 | 後続 Issue（未起票） | 後続 |

**Phase 1.5 の限定スコープ（Issue #30）**:
- daemon ハンドラ（`handler/{add,edit,remove}.rs`）は PR #29 で実装済み（Phase 1 で先行実装、`Vault` 集約に対するドメイン整合は完成）
- 本 Issue #30 のスコープは **CLI 側の経路接続** に限定:
  1. PR #29 で `crates/shikomi-cli/src/lib.rs:119-` に存在する「`args.ipc && !matches!(args.subcommand, Subcommand::List)` → runtime reject」分岐の撤去
  2. `IpcVaultRepository` を `VaultRepository` trait **非実装**に確定し、専用メソッド `add_record` / `edit_record` / `remove_record` を新規追加（PR #29 で `list_summaries` 専用メソッド経路を確立済み、その延長線上に追加）
  3. CLI Composition Root に `enum RepositoryHandle { Sqlite, Ipc }` を導入し、`run_add` / `run_edit` / `run_remove` で `match handle` 分岐
  4. `IpcErrorCode → PersistenceError` 写像表に `NotFound` / `InvalidLabel` / `Domain` バリアント写像を追加（PR #29 では list 経路で発生し得なかったため未実装）

## 機能要件

### REQ-DAEMON-001: daemon プロセス起動とランタイム初期化

| 項目 | 内容 |
|------|------|
| 入力 | `shikomi-daemon` バイナリ起動。コマンドライン引数なし（将来 `--vault-dir` 等を追加余地、本 feature では既定の OS 解決のみ）。環境変数 `SHIKOMI_DAEMON_LOG`（`tracing` レベル: `error` / `warn` / `info` / `debug` / `trace`、未設定時 `info`）/ `SHIKOMI_VAULT_DIR`（vault ディレクトリ上書き、未設定時 OS デフォルト）|
| 処理 | (1) `tokio` 多重スレッドランタイムを `#[tokio::main]` で起動、(2) `panic_hook` 登録（CLI と同型 fixed-message、§REQ-DAEMON-022 参照）、(3) `tracing-subscriber` を `EnvFilter::from_env("SHIKOMI_DAEMON_LOG")` で初期化、(4) シングルインスタンス先取り（§REQ-DAEMON-002 / 003）、(5) `SqliteVaultRepository` 構築 + 起動時 `repo.load()` で `Vault` ロード（暗号化モードなら `tracing::error!` + exit 非 0）、(6) IPC エンドポイント listen 開始 → 接続受付ループへ |
| 出力 | stdout: 起動完了メッセージ（`tracing::info!("shikomi-daemon listening on {socket_path}")` 等）/ stderr: 警告 / panic（fixed-message） |
| エラー時 | シングルインスタンス先取り失敗 → exit 非 0（具体コードは §REQ-DAEMON-024）/ `repo.load()` 失敗 → `tracing::error!` + exit 非 0 / 暗号化 vault 検出 → `tracing::error!("vault is encrypted; daemon does not support encrypted vaults yet")` + exit 非 0 |

### REQ-DAEMON-002: シングルインスタンス保証（Unix）

| 項目 | 内容 |
|------|------|
| 入力 | daemon プロセス起動時の自動処理（ユーザ操作なし） |
| 処理 | **3 段階手順**（`process-model.md` §4.1 ルール 2 / Unix 節）: (1) ソケットディレクトリ（`$XDG_RUNTIME_DIR/shikomi/`）配下の lock ファイル `daemon.lock` を `0600` で `open` → `flock(LOCK_EX \| LOCK_NB)` で非ブロック排他ロック、(2) ロック獲得後にのみ既存 `daemon.sock` を `unlink`（`ENOENT` は無視）、(3) `UnixListener::bind("$XDG_RUNTIME_DIR/shikomi/daemon.sock")` でソケット新規作成（パーミッション `0600`、親ディレクトリ `0700` を事前 `stat` で検証） |
| 出力 | `tracing::info!("acquired single-instance lock at {lock_path}")` |
| エラー時 | (1) で `EWOULDBLOCK`（他 daemon 稼働中）→ `tracing::error!` + exit 非 0 / (2) で `unlink` の `EACCES` 等 → exit 非 0 / (3) で `bind` の `EADDRINUSE` 等 → exit 非 0 / 親ディレクトリの `0700` 違反 → exit 非 0（不正な権限の検出） |
| 設計理由 | `flock` はプロセス終了時にカーネルが自動 release するため、SIGKILL された daemon が残した lock ファイルでも次の起動は必ず獲得できる（stale PID エッジケースが原理的に発生しない）。ロックを先に取ることで他 daemon のソケットを誤って削除しない（race-safe） |

### REQ-DAEMON-003: シングルインスタンス保証（Windows）

| 項目 | 内容 |
|------|------|
| 入力 | daemon プロセス起動時の自動処理（ユーザ操作なし） |
| 処理 | `\\.\pipe\shikomi-daemon-{user-sid}` を `FILE_FLAG_FIRST_PIPE_INSTANCE` フラグ + `PIPE_ACCESS_DUPLEX` モードで `CreateNamedPipeW` 呼び出し。`tokio::net::windows::named_pipe::ServerOptions::first_pipe_instance(true)` を使用。SDDL は **owner-only**（現在ログオンセッションの user SID に `GENERIC_READ \| GENERIC_WRITE` を許可、Everyone / Anonymous / NetworkService は明示的に拒否）|
| 出力 | `tracing::info!("acquired single-instance Named Pipe at {pipe_name}")` |
| エラー時 | 既存インスタンスが存在 → `CreateNamedPipeW` が `ERROR_ACCESS_DENIED` または `ERROR_PIPE_BUSY` で失敗 → exit 非 0。SDDL 設定失敗 → exit 非 0 |
| 設計理由 | `FILE_FLAG_FIRST_PIPE_INSTANCE` は Win32 が「同名パイプの最初のインスタンスのみ作成可能」を保証する標準フラグ。プロセス終了時にカーネルがパイプ実体を release するため stale 残留が発生しない（Unix UDS と異なり、unlink 不要）|

### REQ-DAEMON-004: IPC エンドポイント作成

| 項目 | 内容 |
|------|------|
| 入力 | OS 検出による分岐（`cfg(target_os = ...)`）|
| 処理 | **Unix**: `$XDG_RUNTIME_DIR/shikomi/daemon.sock`（Linux） / `~/Library/Caches/shikomi/daemon.sock`（macOS）に `tokio::net::UnixListener::bind`。ソケットファイルパーミッション `0600`、親ディレクトリ `0700` を起動時 `stat` で検証し異常なら fail fast / **Windows**: `\\.\pipe\shikomi-daemon-{user-sid}` に `tokio::net::windows::named_pipe::ServerOptions` で listener 作成、SDDL owner-only |
| 出力 | listener オブジェクト（daemon プロセスメモリ内）|
| エラー時 | パーミッション設定失敗 → exit 非 0 / 親ディレクトリ未作成 → daemon 起動時に `mkdir -p` で作成（`0700`）してから listener 作成 |

### REQ-DAEMON-005: ピア資格情報検証（Issue #26 スコープ内、必須）

| 項目 | 内容 |
|------|------|
| 入力 | クライアント（CLI / GUI）からの接続受付時の `accept` 直後 |
| 処理 | **Linux**: `SO_PEERCRED`（`getsockopt(fd, SOL_SOCKET, SO_PEERCRED, ...)`）で接続元 UID を取得し、daemon 自身の `geteuid()` と比較 / **macOS**: `LOCAL_PEERCRED`（`getsockopt(fd, SOL_LOCAL, LOCAL_PEERCRED, ...)`）で同様 / **Windows**: `GetNamedPipeClientProcessId` → `OpenProcessToken` → `GetTokenInformation(TokenUser)` で接続元 SID を取得し、daemon 自身の `GetCurrentProcessToken()` から得た SID と比較 |
| 出力 | UID / SID 一致 → 接続継続。不一致 → 即切断 + `tracing::warn!("peer credential mismatch: expected uid={daemon_uid}, got uid={peer_uid}")` |
| エラー時 | `getsockopt` / `GetNamedPipeClientProcessId` 失敗 → 即切断 + `tracing::warn!`（攻撃の可能性） |
| 設計理由 | UDS `0600` / Named Pipe owner-only DACL で他ユーザの接続は OS レイヤで拒否されるが、ピア検証は**バックアップとして必ず行う**（多層防御）。同ユーザ内の悪性プロセス対策は別経路（セッショントークン、後続 Issue）で扱う |

### REQ-DAEMON-006: プロトコルハンドシェイク

| 項目 | 内容 |
|------|------|
| 入力 | クライアント接続直後の最初のフレーム（`IpcRequest::Handshake { client_version: IpcProtocolVersion }`）|
| 処理 | (1) フレーム受信 → MessagePack デコード、(2) `client_version == IpcProtocolVersion::V1`（daemon 自身の対応バージョン）か判定、(3) 一致 → `IpcResponse::Handshake { server_version: IpcProtocolVersion::V1 }` 返送、接続継続 / (4) 不一致 → `IpcResponse::ProtocolVersionMismatch { server: V1, client: <受信値> }` 返送 → 切断（Fail Fast）|
| 出力 | ハンドシェイク成功時: 通常の vault 操作リクエスト受付ループへ進む / 不一致時: `tracing::warn!("protocol mismatch: server={server}, client={client}")` |
| エラー時 | 最初のフレームが `IpcRequest::Handshake` でない（vault 操作が先に届いた）→ 即切断 + `tracing::warn!("first frame must be Handshake")` / フレーム受信タイムアウト（5 秒、後続 Issue で調整余地）→ 切断 |
| 設計理由 | `IpcProtocolVersion` は `#[non_exhaustive] enum` なので、後続 Issue で `V2` 追加時、daemon `V2` ↔ クライアント `V1` の組合せでも上記 fail fast 経路で安全に切断される。**session token は本 Issue のスコープ外**で、`Handshake` バリアントは `client_version` のみ持つ。後続 Issue で `Handshake { client_version, session_token }` への非破壊拡張を予定（`IpcProtocolVersion::V2` バリアント追加と同時） |

### REQ-DAEMON-007: vault 操作 IPC ハンドラ（List）

| 項目 | 内容 |
|------|------|
| 入力 | `IpcRequest::ListRecords`（フィールドなし、フィルタ等は将来追加） |
| 処理 | (1) daemon プロセス内で `Mutex<Vault>` をロック取得（in-memory `Vault` インスタンスへの並行アクセス制御）、(2) `vault.records()` を走査し各レコードから `RecordSummary` を構築（**`RecordSummary` は機密値フィールドを含まない投影型**: `id` / `kind` / `label` / `value_preview`（Text の先頭 40 char）/ `value_masked`（Secret の場合は `true`、`shikomi-core::Record::text_preview` を流用）、(3) `IpcResponse::Records(Vec<RecordSummary>)` 返送 |
| 出力 | `IpcResponse::Records(...)` |
| エラー時 | 暗号化モード（起動時に検出済みなら本来到達しない、防御的コード）→ `IpcResponse::Error(EncryptionUnsupported)` |

### REQ-DAEMON-008: vault 操作 IPC ハンドラ（Add）

| 項目 | 内容 |
|------|------|
| 入力 | `IpcRequest::AddRecord { kind: RecordKind, label: RecordLabel, value: SecretBytes, now: OffsetDateTime }` |
| 処理 | (1) `Mutex<Vault>` ロック、(2) `RecordPayload::Plaintext(SecretString::from_bytes(input.value))` 構築（`SecretBytes` → `SecretString` 変換は `shikomi-core` の API 経由、`expose_secret` を呼ばない）、(3) `uuid::Uuid::now_v7()` → `RecordId::new`、(4) `Record::new(id, kind, label, payload, now)`、(5) `vault.add_record(record)?` で集約に追加、(6) `repo.save(&vault)?` で atomic write、(7) `IpcResponse::Added { id }` 返送 |
| 出力 | `IpcResponse::Added { id }` |
| エラー時 | 暗号化モード → `IpcResponse::Error(EncryptionUnsupported)` / `RecordLabel` 検証失敗（クライアント側で検証済みのはずだが防御的に再検証）→ `IpcResponse::Error(InvalidLabel)` / `vault.add_record` の id 重複 → `IpcResponse::Error(Domain)` / `repo.save` 失敗 → `IpcResponse::Error(Persistence)` |

### REQ-DAEMON-009: vault 操作 IPC ハンドラ（Edit）

| 項目 | 内容 |
|------|------|
| 入力 | `IpcRequest::EditRecord { id: RecordId, label: Option<RecordLabel>, value: Option<SecretBytes>, now: OffsetDateTime }` |
| 処理 | (1) `Mutex<Vault>` ロック、(2) `vault.find_record(&id)` → `None` なら `IpcResponse::Error(NotFound { id })`、(3) `with_updated_label(now)` / `with_updated_payload(RecordPayload::Plaintext(SecretString::from_bytes(value)), now)` を集約メソッド経由で連鎖適用、(4) `vault.update_record(&id, |old| ...)` で置換、(5) `repo.save(&vault)?`、(6) `IpcResponse::Edited { id }` 返送 |
| 出力 | `IpcResponse::Edited { id }` |
| エラー時 | 暗号化モード / `NotFound` / `repo.save` 失敗 → 各 `IpcResponse::Error(...)` |

### REQ-DAEMON-010: vault 操作 IPC ハンドラ（Remove）

| 項目 | 内容 |
|------|------|
| 入力 | `IpcRequest::RemoveRecord { id: RecordId }` |
| 処理 | (1) `Mutex<Vault>` ロック、(2) `vault.find_record(&id)` → `None` なら `IpcResponse::Error(NotFound { id })`、(3) `vault.remove_record(&id)?`、(4) `repo.save(&vault)?`、(5) `IpcResponse::Removed { id }` 返送 |
| 出力 | `IpcResponse::Removed { id }` |
| エラー時 | 暗号化モード / `NotFound` / `repo.save` 失敗 → 各 `IpcResponse::Error(...)` |

### REQ-DAEMON-011: フレーミングと最大長制限

| 項目 | 内容 |
|------|------|
| 入力 | TCP / UDS / Named Pipe バイトストリーム |
| 処理 | `tokio_util::codec::LengthDelimitedCodec::builder().max_frame_length(16 * 1024 * 1024).little_endian().length_field_length(4).new_codec()` を `Framed` でラップ。送信は `Framed::send`、受信は `Framed::next` で `Vec<u8>` を取得 |
| 出力 | フレーム単位の `Bytes` ストリーム |
| エラー時 | フレーム長 16 MiB 超過 → `Codec` がエラー返却 → 該当接続のみ切断 + `tracing::warn!("frame length exceeds 16 MiB; closing connection")` + 部分読込バッファ即解放 |

### REQ-DAEMON-012: MessagePack シリアライズ / デコード

| 項目 | 内容 |
|------|------|
| 入力 | `Bytes`（受信フレーム）または `IpcResponse` 値（送信前）|
| 処理 | (1) 受信: `rmp_serde::from_slice::<IpcRequest>(&bytes)` で構造化、(2) 送信: `rmp_serde::to_vec::<IpcResponse>(&response)` で `Vec<u8>` 化 |
| 出力 | `IpcRequest` / `IpcResponse` 値 / `Vec<u8>` |
| エラー時 | デコード失敗 → 該当接続のみ切断 + `tracing::warn!("MessagePack decode failed: {}", err)` + **daemon プロセスはクラッシュしない**（graceful degradation、他クライアントへの影響ゼロ） |

### REQ-DAEMON-013: 暗号化 vault 拒否（Fail Fast）

| 項目 | 内容 |
|------|------|
| 入力 | daemon 起動時の `repo.load()` 結果 |
| 処理 | `Vault::protection_mode() == ProtectionMode::Encrypted` の場合、daemon は IPC リスナーを開始**せず** `tracing::error!` + exit 非 0（暗号化 vault は本 feature 未対応）|
| 出力 | stderr: `error: vault is encrypted; daemon does not support encrypted vaults yet (Issue #26 scope-out)` |
| エラー時 | daemon 起動失敗。クライアント側は `--ipc` 指定時 `DaemonNotRunning` で Fail Fast |
| 注記 | 万一 daemon 稼働中に vault が外部で再暗号化された場合（例: 別プロセスがアトミック書換）、in-memory `Vault` は古い平文状態のままなので、次の `repo.load()`（GUI 経由の `Reload` IPC 等、後続 Issue 想定）で検出して切替。本 feature では起動時 1 回検証で十分 |

### REQ-DAEMON-014: graceful shutdown

| 項目 | 内容 |
|------|------|
| 入力 | OS シグナル: `SIGTERM` / `SIGINT`（Unix）/ `CTRL_CLOSE_EVENT` / `CTRL_C_EVENT`（Windows）|
| 処理 | (1) `tokio::signal::unix::signal(SignalKind::terminate())` / `tokio::signal::ctrl_c` / `tokio::signal::windows::ctrl_close` で受信、(2) listener を停止（新規接続受付終了）、(3) **in-flight リクエストを待機**（既存接続のレスポンス送信完了まで `JoinSet::join_all` 等で待つ、タイムアウト 30 秒）、(4) `Mutex<Vault>` ドロップ → `repo` ドロップ → `VaultLock` 解除（既存 `shikomi-infra::persistence::lock::VaultLock` の Drop 実装に依存）、(5) ソケットファイル `unlink`（Unix のみ、Windows はカーネルが解放）、(6) lock ファイル close（`flock` 解放）、(7) exit 0 |
| 出力 | `tracing::info!("graceful shutdown complete")` |
| エラー時 | 30 秒タイムアウト → in-flight リクエストを強制切断して exit 0（強制終了の判断は別 Issue で調整） |

### REQ-DAEMON-015: `IpcVaultRepository` クライアント

| 項目 | 内容 |
|------|------|
| 入力 | `IpcVaultRepository::connect(socket_path: &Path) -> Result<Self, PersistenceError>` |
| 処理 | (1) UDS / Named Pipe に `tokio::net::UnixStream::connect` / `NamedPipeClient::connect`、(2) `IpcRequest::Handshake { client_version: IpcProtocolVersion::V1 }` 送信、(3) `IpcResponse::Handshake` 受信で一致確認 → 不一致なら `PersistenceError::ProtocolVersionMismatch` / (4) 接続を `Self { framed: Framed<...>, ... }` に保持し、`VaultRepository` trait の `load` / `save` / `exists` を IPC リクエスト発行で実装 |
| 出力 | `Result<IpcVaultRepository, PersistenceError>` |
| エラー時 | 接続失敗（daemon 未起動）→ `PersistenceError::DaemonNotRunning(socket_path)` / ハンドシェイク失敗 → `PersistenceError::ProtocolVersionMismatch` / I/O エラー → `PersistenceError::Io` |

### REQ-DAEMON-016: CLI `--ipc` オプトインフラグ

| 項目 | 内容 |
|------|------|
| 入力 | `shikomi --ipc <list \| add \| edit \| remove>` 形式の CLI 起動（Issue #30 で 4 サブコマンド全て透過化、PR #29 では `list` のみ） |
| 処理 | clap グローバルフラグ `--ipc`（`bool`、`#[arg(long, global = true)]`）を `CliArgs` に追加。`run()` 内で `args.ipc == true` なら `IpcVaultRepository::connect(default_socket_path())` を構築 → `RepositoryHandle::Ipc(ipc)` で保持、`false` なら従来通り `SqliteVaultRepository::from_directory(...)` → `RepositoryHandle::Sqlite(repo)` で保持。各サブコマンドハンドラ（`run_list` / `run_add` / `run_edit` / `run_remove`）が `match handle` で 2 アーム分岐し、Sqlite 経路は既存 UseCase、IPC 経路は `IpcVaultRepository` 専用メソッドを呼出 |
| 出力 | 同 `shikomi list` 等の各サブコマンドと bit 同一の出力（vault 真実源は同じ）|
| エラー時 | daemon 未起動 → `CliError::DaemonNotRunning` → `MSG-CLI-110` → 終了コード 1 + ヒント「`shikomi-daemon` を起動してください」 / プロトコル不一致 → `CliError::ProtocolVersionMismatch` → `MSG-CLI-111` → 終了コード 1 / IPC 経由 NotFound → `MSG-CLI-106`（**Issue #30 で経路追加、edit/remove 用**）|
| 注記 | `--ipc` のソケットパスは**現時点では env / フラグで上書き不可**（OS デフォルトのみ）。将来 `--ipc-socket <PATH>` フラグ追加余地（YAGNI）。**`IpcVaultRepository` は `VaultRepository` trait を実装しない**（Issue #30 で確定、`docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md §設計方針の確定` を単一真実源とする） |

### REQ-DAEMON-027: PR #29 の runtime reject 経路撤去（Issue #30 新規）

| 項目 | 内容 |
|------|------|
| 入力 | PR #29 で `crates/shikomi-cli/src/lib.rs` に導入された runtime reject 分岐: `if args.ipc && !matches!(args.subcommand, Subcommand::List) { return CliError::UsageError("--ipc currently supports only the `list` subcommand; for add/edit/remove, omit --ipc to use direct vault file access") }` |
| 処理 | 上記 if ブロックを **Issue #30 で完全削除**。サブコマンド網羅は `RepositoryHandle::Ipc` 経路の `match` ディスパッチで型レベル保証 |
| 出力 | `shikomi --ipc add` / `--ipc edit` / `--ipc remove` が、それぞれ `IpcVaultRepository` の専用メソッドを経由して daemon に到達し、成功時は `added: <id>` / `updated: <id>` / `removed: <id>` を stdout 出力 |
| エラー時 | 撤去後の経路で発生し得る IPC 由来エラーは `MSG-CLI-106` / `107` / `108` / `109` / `110` / `111` のいずれかに写像。`MSG-CLI-100` 系の usage error は撤去対象外（既存の `--value` / `--stdin` 衝突等は維持） |
| 検証 | E2E テスト: `shikomi --ipc add --kind text --label L --value V` が daemon 経由で成功（PR #29 では「`--ipc currently supports only the list subcommand`」エラーで終了コード 1 だった挙動が、Issue #30 で**変更される**ことの回帰検証）|

### REQ-DAEMON-017: daemon 未起動時の Fail Fast

| 項目 | 内容 |
|------|------|
| 入力 | `--ipc` 指定で daemon が起動していない状態 |
| 処理 | `IpcVaultRepository::connect` が即時 `PersistenceError::DaemonNotRunning(socket_path)` を返却 → `CliError::DaemonNotRunning` 経由で `MSG-CLI-110` を stderr に出力 |
| 出力 | stderr: `error: shikomi-daemon is not running (socket {path} unreachable)` + **hint は 3 OS の起動コマンドを並記**（英語例）: `hint: start the daemon — Linux/macOS: 'shikomi-daemon &' (or 'systemctl --user start shikomi-daemon' / 'launchctl kickstart gui/$(id -u)/dev.shikomi.daemon'); Windows: 'Start-Process -NoNewWindow shikomi-daemon'`。日本語併記は `basic-design/error.md §MSG-CLI-110 確定文面` を参照 |
| エラー時 | — |
| 注記 | hint 文言は `shikomi-daemon` という**実バイナリ名のみ**を案内する。`shikomi daemon start` のようなサブコマンドは本 feature で追加しない（存在しないコマンド案内はユーザを行き止まりにする、ペガサス指摘 ①）。バイナリ名・ PowerShell コマンド名の確定値は `basic-design/error.md §MSG-CLI-110 確定文面` / `basic-design/error.md §MSG-CLI-111 確定文面` が単一真実源 |

### REQ-DAEMON-018: スキーマ単一真実源（DRY）

| 項目 | 内容 |
|------|------|
| 構造 | `crates/shikomi-core/src/ipc/` 配下に `IpcRequest` / `IpcResponse` / `IpcProtocolVersion` / `RecordSummary` / `IpcErrorCode` を**唯一の定義**として配置 |
| 依存 | daemon / cli は `shikomi_core::ipc::IpcRequest` を import するのみ。再定義しない（grep で `enum IpcRequest` が `shikomi-core/src/ipc/` 配下に 1 箇所のみ存在することを CI で検証） |
| 制約 | `shikomi-core::ipc` は **`tokio` / `rmp-serde` / `tokio-util` を依存に持たない**。`serde::{Serialize, Deserialize}` の derive のみ許可（型定義のみで I/O を持たない、`tech-stack.md` §4.5 と整合） |

### REQ-DAEMON-019: プロトコルバージョン管理

| 項目 | 内容 |
|------|------|
| 型定義 | `pub enum IpcProtocolVersion` を `#[non_exhaustive]` で宣言。バリアント `V1`（初期）/ **`V2`（Sub-E #43 で非破壊昇格、暗号化 vault unlock / lock / change-password / rotate-recovery / rekey 対応）** |
| シリアライズ | `serde(rename_all = "snake_case")` で `"v1"` / `"v2"` 等の文字列表現で送受信（数値より人間可読、ログ可読性向上）|
| 拡張規則 | 破壊的変更時に `V2`、`V3` 等のバリアントを追加。**バリアントの削除 / 改名は禁止**。フィールド追加は新バリアントに留める（`V1` の payload 拡張ではなく `V2` 新設） |
| **Sub-E V2 拡張**（**Sub-E #43 完了**）| `IpcRequest::{Unlock, Lock, ChangePassword, RotateRecovery, Rekey}` 5 新 variant、`IpcResponse::{Unlocked, Locked, PasswordChanged, RecoveryRotated, Rekeyed}` 5 新 variant、`IpcErrorCode::{VaultLocked, BackoffActive, RecoveryRequired, ProtocolDowngrade}` 4 新 variant 追加。詳細は `detailed-design/protocol-types.md` §`Sub-E (#43) IPC V2 拡張`。**V1 クライアント非破壊**: handshake で V1 検出時は V1 サブセットのみ受理、V2 専用 variant 送信時は `ProtocolDowngrade` で拒否（`vault-encryption/requirements.md` MSG-S15 と整合） |

### REQ-DAEMON-020: secret マスキング（IPC 経路）

| 項目 | 内容 |
|------|------|
| 対象 | `IpcRequest::AddRecord.value` / `IpcRequest::EditRecord.value` / `IpcResponse::Records[].value_preview`（Text のみ）|
| シリアライズ型 | `SecretBytes`（`shikomi-core` で本 feature と同時に導入する `Vec<u8>` 系 secret ラッパ。`Debug` で `[REDACTED]` 固定、`serde::Serialize` 実装で `Vec<u8>` 相当のバイト列に直接変換し `expose_secret` を呼ばない経路を提供）|
| 保証機構 | (1) `SecretBytes::Debug` が `[REDACTED]` 固定、(2) `crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/` 配下で `expose_secret()` を**呼ばない**（CI grep で 0 件監査）、(3) daemon 内部で `SecretBytes` → `SecretString::from_bytes` 変換で `RecordPayload::Plaintext` に格納、(4) `Records` 投影時の `value_preview` は `RecordKind::Text` のみで生成（Secret は `None` フィールド）|
| 検証 | UT で `tracing::info!("{:?}", request)` の出力に secret マーカー文字列が含まれないこと、E2E で daemon ログに secret が出ないこと |

### REQ-DAEMON-021: エラー応答の型化

| 項目 | 内容 |
|------|------|
| 型定義 | `pub enum IpcErrorCode` を `#[non_exhaustive]` で宣言。バリアント: `EncryptionUnsupported` / `NotFound { id: RecordId }` / `InvalidLabel { reason: String }` / `Persistence { reason: String }` / `Domain { reason: String }` / `Internal { reason: String }` |
| 制約 | `reason` フィールドは**人間可読の英語短文**で、secret 値・ファイル絶対パス・ピア UID 等を含まない（漏洩経路防止）。`reason` の生成は daemon サーバ側で `PersistenceError::Display` / `DomainError::Display` を文字列化（既存型の Display は secret を含まないことが `cli-vault-commands` で証明済み）|

### REQ-DAEMON-022: panic hook（daemon 側）

| 項目 | 内容 |
|------|------|
| 対象 | `shikomi-daemon` プロセスの panic |
| 処理 | CLI と同型の **fixed-message panic hook**: (1) `info.payload()` / `info.message()` / `info.location()` を**一切参照しない**、(2) `tracing::error!` / `tracing::warn!` 等の tracing マクロを**呼ばない**、(3) `eprintln!` で固定文言のみ（`"error: shikomi-daemon internal bug; please report this issue to https://github.com/shikomi-dev/shikomi/issues\n"`）、(4) panic unwind 続行（Rust ランタイムの既定終了コード 101 を許容、CI で `RUST_BACKTRACE=0` 推奨） |
| 出力 | stderr: 固定文言のみ |
| 検証 | CI grep で `crates/shikomi-daemon/src/` 配下の panic hook 関数で `info.payload()` / `info.message()` / `info.location()` / `tracing::*!` が呼ばれないこと |

### REQ-DAEMON-023: vault 同時アクセス制御（in-process）

| 項目 | 内容 |
|------|------|
| 対象 | daemon プロセス内の `Vault` 集約に対する複数 IPC 接続からの並行操作 |
| 処理 | daemon プロセスは `Arc<Mutex<Vault>>` で `Vault` を保持。各 IPC リクエストハンドラがロック取得 → 操作 → ロック解放を行う。MVP ではシリアル化（並行性は不要、p95 50 ms / リクエストの応答時間で十分） |
| 注記 | `tokio::sync::Mutex` を採用（`std::sync::Mutex` だと `await` を跨いで保持できない）。将来 `RwLock` 化で List の並行読みを許可する余地（YAGNI）|

### REQ-DAEMON-024: daemon プロセス終了コード契約

| コード | 意味 | 典型例 |
|-------|------|-------|
| 0 | 正常終了（graceful shutdown 完了）| `SIGTERM` 受信、in-flight 完了、ソケット削除、`VaultLock` 解放 |
| 1 | システムエラー | `repo.load()` 失敗、ソケット bind 失敗、SDDL 設定失敗 |
| 2 | シングルインスタンス先取り失敗 | `flock` 獲得不能（他 daemon 稼働中）、Named Pipe `FILE_FLAG_FIRST_PIPE_INSTANCE` 失敗 |
| 3 | 暗号化 vault 検出（本 feature 未対応） | 起動時の `repo.load()` で `Vault::protection_mode() == Encrypted` |
| 101 | panic（Rust ランタイム既定）| 想定外バグ |

## 画面・CLI仕様

### `shikomi-daemon` バイナリの起動

```
shikomi-daemon                  # 既定: $XDG_RUNTIME_DIR/shikomi/daemon.sock 等で listen
shikomi-daemon --help           # ヘルプ表示
shikomi-daemon --version        # バージョン表示
```

**本 feature では `shikomi-daemon` のフラグは `--help` / `--version` のみ**。`--vault-dir` / `--socket` 等の上書きは将来 feature（OS 自動起動 + GUI feature）で追加余地。env による設定経路:

| 環境変数 | 意味 | デフォルト |
|---------|------|----------|
| `SHIKOMI_DAEMON_LOG` | `tracing` レベル | `info` |
| `SHIKOMI_VAULT_DIR` | vault ディレクトリ上書き（既存 `shikomi-cli` と同一規約）| OS デフォルト（`dirs::data_dir().join("shikomi")`）|

### `shikomi-cli` の `--ipc` グローバルフラグ

```
shikomi --ipc list              # daemon 経由で list
shikomi --ipc add --kind text --label L --value V
shikomi --ipc edit --id <ID> --label NEW
shikomi --ipc remove --id <ID> --yes
shikomi list                    # 既定（Phase 1、SQLite 直結、本 feature で挙動変更なし）
```

| フラグ | 型 | 意味 | デフォルト |
|-------|---|------|----------|
| `--ipc` | `bool` | daemon 経由経路に切替 | `false`（Phase 1 の SQLite 直結を維持）|

`--ipc` は既存の `--vault-dir` / `--quiet` / `--verbose` と同じく `#[arg(long, global = true)]` で全サブコマンドから利用可。

### 確認プロンプト（`shikomi --ipc remove`）

`--ipc` 経路でも確認プロンプトの挙動は同一（`is_terminal::IsTerminal` 判定 → `--yes` 必須 / TTY プロンプト y/Y）。`ConfirmedRemoveInput` 型強制も同じ。daemon 側は確認済みリクエストとして受理する。

## API仕様

本 feature は HTTP エンドポイントを持たない。**IPC スキーマ**を内部公開 Rust API として列挙する。

**`shikomi-core::ipc` モジュールの公開型**:

| モジュール | 公開型 / 関数 | 用途 |
|----------|-------------|------|
| `shikomi_core::ipc` | `pub enum IpcProtocolVersion { V1 }`（`#[non_exhaustive]`）| プロトコルバージョン |
| `shikomi_core::ipc` | `pub enum IpcRequest { Handshake { client_version: IpcProtocolVersion }, ListRecords, AddRecord { kind, label, value, now }, EditRecord { id, label, value, now }, RemoveRecord { id } }`（`#[non_exhaustive]`、Serialize / Deserialize） | クライアントから daemon への要求 |
| `shikomi_core::ipc` | `pub enum IpcResponse { Handshake { server_version: IpcProtocolVersion }, ProtocolVersionMismatch { server, client }, Records(Vec<RecordSummary>), Added { id }, Edited { id }, Removed { id }, Error(IpcErrorCode) }`（`#[non_exhaustive]`、Serialize / Deserialize） | daemon からクライアントへの応答 |
| `shikomi_core::ipc` | `pub struct RecordSummary { id, kind, label, value_preview: Option<String>, value_masked: bool }` | List 応答の機密値非含有な投影型 |
| `shikomi_core::ipc` | `pub enum IpcErrorCode { EncryptionUnsupported, NotFound { id }, InvalidLabel { reason }, Persistence { reason }, Domain { reason }, Internal { reason } }`（`#[non_exhaustive]`）| 構造化エラー |
| `shikomi_core::ipc` | `pub struct SerializableSecretBytes(pub SecretBytes)` | secret 値の MessagePack 経路専用ラッパ（`Serialize` / `Deserialize` を `expose_secret` 不使用で実装）|

**`shikomi-cli::io::ipc_vault_repository` モジュールの公開型**（Issue #30 で `VaultRepository` trait 非実装に確定、専用メソッド 4 種を公開）:

| モジュール | 公開型 / 関数 | 用途 |
|----------|-------------|------|
| `shikomi_cli::io::ipc_vault_repository` | `pub struct IpcVaultRepository` | IPC 専用クライアント（**`VaultRepository` trait は実装しない**） |
| 〃 | `pub fn IpcVaultRepository::connect(socket_path: &Path) -> Result<Self, PersistenceError>` | daemon 接続 + ハンドシェイク |
| 〃 | `pub fn default_socket_path() -> Result<PathBuf, PersistenceError>` | OS 既定のソケットパス解決 |
| 〃 | `pub fn list_summaries(&self) -> Result<Vec<RecordSummary>, PersistenceError>` | `IpcRequest::ListRecords` 発行（PR #29 既存） |
| 〃 | `pub fn add_record(&self, kind: RecordKind, label: RecordLabel, value: SecretString, now: OffsetDateTime) -> Result<RecordId, PersistenceError>` | `IpcRequest::AddRecord` 発行（**Issue #30 新規**）。id は **daemon 側生成**、CLI 側で `Uuid::now_v7()` を呼ばない |
| 〃 | `pub fn edit_record(&self, id: RecordId, label: Option<RecordLabel>, value: Option<SecretString>, now: OffsetDateTime) -> Result<RecordId, PersistenceError>` | `IpcRequest::EditRecord` 発行（**Issue #30 新規**） |
| 〃 | `pub fn remove_record(&self, id: RecordId) -> Result<RecordId, PersistenceError>` | `IpcRequest::RemoveRecord` 発行（**Issue #30 新規**） |

**`shikomi-daemon` の内部公開 API**（`#[doc(hidden)]`、`[lib] + [[bin]]` 構成、cli と同型）:

| モジュール | 公開型 / 関数 | 用途 |
|----------|-------------|------|
| `shikomi_daemon::run` | `pub async fn run() -> ExitCode` | daemon コンポジションルート |
| `shikomi_daemon::lifecycle` | `pub struct SingleInstanceLock` | `flock` + ソケット再 bind の RAII |
| `shikomi_daemon::ipc::server` | `pub struct IpcServer<R: VaultRepository>` | listen + accept ループ |
| `shikomi_daemon::ipc::handler` | `pub fn handle_request<R: VaultRepository>(repo: &R, vault: &mut Vault, req: IpcRequest) -> IpcResponse` | リクエスト → レスポンス pure 写像（in-flight `Mutex` 保持外）|

**依存方向の厳守**:

- `shikomi-core::ipc` → `shikomi-core::{vault, secret}` のみ（OS / I/O 依存なし、`tokio` / `rmp-serde` 不可）
- `shikomi-cli::io::ipc_vault_repository` → `shikomi-core::ipc` + `shikomi-infra::VaultRepository` trait + `tokio` + `rmp-serde` + `tokio-util`
- `shikomi-daemon` → `shikomi-core::ipc` + `shikomi-infra` + `tokio` + `rmp-serde` + `tokio-util`
- `shikomi-cli` と `shikomi-daemon` は**相互依存しない**（`shikomi-core::ipc` で型を共有するのみ）

## データモデル

本 feature は永続化スキーマを持たない（vault.db は既存 `shikomi-infra` のスキーマを流用）。**IPC 経路で運搬する DTO** を列挙する。

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| `IpcProtocolVersion` | — | `enum { V1 }` | `#[non_exhaustive]`、Serialize（`"v1"`）| Handshake で送受信 |
| `IpcRequest::Handshake` | `client_version` | `IpcProtocolVersion` | 接続直後の必須 1 往復 | — |
| `IpcRequest::ListRecords` | — | unit variant | フィールドなし（フィルタは将来）| — |
| `IpcRequest::AddRecord` | `kind` | `RecordKind` | `Text` / `Secret` | — |
| `IpcRequest::AddRecord` | `label` | `RecordLabel` | 検証済み | — |
| `IpcRequest::AddRecord` | `value` | `SerializableSecretBytes` | secret 経路専用ラッパ | — |
| `IpcRequest::AddRecord` | `now` | `OffsetDateTime` | UTC | — |
| `IpcRequest::EditRecord` | `id` | `RecordId` | 検証済み | — |
| `IpcRequest::EditRecord` | `label` | `Option<RecordLabel>` | 任意 | — |
| `IpcRequest::EditRecord` | `value` | `Option<SerializableSecretBytes>` | 任意 | — |
| `IpcRequest::EditRecord` | `now` | `OffsetDateTime` | UTC | — |
| `IpcRequest::RemoveRecord` | `id` | `RecordId` | 検証済み | — |
| `IpcResponse::Handshake` | `server_version` | `IpcProtocolVersion` | — | — |
| `IpcResponse::ProtocolVersionMismatch` | `server`, `client` | `IpcProtocolVersion` × 2 | — | — |
| `IpcResponse::Records` | — | `Vec<RecordSummary>` | — | — |
| `IpcResponse::Added` / `Edited` / `Removed` | `id` | `RecordId` | — | — |
| `IpcResponse::Error` | — | `IpcErrorCode` | — | — |
| `RecordSummary` | `id` | `RecordId` | — | Record から射影 |
| `RecordSummary` | `kind` | `RecordKind` | — | — |
| `RecordSummary` | `label` | `RecordLabel` | — | — |
| `RecordSummary` | `value_preview` | `Option<String>` | Text のみ Some（先頭 40 char）/ Secret は None | `shikomi-core::Record::text_preview` を流用 |
| `RecordSummary` | `value_masked` | `bool` | Secret は `true`、Text は `false` | UI 側のマスク表示分岐用 |
| `IpcErrorCode` | — | `enum`（6 バリアント）| `#[non_exhaustive]` | — |

**注記**:
- `SerializableSecretBytes` は `shikomi-core::ipc` で**新規定義**する secret 専用ラッパ。`Vec<u8>` 相当のバイト列を MessagePack の `bin` 型で直接シリアライズし、`Serialize::serialize` 内で `expose_secret` を呼ばずに完了する経路（serde の `serialize_bytes` を直接呼び、`SecretBytes` の内部参照を unsafe なしで渡す方式）。詳細は基本設計 `basic-design/security.md §SecretBytes のシリアライズ契約` で確定する
- `IpcRequest::AddRecord.now` / `EditRecord.now` は**クライアント側で生成**して送る（daemon の時刻と差異が出るが、既存 `cli-vault-commands` の UseCase が `now` を引数で受ける設計と整合）。daemon 内部で再取得する案も検討余地（YAGNI、本 feature では client 生成を採用）

**`CliError` の追加バリアント**（`shikomi-cli::error`、本 feature で追加）:

| バリアント | 用途 | 写像 `ExitCode` |
|-----------|------|--------------|
| `DaemonNotRunning(PathBuf)` | `--ipc` 指定で daemon 接続不可 | `UserError (1)` |
| `ProtocolVersionMismatch { server, client }` | ハンドシェイク不一致 | `UserError (1)` |

`CliError::Persistence(PersistenceError)` は既存。`PersistenceError` 側にも本 feature で `DaemonNotRunning` / `ProtocolVersionMismatch` / `Io` 系の IPC 由来バリアントを追加するかは詳細設計で確定（基本案: `PersistenceError` に直接追加して `From<PersistenceError> for CliError` 経路を再利用）。

## ユーザー向けメッセージ一覧

### 成功系（stdout）

`shikomi --ipc list` / `add` / `edit` / `remove` の成功時メッセージは**既存 `MSG-CLI-001〜005` と同一**（`cli-vault-commands` の規約を変更しない）。

### 警告系（stderr）

| ID | メッセージ（英語） | メッセージ（日本語） | 表示条件 |
|----|----------------|------------------|---------|
| MSG-CLI-051 | `warning: --ipc routes operations through shikomi-daemon (preview); default path remains direct SQLite` | `警告: --ipc は shikomi-daemon 経由経路（プレビュー）。既定経路は引き続き SQLite 直結です` | `--ipc` 指定時のみ（情報通知、終了コード 0 維持。`--quiet` で抑止）。Issue #30（Phase 1.5）で 4 サブコマンド全て透過 |

### エラー系（stderr、`error:` 接頭辞 + `hint:` 行、hint は 3 OS 並記の複数行）

| ID | 原因文（英語） | 原因文（日本語） | ヒント構成 | 表示条件 | 終了コード |
|----|-------------|--------------|---------|---------|---------|
| MSG-CLI-110 | `error: shikomi-daemon is not running (socket {path} unreachable)` | `error: shikomi-daemon が起動していません（ソケット {path} に接続できません）` | 3 OS の起動コマンドを並記した複数行 hint（`basic-design/error.md §MSG-CLI-110 確定文面`）| `--ipc` 指定で接続失敗 | 1 |
| MSG-CLI-111 | `error: protocol version mismatch (server={server}, client={client})` | `error: プロトコルバージョン不一致（server={server}, client={client}）` | `hint: rebuild shikomi-cli and shikomi-daemon to the same version` / `hint: shikomi-cli と shikomi-daemon を同一バージョンにビルドし直してください` | ハンドシェイク不一致 | 1 |

**MSG-CLI-110 / 111 の確定文面は `basic-design/error.md` を単一真実源とする**。本 `requirements.md` 表は表示条件・終了コードの規約のみ扱い、文面の版ズレを避ける。

### daemon 側のログメッセージ（`tracing::info!` / `warn!` / `error!`）

daemon は **`tracing` で構造化ログ**を出す。エンドユーザ向け CLI メッセージとは別経路。代表ログ:

| レベル | メッセージテンプレート | 発生条件 |
|-------|------------------|---------|
| `info` | `shikomi-daemon listening on {socket_path}` | 起動成功 |
| `info` | `client connected (peer_uid={uid})` | 接続受付 + ピア検証成功 |
| `warn` | `peer credential mismatch: expected uid={daemon_uid}, got uid={peer_uid}; closing connection` | ピア検証失敗 |
| `warn` | `protocol mismatch: server={server}, client={client}; closing connection` | ハンドシェイク不一致 |
| `warn` | `MessagePack decode failed: {err}; closing connection` | デコード失敗 |
| `warn` | `frame length exceeds 16 MiB; closing connection` | フレーム長超過 |
| `error` | `failed to load vault: {err}` | 起動時 `repo.load()` 失敗 |
| `error` | `vault is encrypted; daemon does not support encrypted vaults yet` | 暗号化 vault 検出 |
| `info` | `graceful shutdown complete` | 終了成功 |

**secret 露出禁止**: `tracing` マクロの引数に `IpcRequest::AddRecord.value` / `IpcResponse::Records[].value_preview` を含む値を渡さない（CI grep で監査）。

## 依存関係

| crate | バージョン | feature | 用途 |
|-------|----------|--------|------|
| `tokio` | `^1.44.2`（既存追加: `^1.44.2`、`workspace.dependencies` 経由）| `rt-multi-thread`, `net`, `io-util`, `sync`, `signal`, `time`, `macros` | 多重スレッドランタイム / UDS / Named Pipe / `Mutex` / `signal::ctrl_c` |
| `tokio-util` | `^0.7`（新規追加） | `codec` | `LengthDelimitedCodec` |
| `rmp-serde` | `^1.3`（新規追加） | — | MessagePack シリアライズ（`Raw`/`RawRef` 不使用契約） |
| `serde` | `1`（既存） | `derive` | `IpcRequest` / `IpcResponse` の `#[derive]` |
| `time` | `0.3`（既存） | `serde`, `macros` | `OffsetDateTime` シリアライズ |
| `bytes` | `^1`（新規追加、`tokio-util` の trait 連携用）| — | `Bytes` 型（`Framed` の `Item` として）|
| `windows-sys` | `^0.59`（新規追加、Windows のみ） | `Win32_System_Pipes`, `Win32_Security`, `Win32_Foundation`, `Win32_System_Threading` | Named Pipe `GetNamedPipeClientProcessId` / `OpenProcessToken` / SDDL 設定 |
| `nix` | `^0.29`（新規追加、Unix のみ） | `feature::user`, `feature::socket`, `feature::fs` | `getsockopt(SO_PEERCRED / LOCAL_PEERCRED)`、`flock` 操作 |
| `shikomi-core` | workspace path | — | ドメイン型 + IPC スキーマ（本 feature で `ipc` モジュール追加）|
| `shikomi-infra` | workspace path | — | `VaultRepository` trait / `SqliteVaultRepository` |
| `dirs` | `5`（既存） | — | OS デフォルトソケットパス解決（XDG_RUNTIME_DIR フォールバック）|
| `tracing` | `0.1`（既存） | — | 構造化ログ |
| `tracing-subscriber` | `0.3`（既存） | `fmt`, `env-filter` | daemon 起動時ログ初期化 |

**dev-dependencies**（テスト用）:

| crate | バージョン | feature | 用途 |
|-------|----------|--------|------|
| `tempfile` | `3`（既存） | — | テスト用一時ソケットディレクトリ |
| `assert_cmd` | `2`（既存） | — | E2E プロセス起動 |
| `predicates` | `3`（既存） | — | `assert_cmd` assertion |
| `tokio` | 同上 + `test-util` feature | `test-util` | `tokio::test` / `time::pause` |

全て `Cargo.toml` ルートの `[workspace.dependencies]` 経由で指定し、`crates/{shikomi-daemon,shikomi-cli,shikomi-core}/Cargo.toml` では `{ workspace = true }` で参照する（`tech-stack.md` §4.4）。

**`tech-stack.md` 反映**: `tokio` / `tokio-util` / `rmp-serde` の追加・バージョンピン根拠は PR #27 の §2.1 IPC 4 行で**既に確定済み**。本 feature では `[workspace.dependencies]` への追加のみ実装 PR で行う（設計 PR では tech-stack.md を変更しない、infra-changes.md と同方針）。

## 関連 feature

| feature | 関係 | 参照先 |
|---------|------|--------|
| `vault`（Issue #7） | 本 feature は `shikomi-core` の `Vault` / `Record` / `RecordLabel` / `RecordId` / `RecordKind` / `RecordPayload` / `SecretString` / `ProtectionMode` / `DomainError` を利用する。**本 feature で `shikomi-core::SecretBytes` が未存在の場合は `shikomi-core::secret` に新規追加する**（`SecretString` のバイト列版、`Debug` で `[REDACTED]` 固定、`zeroize` 対応）。`shikomi-core` への変更は `ipc` モジュール新設 + `secret::SecretBytes` 追加（既存の場合はスキップ）の 2 点のみ | `docs/features/vault/` |
| `vault-persistence`（Issue #10） | 本 feature は `VaultRepository` trait / `SqliteVaultRepository` / `VaultLock` を再利用する。**`shikomi-infra` 側に変更は加えない**（`from_directory` は既に `cli-vault-commands` で追加済み）。`PersistenceError` に IPC 由来の `DaemonNotRunning` / `ProtocolVersionMismatch` バリアントを追加する場合は `shikomi-infra::persistence::error` を編集 | `docs/features/vault-persistence/` |
| `cli-vault-commands`（Issue #21、merge 済み） | 本 feature は `shikomi-cli` の clap 設定とコンポジションルートを編集して `--ipc` フラグと `IpcVaultRepository::connect` 経路を追加する。**変更ファイルは `clap-config.md` / `composition-root.md` / `future-extensions.md` の 3 点**。既存の REQ-CLI-001〜012 / `MSG-CLI-001〜109` の挙動は変更しない（後方互換）| `docs/features/cli-vault-commands/` |
| **未起票 — daemon-hotkey** | daemon に `IpcRequest::HotkeyRegister { ... }` 等のバリアント追加で機能拡張。本 feature の `#[non_exhaustive] enum` 設計が拡張点 | （将来 Issue） |
| **未起票 — daemon-clipboard** | daemon に `IpcRequest::ClipboardInject { record_id, sensitive_hint }` 等のバリアント追加 | （将来 Issue） |
| **未起票 — daemon-vault-encryption** | `IpcRequest::Unlock { master_password: SecretBytes }` / `IpcRequest::Lock` / `IpcRequest::Rekey { ... }` 等のバリアント追加。本 feature の `IpcResponse::Error(EncryptionUnsupported)` が誘導先になる | （将来 Issue） |
| **未起票 — daemon-session-token** | `IpcProtocolVersion::V2` 追加 + `IpcRequest::Handshake { client_version, session_token: SerializableSecretBytes }` への非破壊拡張。`#[non_exhaustive] enum` の効能 | （将来 Issue） |
| **未起票 — shikomi-gui** | GUI が `IpcVaultRepository::connect` を共有する場合、`shikomi-cli::io::IpcVaultRepository` を別 crate に切り出すか GUI feature 内で再実装するかを判断 | （将来 Issue） |

## アーキテクチャ文書への影響

本 feature の設計範囲では `docs/architecture/` 配下への**追加変更が発生しない**。

理由: 工程 0（PR #27）で `process-model.md` §4.1 / §4.1.1 / §4.2、`tech-stack.md` §2.1（IPC 4 行）、`threat-model.md` §S 行 / §A07 / §D 行が**Issue #26 の実装スコープに合わせて既に整合済み**。本 feature の設計書は arch ドキュメントの規定に**準拠する**側であり、再規定しない（DRY）。

実装 PR で `[workspace.dependencies]` に `tokio-util` / `rmp-serde` / `bytes` / `nix` / `windows-sys` を追加する際、`tech-stack.md` §4.5 / §4.6 の crate リストに整合性を取るため**実装 PR 内で**反映する（設計 PR では `tech-stack.md` を変更しない、`cli-vault-commands` の `infra-changes.md` 同方針）。
