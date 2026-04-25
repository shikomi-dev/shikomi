# 詳細設計書 — ipc-vault-repository（CLI 側 IpcVaultRepository / IpcClient）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md -->
<!-- 兄弟: ./index.md, ./protocol-types.md, ./daemon-runtime.md, ./composition-root.md, ./lifecycle.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。シグネチャはインライン `code` で示し、実装本体は書かない。

## モジュール配置

`crates/shikomi-cli/src/io/` 配下に新規ファイルを追加:

```
crates/shikomi-cli/src/io/
  mod.rs                      # 既存、ipc_vault_repository / ipc_client を追加 export
  paths.rs                    # 既存、変更なし
  terminal.rs                 # 既存、変更なし
  ipc_vault_repository.rs     # 新規
  ipc_client.rs               # 新規
```

`crates/shikomi-cli/src/io/mod.rs` への追加:

```
pub mod ipc_vault_repository;
pub mod ipc_client;
```

## `IpcClient`

- 配置: `crates/shikomi-cli/src/io/ipc_client.rs`
- 型: `pub struct IpcClient`
- フィールド（private）:
  - `framed` — `Framed<TransportStream, LengthDelimitedCodec>` の cfg 包み（unix: `Framed<UnixStream, _>` / windows: `Framed<NamedPipeClient, _>`）
- 公開メソッド:
  - `pub async fn connect(socket_path: &Path) -> Result<Self, PersistenceError>` — daemon に接続 + ハンドシェイク 1 往復
  - `pub async fn send_request(&mut self, request: &IpcRequest) -> Result<(), PersistenceError>` — リクエストをエンコードして送信
  - `pub async fn recv_response(&mut self) -> Result<IpcResponse, PersistenceError>` — 次フレームを受信してデコード
  - `pub async fn round_trip(&mut self, request: &IpcRequest) -> Result<IpcResponse, PersistenceError>` — `send_request` + `recv_response` を一括（非同期 vault 操作の主経路）

### `connect` の処理順序

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

### `send_request` / `recv_response`

- `send_request`:
  1. `let bytes = rmp_serde::to_vec(request)?;` → `IpcEncode`
  2. `self.framed.send(bytes.into()).await?;` → `IpcIo`
- `recv_response`:
  1. `let resp_bytes = self.framed.next().await.ok_or(PersistenceError::IpcIo { reason: "connection closed unexpectedly" })??;`
  2. `let response = rmp_serde::from_slice::<IpcResponse>(&resp_bytes)?;` → `IpcDecode`
  3. `Ok(response)`

### `round_trip` ヘルパ

- `send_request(req).await?` → `recv_response().await?` の組合せ
- 設計理由: 90% の使用箇所が「送って受ける」往復のため、ボイラプレート削減。`save` / `add_record` / `edit_record` / `remove_record` / `list_records` 等で全て使用

## `IpcVaultRepository`

- 配置: `crates/shikomi-cli/src/io/ipc_vault_repository.rs`
- 型: `pub struct IpcVaultRepository`
- フィールド（private）:
  - `client: tokio::sync::Mutex<IpcClient>` — IPC クライアントを `Mutex` で包む（`VaultRepository` trait の `&self` メソッドから `&mut self` 経由で `Framed` を更新する必要があるため）
  - `vault_dir: PathBuf` — daemon が管理する vault ディレクトリ（`exists` 等のローカル判定用）
  - `runtime_handle: tokio::runtime::Handle` — 同期 trait method から非同期 IPC を実行する経路
- 公開メソッド:
  - `pub fn connect(socket_path: &Path) -> Result<Self, PersistenceError>` — 同期 wrapper、内部で `Handle::current().block_on(IpcClient::connect(...))` を呼ぶ
  - `pub fn default_socket_path() -> Result<PathBuf, PersistenceError>` — OS 既定のソケットパス解決

### `default_socket_path` の解決ルール

- **Linux**: `std::env::var("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("shikomi/daemon.sock")).or_else(|_| ...)` で取得 → `XDG_RUNTIME_DIR` 未設定時は `dirs::runtime_dir()` フォールバック → さらに失敗時は `PersistenceError::CannotResolveVaultDir`
- **macOS**: `dirs::cache_dir().map(|d| d.join("shikomi/daemon.sock"))` → `~/Library/Caches/shikomi/daemon.sock`
- **Windows**: `format!(r"\\.\pipe\shikomi-daemon-{}", crate::io::windows_sid::resolve_self_user_sid()?)`（詳細は下記 §Windows SID 取得モジュール配置）
- 環境変数 / フラグでの上書きは**本 feature では対応しない**（YAGNI、将来 `--ipc-socket <PATH>` フラグ追加余地）

### Windows SID 取得モジュール配置（CLI 側、服部平次指摘 ③への対応）

- 配置: `crates/shikomi-cli/src/io/windows_sid.rs`（`cfg(windows)` 配下のみコンパイル、新規作成）
- 冒頭: `#![allow(unsafe_code)]` を明示（`../basic-design/security.md §unsafe_code の扱い` の表と整合、CI 監査範囲に登録）
- シグネチャ: `pub fn resolve_self_user_sid() -> Result<String, PersistenceError>`
- 内部実装: `GetCurrentProcessToken` → `GetTokenInformation(TokenUser)` → `ConvertSidToStringSidW` を `unsafe` ブロックでラップし、セーフな `String` を返す（daemon 側 `permission::windows::resolve_self_user_sid` と**同等機能を独立実装**、crate 境界を尊重）
- 失敗時: `Err(PersistenceError::Io(std::io::Error::last_os_error()))` 相当

**`default_socket_path` 本体は unsafe を含まない**:
- CLI の `crates/shikomi-cli/src/io/ipc_vault_repository.rs::default_socket_path` は `crate::io::windows_sid::resolve_self_user_sid()?` を呼ぶのみ
- `io/ipc_vault_repository.rs` / `io/ipc_client.rs` には `unsafe` を書かない（**`io/windows_sid.rs` のみ**が CLI 側 unsafe 許可領域）
- CI grep（**TC-CI-019 拡張**）で `crates/shikomi-cli/src/` 配下の `unsafe` ブロック出現箇所が `io/windows_sid.rs` のみに限定されることを監査

### `VaultRepository` trait の実装

`shikomi-infra::persistence::repository::VaultRepository` trait の以下のメソッドを実装:

- `fn load(&self) -> Result<Vault, PersistenceError>`
- `fn save(&self, vault: &Vault) -> Result<(), PersistenceError>`
- `fn exists(&self) -> bool`

#### `load` の実装方針（重要、`./index.md §設計判断 8` の最終採用案 = 案 C）

`load` は `Result<Vault, PersistenceError>` を返す既存契約だが、IPC 経路では完全な `Vault` 集約を取得しない（`RecordSummary` のみ）。本 feature の採用案:

1. `IpcRequest::ListRecords` を送信 → `IpcResponse::Records(summaries)` 受信
2. **`Vault` を再構築**するため、`RecordSummary` から `Record` を**Secret 値非含有な投影 Record** で構築:
   - `Record::new_for_list_view(id, kind, label, value_preview_or_masked, /* synthetic timestamps */)` のような **`shikomi-core::Record` の追加コンストラクタ**を本 feature で導入する案
   - **問題点**: `shikomi-core::Record` に「投影専用 Record」を追加すると永続化経路と混在、ドメイン汚染
3. **代替案（採用）**: `IpcVaultRepository` が `VaultRepository` を**部分実装**し、`load` は List 結果を `Vault` 風の集約に詰めるが**`save` は受け取った `Vault` の差分操作を IPC で送る**

**最終採用案の具体仕様**:

- `load` は `IpcRequest::ListRecords` 送信 → `RecordSummary` を**ローカルキャッシュ**として保持し、`Record` 集約として再構築する際は **payload を `RecordPayload::Plaintext(SecretString::from_str(""))` で擬似構築**（Secret 値は一切 IPC 経由で得ない）
- ただし `Record` の `payload` をダミー値で構築すると、**`save` 時にダミー値が daemon に送られてレコードが上書きされるリスク**がある
- これを防ぐため、`IpcVaultRepository` は `Vault` を「**read-only シャドウ**」として保持し、`save` で受け取った新 `Vault` と保持シャドウの**差分**を計算して IPC 操作（`AddRecord` / `EditRecord` / `RemoveRecord`）に変換する
- 差分計算ロジック:
  - 新 `Vault` にあって旧シャドウになければ → `IpcRequest::AddRecord { kind, label, value, now }` を発行
  - 旧シャドウにあって新 `Vault` になければ → `IpcRequest::RemoveRecord { id }` を発行
  - 両者にあって `label` / `payload` が変化していれば → `IpcRequest::EditRecord { id, label, value, now }` を発行
  - **value は新 `Vault` 側の `RecordPayload::Plaintext(SecretString)` から `SerializableSecretBytes::from_secret_string(secret)` で抽出**（CLI 側 UseCase が新規作成 / 編集した secret は実値を持つため可能）
- **既存レコードの value 変更検出**: シャドウは `RecordSummary` ベースで実値を持たない。新 `Vault` の `value` が変化したかは `RecordKind::Secret` の場合**判定不可**（シャドウが Masked のため）。**保守的に「Secret kind の `EditRecord` は label 変更時のみシャドウとの差分検出可能、value 変更は常に IPC EditRecord を発行」する方針**

**この案の問題点と回避**:

- 問題: CLI の UseCase は「`load()` → `vault.update_record(...)` → `save(&vault)`」のパターンで動く。シャドウベースの load では Secret 値が Masked のため、`update_record` の差分計算で「Secret value に変更なし」を誤判定してしまう
- **回避策**: `IpcVaultRepository::save` の差分計算で、**`update_record` の closure 内で `with_updated_payload` が呼ばれたかどうかを `Record` の `updated_at` タイムスタンプ差で検出**。新 `Vault.records()[i].updated_at != shadow.records()[i].updated_at` なら EditRecord 発行
- これは堅牢ではない（同 timestamp で payload 変更があれば検出漏れ）が、UseCase が `now: OffsetDateTime` を引数で受け、毎回新 timestamp を渡す既存設計（`cli-vault-commands`）のため、**実用上は問題ない**

**本案の最終位置づけ**: 詳細設計レベルで案 C は妥当な妥協点。**実装担当（坂田銀時）には実装中に問題が顕在化したら、`VaultRepository` trait を分割する案 B（`LoadVault` / `MutateVault` の 2 trait）への移行 PR を別途起こす権限を与える**（`./future-extensions.md §実装担当への引き継ぎ`）。

#### `save` の実装方針

1. `self.client.lock().await` で `IpcClient` ロック取得
2. シャドウ `Vault` と入力 `Vault` の差分計算:
   - 削除レコード: シャドウ `\ 入力` の id 集合 → 各 id に `IpcRequest::RemoveRecord` 発行
   - 追加レコード: 入力 `\ シャドウ` の id 集合 → 各 record に `IpcRequest::AddRecord` 発行
   - 更新レコード: 両者の id 集合に対し `updated_at` 差分検出 → `IpcRequest::EditRecord` 発行
3. 各 IPC 操作で `IpcResponse::Error(...)` を受信した場合、即時 `Err(PersistenceError::Persistence/Domain { reason })` で返す（部分書込状態の検出はクライアント側では困難、daemon 側 atomic write 保証に依存）
4. 全操作成功 → シャドウを入力 `Vault` で更新（次回 load を待たずに反映）→ `Ok(())`

#### `exists` の実装方針

- daemon 接続が成功している = vault が存在する（daemon が起動時に `repo.load()` を実行済みのため）
- IPC 越しの `exists` 確認は不要、常に `true` を返す
- 例外: 起動時の vault 未作成は daemon 側で `add` が初回時に作成する設計だが、本 feature では daemon は既存 vault のみを扱う（daemon 側で `repo.load()` が成功した状態が前提）。CLI からの `--ipc add` で初回 vault 作成は **scope-out**（後続 Issue で daemon 側に「vault 未作成時の自動作成」IPC 操作を追加検討）。本 feature では `--ipc add` を実行しても daemon 側で `IpcResponse::Error(Persistence)` が返り、CLI が終了コード 2 で終わる挙動とする

## `Cargo.toml` への変更（`shikomi-cli`）

`crates/shikomi-cli/Cargo.toml` の `[dependencies]` に以下を追加:

- `tokio = { workspace = true, features = ["rt-multi-thread", "net", "io-util", "sync", "time", "macros"] }`
- `tokio-util = { workspace = true, features = ["codec"] }`
- `rmp-serde = { workspace = true }`
- `bytes = { workspace = true }`
- `[target.'cfg(unix)'.dependencies]` の `nix = { workspace = true, features = ["socket"] }`（既存に追加 or 新規）
- `[target.'cfg(windows)'.dependencies]` の `windows-sys = { workspace = true, features = ["Win32_System_Pipes", "Win32_Security", "Win32_Foundation"] }`

## `tokio` ランタイムの扱い

- `shikomi-cli` は CLI 短命プロセス（`cli-vault-commands` の既存 `run() -> ExitCode` は同期関数）
- `--ipc` 経路では IPC 通信が非同期のため、`run()` 内で `tokio::runtime::Runtime::new()?.block_on(async { ... })` でラップする経路が必要
- **設計判断**: `shikomi_cli::run()` のシグネチャは既存通り `pub fn run() -> ExitCode`（同期）を維持し、内部の `--ipc` 分岐で `tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(IpcVaultRepository::connect(...))` を呼ぶ
- これにより `--ipc` 未指定の経路は tokio ランタイム起動コストを払わない（CLI 起動時間 p95 維持）

**実装方針**:
- `IpcVaultRepository::connect` は同期関数として公開（内部で `block_on`）
- `IpcVaultRepository::load` / `save` も同期 trait メソッド（内部で `block_on`）
- `runtime_handle` フィールドは `IpcVaultRepository` 構築時に取得し、後続の `load` / `save` で使い回す
- **注意**: `block_on` を `&dyn VaultRepository` の trait メソッド内で呼ぶと、外側に既に tokio ランタイムが存在する場合に panic する（`block_on` の二重起動禁止）。CLI の `run()` は同期関数なので外側ランタイムは存在しない、安全

## エラー写像（`PersistenceError` → `CliError`）

`crates/shikomi-cli/src/error.rs`（既存編集）に以下のバリアントを追加:

```
pub enum CliError {
    // 既存バリアント...
    DaemonNotRunning(PathBuf),
    ProtocolVersionMismatch { server: IpcProtocolVersion, client: IpcProtocolVersion },
    // ...
}
```

`From<PersistenceError> for CliError` の既存実装を以下に拡張:

| 入力 `PersistenceError` | 出力 `CliError` |
|---------------------|----------------|
| `DaemonNotRunning(path)` | `CliError::DaemonNotRunning(path)` |
| `ProtocolVersionMismatch { server, client }` | `CliError::ProtocolVersionMismatch { server, client }` |
| `IpcDecode { reason }` / `IpcEncode { reason }` / `IpcIo { reason }` | `CliError::Persistence(...)`（既存写像、`PersistenceError` のまま）|
| その他既存バリアント | 既存写像維持 |

`From<&CliError> for ExitCode` の拡張:

| `CliError` バリアント | `ExitCode` |
|------------------|-----------|
| `DaemonNotRunning(_)` | `UserError (1)` |
| `ProtocolVersionMismatch { .. }` | `UserError (1)` |

## `presenter::error::render_error` への追加

`crates/shikomi-cli/src/presenter/error.rs`（既存編集）に `MSG-CLI-110` / `MSG-CLI-111` の写像を追加する。**出力本文の確定文面は `../basic-design/error.md §MSG-CLI-110 確定文面` / `§MSG-CLI-111 確定文面` を単一真実源として参照**し、本書では責務分担のみ記述:

| `CliError` バリアント | 写像先 MSG-ID | 文面出典 |
|------------------|--------------|---------|
| `DaemonNotRunning(path)` | `MSG-CLI-110`（3 OS 並記の複数行 hint）| `../basic-design/error.md §MSG-CLI-110 確定文面` |
| `ProtocolVersionMismatch { server, client }` | `MSG-CLI-111` | `../basic-design/error.md §MSG-CLI-111 確定文面` |

**実装注意**:
- `render_error` 内で `{path}` / `{server}` / `{client}` をフォーマット引数として埋め込む。`shikomi-daemon` / `Start-Process -NoNewWindow shikomi-daemon` 等のコマンド文字列は**ハードコード**（ユーザに case-by-case で変化させない固定案内）
- `Locale::English` / `Locale::JapaneseEn` の分岐は既存 `render_error` のパターン踏襲
- `shikomi daemon start` 等の**非実在サブコマンドを hint に絶対に含めない**（ペガサス指摘 ①）

## テスト観点（テスト設計担当向け）

**ユニットテスト**:
- `IpcClient::connect` のハンドシェイク失敗（タイムアウト / 不一致）の各 `PersistenceError` バリアント生成
- `IpcVaultRepository::load` のシャドウキャッシュ更新ロジック
- `IpcVaultRepository::save` の差分計算（追加 / 削除 / 更新の組合せ）
- `default_socket_path` の OS 別解決（cfg 分岐）
- `From<PersistenceError> for CliError` の追加バリアント写像
- `render_error` の `MSG-CLI-110` / `MSG-CLI-111` 出力（英日 2 locale × 2 = 4 パターン）

**結合テスト**:
- `tokio::test` + `tokio::io::duplex` で `IpcClient` を立て、サーバスタブが各 `IpcResponse` を返した時の挙動検証
- `IpcVaultRepository` のシャドウベース `save` で「実 daemon と同じ差分操作が発行される」ことの検証（モックサーバで検収）

**E2E**:
- 実 daemon プロセスを `assert_cmd::Command::cargo_bin("shikomi-daemon").spawn()` で起動
- `assert_cmd::Command::cargo_bin("shikomi").args(&["--ipc", "list"])` で CLI 実行 → daemon 経由の出力が SQLite 直結版と bit 同一
- `--ipc` 指定で daemon 未起動 → 終了コード 1 + `MSG-CLI-110` の stderr 出力
- プロトコル不一致シナリオ（V2 client スタブ → V1 daemon）→ `MSG-CLI-111`

テストケース番号の割当は `test-design/` が担当する。
