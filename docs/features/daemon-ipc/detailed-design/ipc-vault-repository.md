# 詳細設計書 — ipc-vault-repository（CLI 側 IpcVaultRepository / IpcClient）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 (Phase 1: list) / Issue #30 (Phase 1.5: add/edit/remove) -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md -->
<!-- 兄弟: ./index.md, ./protocol-types.md, ./daemon-runtime.md, ./composition-root.md, ./lifecycle.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。シグネチャはインライン `code` で示し、実装本体は書かない。

## 設計方針の確定（Issue #30 で更新、案 C → 案 D）

PR #29（Issue #26 / Phase 1）で `IpcVaultRepository` を `VaultRepository` trait の**シャドウ差分実装**（`./index.md` 設計判断 8 旧案 C）として実装しようとしたところ、以下の構造的破綻が顕在化した:

- `load()` で受け取る `Vec<RecordSummary>` から `Vault` 集約を再構築する際、`RecordPayload::Plaintext(SecretString::from_str(""))` のような**「嘘の Plaintext(empty)」を注入**しなければ `Record` を構築できない
- `add_record` で発行する `IpcResponse::Added { id }` を CLI 側で受け取って `Vault::add_record` に流すと、daemon 側生成の真実 ID と CLI 側の `Uuid::now_v7()` 生成 ID が**二重発行**になり、`save()` の差分計算で**嘘 ID の上書き**が起こる
- `exists()` が IPC 経路では本質的に**常に `true`**（接続成功 = vault 存在）となり、Phase 1 直結経路で意味があった `VaultNotInitialized` Fail Fast 経路が崩壊
- 「`load()` → `update_record(closure)` → `save()`」の `VaultRepository` の使い方を IPC 経路で再現するには、シャドウと新 `Vault` の `updated_at` timestamp 差分を見て差分計算する以外に手段がない。これは「同 timestamp で payload 変更」のエッジケースで検出漏れし、堅牢性が破綻

PR #29 ではこれらを**「`--ipc add/edit/remove` を runtime reject」して先送り**することで Phase 1 を成立させた。Issue #30 では、`IpcVaultRepository` が `VaultRepository` trait を**実装しない**設計（**案 D**）に確定し、上記の「嘘」を構造的に消滅させる。

### 案 D の核（本書の単一真実源）

- `IpcVaultRepository` は **`VaultRepository` trait を実装しない**。`load` / `save` / `exists` メソッドは持たない
- 代わりに、IPC 経路の責務に対応した**専用メソッド**を公開する: `list_summaries` / `add_record` / `edit_record` / `remove_record`
- CLI 側 Composition Root（`shikomi-cli::run`）は **`enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }`** で 2 経路を保持し、サブコマンドハンドラ（`run_list` / `run_add` / `run_edit` / `run_remove`）が `match handle` で経路分岐する
- `args.ipc == false` 経路は既存 UseCase（`usecase::list::list_records(&dyn VaultRepository)` 等）を呼出（無変更）
- `args.ipc == true` 経路は `IpcVaultRepository` の専用メソッドを**直接**呼出。UseCase 関数は通らない（IPC は daemon 側ハンドラがドメイン操作を完結させるため、CLI 側に再度ドメイン orchestration を置く必要がない）

### 案 D を採用する理由（設計原則との整合）

| 原則 | 案 D での体現 |
|------|--------------|
| **Tell, Don't Ask** | `IpcVaultRepository` は「list せよ」「add せよ」と命令される。`load()` で `Vault` を取り出して CLI 側で更新計算してから `save()` する Ask パターンを排除 |
| **Fail Fast** | 嘘の Plaintext(empty) / 嘘 ID / 常時 true な exists() を**型と責務の分離**で構造的に発生不能にする |
| **DRY** | daemon 側の `IpcRequestHandler` に既に存在するドメイン整合（`vault.add_record(record)?` / `vault.update_record(...)?`）を CLI 側で再実装しない |
| **YAGNI** | 「IPC 経路でも UseCase を共有する」という汎化を捨て、現実に必要な 4 メソッド経路のみ実装 |
| **Composition over Inheritance** | trait 実装による継承構造ではなく、enum dispatch + 専用メソッドという**委譲構造**で経路分離 |

### 案 D で失う性質と再獲得手段

| 失うもの | 影響 | 再獲得手段 |
|---------|------|----------|
| 「`run()` の 1 行差し替えで Phase 1 → Phase 2」（`cli-vault-commands` 旧設計） | Composition Root の `args.ipc` 分岐は 1 箇所のままだが、`run_*` 各サブコマンドハンドラに `match handle` 分岐が増える（4 箇所） | サブコマンドハンドラは引数 `RepositoryHandle` を受けて 2 アーム match するのみ。経路追加時のコストは線形だが**型システムで網羅性が保証**される（`#[non_exhaustive]` を付けない通常の enum） |
| `&dyn VaultRepository` 抽象による `usecase` 層の経路非依存性 | `usecase::*` は SQLite 経路でのみ呼ばれる。IPC 経路の `IpcVaultRepository` は UseCase を呼ばずに直接 `IpcClient::round_trip` を発行 | UseCase 層と IPC 直接層の責務が**明示的に分離**される。Phase 1 経路は「ローカル vault に対するドメイン orchestration」、IPC 経路は「daemon に対するリクエスト発行」と意味論が異なるため、無理に同一抽象に押し込まない方が**意味論の保全**になる |

## モジュール配置

`crates/shikomi-cli/src/io/` 配下のファイル構成（PR #29 で確定済み、Issue #30 で内部メソッドのみ拡張）:

```
crates/shikomi-cli/src/io/
  mod.rs                      # 既存、ipc_vault_repository / ipc_client を export
  paths.rs                    # 既存、変更なし
  terminal.rs                 # 既存、変更なし
  ipc_vault_repository.rs     # PR #29 で新設、Issue #30 で add/edit/remove メソッド追加
  ipc_client.rs               # PR #29 で新設、変更なし（round_trip 経路を再利用）
  windows_sid.rs              # PR #29 で新設、変更なし（Windows のみ、SID 解決）
```

`crates/shikomi-cli/src/io/mod.rs` の export は変更なし:

```
pub mod ipc_vault_repository;
pub mod ipc_client;
#[cfg(windows)]
pub mod windows_sid;
```

## `IpcClient`

PR #29 で確定した責務を維持。Issue #30 で変更なし。

- 配置: `crates/shikomi-cli/src/io/ipc_client.rs`
- 型: `pub struct IpcClient`
- フィールド（private）:
  - `framed` — `Framed<TransportStream, LengthDelimitedCodec>` の cfg 包み（unix: `Framed<UnixStream, _>` / windows: `Framed<NamedPipeClient, _>`）
- 公開メソッド:
  - `pub async fn connect(socket_path: &Path) -> Result<Self, PersistenceError>` — daemon に接続 + ハンドシェイク 1 往復
  - `pub async fn send_request(&mut self, request: &IpcRequest) -> Result<(), PersistenceError>` — リクエストをエンコードして送信
  - `pub async fn recv_response(&mut self) -> Result<IpcResponse, PersistenceError>` — 次フレームを受信してデコード
  - `pub async fn round_trip(&mut self, request: &IpcRequest) -> Result<IpcResponse, PersistenceError>` — `send_request` + `recv_response` を一括（全 IPC 操作の主経路）

### `connect` の処理順序

PR #29 から変更なし:

1. **OS 別接続**:
   - Unix: `tokio::net::UnixStream::connect(socket_path).await` → `std::io::Error` を `PersistenceError::DaemonNotRunning(socket_path)` に写像（`ECONNREFUSED` / `ENOENT` / `EACCES` 等）
   - Windows: `tokio::net::windows::named_pipe::ClientOptions::new().open(pipe_name).await` → 同様
2. **Framing 適用**: `Framed::new(stream, framing::codec())`、`codec()` は `LengthDelimitedCodec::builder().max_frame_length(shikomi_core::ipc::MAX_FRAME_LENGTH).little_endian().length_field_length(4).new_codec()`
3. **ハンドシェイク 1 往復**:
   - `let request = IpcRequest::Handshake { client_version: IpcProtocolVersion::current() };`
   - `let bytes = rmp_serde::to_vec(&request)?;` → `IpcEncode` 失敗時 `PersistenceError::IpcEncode { reason }`
   - `framed.send(bytes.into()).await?;` → `IpcIo` 失敗時 `PersistenceError::IpcIo { reason }`
   - `let resp_bytes = framed.next().await.ok_or(PersistenceError::IpcIo { reason: "connection closed before handshake response" })??;`
   - `let response = rmp_serde::from_slice::<IpcResponse>(&resp_bytes)?;` → `IpcDecode` 失敗時 `PersistenceError::IpcDecode { reason }`
   - `match response`:
     - `IpcResponse::Handshake { server_version }` if `server_version == IpcProtocolVersion::current()` → `Ok(IpcClient { framed })`
     - `IpcResponse::ProtocolVersionMismatch { server, client }` → `Err(PersistenceError::ProtocolVersionMismatch { server, client })`
     - その他 → `Err(PersistenceError::IpcDecode { reason: "unexpected handshake response" })`（ハードコード固定文言、`variant_name` 等の動的文字列化を**しない**。動的情報が必要な場合は `tracing::warn!` で別途 CLI 側ログに出す）

### `send_request` / `recv_response` / `round_trip`

PR #29 から変更なし:

- `send_request`:
  1. `let bytes = rmp_serde::to_vec(request)?;` → `IpcEncode`
  2. `self.framed.send(bytes.into()).await?;` → `IpcIo`
- `recv_response`:
  1. `let resp_bytes = self.framed.next().await.ok_or(PersistenceError::IpcIo { reason: "connection closed unexpectedly" })??;`
  2. `let response = rmp_serde::from_slice::<IpcResponse>(&resp_bytes)?;` → `IpcDecode`
  3. `Ok(response)`
- `round_trip`: `send_request(req).await?` → `recv_response().await?` の組合せ。全 IPC 操作で使用

## `IpcVaultRepository`

**Issue #30 で責務を確定**。`VaultRepository` trait を**実装しない**。

- 配置: `crates/shikomi-cli/src/io/ipc_vault_repository.rs`
- 型: `pub struct IpcVaultRepository`
- フィールド（private）:
  - `client: tokio::sync::Mutex<IpcClient>` — IPC クライアントを `Mutex` で包む（`&self` メソッドから `&mut Framed` を得るため）
  - `runtime: tokio::runtime::Runtime` — CLI の同期コンテキストから非同期 IPC を駆動する current-thread ランタイム（`IpcVaultRepository` のライフタイムと束ねる、`Drop` で解放）
- 公開メソッド（**全て同期**、内部で `runtime.block_on` を呼ぶ）:
  - `pub fn connect(socket_path: &Path) -> Result<Self, PersistenceError>` — daemon に接続 + ハンドシェイク
  - `pub fn default_socket_path() -> Result<PathBuf, PersistenceError>` — OS 既定のソケットパス解決
  - `pub fn list_summaries(&self) -> Result<Vec<RecordSummary>, PersistenceError>` — `IpcRequest::ListRecords` を発行（PR #29 既存）
  - `pub fn add_record(&self, kind: RecordKind, label: RecordLabel, value: SecretString, now: OffsetDateTime) -> Result<RecordId, PersistenceError>` — `IpcRequest::AddRecord` を発行（**Issue #30 で新規**）
  - `pub fn edit_record(&self, id: RecordId, label: Option<RecordLabel>, value: Option<SecretString>, now: OffsetDateTime) -> Result<RecordId, PersistenceError>` — `IpcRequest::EditRecord` を発行（**Issue #30 で新規**）
  - `pub fn remove_record(&self, id: RecordId) -> Result<RecordId, PersistenceError>` — `IpcRequest::RemoveRecord` を発行（**Issue #30 で新規**）

**`load` / `save` / `exists` は実装しない**: `VaultRepository` trait の制約から外れた IPC 専用クライアントとして再定義する。これにより案 C で発生した「嘘の Plaintext(empty)」「嘘 ID」「常時 true な exists()」を**型として持てなくする**。

### `default_socket_path` の解決ルール

PR #29 から変更なし:

- **Linux**: `std::env::var("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("shikomi/daemon.sock")).or_else(|_| ...)` で取得 → `XDG_RUNTIME_DIR` 未設定時は `dirs::runtime_dir()` フォールバック → さらに失敗時は `PersistenceError::CannotResolveVaultDir`
- **macOS**: `dirs::cache_dir().map(|d| d.join("shikomi/daemon.sock"))` → `~/Library/Caches/shikomi/daemon.sock`
- **Windows**: `format!(r"\\.\pipe\shikomi-daemon-{}", crate::io::windows_sid::resolve_self_user_sid()?)`
- 環境変数 / フラグでの上書きは**本 feature では対応しない**（YAGNI、将来 `--ipc-socket <PATH>` フラグ追加余地）

### Windows SID 取得モジュール配置

PR #29 から変更なし:

- 配置: `crates/shikomi-cli/src/io/windows_sid.rs`（`cfg(windows)` 配下のみコンパイル）
- 冒頭: `#![allow(unsafe_code)]` を明示（`../basic-design/security.md §unsafe_code の扱い` の表と整合、CI 監査範囲に登録済み）
- シグネチャ: `pub fn resolve_self_user_sid() -> Result<String, PersistenceError>`
- 内部実装: `GetCurrentProcessToken` → `GetTokenInformation(TokenUser)` → `ConvertSidToStringSidW` を `unsafe` ブロックでラップし、セーフな `String` を返す
- 失敗時: `Err(PersistenceError::Io(std::io::Error::last_os_error()))` 相当

**`default_socket_path` 本体は unsafe を含まない**。`io/ipc_vault_repository.rs` / `io/ipc_client.rs` には `unsafe` を書かない。CI grep（**TC-CI-019 拡張**）で `crates/shikomi-cli/src/` 配下の `unsafe` ブロック出現箇所が `io/windows_sid.rs` のみに限定されることを監査。

### 公開メソッドの実装方針

#### `list_summaries`（PR #29 既存）

1. `let mut client = self.client.blocking_lock();` （`tokio::sync::Mutex::blocking_lock`、current-thread runtime の外から呼ぶため）
2. `let response = self.runtime.block_on(client.round_trip(&IpcRequest::ListRecords))?;`
3. `match response`:
   - `IpcResponse::Records(summaries)` → `Ok(summaries)`
   - `IpcResponse::Error(code)` → `Err(PersistenceError::from(code))`（`IpcErrorCode → PersistenceError` 写像、§エラー写像 参照）
   - その他 → `Err(PersistenceError::IpcDecode { reason: "unexpected response for ListRecords" })`（ハードコード固定文言）

#### `add_record`（Issue #30 新規）

1. `let value_bytes = SerializableSecretBytes::from_secret_string(value);` — `SecretString` → `SerializableSecretBytes`（`shikomi-core::ipc` で提供される変換、`expose_secret` を呼ばない経路、`./protocol-types.md §SerializableSecretBytes` 参照）
2. `let request = IpcRequest::AddRecord { kind, label, value: value_bytes, now };`
3. `let mut client = self.client.blocking_lock();`
4. `let response = self.runtime.block_on(client.round_trip(&request))?;`
5. `match response`:
   - `IpcResponse::Added { id }` → `Ok(id)`
   - `IpcResponse::Error(code)` → `Err(PersistenceError::from(code))`
   - その他 → `Err(PersistenceError::IpcDecode { reason: "unexpected response for AddRecord" })`

**設計理由**: id の真実源は **daemon 側**（`Uuid::now_v7()` を daemon プロセスで生成）。CLI 側は受信した `id` をそのまま presenter に渡し、`render_added(&id, locale)` で表示する。CLI 側で **`Uuid::now_v7()` を呼ばない**ことで「嘘 ID 出荷」（CLI と daemon で異なる id を生成して二重存在）を構造的に排除。

#### `edit_record`（Issue #30 新規）

1. `let value_bytes = value.map(SerializableSecretBytes::from_secret_string);` — `Option<SecretString>` → `Option<SerializableSecretBytes>`
2. `let request = IpcRequest::EditRecord { id: id.clone(), label, value: value_bytes, now };`
3. `let mut client = self.client.blocking_lock();`
4. `let response = self.runtime.block_on(client.round_trip(&request))?;`
5. `match response`:
   - `IpcResponse::Edited { id }` → `Ok(id)`
   - `IpcResponse::Error(IpcErrorCode::NotFound { id })` → `Err(PersistenceError::RecordNotFound(id))`（`PersistenceError` に `RecordNotFound` バリアント追加、§エラー写像 参照）
   - `IpcResponse::Error(code)` → `Err(PersistenceError::from(code))`
   - その他 → `Err(PersistenceError::IpcDecode { reason: "unexpected response for EditRecord" })`

#### `remove_record`（Issue #30 新規）

1. `let request = IpcRequest::RemoveRecord { id: id.clone() };`
2. `let mut client = self.client.blocking_lock();`
3. `let response = self.runtime.block_on(client.round_trip(&request))?;`
4. `match response`:
   - `IpcResponse::Removed { id }` → `Ok(id)`
   - `IpcResponse::Error(IpcErrorCode::NotFound { id })` → `Err(PersistenceError::RecordNotFound(id))`
   - `IpcResponse::Error(code)` → `Err(PersistenceError::from(code))`
   - その他 → `Err(PersistenceError::IpcDecode { reason: "unexpected response for RemoveRecord" })`

### `Cargo.toml` への変更（`shikomi-cli`）

PR #29 で追加済み、Issue #30 で新規追加なし:

- `tokio = { workspace = true, features = ["rt-multi-thread", "net", "io-util", "sync", "time", "macros"] }`
- `tokio-util = { workspace = true, features = ["codec"] }`
- `rmp-serde = { workspace = true }`
- `bytes = { workspace = true }`
- `[target.'cfg(unix)'.dependencies]` の `nix = { workspace = true, features = ["socket"] }`
- `[target.'cfg(windows)'.dependencies]` の `windows-sys = { workspace = true, features = ["Win32_System_Pipes", "Win32_Security", "Win32_Foundation"] }`

## `tokio` ランタイムの扱い

PR #29 の方針を維持し、Issue #30 でも変更なし:

- `shikomi-cli` は CLI 短命プロセス（`shikomi_cli::run() -> ExitCode` は同期関数）
- `--ipc` 経路では IPC 通信が非同期のため、`IpcVaultRepository::connect` 内で `tokio::runtime::Builder::new_current_thread().enable_all().build()?` を構築し、戻り値の `Runtime` を `IpcVaultRepository` のフィールドとして保持する
- 後続の `list_summaries` / `add_record` / `edit_record` / `remove_record` は内部で `self.runtime.block_on(...)` を呼ぶ
- `--ipc` 未指定経路は tokio ランタイム未起動でオーバーヘッドゼロ（CLI 起動時間 p95 維持）

**実装方針**:
- `IpcVaultRepository` の全公開メソッドは同期（`pub fn`）
- `Runtime` フィールドは `IpcVaultRepository` の `Drop` で破棄（接続切断・タスク中断）
- **注意**: `block_on` を `&dyn VaultRepository` の trait メソッド内で呼ぶことは想定しない（`IpcVaultRepository` は trait を実装しないため、二重ランタイム問題が原理的に発生しない）

## `RepositoryHandle` enum（CLI 側 Composition Root）

**Issue #30 で新規導入**。`shikomi-cli::run` が `args.ipc` 値で構築する経路選択値:

- 配置: `crates/shikomi-cli/src/lib.rs`（`run()` の内部、`pub` でない non-public enum）
- 定義（型シグネチャのみ、疑似コード禁止）:
  - `enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }`
  - `#[non_exhaustive]` を付けない（CLI 内部限定 enum、網羅性検査を活用したい）
- 構築: `args.ipc == false` → `RepositoryHandle::Sqlite(SqliteVaultRepository::from_directory(&path)?)` / `args.ipc == true` → `RepositoryHandle::Ipc(IpcVaultRepository::connect(&socket_path)?)`
- ディスパッチ: `run_list` / `run_add` / `run_edit` / `run_remove` が `&RepositoryHandle` を受け、`match` で 2 アーム分岐

**設計判断**: `Box<dyn VaultRepository>` 抽象化を**廃止**する。理由:
- `IpcVaultRepository` が `VaultRepository` trait を実装しないため、`Box<dyn VaultRepository>` で両者を扱えない
- `enum` で明示的に表現することで、`match` の網羅性検査が経路追加時の修正漏れを**コンパイル時に検出**する（dyn 経由ではこれが効かない）
- 将来 Phase 2 移行時に `RepositoryHandle::Sqlite` バリアントを廃止する場合も、enum の修正のみで全ハンドラの match 強制から漏れない

## エラー写像（`IpcErrorCode → PersistenceError`、`PersistenceError → CliError`）

### `IpcErrorCode → PersistenceError`（**Issue #30 で写像表確定、方針 X：Internal 集約**）

`crates/shikomi-infra/src/persistence/error.rs` 内に `From<IpcErrorCode> for PersistenceError` を実装する（orphan rule 回避のため infra 側に配置、`shikomi-cli` から `From` を再利用）:

| 入力 `IpcErrorCode` | 出力 `PersistenceError` |
|--------------------|------------------------|
| `EncryptionUnsupported` | `EncryptionUnsupported`（既存バリアント） |
| `NotFound { id }` | `RecordNotFound(id)`（**Issue #30 で `PersistenceError` に新規追加**） |
| `InvalidLabel { reason }` | **`Internal { reason }`（方針 X：集約）** |
| `Persistence { reason }` | **`Internal { reason }`（方針 X：集約）** |
| `Domain { reason }` | **`Internal { reason }`（方針 X：集約）** |
| `Internal { reason }` | `Internal { reason }`（**Issue #30 で `PersistenceError` に新規追加**） |

**方針 X（Internal 集約戦略）の採用理由**:

旧設計（Issue #30 の初期案）では `PersistenceError` に `InvalidLabel` / `Domain` 等の個別バリアントを追加して6個別写像を実現する案だった。リーダー指示「`PersistenceError` への新規追加は `RecordNotFound(RecordId)` / `Internal { reason: String }` の 2 バリアントに留める」に従い、以下の**方針 X**を採用:

- **infra crate の API 表面拡大を最小化**: `PersistenceError` 新規バリアントは 2 個（`RecordNotFound` / `Internal`）のみ
- **CLI 側 `presenter::error::render_error` で文字列マッチ識別**: daemon が `IpcErrorCode.reason` をハードコード固定文言（`"invalid label"` / `"persistence error"` / `"domain error"` 等の有限集合）で構築する契約のため、CLI 側で `match reason.as_str()` の固定マッチによる `MSG-CLI-101/102/107/108/109` 分岐が安定実装可能
- **`reason` 文字列の動的フォーマット禁止契約は維持**: 比較対象の文字列は daemon 側の固定文言と同一値、`format!` での加工は CLI 側でも一切行わない
- **`RecordNotFound` のみ独立バリアントの理由**: id を `RecordId` 強型で運搬する必要があり、`reason: String` では型情報が落ちる（`MSG-CLI-106` 表示時に id 値が必要）

**`PersistenceError` への新規バリアント追加（最小 2 個）**:
- `RecordNotFound(RecordId)` — IPC 経路の `NotFound { id }` 受信時の写像先
- `Internal { reason: String }` — IPC 経路の `InvalidLabel` / `Persistence` / `Domain` / `Internal` 受信時の写像先（`reason` は固定文言）

詳細整合: test-design `test-design/unit.md §3.9` および basic-design `../basic-design/error.md §IpcErrorCode バリアント詳細` の方針 X 記述と一致。実装は `crates/shikomi-infra/src/persistence/error.rs` の `From<IpcErrorCode>` 実装、PR #32 commit `e41a5a3` の `error.rs:355-372` で確定済み。

### `PersistenceError → CliError`（既存に追加）

`crates/shikomi-cli/src/error.rs`（既存編集）の `From<PersistenceError> for CliError` 拡張:

| 入力 `PersistenceError` | 出力 `CliError` |
|---------------------|----------------|
| `DaemonNotRunning(path)` | `CliError::DaemonNotRunning(path)`（PR #29 で追加済み） |
| `ProtocolVersionMismatch { server, client }` | `CliError::ProtocolVersionMismatch { server, client }`（PR #29 で追加済み） |
| `RecordNotFound(id)` | `CliError::RecordNotFound(id)`（既存、**Issue #30 で IPC 経由のケースを追加写像**） |
| `InvalidLabel(reason)` | `CliError::InvalidLabel(reason)`（既存、**Issue #30 で IPC 経由のケースを追加写像**） |
| `Domain { reason }` | `CliError::Domain(...)`（既存、**Issue #30 で IPC 経由のケースを追加写像**） |
| `IpcDecode { reason }` / `IpcEncode { reason }` / `IpcIo { reason }` | `CliError::Persistence(...)`（既存写像、`PersistenceError` のまま）|
| その他既存バリアント | 既存写像維持 |

`From<&CliError> for ExitCode` の拡張:

| `CliError` バリアント | `ExitCode` |
|------------------|-----------|
| `DaemonNotRunning(_)` | `UserError (1)`（PR #29 で追加済み） |
| `ProtocolVersionMismatch { .. }` | `UserError (1)`（PR #29 で追加済み） |

`RecordNotFound` / `InvalidLabel` / `Domain` の `ExitCode` 写像は既存のまま（IPC 経由でも非 IPC 経由でも同じ `UserError (1)`）。

## `presenter::error::render_error` への追加

PR #29 で `MSG-CLI-110` / `MSG-CLI-111` の写像を追加済み。Issue #30 では**変更なし**（add/edit/remove の IPC 経由エラーは、既存の `MSG-CLI-101 〜 109` を経由して同じ presenter で扱える）。

| `CliError` バリアント | 写像先 MSG-ID | 文面出典 | 追加した PR |
|------------------|--------------|---------|----------|
| `DaemonNotRunning(path)` | `MSG-CLI-110`（3 OS 並記の複数行 hint）| `../basic-design/error.md §MSG-CLI-110 確定文面` | PR #29 |
| `ProtocolVersionMismatch { server, client }` | `MSG-CLI-111` | `../basic-design/error.md §MSG-CLI-111 確定文面` | PR #29 |

**実装注意**:
- `render_error` 内で `{path}` / `{server}` / `{client}` をフォーマット引数として埋め込む。`shikomi-daemon` / `Start-Process -NoNewWindow shikomi-daemon` 等のコマンド文字列は**ハードコード**
- `Locale::English` / `Locale::JapaneseEn` の分岐は既存 `render_error` のパターン踏襲
- `shikomi daemon start` 等の**非実在サブコマンドを hint に絶対に含めない**（ペガサス指摘 ①）

## CLI 側 `run_*` ハンドラとのディスパッチ契約

`crates/shikomi-cli/src/lib.rs` の `run_list` / `run_add` / `run_edit` / `run_remove` が `&RepositoryHandle` を受けて 2 アーム match する責務を持つ。詳細は `../../cli-vault-commands/detailed-design/composition-root.md §RepositoryHandle 経路ディスパッチ` を単一真実源とする。本書では契約のみ:

| サブコマンド | `RepositoryHandle::Sqlite(repo)` 経路 | `RepositoryHandle::Ipc(ipc)` 経路 |
|-----------|-----------------------------------|--------------------------------|
| `list` | `usecase::list::list_records(&repo)?` → `presenter::list::render_list(&views, locale)` | `ipc.list_summaries()?` → `RecordSummary` から `RecordView` 直接構築 → `presenter::list::render_list(&views, locale)` |
| `add` | `usecase::add::add_record(&repo, input, now)?` | `ipc.add_record(input.kind, input.label, input.value, now)?` |
| `edit` | `usecase::edit::edit_record(&repo, input, now)?` | `ipc.edit_record(input.id, input.label, input.value, now)?` |
| `remove` | `usecase::remove::remove_record(&repo, input)?` | `ipc.remove_record(input.id().clone())?` |

**`list` 経路の補足**: `RecordSummary` は `value_preview: Option<String>` / `value_masked: bool` を持ち、`RecordView::from_summary(&summary) -> RecordView` の専用コンストラクタで直接 `RecordView` に射影できる（`shikomi-core::Record` の `expose_secret` 経路を通らない）。これは PR #29 で `run_list_via_ipc` として確立済みの経路。

**入力 DTO の経路**: `AddInput` / `EditInput` / `ConfirmedRemoveInput` は **両経路で同じ** （CLI 側で clap 引数から構築する責務に変化なし）。経路分岐は「DTO を UseCase に渡すか、IPC メソッドに渡すか」のみ。

## 削除する記述（PR #29 + 旧設計の遺物、Issue #30 で完全廃止）

旧設計（案 C / シャドウ差分）に由来する以下の記述・実装方針は**全て廃止**する:

- ❌ `IpcVaultRepository::load(&self) -> Result<Vault, PersistenceError>` の実装方針（`RecordSummary` から `Vault` 集約を再構築する案、`Plaintext(empty)` 注入経路）
- ❌ `IpcVaultRepository::save(&self, vault: &Vault) -> Result<(), PersistenceError>` の差分計算実装（シャドウベースの `add` / `edit` / `remove` 推論）
- ❌ `IpcVaultRepository::exists(&self) -> bool` の常時 `true` 返却
- ❌ シャドウ `Vault` の保持と `updated_at` timestamp 差分検出
- ❌ `Record::new_for_list_view` のような「投影専用 Record」を `shikomi-core` に追加する案（`shikomi-core` ドメイン汚染）
- ❌ `VaultRepository` trait を `LoadVault` / `MutateVault` の 2 trait に分割する案（既存型に大改修、ROI 不足、却下）
- ❌ CLI の `usecase::add::add_record` 等を IPC 経路でも共有する設計（責務違反、daemon 側で完結するドメイン整合を CLI 側で再実装する重複）
- ❌ `runtime_handle: tokio::runtime::Handle` フィールド（PR #29 で導入された旧フィールド、Issue #30 で `runtime: tokio::runtime::Runtime` に変更）

## テスト観点（テスト設計担当向け）

**ユニットテスト**:
- `IpcClient::connect` のハンドシェイク失敗（タイムアウト / 不一致）の各 `PersistenceError` バリアント生成
- `IpcVaultRepository::list_summaries` / `add_record` / `edit_record` / `remove_record` 各メソッドが、対応する `IpcResponse` バリアントから期待する戻り値を生成すること（`tokio::io::duplex` でモック）
- 各メソッドが `IpcResponse::Error(NotFound { id })` を `PersistenceError::RecordNotFound(id)` に写像すること
- `default_socket_path` の OS 別解決（cfg 分岐）
- `From<IpcErrorCode> for PersistenceError` の全 6 バリアント写像
- `From<PersistenceError> for CliError` の追加バリアント写像
- `render_error` の `MSG-CLI-110` / `MSG-CLI-111` 出力（英日 2 locale × 2 = 4 パターン、PR #29 既存）

**結合テスト**:
- `tokio::test` + `tokio::io::duplex` で `IpcClient` を立て、サーバスタブが各 `IpcResponse` を返した時の挙動検証
- `RepositoryHandle::Ipc` を `run_add` に渡し、`IpcRequest::AddRecord` が daemon スタブに到達して `IpcResponse::Added { id }` で応答することの round-trip 検証
- 同様に `edit` / `remove` の round-trip 検証
- `run_list` の `RecordSummary → RecordView` 射影が SQLite 経路と bit 同一の出力になること

**E2E**:
- 実 daemon プロセスを `assert_cmd::Command::cargo_bin("shikomi-daemon").spawn()` で起動
- `shikomi --ipc add --kind text --label L --value V` で daemon 経由 add → 終了コード 0 + stdout `added: <id>`
- `shikomi --ipc edit --id <id> --label L2` で daemon 経由 edit → 終了コード 0 + stdout `updated: <id>`
- `shikomi --ipc remove --id <id> --yes` で daemon 経由 remove → 終了コード 0 + stdout `removed: <id>`
- `--ipc` 指定で daemon 未起動 → 終了コード 1 + `MSG-CLI-110` の stderr 出力（PR #29 既存）
- プロトコル不一致シナリオ（V2 client スタブ → V1 daemon）→ `MSG-CLI-111`（PR #29 既存）
- `--ipc add` で存在しない vault.db → daemon 側で `IpcResponse::Error(Persistence)` → CLI で `MSG-CLI-107` 写像（PR #29 既存の経路）
- `--ipc edit --id <非存在 id>` → daemon `NotFound` → CLI `MSG-CLI-106`
- `--ipc list` で daemon 経由と SQLite 直結 list の bit 同一出力検証（PR #29 既存）

テストケース番号の割当は `test-design/` が担当する。
