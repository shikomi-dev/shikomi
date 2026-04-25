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
- **粒度**: 1 テスト 1 アサーション。命名 `test_<対象>_<状況>_<期待>`
- **モック**: I/O バウンダリのみ。`PeerCredentialSource` trait（後述）でピア取得を抽象化し、in-test 実装で固定値を返す
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

**引き継ぎ事項**: `PeerCredentialSource` trait（or 関数ポインタでも可）は**詳細設計 `daemon-runtime.md §peer_credential` に暗黙的に提示されている**が、trait 境界が明示されていない。実装担当（坂田銀時）は本 UT を書けるように **`#[cfg(test)]` で差替可能な引数経路を `verify_unix` / `verify_windows` に設ける**こと。別案: 実 syscall をラップする `peer_uid(fd) -> Result<u32, _>` / `peer_sid(handle) -> Result<String, _>` を pub(crate) で切り出し、`verify_*` が高階関数でこれを受け取る。どちらでも UT が可能な形で OK。

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

### 2.11 `shikomi_cli::io::ipc_vault_repository::IpcVaultRepository::save` — 差分計算ロジック

詳細設計 `ipc-vault-repository.md §save の実装方針` で確定した**シャドウ差分計算**の pure 部分。

配置: `crates/shikomi-cli/src/io/ipc_vault_repository.rs` の `#[cfg(test)] mod tests`。内部関数 `compute_diff(shadow: &Vault, new: &Vault) -> Vec<IpcRequest>` を pub(crate) で抽出し、in-memory `Vault` 2 つを入力として判定する。

| TC-ID | 種別 | shadow / new | 期待発行順 |
|-------|------|------------|----------|
| TC-UT-060 | 正常（追加のみ） | shadow 空、new 1 件 | `[AddRecord { ... }]` |
| TC-UT-061 | 正常（削除のみ） | shadow 1 件、new 空 | `[RemoveRecord { id: 同 }]` |
| TC-UT-062 | 正常（label 更新のみ） | shadow/new 同 id、label 異、`updated_at` 異 | `[EditRecord { id, label: Some, value: Some, now }]`（保守的方針で value も送る可能性あり、実装で確定） |
| TC-UT-063 | 正常（変更なし） | shadow/new 同値 | `[]`（空） |
| TC-UT-064 | 境界（全差異） | shadow A1B2、new A1'C3 | `[EditRecord(A1'), RemoveRecord(B2), AddRecord(C3)]` の 3 件（順序は実装依存、**発行内容の集合一致**を assert） |

---

## 3. 実装担当への引き継ぎ事項

詳細設計で未確定な実装ポイントをここに集約する。

### 3.1 `PeerCredentialSource` trait（TC-UT-020〜024 のため）

**詳細設計 `daemon-runtime.md §peer_credential`** では `peer_credential::verify(&stream)` の呼出箇所まで示されているが、**ユニットテスト可能な注入点が未確定**。

**推奨 API**: `pub(crate) trait PeerCredentialSource { fn peer_uid(&self) -> Result<u32, PeerVerificationError>; fn self_uid(&self) -> u32; }` と `impl PeerCredentialSource for tokio::net::UnixStream`（cfg(unix)）を提供し、`verify_unix(source: &impl PeerCredentialSource) -> Result<(), PeerVerificationError>` で受け取る。テストでは `struct TestPeerCred { peer: u32, slf: u32 }` を `impl PeerCredentialSource` で渡す。Windows も同型（`peer_sid` / `self_sid`）。

**代替案**: 関数ポインタ or クロージャで `fn(&Stream) -> Result<u32, _>` を高階関数として受ける。いずれでも UT 可能。

### 3.2 `default_socket_path` の pure 切り出し（TC-UT-050〜054 のため）

**推奨**: `fn default_socket_path_from(xdg: Option<&str>, dirs_runtime: Option<PathBuf>, dirs_cache: Option<PathBuf>, user_sid: Option<&str>) -> Result<PathBuf, PersistenceError>` を pub(crate) で抽出し、公開 `default_socket_path()` は env / dirs を取得してこれに委譲する。env 操作を排除できユニット並列実行で安全。

### 3.3 `compute_diff` の pub(crate) 公開（TC-UT-060〜064 のため）

詳細設計 `ipc-vault-repository.md` で差分計算を `save` 内 private function と仮置きしているが、pub(crate) で抽出し UT 可能にすること。

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

---

## 4. ユニットテスト実行コマンド

```bash
# shikomi-core::ipc の型テスト（serde_json round-trip）
cargo test -p shikomi-core --lib ipc

# daemon の handler / peer_credential UT
cargo test -p shikomi-daemon --lib

# cli の io / error / presenter UT
cargo test -p shikomi-cli --lib -- ipc_vault_repository default_socket_path render_error
```

各テストの docstring に対応 REQ-ID と Issue 番号（`// REQ-DAEMON-xxx / Issue #26`）を書くこと（テスト戦略ガイド準拠）。

---

*この文書は `index.md` の分割成果。E2E は `e2e.md`、結合は `integration.md`、CI は `ci.md` を参照*
