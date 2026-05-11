# テスト設計書 — ipc-client（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: ipc-client / Issue #95 -->
<!-- 配置先: docs/features/shikomi-gui/ipc-client/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->
<!-- 参照: basic-design.md §モジュール契約 / detailed-design.md §1〜5 -->

## 0. テスト方針参照

本テスト設計書は `config/prompts/test_strategy.md` に定めるテスト戦略（Vモデル階層化・ダブル方針・CI ワークフロー対応）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは親 `system-test-design.md` に委ねる。

---

## 1. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 | Fixture 状態 |
|-------|---------|---------|------|------------|
| IT（接続・Commands） | UDS / Named Pipe（daemon接続） | `tokio::net::UnixStream` / Named Pipe | `MockDaemon`（テスト用UDSサーバー）で差し替え | 要起票（characterization不要：既存CLI IPC仕様と同一フォーマット、raw fixtureはCLI統合テストで管理済み） |
| IT（接続） | `OffsetDateTime::now_utc()` | 時刻 | テスト実装内で固定 UUID / 固定時刻を使用（Vault ヘルパ経由） | 不要 |
| UT（validation） | なし | 純粋計算（正規表現・String比較） | モック不要 | 不要 |
| UT（GUIError Serialize） | `serde_json` | 純粋計算 | モック不要 | 不要 |

> **assumed mock 禁止**: IT 用 `MockDaemon` は `shikomi-core::ipc` の実 MessagePack フォーマットを使用する。
> fixture 構造は CLI・daemon 間で既に検証済みの仕様に依存するため、別途 characterization task は不要。
> ただし GUI 側 IPC 型変換（`IpcResponse` → GUI JSON）は IT で実観測を行うこと。

---

## 2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT（validation + Serialize） | `crates/shikomi-gui/src/ipc_client/error.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-gui` |
| UT（validation） | `crates/shikomi-gui/src/ipc_client/commands/entries.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-gui` |
| UT（validation） | `crates/shikomi-gui/src/ipc_client/commands/hotkey.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-gui` |
| UT（validation） | `crates/shikomi-gui/src/ipc_client/commands/vault.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-gui` |
| IT（接続 + round_trip） | `crates/shikomi-gui/tests/it_ipc_client.rs` | `cargo test -p shikomi-gui` |
| IT（Tauri Commands） | `crates/shikomi-gui/tests/it_ipc_commands.rs` | `cargo test -p shikomi-gui` |

---

## 3. テスト用ダブルの方針

### 3.1 `MockDaemon`

IT 専用テスト用 UDS サーバー。`tests/common/mock_daemon.rs` に物理分離して配置する（本番コードへの混入禁止）。

| 項目 | 仕様 |
|------|------|
| 実装 | `tokio` 非同期タスク。`tempfile::TempDir` で一時ソケットパスを生成 |
| 接続受付 | 1 接続のみ受け付け、Handshake を処理後に事前設定レスポンスを返す |
| フレームコーデック | `basic-design.md §1.3` と同一（little-endian 4バイト長 / MessagePack） |
| 設定 | `MockDaemon::new(response: IpcResponse)` で返却レスポンスを 1 件設定 |
| 返却方法 | Handshake 受信後、最初の `IpcRequest` に対して設定済み `IpcResponse` を返して終了 |

### 3.2 `AppState` の直接構築

Tauri Command ハンドラは `tauri::State<AppState>` を受け取る。テストでは `AppState`（`Arc<Mutex<Option<GuiIpcClient>>>`）を直接構築してハンドラに渡す。

| パターン | 構築方法 |
|---------|---------|
| daemon 接続済み | `Arc::new(tokio::sync::Mutex::new(Some(client)))` |
| daemon 未接続（Fail Fast検証） | `Arc::new(tokio::sync::Mutex::new(None))` |

---

## 4. テストマトリクス（トレーサビリティ）

### 4.1 ユニットテスト

| テスト ID | REQ-IPC | 設計根拠 | テスト内容 | 種別 |
|---------|---------|--------|----------|------|
| TC-GUI-IPC-UT01 | REQ-IPC-02 | `detailed-design.md §4.2` | `add_entry` — ラベル空文字 → `GUIError::InvalidInput("label must not be empty")` | 異常系 |
| TC-GUI-IPC-UT02 | REQ-IPC-02 | `detailed-design.md §4.2` | `add_entry` — 値空文字 → `GUIError::InvalidInput("value must not be empty")` | 異常系 |
| TC-GUI-IPC-UT03 | REQ-IPC-05 | `detailed-design.md §3.5` | `assign_hotkey` — `Ctrl+Alt+0`（1-9 範囲外）→ `GUIError::InvalidInput("hotkey must be Ctrl+Alt+[1-9]")` | 異常系（境界値） |
| TC-GUI-IPC-UT04 | REQ-IPC-05 | `detailed-design.md §3.5` | `assign_hotkey` — `ctrl+alt+1`（小文字）→ `GUIError::InvalidInput` | 異常系 |
| TC-GUI-IPC-UT05 | REQ-IPC-05 | `detailed-design.md §3.5` | `assign_hotkey` — `Ctrl+Alt+1`（正常最小値）→ validation PASS | 正常系（境界値） |
| TC-GUI-IPC-UT06 | REQ-IPC-05 | `detailed-design.md §3.5` | `assign_hotkey` — `Ctrl+Alt+9`（正常最大値）→ validation PASS | 正常系（境界値） |
| TC-GUI-IPC-UT07 | REQ-IPC-09 | `detailed-design.md §3.9` | `decrypt_vault` — `confirmed: false` → `GUIError::InvalidInput("decrypt confirmation required")` | 異常系 |
| TC-GUI-IPC-UT08 | REQ-IPC-04 | `detailed-design.md §4.2` | `delete_entry` — 不正 UUID 文字列 → `GUIError::InvalidInput("invalid record id format")` | 異常系 |
| TC-GUI-IPC-UT09 | REQ-IPC-03 | `detailed-design.md §3.3` | `update_entry` — `label: None, value: None`（全フィールド `None`）→ IPC 送信省略、即時 `Edited { id }` 返却 | 正常系 |
| TC-GUI-IPC-UT10 | `basic-design.md §2.2` | `detailed-design.md §2.1` | `GUIError::DaemonNotRunning` を `serde_json::to_value` → `kind == "daemon_not_running"` | 正常系 |
| TC-GUI-IPC-UT11 | `basic-design.md §2.2` | `detailed-design.md §2.1` | `GUIError::NotConnected` → `kind == "not_connected"` | 正常系 |
| TC-GUI-IPC-UT12 | `basic-design.md §2.2` | `detailed-design.md §2.1` | `GUIError::ProtocolVersionMismatch { server: "V1", client: "V2" }` → `kind == "protocol_version_mismatch"` | 正常系 |
| TC-GUI-IPC-UT13 | `basic-design.md §2.2` | `detailed-design.md §2.3` | `GUIError::Ipc(IpcErrorCode::VaultLocked)` → `kind == "ipc_error"`、`message` に `IpcErrorCode::VaultLocked` の `Display` 文字列 | 正常系 |
| TC-GUI-IPC-UT14 | `basic-design.md §2.2` | `detailed-design.md §2.2` | `GUIError::InvalidInput("test message")` → `kind == "invalid_input"`, `message == "test message"` | 正常系 |

### 4.2 結合テスト

| テスト ID | REQ-IPC | 設計根拠 | テスト内容 | 種別 |
|---------|---------|--------|----------|------|
| TC-GUI-IPC-IT01 | REQ-IPC-11 | `detailed-design.md §1.4` | `GuiIpcClient::connect()` — ソケットファイル不存在 → `GUIError::DaemonNotRunning` | 異常系 |
| TC-GUI-IPC-IT02 | REQ-IPC-11 | `detailed-design.md §1.2` | `GuiIpcClient::connect()` — MockDaemon が V2 Handshake 成功 → `Ok(GuiIpcClient)` | 正常系 |
| TC-GUI-IPC-IT03 | REQ-IPC-11 | `detailed-design.md §1.2` | `GuiIpcClient::connect()` — MockDaemon が `IpcResponse::Handshake { server_version: V1 }` → `GUIError::ProtocolVersionMismatch` | 異常系 |
| TC-GUI-IPC-IT04 | REQ-IPC-12 | `basic-design.md §2.4` / `detailed-design.md §5` | `AppState = None` で `list_entries` 呼び出し → `GUIError::NotConnected`（IPC 送信なし） | 異常系（Fail Fast） |
| TC-GUI-IPC-IT05 | REQ-IPC-12 | `detailed-design.md §5` | `AppState = None` で `add_entry` 呼び出し → `GUIError::NotConnected` | 異常系（Fail Fast） |
| TC-GUI-IPC-IT06 | REQ-IPC-01 | `detailed-design.md §3.1` | `list_entries` — MockDaemon `IpcResponse::Records { records, protection_mode }` → `{ entries, vault_status }` が返る | 正常系 |
| TC-GUI-IPC-IT07 | REQ-IPC-02 | `detailed-design.md §3.2` | `add_entry` 正常系 — MockDaemon `IpcResponse::Added { id }` → `{ id }` が返る | 正常系 |
| TC-GUI-IPC-IT08 | REQ-IPC-02 | `detailed-design.md §3.2` | `add_entry` — MockDaemon `IpcResponse::Error(IpcErrorCode::HotkeyConflict)` → `GUIError::Ipc(HotkeyConflict)` | 異常系 |
| TC-GUI-IPC-IT09 | REQ-IPC-03 | `detailed-design.md §3.3` | `update_entry` — MockDaemon `IpcResponse::Edited { id }` → `{ id }` が返る | 正常系 |
| TC-GUI-IPC-IT10 | REQ-IPC-04 | `detailed-design.md §3.4` | `delete_entry` 正常系 — MockDaemon `IpcResponse::Removed { id }` → `{ id }` が返る | 正常系 |
| TC-GUI-IPC-IT11 | REQ-IPC-05 | `detailed-design.md §3.5` | `assign_hotkey` `Ctrl+Alt+3` 正常系 — MockDaemon `IpcResponse::Edited { id }` → `{ id }` が返る | 正常系 |
| TC-GUI-IPC-IT12 | REQ-IPC-05 | `detailed-design.md §3.5` | `assign_hotkey` — MockDaemon `IpcResponse::Error(HotkeyConflict)` → `GUIError::Ipc(HotkeyConflict)` | 異常系 |
| TC-GUI-IPC-IT13 | REQ-IPC-06 | `detailed-design.md §3.6` | `remove_hotkey` — MockDaemon `IpcResponse::Edited { id }` → `{ id }` が返る | 正常系 |
| TC-GUI-IPC-IT14 | REQ-IPC-07 | `detailed-design.md §3.7` | `get_vault_status` — MockDaemon `IpcResponse::Records { protection_mode: Encrypted, records: [] }` → `{ vault_status: Encrypted }` のみ返る（records は含まれない） | 正常系 |
| TC-GUI-IPC-IT15 | REQ-IPC-08 | `detailed-design.md §3.8` | `encrypt_vault` — MockDaemon `IpcResponse::Encrypted { disclosure: [24語] }` → `{ disclosure: Vec<String> }` 24 件 | 正常系 |
| TC-GUI-IPC-IT16 | REQ-IPC-09 | `detailed-design.md §3.9` | `decrypt_vault` `confirmed: true` — MockDaemon `IpcResponse::Decrypted` → `{}` 成功 | 正常系 |
| TC-GUI-IPC-IT17 | REQ-IPC-10 | `detailed-design.md §3.10` | `unlock_vault` — MockDaemon `IpcResponse::Unlocked` → `{}` 成功 | 正常系 |
| TC-GUI-IPC-IT18 | REQ-IPC-11, REQ-IPC-12 | `detailed-design.md §5` | `round_trip` 中に MockDaemon が接続を強制切断 → `GUIError::ConnectionFailed`、`AppState` が `None` にリセットされる | 異常系（切断復旧） |

---

## 5. ユニットテスト詳細設計

### TC-GUI-IPC-UT01: `add_entry` — ラベル空文字

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-UT01 |
| 対応する要件ID | REQ-IPC-02（R1-GUI-05, R1-GUI-19） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §4.2`） |
| 種別 | 異常系 |
| 前提条件 | `AppState = Some(client)`（Validation は IPC 送信前に実行される） |
| 操作 | `add_entry(state, kind=Text, label="", value="hello", hotkey=None)` |
| 期待結果 | `Err(GUIError::InvalidInput("label must not be empty"))` が返る。IPC 送信は行われない |

### TC-GUI-IPC-UT02: `add_entry` — 値空文字

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-UT02 |
| 対応する要件ID | REQ-IPC-02（R1-GUI-19） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §4.2`） |
| 種別 | 異常系 |
| 前提条件 | `AppState = Some(client)` |
| 操作 | `add_entry(state, kind=Text, label="my label", value="", hotkey=None)` |
| 期待結果 | `Err(GUIError::InvalidInput("value must not be empty"))` |

### TC-GUI-IPC-UT03〜UT06: `assign_hotkey` — ホットキー形式検証

| テスト ID | 入力 combo | 期待結果 | 種別 |
|---------|----------|---------|------|
| TC-GUI-IPC-UT03 | `"Ctrl+Alt+0"` | `Err(GUIError::InvalidInput("hotkey must be Ctrl+Alt+[1-9]"))` | 異常系（境界値） |
| TC-GUI-IPC-UT04 | `"ctrl+alt+1"` | `Err(GUIError::InvalidInput("hotkey must be Ctrl+Alt+[1-9]"))` | 異常系（大文字小文字） |
| TC-GUI-IPC-UT05 | `"Ctrl+Alt+1"` | 検証通過（`Ok` または IPC 送信ステップへ進む） | 正常系（最小境界値） |
| TC-GUI-IPC-UT06 | `"Ctrl+Alt+9"` | 検証通過 | 正常系（最大境界値） |

**前提条件**: 検証のみを単体で呼ぶ。`AppState` は未使用（validation 層のみテスト）
**操作**: `validate_hotkey_combo(combo)` または相当するヘルパ関数を直接呼び出す

### TC-GUI-IPC-UT07: `decrypt_vault` — `confirmed: false` Fail Fast

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-UT07 |
| 対応する要件ID | REQ-IPC-09（R1-GUI-12, R1-GUI-19） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §3.9`） |
| 種別 | 異常系 |
| 前提条件 | `AppState = Some(client)` |
| 操作 | `decrypt_vault(state, master_password="correct", confirmed=false)` |
| 期待結果 | `Err(GUIError::InvalidInput("decrypt confirmation required"))` が返る。IPC 送信は行われない |

### TC-GUI-IPC-UT08: `delete_entry` — 不正 UUID

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-UT08 |
| 対応する要件ID | REQ-IPC-04（`detailed-design.md §4.2`） |
| 対応する工程 | 階層 3 詳細設計 |
| 種別 | 異常系 |
| 操作 | `delete_entry(state, id="not-a-uuid")` |
| 期待結果 | `Err(GUIError::InvalidInput("invalid record id format"))` |

### TC-GUI-IPC-UT09: `update_entry` — 全フィールド `None`（IPC 省略）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-UT09 |
| 対応する要件ID | REQ-IPC-03（`detailed-design.md §3.3`） |
| 対応する工程 | 階層 3 詳細設計 |
| 種別 | 正常系（IPC 省略経路） |
| 前提条件 | `AppState = Some(client)`（IPC は呼ばれないはずだが、AppState は接続済みで準備） |
| 操作 | `update_entry(state, id=valid_uuid, label=None, value=None)` |
| 期待結果 | `Ok({ id: valid_uuid })` が返る。MockDaemon への IPC リクエストは 0 件 |

### TC-GUI-IPC-UT10〜UT14: `GUIError` Serialize 検証

| テスト ID | 入力 GUIError | 期待 `kind` | 期待 `message` | 種別 |
|---------|-------------|-----------|--------------|------|
| TC-GUI-IPC-UT10 | `GUIError::DaemonNotRunning` | `"daemon_not_running"` | 非空文字列 | 正常系 |
| TC-GUI-IPC-UT11 | `GUIError::NotConnected` | `"not_connected"` | 非空文字列 | 正常系 |
| TC-GUI-IPC-UT12 | `GUIError::ProtocolVersionMismatch { server: "V1", client: "V2" }` | `"protocol_version_mismatch"` | `"V1"` / `"V2"` 両方含む | 正常系 |
| TC-GUI-IPC-UT13 | `GUIError::Ipc(IpcErrorCode::VaultLocked)` | `"ipc_error"` | `IpcErrorCode::VaultLocked` の Display 文字列と一致 | 正常系 |
| TC-GUI-IPC-UT14 | `GUIError::InvalidInput("test message")` | `"invalid_input"` | `"test message"` と完全一致 | 正常系 |

**操作共通**: `serde_json::to_value(&error).unwrap()` で JSON 変換し、`["kind"]` / `["message"]` フィールドを assert する

---

## 6. 結合テスト詳細設計

### TC-GUI-IPC-IT01: `connect()` — daemon 未起動（ソケット不存在）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT01 |
| 対応する要件ID | REQ-IPC-11（R1-GUI-02） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §1.4`） |
| 種別 | 異常系 |
| 前提条件 | ソケットファイルが存在しないパスを用意（`TempDir` の未作成ファイルパス） |
| 操作 | `GuiIpcClient::connect(&non_existent_path).await` |
| 期待結果 | `Err(GUIError::DaemonNotRunning)` が返る |

### TC-GUI-IPC-IT02: `connect()` — V2 Handshake 成功

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT02 |
| 対応する要件ID | REQ-IPC-11（R1-GUI-02） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §1.2`） |
| 種別 | 正常系 |
| 前提条件 | `MockDaemon` を起動（V2 Handshake を正常処理） |
| 操作 | `GuiIpcClient::connect(&socket_path).await` |
| 期待結果 | `Ok(GuiIpcClient)` が返る。接続済み状態に遷移 |

### TC-GUI-IPC-IT03: `connect()` — プロトコルバージョン不一致

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT03 |
| 対応する要件ID | REQ-IPC-11（R1-GUI-02） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §1.2`） |
| 種別 | 異常系 |
| 前提条件 | `MockDaemon` が Handshake に `server_version: V1`（仮の古いバージョン）を返すよう設定 |
| 操作 | `GuiIpcClient::connect(&socket_path).await` |
| 期待結果 | `Err(GUIError::ProtocolVersionMismatch { server: "V1", client: "V2" })` |

### TC-GUI-IPC-IT04〜IT05: daemon 未接続 Fail Fast（REQ-IPC-12）

| テスト ID | 呼び出す Command | 期待結果 |
|---------|--------------|---------|
| TC-GUI-IPC-IT04 | `list_entries` | `Err(GUIError::NotConnected)` |
| TC-GUI-IPC-IT05 | `add_entry` (label/value 非空) | `Err(GUIError::NotConnected)` |

**前提条件**: `AppState = Arc::new(Mutex::new(None))`（未接続状態）
**操作**: 各 Command ハンドラを直接呼び出す
**期待結果共通**: IPC 送信は発生しない（MockDaemon はリクエストを受け取らない）

> 全 10 Commands は同一ガードロジックを経由するため、TC-IT04〜IT05 の 2 件で代表検証する。
> 全件を個別に実行する場合は実装担当と調整のこと。

### TC-GUI-IPC-IT06: `list_entries` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT06 |
| 対応する要件ID | REQ-IPC-01（R1-GUI-04） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §3.1`） |
| 種別 | 正常系 |
| 前提条件 | `AppState = Some(client)`、MockDaemon が `IpcResponse::Records { records: [1件], protection_mode: Plaintext }` を返す |
| 操作 | `list_entries(state).await` |
| 期待結果 | `Ok({ entries: [1件], vault_status: Plaintext })` が返る |

### TC-GUI-IPC-IT07: `add_entry` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT07 |
| 対応する要件ID | REQ-IPC-02（R1-GUI-05） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §3.2`） |
| 種別 | 正常系 |
| 前提条件 | MockDaemon が `IpcResponse::Added { id: some_uuid }` を返す |
| 操作 | `add_entry(state, kind=Text, label="my label", value="hello", hotkey=None).await` |
| 期待結果 | `Ok({ id: some_uuid })` が返る |

### TC-GUI-IPC-IT08: `add_entry` — daemon が HotkeyConflict を返す

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT08 |
| 対応する要件ID | REQ-IPC-02（`detailed-design.md §2.3`） |
| 対応する工程 | 階層 3 基本設計 |
| 種別 | 異常系 |
| 前提条件 | MockDaemon が `IpcResponse::Error(IpcErrorCode::HotkeyConflict { combo, conflicting_label })` を返す |
| 操作 | `add_entry(state, kind=Text, label="new", value="v", hotkey=Some("Ctrl+Alt+1")).await` |
| 期待結果 | `Err(GUIError::Ipc(IpcErrorCode::HotkeyConflict { .. }))` が返る |

### TC-GUI-IPC-IT09: `update_entry` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT09 |
| 対応する要件ID | REQ-IPC-03（R1-GUI-06） |
| 操作 | `update_entry(state, id=valid_uuid, label=Some("new label"), value=None).await` |
| 期待結果 | MockDaemon が `Edited { id }` を返す → `Ok({ id })` |

### TC-GUI-IPC-IT10: `delete_entry` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT10 |
| 対応する要件ID | REQ-IPC-04（R1-GUI-07） |
| 操作 | `delete_entry(state, id=valid_uuid).await` |
| 期待結果 | MockDaemon が `Removed { id }` を返す → `Ok({ id })` |

### TC-GUI-IPC-IT11: `assign_hotkey` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT11 |
| 対応する要件ID | REQ-IPC-05（R1-GUI-08, R1-GUI-09） |
| 操作 | `assign_hotkey(state, id=valid_uuid, combo="Ctrl+Alt+3").await` |
| 期待結果 | MockDaemon が `Edited { id }` を返す → `Ok({ id })`。daemon 送信リクエストの `hotkey == Some("Ctrl+Alt+3")`, `clear_hotkey == false` |

### TC-GUI-IPC-IT12: `assign_hotkey` — HotkeyConflict

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT12 |
| 対応する要件ID | REQ-IPC-05（`detailed-design.md §2.3`） |
| 操作 | `assign_hotkey(state, id=valid_uuid, combo="Ctrl+Alt+5").await`、MockDaemon が `Error(HotkeyConflict)` 返却 |
| 期待結果 | `Err(GUIError::Ipc(IpcErrorCode::HotkeyConflict { .. }))` |

### TC-GUI-IPC-IT13: `remove_hotkey` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT13 |
| 対応する要件ID | REQ-IPC-06（R1-GUI-08） |
| 操作 | `remove_hotkey(state, id=valid_uuid).await` |
| 期待結果 | MockDaemon が `Edited { id }` を返す → `Ok({ id })`。送信リクエストの `clear_hotkey == true` |

### TC-GUI-IPC-IT14: `get_vault_status` — `protection_mode` のみ返却

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT14 |
| 対応する要件ID | REQ-IPC-07（R1-GUI-04, R1-GUI-13） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §3.7`） |
| 種別 | 正常系 |
| 前提条件 | MockDaemon が `IpcResponse::Records { records: [2件], protection_mode: Encrypted }` を返す |
| 操作 | `get_vault_status(state).await` |
| 期待結果 | `Ok({ vault_status: Encrypted })` が返る。`entries` フィールドは含まれない（R1-GUI-13：vault 状態のみ返却） |

### TC-GUI-IPC-IT15: `encrypt_vault` — disclosure 24 語

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT15 |
| 対応する要件ID | REQ-IPC-08（R1-GUI-10, R1-GUI-11） |
| 前提条件 | MockDaemon が `IpcResponse::Encrypted { disclosure: [24語分のバイト列] }` を返す |
| 操作 | `encrypt_vault(state, master_password="StrongPass123!").await` |
| 期待結果 | `Ok({ disclosure: Vec<String> })` が返り、`disclosure.len() == 24` |

### TC-GUI-IPC-IT16: `decrypt_vault` — confirmed=true 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT16 |
| 対応する要件ID | REQ-IPC-09（R1-GUI-12） |
| 前提条件 | MockDaemon が `IpcResponse::Decrypted` を返す |
| 操作 | `decrypt_vault(state, master_password="correct", confirmed=true).await` |
| 期待結果 | `Ok({})` が返る |

### TC-GUI-IPC-IT17: `unlock_vault` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT17 |
| 対応する要件ID | REQ-IPC-10（R1-GUI-13） |
| 前提条件 | MockDaemon が `IpcResponse::Unlocked` を返す |
| 操作 | `unlock_vault(state, master_password="correct").await` |
| 期待結果 | `Ok({})` が返る。送信リクエストの `recovery == None` |

### TC-GUI-IPC-IT18: IO 切断 → AppState が None にリセット

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-IPC-IT18 |
| 対応する要件ID | REQ-IPC-11, REQ-IPC-12（`detailed-design.md §5`） |
| 対応する工程 | 階層 3 基本設計（`detailed-design.md §1.4 round_trip`） |
| 種別 | 異常系（切断復旧） |
| 前提条件 | `AppState = Some(client)` 接続済み。MockDaemon が `round_trip` 中に接続を強制切断 |
| 操作 | 任意の Command（例: `list_entries(state).await`） |
| 期待結果 | `Err(GUIError::ConnectionFailed("..."))` が返る。呼び出し後に `AppState.lock() == None`（リセット確認） |

---

## 7. モック方針まとめ

| テスト対象 | モック要否 | 実装方法 |
|----------|---------|---------|
| UDS / Named Pipe（daemon接続） | **IT で差し替え** | `MockDaemon`（tokio UDS）を `tests/common/mock_daemon.rs` に配置 |
| `OffsetDateTime::now_utc()` | **差し替え不要** | Tauri Command 内部で生成。テストは ID とレスポンス一致を確認 |
| MessagePack エンコード/デコード | **差し替え不要** | 純粋計算。実 `rmp-serde` を通す |
| `AppState` | **UT は不要、IT は直接構築** | `Arc::new(Mutex::new(Some(client)))` / `None` で構築 |

**assumed mock 禁止**: `MockDaemon` が返す `IpcResponse` は `shikomi-core::ipc` の実型を使い、実 MessagePack でシリアライズして送信すること。インライン辞書リテラルや手動バイト列の使用は却下対象。

---

## 8. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-GUI-IPC-UT01〜UT14 | `lint.yml` + 新設 `test-gui.yml` | UDS 不使用のためヘッドレス OK |
| TC-GUI-IPC-IT01〜IT18 | 新設 `test-gui.yml` | tempfile + UDS 使用。Linux/macOS で実行 |
| Windows IT | `windows.yml`（拡張要）| Named Pipe 経路で TC-GUI-IPC-IT01〜IT05 相当を実行（UDS → Named Pipe 切り替え） |

> **`test-gui.yml` 新設要**: `shikomi-gui` 用 CI ワークフローが Sub-A では存在しない。
> Sub-B 実装時に `cargo test -p shikomi-gui` を実行するワークフローを追加すること。
> Tauri ビルド（バイナリビルド）は不要。Rust ライブラリ単体の `cargo test` で十分。

---

## 9. カバレッジ基準

| 観点 | 基準 |
|------|------|
| REQ-IPC 全件網羅 | REQ-IPC-01〜12 全件が IT または UT でカバーされること |
| 正常系 | 全 Command の正常経路（IT）必須 |
| 異常系 | Fail Fast（NotConnected）、validation 失敗（InvalidInput）、daemon エラー透過伝搬（Ipc）を網羅 |
| 境界値 | `Ctrl+Alt+1`（最小）、`Ctrl+Alt+9`（最大）、`Ctrl+Alt+0`（範囲外）を必ず含む |
| Serialize | `GUIError` 全 variant（9 種）のうち IT で直接現れない variant は UT で補完すること |

---

*作成: 涅マユリ（テスト担当）/ 2026-05-11*
*設計根拠: `docs/features/shikomi-gui/ipc-client/basic-design.md` §モジュール契約 / `detailed-design.md` §1〜5 / Issue #95*
