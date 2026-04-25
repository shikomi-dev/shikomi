# テスト設計書 — daemon-ipc / ユニットテスト

> `index.md` の §2 索引からの分割ファイル。pure function のユニットテストと実装担当への引き継ぎ事項を扱う。

## 1. 設計方針

- **対象**: public / crate-internal の pure function および I/O 抽象化済み関数。具体的には
  - `shikomi-core::ipc` 全型の serde round-trip（`rmp-serde` は daemon/cli 側の integration test に置くため、本節では serde の `Serialize`/`Deserialize` 導出のみ検証、JSON 簡易チェックも含む）
  - `RecordSummary::from_record` の pure 写像
  - `SerializableSecretBytes` の `Debug` / `Serialize` / `Deserialize` 実装（`expose_secret` 不使用）
  - `handle_request(repo, vault, req)` の pure 写像（モック `VaultRepository` + 固定 `Vault`）
  - `peer_credential::verify_*` の trait 経由モック判定
  - `From<PersistenceError> for CliError`（`DaemonNotRunning` / `ProtocolVersionMismatch` の追加写像）
  - `From<&CliError> for ExitCode`（新 2 バリアントの写像）
  - `presenter::error::render_error` の `MSG-CLI-110` / `MSG-CLI-111` 英日 4 パターン
  - `io::ipc_vault_repository::default_socket_path` の OS cfg 分岐
  - **[Phase 1.5 追加]** `IpcVaultRepository::add_record` / `edit_record` / `remove_record` の専用メソッド単体契約（`tokio::io::duplex` スタブ経由、§2.12）
  - **[Phase 1.5 追加]** `From<IpcErrorCode> for PersistenceError` の 6 バリアント完全写像（§2.13）
  - **[Phase 1.5 追加]** `RepositoryHandle` enum dispatch の網羅性検査（§2.14）
  - **[Phase 1.5 追加]** 嘘 ID 不在の契約検証（§2.15、案 D の核）
  - **[Phase 1.5 追加]** `IpcVaultRepository` が `VaultRepository` trait を実装しない型契約（§2.16、`compile_fail` doctest）
- **粒度**: 1 テスト 1 アサーション。命名 `test_<対象>_<状況>_<期待>`
- **モック**: I/O バウンダリのみ。`PeerCredentialSource` trait（後述）でピア取得を抽象化し、in-test 実装で固定値を返す。**Phase 1.5 の専用メソッド UT では `tokio::io::duplex` でクライアント-サーバ stream を作り、サーバ側を**手書きスタブ（受信記録 + 固定応答）**として注入する**（factory ではなく構造体ベース）
- **配置**: Rust 慣習、`#[cfg(test)] mod tests` でソースモジュール内
- **MessagePack round-trip**: `shikomi-core` に `rmp-serde` を dev-dep として追加**しない**（純粋性保持、詳細設計 `../detailed-design/index.md §設計判断 12` 採用案 B）。MessagePack 経由の round-trip は **integration test に移す**（`integration.md §IPC ラウンドトリップ`）

---

## 2. テストケース一覧

### 2.1 `shikomi_core::ipc::version::IpcProtocolVersion` — serde 文字列表現

配置: `crates/shikomi-core/src/ipc/version.rs` の `#[cfg(test)] mod tests`。`serde_json` を使った軽量 round-trip（`shikomi-core` の既存 dev-deps の範囲）。

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-001 | 正常 | `IpcProtocolVersion::V1` → `serde_json::to_string` | `"\"v1\""`（`rename_all = "snake_case"`） |
| TC-UT-002 | 正常 | `"\"v1\""` → `serde_json::from_str::<IpcProtocolVersion>` | `Ok(IpcProtocolVersion::V1)` |
| TC-UT-003 | 異常 | `"\"v99\""` → `serde_json::from_str` | `Err(_)`（未知バリアント、`#[non_exhaustive]` で外部追加はできないが既存への誤値は reject） |
| TC-UT-004 | 契約 | `IpcProtocolVersion::V1.current_version_string()` 等の`const` or 関数（**実装で提供される想定**） | `"v1"` を返す |

### 2.2 `shikomi_core::ipc::request::IpcRequest` / `response::IpcResponse` — serde 構造

| TC-ID | 種別 | 対象バリアント | 期待 |
|-------|------|-------------|------|
| TC-UT-005 | 正常 | `IpcRequest::Handshake { client_version: V1 }` → JSON round-trip | 元値と一致 |
| TC-UT-006 | 正常 | `IpcRequest::ListRecords` → JSON | unit variant の snake_case 表現 |
| TC-UT-007 | 正常 | `IpcRequest::AddRecord { kind, label, value, now }` → JSON round-trip | 元値と一致、`value`（`SerializableSecretBytes`）のバイト列が復元される |
| TC-UT-008 | 正常 | `IpcResponse::Records(vec![RecordSummary{...}])` → JSON round-trip | 元値と一致 |
| TC-UT-009 | 正常 | `IpcResponse::Error(IpcErrorCode::NotFound { id })` → JSON round-trip | id が維持、`reason` フィールド無し |
| TC-UT-010 | 異常（プロトコル） | `IpcResponse::ProtocolVersionMismatch { server: V1, client: V1 }` は意味論的には「一致しているのに mismatch 応答」だが**型レベルでは可能** | JSON round-trip 成功、ハンドラ側で論理矛盾検出は別層（本 UT は型検証のみ） |

### 2.3 `shikomi_core::ipc::summary::RecordSummary::from_record`

**pure 写像の責務**: `shikomi_core::Record` から `value_preview: Option<String>` / `value_masked: bool` を **`Record::text_preview(40)` と `record.kind() == Secret`** から導出。`expose_secret` を呼ばない（`text_preview` が既に `cli-vault-commands` で secret 非含有を保証済み）。

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-011 | 正常（Text 短） | Text, value="hello", label="L" | `RecordSummary { value_preview: Some("hello"), value_masked: false, ... }` |
| TC-UT-012 | 境界（Text 41 char） | Text, value="a"×41 | `value_preview: Some("a"×40)`（`text_preview(40)` の truncate 経路） |
| TC-UT-013 | セキュリティ（Secret） | Secret, value="SECRET_TEST_VALUE" | `value_preview: None, value_masked: true`（Secret kind では `text_preview` が `None`） |
| TC-UT-014 | 境界（Text 0 char） | Text, value="" | `value_preview: Some(""), value_masked: false` |

### 2.4 `shikomi_core::ipc::secret_bytes::SerializableSecretBytes`

| TC-ID | 種別 | 対象 | 期待結果 |
|-------|------|------|---------|
| TC-UT-015 | セキュリティ | `SerializableSecretBytes(SecretBytes::from_vec(b"topsecret".to_vec()))` → `format!("{:?}", ...)` | `"topsecret"` を**含まない** / `[REDACTED`（`:SerializableSecretBytes` 付き or なし、実装規約次第）を含む |
| TC-UT-016 | セキュリティ | 同上 → `format!("{:?}", IpcRequest::AddRecord { ..., value: ..., ...})` | `"topsecret"` を**含まない** |
| TC-UT-017 | 正常 | JSON round-trip（`serde_json`）for `SerializableSecretBytes(vec![0x01, 0x02, 0x03])` | MessagePack と異なり JSON は bytes を array として表現するが、**バイト列の復元**が成立 |
| TC-UT-018 | 契約（grep） | `crates/shikomi-core/src/ipc/secret_bytes.rs` 内で `expose_secret` が**呼ばれない** | `ci.md TC-CI-015` で静的監査、本 UT は補強用の文字列検査（`include_str!("secret_bytes.rs")` + `.contains("expose_secret")` が `false`）|

### 2.5 `shikomi_daemon::ipc::handler::handle_request`（pure 写像）

**pure 化の責務**: `&dyn VaultRepository` と `&mut Vault` を引数で受け、`IpcRequest` → `IpcResponse` を返す I/O なし関数。本 UT は **in-memory モック `VaultRepository`**（既存 `shikomi-infra` の `test-fixtures` or テスト専用 `FakeRepo`）と**固定 `Vault`** で網羅する。

配置: `crates/shikomi-daemon/src/ipc/handler.rs` の `#[cfg(test)] mod tests`。

| TC-ID | 種別 | 入力 | 前提 vault 状態 | 期待 `IpcResponse` |
|-------|------|------|--------------|---------------|
| TC-UT-030 | 正常（List 0 件） | `IpcRequest::ListRecords` | 空 Vault | `Records(vec![])` |
| TC-UT-031 | 正常（List 複数） | 同上 | Text 1, Secret 1 | `Records(vec![RecordSummary{value_preview: Some,...}, RecordSummary{value_preview: None, value_masked: true}])`（順序は `Vault::records()` の挙動に従う） |
| TC-UT-032 | 正常（Add Text） | `AddRecord { kind: Text, label: "L", value: SerializableSecretBytes("v".bytes()), now: T }` | 空 | `Added { id }` + repo.save 経由で vault に 1 件追加（mock repo が save 呼出を記録） |
| TC-UT-033 | 異常（Add 重複ラベル） | 同上 | 既存に同 label が `vault.add_record` で reject される前提 | `Error(IpcErrorCode::Domain { reason })`（reason は**固定文言**「服部 review 後改訂」、secret / 絶対パス / PID 非含有） |
| TC-UT-034 | 異常（Edit NotFound） | `EditRecord { id: 存在しないUUID, ... }` | 空 | `Error(IpcErrorCode::NotFound { id })` |
| TC-UT-035 | 正常（Edit label のみ） | `EditRecord { id: 既存id, label: Some("NEW"), value: None, now: T }` | Text 1 件 | `Edited { id }` + mock repo.save が呼ばれる |
| TC-UT-036 | 異常（Remove NotFound） | `RemoveRecord { id: 存在しないUUID }` | 空 | `Error(IpcErrorCode::NotFound { id })` |
| TC-UT-037 | 正常（Remove） | `RemoveRecord { id: 既存id }` | Text 1 件 | `Removed { id }` + mock repo.save が呼ばれる |
| TC-UT-038 | 異常（Persistence） | `AddRecord { ... }`、mock repo.save が `PersistenceError::Io(_)` を返すよう設定 | 空 | `Error(IpcErrorCode::Persistence { reason })`（reason が `Io` の `Display` 文字列、secret 非含有） |
| TC-UT-039 | 防御的（EncryptionUnsupported） | 任意の vault 操作、vault が `ProtectionMode::Encrypted` | 暗号化 vault | `Error(IpcErrorCode::EncryptionUnsupported)`（daemon 起動時に Fail Fast するが、ハンドラ防御的再チェックで到達可能なら経由） |

**注記**: TC-UT-032 / 035 / 037 で **mock repo の `save` 呼出回数**を検証（`AtomicUsize` カウンタ or `RefCell<Vec<Vault>>` に保存される引数を記録）。ハンドラが `save` を忘れれば fail。

### 2.6 `shikomi_daemon::permission::peer_credential::verify_*` — trait 経由モック

**設計**: `PeerCredentialSource` trait を `peer_credential/mod.rs` に定義し、`verify_unix(stream)` / `verify_windows(stream)` がこの trait を介して UID / SID を取得する。ユニットテストでは **`TestPeerCredential { peer: uid_or_sid, self_uid: uid }` を trait 実装**として与え、比較ロジックのみを検証する。

配置: `crates/shikomi-daemon/src/permission/peer_credential/unix.rs` と `windows.rs` の `#[cfg(test)] mod tests`。

| TC-ID | 種別 | 入力（trait mock） | 期待結果 |
|-------|------|----------------|---------|
| TC-UT-020 | 正常（一致） | `TestPeerCredential { peer: 1000, self: 1000 }` | `Ok(())`（多層防御下で接続継続） |
| TC-UT-021 | 異常（不一致） | `TestPeerCredential { peer: 2000, self: 1000 }` | `Err(PeerVerificationError::UidMismatch { expected: 1000, got: 2000 })` |
| TC-UT-022 | 異常（syscall 失敗） | `TestPeerCredential::fail_with(nix::Error::Sys(EINVAL))` | `Err(PeerVerificationError::SyscallFailed(_))`（接続切断経路、`server.rs` 側で `drop(stream)`） |
| TC-UT-023 | Windows 版 正常 | `TestPeerCredential { peer_sid: "S-1-5-21-...", self_sid: 同 }` | `Ok(())` |
| TC-UT-024 | Windows 版 異常 | 異なる SID | `Err(PeerVerificationError::SidMismatch)` |

**引き継ぎ事項（断定）**: 実装担当（坂田銀時）は **`pub(crate) trait PeerCredentialSource` を `crates/shikomi-daemon/src/permission/peer_credential/mod.rs` に定義し、`verify_unix` / `verify_windows` がこの trait を介して UID / SID を取得する** 形に実装する（§3.1 で仕様確定）。本番コードに `#[cfg(test)]` バイパス分岐を入れない——trait 注入一本化で UT・IT ともに差替可能（integration.md §8.1「src/ 配下の `#[cfg(test)]` は `mod tests` の UT のみ」と完全整合）。

### 2.7 `shikomi_cli::error` — `From<PersistenceError> for CliError` 追加写像

配置: `crates/shikomi-cli/src/error.rs` の `#[cfg(test)] mod tests`。

**採番注記**: §2.5 handler pure 写像（TC-UT-030〜039）との重複を避けるため本節は **TC-UT-090 系**で採番（ペテルギウス review 指摘 ① 対応、2026-04-25 renumber）。

| TC-ID | 入力 `PersistenceError` | 期待 `CliError` |
|-------|---------------------|----------------|
| TC-UT-090 | `DaemonNotRunning(PathBuf::from("/tmp/daemon.sock"))` | `CliError::DaemonNotRunning(same_path)` |
| TC-UT-091 | `ProtocolVersionMismatch { server: V1, client: V1 }`（同値でも型として可能） | `CliError::ProtocolVersionMismatch { server: V1, client: V1 }` |
| TC-UT-092 | `IpcDecode { reason: "bad format".into() }` | `CliError::Persistence(_)`（既存 `From` 経由、reason 保持） |
| TC-UT-093 | `IpcEncode { reason: "encode failed".into() }` | `CliError::Persistence(_)`（同上） |
| TC-UT-094 | `IpcIo { reason: "eof".into() }` | `CliError::Persistence(_)`（同上） |

### 2.8 `shikomi_cli::error::ExitCode::from(&CliError)` — 追加バリアント写像

| TC-ID | 入力 | 期待 `ExitCode` |
|-------|------|--------------|
| TC-UT-040 | `CliError::DaemonNotRunning(_)` | `ExitCode::UserError (1)` |
| TC-UT-041 | `CliError::ProtocolVersionMismatch { .. }` | `ExitCode::UserError (1)` |

既存 TC-UT-001〜009（cli-vault-commands）は本 feature で**破壊しない**ことを `cargo test` 全体で再確認（**本テスト設計での追加責務ではなく、既存テスト維持の確認**）。

### 2.9 `shikomi_cli::presenter::error::render_error` — MSG-CLI-110 / 111 英日

配置: `crates/shikomi-cli/src/presenter/error.rs` の `#[cfg(test)] mod tests`。

| TC-ID | 入力 | Locale | 期待 stderr 文字列（部分一致） |
|-------|------|-------|---------------------------|
| TC-UT-070 | `DaemonNotRunning("/tmp/foo.sock".into())` | English | `error: shikomi-daemon is not running (socket /tmp/foo.sock unreachable)` + `hint: start the daemon` |
| TC-UT-071 | 同上 | JapaneseEn | 英語行 + `error: shikomi-daemon が起動していません（ソケット /tmp/foo.sock に接続できません）` + 日本語 hint |
| TC-UT-072 | `ProtocolVersionMismatch { server: V1, client: V1 }`（server=client 同値でも可） | English | `error: protocol version mismatch (server=v1, client=v1)` + `hint: rebuild shikomi-cli and shikomi-daemon` |
| TC-UT-073 | 同上 | JapaneseEn | 英語行 + 日本語行（`error: プロトコルバージョン不一致...`） |

**横串アサート**: 全 4 パターンで `secret` / `expose_secret` / 絶対パス（`/home/...`）/ UID 数値が出現しないこと（`predicates::str::contains("/home/").not()` は本ケースの対象ファイルに特化、sock path は `/tmp/` で安全）。

### 2.10 `shikomi_cli::io::ipc_vault_repository::default_socket_path` — OS cfg 分岐

| TC-ID | OS cfg | 前提 | 期待 |
|-------|-------|------|------|
| TC-UT-050 | `cfg(target_os = "linux")` | `XDG_RUNTIME_DIR=/run/user/1000` 環境変数 | `Ok(PathBuf::from("/run/user/1000/shikomi/daemon.sock"))` |
| TC-UT-051 | 同上 | `XDG_RUNTIME_DIR` 未設定、`dirs::runtime_dir()` が `Some(/run/user/1000)` | `Ok(/run/user/1000/shikomi/daemon.sock)` |
| TC-UT-052 | 同上 | `XDG_RUNTIME_DIR` 未設定、`dirs::runtime_dir()` が `None` | `Err(PersistenceError::CannotResolveVaultDir)` 相当 |
| TC-UT-053 | `cfg(target_os = "macos")` | `dirs::cache_dir()` が `Some(~/Library/Caches)` | `Ok(~/Library/Caches/shikomi/daemon.sock)` |
| TC-UT-054 | `cfg(target_os = "windows")` | ユーザ SID が取得可能な mock | `Ok(PathBuf::from(r"\\.\pipe\shikomi-daemon-S-1-5-21-..."))` |

**注記**: TC-UT-050 / 053 は env 操作が必要なため `serial_test` 経由（`#[serial]`）または **pure 切り出し**（`default_socket_path_from(xdg: Option<&str>, dirs_runtime: Option<PathBuf>) -> Result<PathBuf, _>`）を実装担当に推奨（§3 引き継ぎ 参照）。

### 2.11 [DEPRECATED — Phase 1.5 案 D で構造的に発生不能] `IpcVaultRepository::save` 差分計算ロジック

**🚫 廃止判定（Issue #30、2026-04-25）**: 旧 TC-UT-060〜064 は**案 C（シャドウ差分）前提**だった。Issue #30 で **`IpcVaultRepository` が `VaultRepository` trait を実装しない**（案 D）に確定したため、`save(&Vault)` メソッド自体が**存在しない**。`compute_diff` 関数も実装されない。よって本節の TC は**全廃**する。

| 旧 TC-ID | 旧シナリオ | 廃止理由 | 後継 |
|---------|----------|---------|------|
| 旧 TC-UT-060 | 追加のみ差分発行 | `save` 不在、差分推論経路が型として消滅 | **TC-UT-100〜102b（`add_record` 専用メソッド round-trip、§2.12）** |
| 旧 TC-UT-061 | 削除のみ差分発行 | 同上 | **TC-UT-106〜108（`remove_record` 専用メソッド）** |
| 旧 TC-UT-062 | label 更新差分 | 同上 | **TC-UT-103〜105b（`edit_record` 専用メソッド）** |
| 旧 TC-UT-063 | 変更なし → 空発行 | 同上、Tell-Don't-Ask 原則違反のため発生不能 | — |
| 旧 TC-UT-064 | 全差異 → 3 発行 | 同上 | TC-UT-100〜108 の集合 |

**構造契約**: §2.16 に `IpcVaultRepository` が **`VaultRepository` trait を実装しない** ことを型レベルで担保する **TC-UT-119**（`compile_fail` doctest）を新設。CI grep（`ci.md` TC-CI-030）でも `impl VaultRepository for IpcVaultRepository` 出現 0 件を強制。

### 2.12 [Phase 1.5 新設] `IpcVaultRepository::add_record` / `edit_record` / `remove_record` — 専用メソッド round-trip

**配置**: `crates/shikomi-cli/src/io/ipc_vault_repository.rs` の `#[cfg(test)] mod tests`、`tokio::io::duplex(64 * 1024)` で client/server stream を作り、サーバ側はテストスタブ（受信フレームを `Arc<Mutex<Vec<IpcRequest>>>` に記録 + 固定応答返送）。

**設計理由**: 案 D で `IpcVaultRepository` の専用メソッドが直接 IPC を発行する構造になったため、本来 `integration.md` の責務に近いが、**個別メソッド契約の網羅**は UT 粒度で押さえる（in-memory duplex で並列実行可、E2E より高速）。境界は「ハンドシェイク含む完全 round-trip」は IT、「メソッド単体の入出力契約」は UT に切る。

#### `add_record`

| TC-ID | 種別 | 入力（呼出引数） | スタブ応答 | 期待戻り値 / スタブ受信 |
|-------|------|-------------|---------|--------------------|
| TC-UT-100 | 正常 | `kind: Text, label: "L", value: SecretString::from_str("v"), now: T` | `IpcResponse::Added { id: <fixed_uuid> }` | `Ok(<fixed_uuid>)`、スタブ受信 = `IpcRequest::AddRecord { kind: Text, label: "L", value: SerializableSecretBytes(b"v"), now: T }`（**`SerializableSecretBytes::from_secret_string` 経由**、`expose_secret` 不使用契約） |
| TC-UT-101 | 異常（Persistence） | 同上 | `IpcResponse::Error(IpcErrorCode::Persistence { reason: "persistence error" })` | `Err(PersistenceError::Persistence { reason: "persistence error" })`（写像表 §エラー写像 経由） |
| TC-UT-102 | 異常（Domain） | 同上 | `IpcResponse::Error(IpcErrorCode::Domain { reason: "domain error" })` | `Err(PersistenceError::Domain { reason })` |
| TC-UT-102b | 異常（unexpected） | 同上 | `IpcResponse::Records(vec![])`（不正応答型） | `Err(PersistenceError::IpcDecode { reason: "unexpected response for AddRecord" })`（**ハードコード固定文言**、詳細設計 §add_record 参照） |

#### `edit_record`

| TC-ID | 種別 | 入力 | スタブ応答 | 期待 |
|-------|------|------|---------|------|
| TC-UT-103 | 正常（label のみ） | `id: <id>, label: Some("NEW"), value: None, now: T` | `IpcResponse::Edited { id: <id> }` | `Ok(<id>)`、スタブ受信 = `EditRecord { id, label: Some, value: None, now }` |
| TC-UT-104 | 正常（value のみ） | `id, label: None, value: Some(SecretString::from_str("new"))` | `Edited { id }` | `Ok(<id>)`、スタブ受信の value が `Some(SerializableSecretBytes(b"new"))` |
| TC-UT-105 | 異常（NotFound） | 同上 | `IpcResponse::Error(IpcErrorCode::NotFound { id })` | `Err(PersistenceError::RecordNotFound(id))`（**Issue #30 で `PersistenceError` に追加された新バリアント、§エラー写像**） |
| TC-UT-105b | 異常（unexpected） | 同上 | `IpcResponse::Added { id }` | `Err(PersistenceError::IpcDecode { reason: "unexpected response for EditRecord" })` |

#### `remove_record`

| TC-ID | 種別 | 入力 | スタブ応答 | 期待 |
|-------|------|------|---------|------|
| TC-UT-106 | 正常 | `id: <id>` | `IpcResponse::Removed { id }` | `Ok(<id>)`、スタブ受信 = `RemoveRecord { id }` |
| TC-UT-107 | 異常（NotFound） | 同上 | `IpcResponse::Error(NotFound { id })` | `Err(PersistenceError::RecordNotFound(id))` |
| TC-UT-108 | 異常（unexpected） | 同上 | `IpcResponse::Edited { id }` | `Err(PersistenceError::IpcDecode { reason: "unexpected response for RemoveRecord" })` |

**横串アサート（全 TC-UT-100〜108 共通）**:

- スタブが受信した `IpcRequest` のデバッグ表示（`{:?}`）に **`SECRET_TEST_VALUE`** が出現しない（`SerializableSecretBytes` の `Debug` が `[REDACTED]` 固定であることの再確認）
- メソッド呼出後、`IpcVaultRepository` が**保持する内部 tokio runtime が drop されていない**こと（再呼出可能性、`drop(repo); drop(runtime)` の順序契約は `Drop for IpcVaultRepository` で担保）

### 2.13 [Phase 1.5 新設] `From<IpcErrorCode> for PersistenceError` — 6 バリアント完全写像

**配置**: `crates/shikomi-cli/src/io/ipc_vault_repository.rs` の `#[cfg(test)] mod tests`（写像実装と同居）。詳細設計 `ipc-vault-repository.md §エラー写像` を単一真実源とする。

| TC-ID | 入力 `IpcErrorCode` | 期待 `PersistenceError` |
|-------|--------------------|------------------------|
| TC-UT-110 | `EncryptionUnsupported` | `PersistenceError::EncryptionUnsupported`（既存バリアント） |
| TC-UT-111 | `NotFound { id }` | `PersistenceError::RecordNotFound(id)`（**Issue #30 新規追加バリアント**） |
| TC-UT-112 | `InvalidLabel { reason: "invalid label" }` | **`PersistenceError::Internal { reason: "invalid label".into() }`（方針 X：Internal 集約、固定文言保持）** |
| TC-UT-113 | `Persistence { reason: "persistence error" }` | **`PersistenceError::Internal { reason: "persistence error".into() }`（方針 X：Internal 集約、固定文言保持）** |
| TC-UT-114 | `Domain { reason: "domain error" }` | **`PersistenceError::Internal { reason: "domain error".into() }`（方針 X：Internal 集約、固定文言保持）** |
| TC-UT-114b | `Internal { reason: "unexpected error" }` | `PersistenceError::Internal { reason: "unexpected error".into() }`（**Issue #30 新規追加バリアント**、方針 X 集約先と同一型） |

**方針 X（Internal 集約）採用に伴う TC-UT-112/113/114 期待値変更**:
- 旧仕様（`PersistenceError::InvalidLabel(_)` / `Persistence { reason }` / `Domain { reason }` の個別バリアント）から、新仕様 **`PersistenceError::Internal { reason }`（集約バリアント）** に変更
- リーダー指示「`PersistenceError` への新規追加は `RecordNotFound` / `Internal` の 2 バリアントに留める」に整合
- daemon 側 `IpcErrorCode.reason` がハードコード固定文言（`"invalid label"` / `"persistence error"` / `"domain error"` 等の有限集合）を構築する契約のため、`reason.as_str()` の固定マッチで presenter 段の `MSG-CLI-101/102/107/108/109` 分岐が機能する
- TC-UT-114b と TC-UT-112/113/114 の出力型は**全て `PersistenceError::Internal { reason }`** に統一されるが、`reason` の文字列値が異なるため後段で区別可能
- 設計書写像表（`../basic-design/error.md §IpcErrorCode バリアント詳細` / `../detailed-design/ipc-vault-repository.md §エラー写像`）と一致

**横串アサート**: 全 6 写像の出力 `Display` 文字列が secret マーカー `SECRET_TEST_VALUE` / 絶対パス `/home/` / lock holder PID 文字列を**含まない**（`predicates::str::contains(...).not()`）。固定文言契約の構造的検証。

### 2.14 [Phase 1.5 新設] `RepositoryHandle` enum dispatch — 網羅性検査

**配置**: `crates/shikomi-cli/src/lib.rs` の `#[cfg(test)] mod tests`。

**目的**: `enum RepositoryHandle { Sqlite, Ipc }` の `match` が**網羅**されていることをコンパイル時に保証し、将来 Phase 2 で `Sqlite` バリアント削除時に修正漏れが起きないことを構造的に担保する。

| TC-ID | 種別 | 検証内容 |
|-------|------|---------|
| TC-UT-115 | 構造（網羅性） | `enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }` の全バリアントを `match` で扱う `run_list` / `run_add` / `run_edit` / `run_remove` がコンパイルされる（4 関数 × 2 アーム = 8 ペアが**全て存在**することを `#[test]` 内 `match` パターンで構造確認、`#[non_exhaustive]` を**付けない**ことの恩恵検証） |
| TC-UT-116 | 構造（dispatch） | `RepositoryHandle::Sqlite` で構築 → `run_list` 呼出 → 内部で `usecase::list::list_records` 経路を通る（fake repo で副作用観測） |
| TC-UT-117 | 構造（dispatch） | `RepositoryHandle::Ipc` で構築（`tokio::io::duplex` スタブで `IpcVaultRepository` 構築）→ `run_add` 呼出 → 内部で `ipc.add_record` 経路を通る（**`usecase::add::add_record` を呼ばない**＝重複ドメイン orchestration の構造的不在検証、案 D の核） |

### 2.15 [Phase 1.5 新設] 嘘 ID 出荷の構造的不在検証

**目的**: 案 C で発生していた「CLI 側 `Uuid::now_v7()` × daemon 側 `Uuid::now_v7()` の二重発行」を**構造的に消滅**させた契約（詳細設計 §add_record「id の真実源は daemon 側」）の UT 表現。

| TC-ID | 種別 | 検証内容 |
|-------|------|---------|
| TC-UT-118 | 契約（嘘 ID 不在） | `IpcVaultRepository::add_record(...)` 呼出時、スタブが `IpcResponse::Added { id: <daemon_generated_uuid> }` を返す → メソッド戻り値が**スタブ送信の id と bit 同一**（`assert_eq!(returned_id, daemon_generated_uuid)`）。CLI 側で `Uuid::now_v7()` を呼んで生成した別 id が紛れ込んでいない構造検証。**ペア CI grep**: TC-CI-029（`crates/shikomi-cli/src/io/ipc_*` 配下に `Uuid::now_v7\|Uuid::new_v` が grep 0 件） |

### 2.16 [Phase 1.5 新設] `IpcVaultRepository` が `VaultRepository` trait を実装しない型契約

**目的**: 案 D の核。`IpcVaultRepository` が `VaultRepository` trait を実装すると、案 C の構造的破綻（嘘 Plaintext / 嘘 ID / 常時 true exists）が再発する。**型システムでの再発防止**。

| TC-ID | 種別 | 検証内容 |
|-------|------|---------|
| TC-UT-119 | 構造（trait 非実装、`compile_fail` doctest） | `crates/shikomi-cli/src/io/ipc_vault_repository.rs` 内の doctest として ` ```compile_fail\nfn _assert<T: shikomi_infra::VaultRepository>() {}\n_assert::<shikomi_cli::io::ipc_vault_repository::IpcVaultRepository>();\n``` ` を配置。`IpcVaultRepository` が `VaultRepository` を実装していたら**コンパイルが通ってしまう**ため、doctest が `compile_fail` で fail する → 実装側のテストが**赤になる**ことで再発を検出。**ペア CI grep**: TC-CI-030（`grep impl VaultRepository for IpcVaultRepository` が 0 件） |
| TC-UT-120 | 構造（専用メソッドの公開 API 確認） | `IpcVaultRepository` に `pub fn list_summaries`, `add_record`, `edit_record`, `remove_record`, `connect`, `default_socket_path` の 6 メソッドが**全て pub** で存在することを `cargo doc` 出力 grep + 簡易 reflection（`#[test]` 内で関数ポインタ取得）で確認。`load` / `save` / `exists` の 3 メソッドが**存在しない**ことは TC-UT-119 で構造的に保証済み |

### 2.17 [Phase 1.5 / Issue #33 新設] `run_edit` IPC 経路の fail-secure 構造検証

**目的**: PR #32 の方針 B（`composition-root.md §run_edit IPC 経路の方針 B`）が**型レベルで強制**されていることを担保する。すなわち、IPC 経路で既存 kind が判明できないとき `kind_for_input == Secret` に**確定**し、後段の `read_value_from_stdin(kind)` が TTY 上で `read_password`（非エコー）経路を選択する——この決定論を構造検証する。実装は `crates/shikomi-cli/src/lib.rs::run_edit` 内 `match (existing_kind, handle)` の 3 アーム（`lib.rs:368-372` 周辺）。

**配置**: `crates/shikomi-cli/src/lib.rs` の `#[cfg(test)] mod tests`（`run_edit` 直近、抽出した pure 関数に対するテスト）。

**抽出推奨（実装担当への要請、§3.10 で再掲）**: 現状 `lib.rs:368-372` の `match (existing_kind, handle)` 式を以下のいずれかで pure 化抽出すること:

- **案 1（推奨）**: `pub(super) enum RepositoryHandleDiscriminant { Sqlite, Ipc }` を新設し、`pub(super) fn discriminant(handle: &RepositoryHandle) -> RepositoryHandleDiscriminant` + `pub(super) fn decide_kind_for_input(existing_kind: Option<RecordKind>, handle: RepositoryHandleDiscriminant) -> RecordKind` の 2 関数で抽出。**`IpcVaultRepository` 構築不要**（実 daemon spawn 不要）でテスト並列実行可
- **案 2（不採用、参考）**: `pub(super) fn decide_kind_for_input(existing_kind: Option<RecordKind>, handle: &RepositoryHandle) -> RecordKind` で直接抽出。`RepositoryHandle::Ipc` を構築するためにスタブ daemon spawn が必要、テスト遅延

**本テスト設計は案 1 を採用**。実装担当はそれに合わせて関数抽出を行うこと。

#### TC-UT 一覧（`decide_kind_for_input` 単体）

| TC-ID | 種別 | 入力 | 期待 |
|-------|------|------|------|
| TC-UT-130 | 正常（既存 Text 判明） | `existing_kind = Some(RecordKind::Text)`, handle = `Sqlite` | `RecordKind::Text` |
| TC-UT-131 | 正常（既存 Secret 判明） | `existing_kind = Some(RecordKind::Secret)`, handle = `Sqlite` | `RecordKind::Secret` |
| **TC-UT-132** | **fail-secure（IPC 経路 + kind 不明）** | `existing_kind = None`, handle = `Ipc` | **`RecordKind::Secret`（強制）** ★方針 B の核 |
| TC-UT-133 | dummy（Sqlite + needs_value_input == false） | `existing_kind = None`, handle = `Sqlite` | `RecordKind::Text`（dummy、`resolve_secret_value` の `--value` 経路で kind 非参照のため副作用なし） |
| TC-UT-134 | 横串（IPC アーム不変条件） | `existing_kind ∈ {None, Some(Text), Some(Secret)}` × handle = `Ipc` の全 3 入力 | **戻り値が `RecordKind::Text` を一切返さない**（IPC アームに dummy Text が紛れ込まないことの構造保証）。実装は 3 入力の単純列挙で網羅 |

#### TC-UT-135: `read_value_from_stdin` の kind 引数 → 経路選択ホワイトボックス

`crates/shikomi-cli/src/lib.rs::read_value_from_stdin` 内の `if matches!(kind, RecordKind::Secret) && io::terminal::is_stdin_tty()` 分岐に対するホワイトボックス検証:

| TC-ID | 入力 | 前提（trait 経由 mock） | 期待経路 |
|-------|------|--------------------|---------|
| TC-UT-135a | `kind = Secret`, TTY 模擬 | `is_stdin_tty()` mock = `true`（trait 抽象化要請、§3.10 ② を参照） | `read_password` 呼出（call counter で観測） |
| TC-UT-135b | `kind = Text`, TTY 模擬 | 同上 | `read_line` 呼出（`read_password` 非呼出） |
| TC-UT-135c | `kind = Secret`, **非 TTY** | `is_stdin_tty()` mock = `false` | `read_line` 呼出（パイプ入力経路、`read_password` 非呼出—画面エコーがそもそも発生しないため） |

**TC-UT-135 の代替案**: `is_stdin_tty()` の trait 抽象化が**過剰**と判断される場合（YAGNI 観点で本 Issue 範囲外）、TC-UT-135 を廃止して **TC-E2E-017a/017b（`e2e.md`）の pty 観測のみで担保**する選択肢もある。実装担当判断で以下を選べ:

- **案 X（trait 抽出して UT-135 を実装、本テスト設計の第一案）**: 高速並列、本番経路に thin wrapper コスト発生
- **案 Y（UT-135 廃止、E2E のみで担保）**: 本番コード無変更、pty テスト 1〜2 件で実機観測

E2E（TC-E2E-017）が pty で実観測する以上、案 Y でも fail-secure 経路の振る舞いは網羅される。**本 Issue では案 Y を許容**し、UT-135 は trait 抽出が他の利用（後続 feature）で発生したタイミングで実装する。

---

## 3. 実装担当への引き継ぎ事項

詳細設計で未確定な実装ポイントをここに集約する。

### 3.1 `PeerCredentialSource` trait（TC-UT-020〜024 のため、**断定仕様**）

**ペテルギウス review 指摘 ④ 対応（2026-04-25）**: 前版の「推奨 API / 代替案」両論併記を撤廃し、**trait 注入一本化の断定仕様**として確定する。`#[cfg(test)]` 差替機構・関数ポインタ方式は**採用しない**。

**確定 API**:

- `pub(crate) trait PeerCredentialSource { fn peer_uid(&self) -> Result<u32, PeerVerificationError>; fn self_uid(&self) -> u32; }` を `crates/shikomi-daemon/src/permission/peer_credential/mod.rs` に定義
- 本番実装: `impl PeerCredentialSource for tokio::net::UnixStream`（cfg unix）/ `impl PeerCredentialSource for tokio::net::windows::named_pipe::NamedPipeServer`（cfg windows）
- 検証関数: `pub(crate) fn verify_unix(source: &impl PeerCredentialSource) -> Result<(), PeerVerificationError>`（Windows 版 `verify_windows` も同型、SID 比較）
- テスト実装: `crates/shikomi-daemon/tests/common/peer_mock.rs` に `pub struct TestPeerCredential { peer: u32, slf: u32 }` を `impl PeerCredentialSource` で配置、UT は `verify_unix(&TestPeerCredential { peer: 1000, slf: 1000 })` で直接呼び出す
- `IpcServer` 側: `pub fn new(...)` は `UnixStream` の `impl PeerCredentialSource` を自動使用、`#[cfg(test)] pub fn new_for_test(..., source: Box<dyn PeerCredentialSource>)` で IT から mock を差替（integration.md §8.1 で確定）

**禁止事項**:
- 本番 `src/` 配下に `#[cfg(test)]` バイパス分岐を書かない
- `std::env::var("SHIKOMI_DAEMON_SKIP_*")` 系の読取を本番 / テストコードに書かない
- Windows 版は `peer_sid` / `self_sid` を返す同型 trait（署名は `Result<String, PeerVerificationError>` で SID 文字列）

### 3.2 `default_socket_path` の pure 切り出し（TC-UT-050〜054 のため）

**推奨**: `fn default_socket_path_from(xdg: Option<&str>, dirs_runtime: Option<PathBuf>, dirs_cache: Option<PathBuf>, user_sid: Option<&str>) -> Result<PathBuf, PersistenceError>` を pub(crate) で抽出し、公開 `default_socket_path()` は env / dirs を取得してこれに委譲する。env 操作を排除できユニット並列実行で安全。

### 3.3 [DEPRECATED — Phase 1.5 案 D で消滅] `compute_diff` の pub(crate) 公開

**廃止判定（Issue #30、2026-04-25）**: 案 C の `IpcVaultRepository::save` + `compute_diff` 経路自体が**設計から消滅**したため、本引き継ぎは不要。実装担当は `compute_diff` を**実装しない**。後継は §3.8（Phase 1.5 専用メソッド + RepositoryHandle）。

### 3.4 MessagePack round-trip を **integration test に移す**根拠

`shikomi-core` に `rmp-serde` を dev-dep に追加すると純粋性が揺らぐ（詳細設計 §設計判断 12 採用案 B）。本 UT では `serde_json` round-trip で型安全性を検証し、`rmp-serde` wire 互換は `integration.md TC-IT-005〜009` で検証。

### 3.5 `IpcErrorCode::InvalidLabel { reason }` の reason 内容規約

UT では `reason` の**固定文言**性を assert（TC-UT-033 の補強、服部 review 後改訂）。詳細設計 `../detailed-design/daemon-runtime.md §Result→IpcErrorCode 写像` の改訂で `format!("{}", err)` ではなく固定リテラル（例: `"invalid label"` / `"persistence error"` / `"domain error"`）を返す方針に変更。`DomainError::Display` / `PersistenceError::Display` の secret / 絶対パス / lock holder PID 内容は **IPC 経路には一切出さず**、詳細は daemon 側 `tracing::warn!` で運用者向けに出す（クライアント向け wire には漏らさない）。

### 3.6 `RecordSummary::from_record` の `text_preview` 呼出集約

**重要**: `RecordSummary::from_record` の中で `record.kind() == Secret` を判定するのはよいが、**`record.text_preview(40)` が Secret kind で `None` を返す既存契約に依存する**。この契約は `cli-vault-commands` の `TC-UT-100〜103` で証明済み。本 feature で `text_preview` の挙動を変更しない。

### 3.7 daemon 側 panic hook の CLI 同型性

詳細設計 `composition-root.md §panic hook` で CLI の既存 panic hook 関数を**そのまま daemon でも使える形**（`shikomi_cli::panic_hook::install` 等を `shikomi-cli` の lib から呼べる公開 API にする or daemon に同型のコピーを置く）を要確認。コピーの場合、本 UT では文字列比較で「固定文言が英語 1 行」を確認する UT を `panic_hook.rs` に追加（本節の追加 TC-UT-080 相当）:

| TC-ID | 入力 | 期待 |
|-------|------|------|
| TC-UT-080 | `std::panic::catch_unwind(|| { panic!("secret=ABC") })` を行い、panic hook が stderr に出力した内容 | 固定文言 `"error: shikomi-daemon internal bug;"` を含み、`"ABC"` を**含まない** |

### 3.8 [Phase 1.5 新設] 専用メソッド + RepositoryHandle に関する実装契約

Issue #30 の案 D 実装で、テスト設計の前提として実装担当（坂田銀時）は以下の構造を**断定的に**実装する:

1. `crates/shikomi-cli/src/io/ipc_vault_repository.rs` に `pub fn add_record(&self, kind, label, value, now) -> Result<RecordId, PersistenceError>` 等の専用メソッド 3 種を追加
2. `IpcVaultRepository` に **`impl VaultRepository for IpcVaultRepository`** を**書かない**。書いた瞬間に TC-UT-119（`compile_fail` doctest）が **赤になる** → CI fail
3. `crates/shikomi-cli/src/lib.rs` 内に `enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }` を non-public で定義（`#[non_exhaustive]` 付与禁止）
4. `run_list` / `run_add` / `run_edit` / `run_remove` の各サブコマンドハンドラが `&RepositoryHandle` を受け、`match handle` で 2 アーム分岐
5. **PR #29 の runtime reject 経路（`crates/shikomi-cli/src/lib.rs:119-` 周辺の `if args.ipc && !matches!(args.subcommand, Subcommand::List) { return CliError::UsageError("--ipc currently supports only the `list` subcommand; ...") }` 相当）を完全削除**。CI grep（`ci.md` TC-CI-028）で文言不在を強制
6. **`crates/shikomi-cli/src/io/ipc_*` 配下で `Uuid::now_v7()` / `Uuid::new_v4()` 等の UUID 生成を呼ばない**（id 生成は daemon 側集約契約、TC-UT-118 + TC-CI-029）
7. `IpcVaultRepository` の `Drop` 実装が **`tokio::runtime::Runtime`** フィールドを破棄する。テスト用に `drop(repo)` の順序契約を観測可能にしておく（§2.12 横串アサート）

### 3.9 [Phase 1.5 新設] `From<IpcErrorCode> for PersistenceError` の配置と固定文言契約

- 配置: `crates/shikomi-cli/src/io/ipc_vault_repository.rs`（`shikomi-infra` 側ではなく CLI 側、IPC 由来コードの写像責務は CLI に閉じる、詳細設計 §エラー写像）
- `PersistenceError` 側に **`RecordNotFound(RecordId)` / `Internal { reason: String }`** の 2 バリアント新規追加（Issue #30）
- 写像時に `IpcErrorCode::*::reason` を**そのまま `PersistenceError::*::reason` に渡す**（追加加工せず）。daemon 側が固定文言にしているため、CLI 側で再フォーマットしない（DRY、固定文言契約の一貫性）
- `Display` 実装も daemon 側 reason を**そのまま埋め込む**（フォーマット文字列で `{reason}` のみ、絶対パス補完禁止）

### 3.10 [Phase 1.5 / Issue #33 新設] `run_edit` fail-secure 経路の関数抽出契約

§2.17（TC-UT-130〜134）の前提として、実装担当（坂田銀時）は以下の抽出を行う:

**① `decide_kind_for_input` の pure 関数化（必須）**

- 現状: `crates/shikomi-cli/src/lib.rs::run_edit` 内 `lib.rs:368-372` の `match (existing_kind, handle)` 式
- 抽出後シグネチャ: `pub(super) fn decide_kind_for_input(existing_kind: Option<RecordKind>, handle: RepositoryHandleDiscriminant) -> RecordKind`
- 補助型: `pub(super) enum RepositoryHandleDiscriminant { Sqlite, Ipc }` + `pub(super) fn discriminant(handle: &RepositoryHandle) -> RepositoryHandleDiscriminant`
- 利点: `IpcVaultRepository` 構築不要（実 daemon spawn 不要）でテスト並列実行可。`#[derive(Debug, Clone, Copy, PartialEq, Eq)]` を付与し UT で `assert_eq!` 可能化
- 本番 `run_edit` 内の呼出は `decide_kind_for_input(existing_kind, discriminant(handle))` に置換

**② `is_stdin_tty` の trait 抽象化（任意、§2.17 案 X 採用時のみ必要）**

- §2.17 で**案 Y（UT-135 廃止、E2E のみで担保）を許容**する方針のため、本抽出は**任意**。後続 feature で trait 注入が必要になった時点で実装可
- 案 X 採用時のみ: `pub trait TerminalProbe { fn is_tty(&self) -> bool; }` を `crates/shikomi-cli/src/io/terminal.rs` に新設、`read_value_from_stdin` を `read_value_from_stdin_with(probe: &dyn TerminalProbe, ...)` に変更し本番経路は thin wrapper で既存 API 維持

**禁止事項（既存契約継続）**:

- `#[cfg(test)]` バイパス分岐を本番 `src/` 直下に書かない（trait 注入一本化、`integration.md §8.1` と同型）
- `std::env::var("SHIKOMI_*_FORCE_*")` 系の env 裏口を作らない（CI grep TC-CI-027 と同型契約、新規 grep `TC-CI-031` を追加するか検討余地あり、本 Issue では既存 027 範囲で十分と判断）

---

## 4. ユニットテスト実行コマンド

```bash
# shikomi-core::ipc の型テスト（serde_json round-trip）
cargo test -p shikomi-core --lib ipc

# daemon の handler / peer_credential UT
cargo test -p shikomi-daemon --lib

# cli の io / error / presenter UT
cargo test -p shikomi-cli --lib -- ipc_vault_repository default_socket_path render_error

# [Phase 1.5] 専用メソッド + From<IpcErrorCode> + RepositoryHandle dispatch UT
cargo test -p shikomi-cli --lib -- \
    ipc_vault_repository::tests::test_add_record \
    ipc_vault_repository::tests::test_edit_record \
    ipc_vault_repository::tests::test_remove_record \
    ipc_vault_repository::tests::test_from_ipc_error_code \
    repository_handle::tests::test_dispatch

# [Phase 1.5] 構造契約（compile_fail doctest）— IpcVaultRepository が VaultRepository を実装しないこと
cargo test -p shikomi-cli --doc -- ipc_vault_repository::no_vault_repository_impl

# [Phase 1.5 / Issue #33] run_edit fail-secure（decide_kind_for_input）UT
cargo test -p shikomi-cli --lib -- decide_kind_for_input
```

各テストの docstring に対応 REQ-ID と Issue 番号（`// REQ-DAEMON-xxx / Issue #26`）を書くこと（テスト戦略ガイド準拠）。

---

*この文書は `index.md` の分割成果。E2E は `e2e.md`、結合は `integration.md`、CI は `ci.md` を参照*
