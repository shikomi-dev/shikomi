# 詳細設計書 — daemon-runtime（tokio ランタイム / IpcServer / handler / peer credential）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/daemon-runtime.md -->
<!-- 兄弟: ./index.md, ./protocol-types.md, ./ipc-vault-repository.md, ./composition-root.md, ./lifecycle.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。各層のシグネチャをインライン `code` で示し、実装本体は書かない。

## モジュール構成（`shikomi-daemon`）

```
crates/shikomi-daemon/src/
  main.rs                     # tokio::main エントリ、3 行ラッパ
  lib.rs                      # pub async fn run() -> ExitCode
  panic_hook.rs               # CLI 同型 fixed-message
  lifecycle/
    mod.rs                    # 再エクスポート
    single_instance.rs        # SingleInstanceLock (./lifecycle.md 参照)
    shutdown.rs               # signal handler + in-flight wait
  ipc/
    mod.rs                    # 再エクスポート
    server.rs                 # IpcServer<R: VaultRepository>
    framing.rs                # LengthDelimitedCodec wrapper
    handshake.rs              # 初回フレーム検証
    handler.rs                # handle_request pure 写像
    transport/
      mod.rs                  # cfg 分岐の入口
      unix.rs                 # cfg(unix): UnixListener / UnixStream
      windows.rs              # cfg(windows): NamedPipeServer / NamedPipeClient
  permission/
    mod.rs                    # 非依存エントリ
    peer_credential/
      mod.rs                  # OS 非依存トレイト
      unix.rs                 # cfg(unix): SO_PEERCRED / LOCAL_PEERCRED
      windows.rs              # cfg(windows): GetNamedPipeClientProcessId
```

## `IpcServer<R: VaultRepository>`

- 配置: `crates/shikomi-daemon/src/ipc/server.rs`
- 型: `pub struct IpcServer<R: VaultRepository + Send + Sync + 'static>`
- フィールド（概念、private）:
  - `listener` — `tokio::net::UnixListener`（unix）or `NamedPipeServer`（windows）の enum 包み or `cfg`
  - `repo: Arc<R>` — 共有 repository
  - `vault: Arc<Mutex<Vault>>` — 共有 vault 集約（`tokio::sync::Mutex`）
  - `shutdown: Arc<tokio::sync::Notify>` — graceful shutdown シグナル
  - `connections: tokio::task::JoinSet<()>` — in-flight 接続タスク管理

- メソッド:
  - `pub fn new(listener: ListenerEnum, repo: Arc<R>, vault: Arc<Mutex<Vault>>) -> Self`
  - `pub async fn start(&mut self) -> Result<(), DaemonError>` — accept ループ起動 + 接続ごとに `tokio::spawn`
  - `pub fn shutdown_notify(&self)` — `Notify::notify_waiters()`
  - `pub async fn join_all(self) -> Result<(), DaemonError>` — `JoinSet::join_all()` で全接続タスク完了待機

### `start` の処理順序

1. `tokio::select!` で:
   - `accept_branch`: `listener.accept().await` で新規接続
   - `shutdown_branch`: `shutdown.notified().await` で shutdown 通知
2. `accept_branch` 採用時:
   - `peer_credential::verify(&stream)?` でピア検証
     - 失敗時: `tracing::warn!(target: "shikomi_daemon::permission", "peer credential mismatch")` + `drop(stream)` でクローズ → loop 先頭へ
   - `Framed::new(stream, framing::codec())` でフレーム化
   - `tokio::spawn(handle_connection(framed, repo.clone(), vault.clone(), shutdown.clone()))` で接続タスク spawn
   - `JoinSet::spawn` で管理（graceful shutdown で待機可能にする）
3. `shutdown_branch` 採用時:
   - listener を drop（新規接続受付停止）
   - `JoinSet::join_all().await` で in-flight 接続の完了待機（タイムアウト 30 秒、`tokio::time::timeout` でラップ）
   - 完了 → return `Ok(())`

### `handle_connection` 関数（接続単位タスク）

- シグネチャ: `async fn handle_connection<R: VaultRepository + Send + Sync + 'static>(framed: Framed<...>, repo: Arc<R>, vault: Arc<Mutex<Vault>>, shutdown: Arc<Notify>) -> Result<(), DaemonError>`
- 処理順序:
  1. `handshake::negotiate(&mut framed).await?` でハンドシェイク 1 往復（タイムアウト 5 秒）
     - 失敗時: 接続切断、return（`tokio::spawn` の戻り値として観測される）
  2. loop:
     - `tokio::select!` で:
       - `frame_branch`: `framed.next().await` で次フレーム
       - `shutdown_branch`: `shutdown.notified().await`（接続単位でも shutdown 受信して in-flight 完了優先処理）
     - `frame_branch`:
       - `Some(Ok(bytes))` → `rmp_serde::from_slice::<IpcRequest>(&bytes)`
         - 失敗 → `tracing::warn!("MessagePack decode failed: {}", err)` → 切断
       - `Some(Err(_))`（`LengthDelimitedCodec` のエラー、フレーム長超過等）→ `tracing::warn!("frame error: {}", err)` → 切断
       - `None`（クライアント切断）→ 通常終了、return `Ok(())`
     - リクエスト受信成功 → 次の処理:
       - `let mut vault_guard = vault.lock().await;`
       - `let response = handler::handle_request(&*repo, &mut *vault_guard, request);`
       - `drop(vault_guard);` （ロック早期解放、次の `framed.send` を他接続と並行可能にする）
       - `let response_bytes = rmp_serde::to_vec(&response)?;`
       - `framed.send(response_bytes.into()).await?;`
     - `shutdown_branch`:
       - 現在のフレーム処理（in-flight）が完了していれば return
       - in-flight 中なら `?` で中断（既に処理は完了しているはず、待機経路では shutdown 採用が後勝ち）

## `framing::codec` 関数

- 配置: `crates/shikomi-daemon/src/ipc/framing.rs`
- シグネチャ: `pub fn codec() -> LengthDelimitedCodec`
- 内容: `LengthDelimitedCodec::builder().max_frame_length(16 * 1024 * 1024).little_endian().length_field_length(4).new_codec()`
- 配置理由: クライアント側（`shikomi-cli::io::ipc_client`）と同じ codec 設定を使うため、共通関数化を検討したが crate 境界（`shikomi-daemon` ↔ `shikomi-cli`）を跨ぐと依存関係が複雑化する。**両側で同じ設定値を独立に書く**（DRY 違反だが crate 分離を優先、定数 `MAX_FRAME_LENGTH = 16 * 1024 * 1024` は別途 `shikomi-core::ipc` に置く案も検討余地、本 feature では各 crate で独立記述）

**MAX_FRAME_LENGTH 定数の配置**:
- 案 A: `shikomi-core::ipc::MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024;` を pub const で公開し、daemon / cli の codec 設定で参照
- 案 B: 各 crate で個別記述（DRY 違反だが疎結合）
- **本 feature 採用: 案 A**（`shikomi-core::ipc` の純粋性を保ちつつ定数 1 つの公開で DRY 達成）

## `handshake::negotiate` 関数

- 配置: `crates/shikomi-daemon/src/ipc/handshake.rs`
- シグネチャ: `pub async fn negotiate<S>(framed: &mut Framed<S, LengthDelimitedCodec>) -> Result<(), HandshakeError> where S: AsyncRead + AsyncWrite + Unpin`
- 処理:
  1. `tokio::time::timeout(Duration::from_secs(5), framed.next())` で次フレーム待機
     - タイムアウト → `Err(HandshakeError::Timeout)`
     - `None` → `Err(HandshakeError::ConnectionClosed)`
     - `Some(Err(_))` → `Err(HandshakeError::FrameError)`
     - `Some(Ok(bytes))` → 次へ
  2. `rmp_serde::from_slice::<IpcRequest>(&bytes)?` でデコード
     - 失敗 → `Err(HandshakeError::Decode)`
  3. `match request`:
     - `IpcRequest::Handshake { client_version }` → 続行
     - その他 → `Err(HandshakeError::ExpectedHandshake { got: variant_name })`
  4. `client_version == IpcProtocolVersion::current()` 判定:
     - 一致 → `let response = IpcResponse::Handshake { server_version: IpcProtocolVersion::current() };` → `framed.send(rmp_serde::to_vec(&response)?.into()).await?;` → return `Ok(())`
     - 不一致 → `let response = IpcResponse::ProtocolVersionMismatch { server: current(), client: client_version };` → `framed.send(...)?` → return `Err(HandshakeError::VersionMismatch { server, client })`

- エラー型: `pub enum HandshakeError`
  - `Timeout`
  - `ConnectionClosed`
  - `FrameError`
  - `Decode`
  - `ExpectedHandshake { got: &'static str }`
  - `VersionMismatch { server: IpcProtocolVersion, client: IpcProtocolVersion }`

## `handler::handle_request` 関数

- 配置: `crates/shikomi-daemon/src/ipc/handler.rs`
- シグネチャ: `pub fn handle_request<R: VaultRepository>(repo: &R, vault: &mut Vault, request: IpcRequest) -> IpcResponse`
- 純粋性: I/O なし（`Mutex` ロックは呼び出し側 = `IpcServer::handle_connection` の責務、ハンドラ自体は `&mut Vault` を受け取る pure 写像）
- 処理: `match request`:

| 入力バリアント | 処理 | 出力 |
|--------------|------|------|
| `Handshake { ... }` | 不到達（ハンドシェイクは別経路で扱う、防御的に panic ではなく `IpcResponse::Error(Internal { reason: "handshake should be handled separately" })` を返す） | `IpcResponse::Error(Internal)` |
| `ListRecords` | `vault.records().iter().map(RecordSummary::from_record).collect::<Vec<_>>()` で投影 | `IpcResponse::Records(summaries)` |
| `AddRecord { kind, label, value, now }` | (1) `RecordPayload::Plaintext(SecretString::from_bytes(value.0))`、(2) `RecordId::new(uuid::Uuid::now_v7())`、(3) `Record::new(id, kind, label, payload, now)`、(4) `vault.add_record(record)` の `Result` を `IpcErrorCode::Domain` に写像、(5) `repo.save(vault)` の `Result` を `IpcErrorCode::Persistence` に写像 | 成功: `IpcResponse::Added { id }` / 失敗: `IpcResponse::Error(...)` |
| `EditRecord { id, label, value, now }` | (1) `vault.find_record(&id)` → `None` なら `IpcResponse::Error(IpcErrorCode::NotFound { id })`、(2) `vault.update_record(&id, |old| { ... })` で label / value 更新（`with_updated_label` / `with_updated_payload` 連鎖）、(3) `repo.save(vault)` | 成功: `IpcResponse::Edited { id }` / 失敗: `IpcResponse::Error(...)` |
| `RemoveRecord { id }` | (1) `vault.find_record(&id)` → `None` なら `NotFound`、(2) `vault.remove_record(&id)`、(3) `repo.save(vault)` | 成功: `IpcResponse::Removed { id }` / 失敗: `IpcResponse::Error(...)` |

**設計理由**:
- `match request` で全バリアント網羅。`#[non_exhaustive]` のため `_ => IpcResponse::Error(Internal { reason: "unknown request variant" })` で wildcard を入れる（後続 feature で V2 バリアントが届いた場合の防御）。実際には daemon が V1 の場合、ハンドシェイクで V2 client は弾かれるはずだが多層防御として
- `Result<IpcResponse, IpcErrorCode>` 風の構造を内部で組み、最終的に必ず `IpcResponse` を返す（単一の戻り値型でハンドラ呼び出し側を簡潔に）

**`Result` → `IpcErrorCode` 写像の規約**:

**絶対規則**: `IpcErrorCode` の `reason` フィールドは**ハードコードされた英語固定文言のみ**を格納する。`format!("{}", err)` / `err.to_string()` / `{:?}` デバッグ出力による動的文字列化を**禁止**する（服部平次指摘への対応）。

根拠: `shikomi-infra::PersistenceError` の `Display` 実装は以下の漏洩経路を含み、IPC 経由で同 UID 悪性プロセスにファイルシステム情報を開示する:
- `"IO error on {path}: {source}"` — vault 絶対パス漏洩
- `"invalid permission on {path}: expected ..."` — 絶対パス + 期待 perm 漏洩
- `"invalid vault dir {path}: {reason}"` — 絶対パス漏洩
- `"vault is locked at {path}{holder}"` — **lock holder PID 漏洩**（後続攻撃の起点）

これを構造化エラーの `reason` に含めると `basic-design/security.md §IpcErrorCode 設計規約`「secret 値・絶対パス・ピア UID を含めない」契約と正面衝突し、OWASP A07 / A09 違反となる。

**写像規約（固定文言版、本書が単一真実源）**:

| 入力 | 出力 `IpcErrorCode` | reason 固定文言 |
|-----|-------------------|---------------|
| `PersistenceError::Io(_)` / `Locked { .. }` / `Permission { .. }` / `InvalidVaultDir { .. }` 等の全パターン | `IpcErrorCode::Persistence { reason: "persistence error" }` | `"persistence error"`（固定）|
| `PersistenceError::Corrupted { .. }` | `IpcErrorCode::Persistence { reason: "vault corrupted" }` | `"vault corrupted"`（固定、破損検出の分類は許容。path を含めない）|
| `PersistenceError::CannotResolveVaultDir` | `IpcErrorCode::Persistence { reason: "vault directory not resolvable" }` | 固定 |
| `DomainError::InvalidRecordLabel(_)` | `IpcErrorCode::InvalidLabel { reason: "invalid label" }` | 固定 |
| `DomainError::InvalidRecordId(_)` | `IpcErrorCode::InvalidLabel { reason: "invalid record id" }` | 固定（クライアント側で検証済みのはずだが防御的）|
| `DomainError::VaultConsistencyError(VaultConsistencyError::RecordNotFound(id))` | `IpcErrorCode::NotFound { id }` | id は RecordId（UUIDv7、秘密情報ではない、既に `IpcRequest` で受取済みの値を返すだけ）|
| `DomainError::VaultConsistencyError(VaultConsistencyError::DuplicateRecordId(_))` | `IpcErrorCode::Domain { reason: "duplicate record id" }` | 固定 |
| その他の `DomainError` | `IpcErrorCode::Domain { reason: "domain error" }` | 固定 |
| 想定外（`unwrap` / `expect` 経路、本来到達しないが防御的）| `IpcErrorCode::Internal { reason: "unexpected error" }` | 固定 |

**観測性の確保（詳細は daemon 側 `tracing` のみ）**:
- daemon 側で `tracing::warn!(target: "shikomi_daemon::ipc::handler", "persistence error: {}", err)` のように `Display` で詳細を出す（path を含む）
- ただし `tracing::*!` マクロに**`IpcRequest` 全体や `SecretString` / `SerializableSecretBytes` を渡さない**（既存規約、`../basic-design/security.md §脅威モデル`）
- 運用者は `SHIKOMI_DAEMON_LOG` で詳細ログを有効化し、`journalctl` / `launchctl log` / stderr で観測する。クライアント側には固定文言のみ返す（**運用者はログ特権あり、クライアント（同 UID 別プロセス）は特権なし**、という権限分離）

**CLI 側の体験**:
- CLI は `IpcResponse::Error(IpcErrorCode::Persistence { reason: "persistence error" })` を受信 → `render_error` で `MSG-CLI-107`（既存、`error: failed to access vault ...`）を出力。`reason` の "persistence error" 文字列は**無視**（i18n / ユーザ向け文面の単一真実源は `presenter::error` 側）
- daemon 側で `tracing::warn!` に出た詳細は、デバッグ時に運用者が daemon ログを確認する経路で補足

**テスト観点追加**:
- `handler::handle_request` の各エラーケースで `reason` が**絶対パス文字列・PID 数値・ピア UID を含まないこと**を文字列 assertion で確認（**TC-UT-xxx** にテスト設計担当が追加）
- 具体検証: `!response.contains("/home/")` / `!response.contains("C:\\Users\\")` / `!response.contains("pid=")` / `!response.contains("uid=")` 等の negative assertion

## `permission::peer_credential::verify` 関数

- 配置（OS 非依存エントリ）: `crates/shikomi-daemon/src/permission/peer_credential/mod.rs`
- シグネチャ: `pub fn verify<S: PeerCredentialSource>(stream: &S) -> Result<(), PeerVerificationError>`
- `PeerCredentialSource` trait（OS 別の実装が満たす）:
  - `pub trait PeerCredentialSource`
  - `fn peer_uid(&self) -> Result<u32, std::io::Error>`（Unix）
  - `fn peer_sid(&self) -> Result<String, std::io::Error>`（Windows）
  - 両 OS で共通: `fn matches_self(&self, peer: &PeerIdentity) -> bool`

**OS 別実装**:

### Unix

- 配置: `crates/shikomi-daemon/src/permission/peer_credential/unix.rs`
- 冒頭: `#![allow(unsafe_code)]`（モジュール限定 lint オーバーライド、`../../basic-design/security.md §unsafe_code の扱い`）
- `impl PeerCredentialSource for tokio::net::UnixStream`:
  - `fn peer_uid(&self) -> Result<u32, std::io::Error>` の実装:
    - **Linux**: `getsockopt(self.as_raw_fd(), SOL_SOCKET, SO_PEERCRED, ...)` で `ucred { pid, uid, gid }` 取得
    - **macOS**: `getsockopt(self.as_raw_fd(), SOL_LOCAL, LOCAL_PEERCRED, ...)` で `xucred { cr_uid, ... }` 取得
    - `cfg(target_os = "linux")` / `cfg(target_os = "macos")` で分岐
  - 採用 crate: `nix` v0.29 の `nix::sys::socket::getsockopt`（`PeerCredentials` option for Linux、macOS 用は `LocalPeerCred` option もしくは `libc` 直叩き）

### Windows

- 配置: `crates/shikomi-daemon/src/permission/peer_credential/windows.rs`
- 冒頭: `#![allow(unsafe_code)]`
- `impl PeerCredentialSource for tokio::net::windows::named_pipe::NamedPipeServer`:
  - `fn peer_sid(&self) -> Result<String, std::io::Error>`:
    - `GetNamedPipeClientProcessId(handle, &mut pid)` で接続元 PID 取得
    - `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid)` で HANDLE 取得
    - `OpenProcessToken(handle, TOKEN_QUERY, &mut token)` でトークン取得
    - `GetTokenInformation(token, TokenUser, ...)` で SID 取得
    - SID を文字列化（`ConvertSidToStringSidW`）して `String` で返す
  - 採用 crate: `windows-sys` v0.59 の `Win32_System_Pipes` / `Win32_System_Threading` / `Win32_Security` features

### `verify` 関数の判定ロジック

- 概念実装（疑似コードではなく**判定論理**のみ記述）:
  1. `let peer = stream.peer_identity()?;`（Unix なら uid、Windows なら SID）
  2. `let self_id = current_process_identity()?;`
  3. `if peer != self_id { return Err(PeerVerificationError::IdentityMismatch { ... }); }`
  4. `Ok(())`
- 実装は OS 別の trait 実装に委ね、`verify` 関数は trait 越しの汎用的な比較のみ

## エラー型: `DaemonError` / `PeerVerificationError` / `HandshakeError`

- 配置: `crates/shikomi-daemon/src/error.rs`（新規）
- 型:
  - `pub enum DaemonError` — daemon プロセス全体の総合エラー
    - `SingleInstance(SingleInstanceError)` — `./lifecycle.md §SingleInstanceLock`
    - `LoadVault(PersistenceError)`
    - `EncryptionUnsupported`
    - `Server(ServerError)`
    - `Io(std::io::Error)`
  - `pub enum ServerError` — サーバ実行時エラー
    - `Listen(std::io::Error)`
    - `Accept(std::io::Error)`
    - `JoinAll(tokio::task::JoinError)`
  - `pub enum PeerVerificationError` — `./security.md` で定義済みの内容に対応
    - `IdentityMismatch { expected, got }`
    - `Lookup(std::io::Error)`
  - `pub enum HandshakeError` — 上記節で定義済み
- すべて `#[derive(Debug, thiserror::Error)]`
- daemon の `run()` は `DaemonError` を `ExitCode` に写像（`./composition-root.md §run の処理順序`）

## tracing target 規約

daemon のログは `tracing::*!` マクロの `target` 引数で機能領域を分類:

| target | 用途 |
|--------|------|
| `shikomi_daemon::lifecycle` | 起動 / シャットダウン / シングルインスタンス |
| `shikomi_daemon::ipc::server` | accept ループ / 接続管理 |
| `shikomi_daemon::ipc::handshake` | ハンドシェイク交渉 |
| `shikomi_daemon::ipc::handler` | リクエスト処理 |
| `shikomi_daemon::permission` | ピア検証 |

`SHIKOMI_DAEMON_LOG=shikomi_daemon::ipc::handler=debug` のような細粒度制御を許す。

## 並行性とロック粒度

- `Arc<Mutex<Vault>>` を全接続が共有
- ハンドラ呼出時にロック取得 → ハンドラ実行 → ロック解放 → 応答送信
- `framed.send` は**ロック解放後**に行う（応答送信中に他接続の vault 操作をブロックしない）
- MVP ではシリアル化で十分（p95 50 ms / リクエストの応答時間で実用上問題なし）
- 将来 `RwLock` 化で List の並行読みを許す余地あり（YAGNI）

## CLI と daemon の同時実行（Phase 1 と Phase 2 の併存）

`process-model.md` §4.1.1 末尾で規定済み: Phase 1 期間中に CLI 直結と手動起動 daemon が同一 vault.db に同時アクセスする可能性は理論上ある。このケースは既存 `shikomi-infra::persistence::lock::VaultLock`（Unix `flock` / Windows `LockFileEx`）により一方が `PersistenceError::Locked` で Fail Fast する。本 feature でも同じ挙動。

`SingleInstanceLock`（daemon プロセスの単一性保証、`daemon.lock` ファイル）と `VaultLock`（vault データ側の永続化ロック、`vault.db.lock` ファイル）は**別層・別ファイル**で扱う。両者は責務が異なる（プロセス排他 vs データ排他）。

## テスト観点（テスト設計担当向け）

**ユニットテスト**:
- `handler::handle_request` の全 IpcRequest バリアント × Vault 状態（空 / 1 件 / 複数件 / 重複 id 等）の網羅
- `HandshakeError` 各バリアントの発生条件（タイムアウト / 切断 / decode 失敗 / 非 Handshake / 不一致）
- `PeerVerificationError` の発生条件（uid 不一致のモック）

**結合テスト（`tokio::test` + `tokio::io::duplex`）**:
- `IpcServer::handle_connection` をメモリ内 `duplex` ストリームで起動 → クライアントスタブが `IpcRequest` を送り `IpcResponse` を受信、各バリアント網羅
- ハンドシェイク成功 / プロトコル不一致 / タイムアウト / 非 Handshake 先送りの 4 経路
- MessagePack 破損送信時に該当接続のみ切断、他接続が継続することを検証

**E2E（実 daemon プロセス + 実 UDS / Named Pipe）**:
- `tempfile::TempDir` で独立ソケットディレクトリ
- daemon を `assert_cmd::Command::cargo_bin("shikomi-daemon").spawn()` で起動
- CLI を `--ipc` 付きで実行し各受入基準を検証
- daemon を `kill` / `Ctrl+C` で終了させ、graceful shutdown / 強制 shutdown の挙動確認

**3 OS matrix CI**: 既存 `.github/workflows/test-{core,infra}.yml` と同型の `daemon.yml`（または既存ワークフロー拡張）で Linux / macOS / Windows での E2E テスト実行。実装 PR で `.github/workflows/` 編集（本設計 PR では扱わない）。

テストケース番号の割当は `test-design/` が担当する。
