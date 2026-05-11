//! 結合テスト — Tauri Commands IPC (TC-GUI-IPC-IT04〜IT17)。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/test-design.md §6
//!
//! # UT09 設計書不整合（バグ報告 #GUI-BUG-001）
//!
//! test-design.md UT09 は「update_entry(label=None, value=None) → IPC 省略」と記述するが、
//! 実装は B案（Tell, Don't Ask）を採用し、常に IPC を送信する（entries.rs コメント参照）。
//! IT09 はこの B案 正常系を検証し、IPC が実際に送信されることを確認する。

mod common;

use shikomi_core::ipc::{IpcErrorCode, IpcRequest, IpcResponse, ProtectionModeBanner};
use shikomi_core::{RecordId, RecordKind, SecretBytes};
use shikomi_gui::ipc_client::{
    commands::{
        add_entry, assign_hotkey, decrypt_vault, delete_entry, encrypt_vault, get_vault_status,
        list_entries, remove_hotkey, unlock_vault, update_entry,
    },
    error::GUIError,
    AppState, GuiIpcClient,
};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;
use tokio::sync::Mutex;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ヘルパ
// ---------------------------------------------------------------------------

fn new_valid_record_id() -> RecordId {
    RecordId::new(Uuid::now_v7()).unwrap()
}

fn build_none_app() -> tauri::App<tauri::test::MockRuntime> {
    mock_builder()
        .manage(Mutex::new(None::<GuiIpcClient>) as AppState)
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app with None state")
}

async fn build_connected_app(
    socket_path: &std::path::Path,
) -> tauri::App<tauri::test::MockRuntime> {
    let client = GuiIpcClient::connect(socket_path)
        .await
        .expect("GuiIpcClient::connect failed in test setup");
    mock_builder()
        .manage(Mutex::new(Some(client)) as AppState)
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app with connected client")
}

// ---------------------------------------------------------------------------
// IT04: list_entries — NotConnected (AppState = None)
// ---------------------------------------------------------------------------

/// `AppState = None` の状態で `list_entries` を呼ぶと `GUIError::NotConnected`。
#[cfg(unix)]
#[tokio::test]
async fn it04_list_entries_not_connected() {
    let app = build_none_app();
    let state = app.state::<AppState>();
    let result = list_entries(state).await;
    assert!(
        matches!(result, Err(GUIError::NotConnected)),
        "Expected NotConnected, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// IT05: add_entry — NotConnected (AppState = None, valid inputs)
// ---------------------------------------------------------------------------

/// 有効な入力でも `AppState = None` なら `GUIError::NotConnected`。
#[cfg(unix)]
#[tokio::test]
async fn it05_add_entry_not_connected() {
    let app = build_none_app();
    let state = app.state::<AppState>();
    let result = add_entry(
        state,
        RecordKind::Text,
        "valid-label".to_owned(),
        "valid-value".to_owned(),
        None,
    )
    .await;
    assert!(
        matches!(result, Err(GUIError::NotConnected)),
        "Expected NotConnected, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// IT06: list_entries 正常系
// ---------------------------------------------------------------------------

/// MockDaemon が `Records { records: [], protection_mode: Plaintext }` を返すとき
/// `list_entries` は `Ok(ListEntriesOutput { entries: [], vault_status: Plaintext })` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it06_list_entries_success() {
    use shikomi_gui::ipc_client::commands::entries::ListEntriesOutput;

    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::Records {
        records: vec![],
        protection_mode: ProtectionModeBanner::Plaintext,
    })
    .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = list_entries(state).await;

    let output = result.expect("list_entries should succeed");
    assert_eq!(output.vault_status, ProtectionModeBanner::Plaintext);
    assert!(output.entries.is_empty());
}

// ---------------------------------------------------------------------------
// IT07: add_entry 正常系
// ---------------------------------------------------------------------------

/// MockDaemon が `Added { id }` を返すとき `add_entry` は `Ok(EntryIdOutput { id })` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it07_add_entry_success() {
    let record_id = new_valid_record_id();
    let expected_id = record_id.to_string();

    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::Added { id: record_id }).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = add_entry(
        state,
        RecordKind::Text,
        "test-label".to_owned(),
        "test-value".to_owned(),
        None,
    )
    .await;

    let output = result.expect("add_entry should succeed");
    assert_eq!(output.id, expected_id);
}

// ---------------------------------------------------------------------------
// IT08: add_entry — daemon が HotkeyConflict を返す
// ---------------------------------------------------------------------------

/// MockDaemon が `Error(HotkeyConflict)` を返すとき `add_entry` は `GUIError::Ipc(HotkeyConflict)` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it08_add_entry_hotkey_conflict() {
    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Error(IpcErrorCode::HotkeyConflict {
            reason: "hotkey conflict".to_owned(),
        }))
        .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = add_entry(
        state,
        RecordKind::Text,
        "new-label".to_owned(),
        "new-value".to_owned(),
        Some("Ctrl+Alt+1".to_owned()),
    )
    .await;

    assert!(
        matches!(
            result,
            Err(GUIError::Ipc(IpcErrorCode::HotkeyConflict { .. }))
        ),
        "Expected Ipc(HotkeyConflict), got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// IT09: update_entry 全フィールド None — B案: IPC は送信される（test-design UT09 修正版）
//
// BUG REPORT #GUI-BUG-001: test-design.md UT09 は「IPC 省略」と記述するが、
// 実装は B案（必ず IPC 送信）を採用。本テストは正しい B案 挙動を検証する。
// ---------------------------------------------------------------------------

/// `update_entry(label=None, value=None)` でも IPC は送信される（B案: Tell, Don't Ask）。
/// MockDaemon が `Edited { id }` を返せば `Ok(EntryIdOutput { id })` になる。
#[cfg(unix)]
#[tokio::test]
async fn it09_update_entry_all_none_sends_ipc_b_plan() {
    let record_id = new_valid_record_id();
    let expected_id = record_id.to_string();
    let uuid_str = expected_id.clone();

    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Edited { id: record_id }).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = update_entry(state, uuid_str, None, None).await;

    let output = result.expect("update_entry with all-None should succeed (B案: always IPC)");
    assert_eq!(output.id, expected_id);

    // MockDaemon が EditRecord を受信したことを確認（IPC が実際に送信された証拠）
    let received = daemon
        .received_request
        .await
        .expect("MockDaemon should have received a request");
    assert!(
        matches!(
            received,
            IpcRequest::EditRecord {
                label: None,
                value: None,
                ..
            }
        ),
        "EditRecord with label=None, value=None should be sent: {received:?}"
    );
}

// ---------------------------------------------------------------------------
// IT10: delete_entry 正常系
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn it10_delete_entry_success() {
    let record_id = new_valid_record_id();
    let expected_id = record_id.to_string();
    let uuid_str = expected_id.clone();

    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Removed { id: record_id }).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = delete_entry(state, uuid_str).await;

    let output = result.expect("delete_entry should succeed");
    assert_eq!(output.id, expected_id);
}

// ---------------------------------------------------------------------------
// IT11: assign_hotkey 正常系 + リクエスト内容検証
// ---------------------------------------------------------------------------

/// `assign_hotkey(combo="Ctrl+Alt+3")` が `EditRecord { hotkey: Some("Ctrl+Alt+3"), clear_hotkey: false }` を送信する。
#[cfg(unix)]
#[tokio::test]
async fn it11_assign_hotkey_success_and_verify_request() {
    let record_id = new_valid_record_id();
    let expected_id = record_id.to_string();
    let uuid_str = expected_id.clone();

    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Edited { id: record_id }).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = assign_hotkey(state, uuid_str, "Ctrl+Alt+3".to_owned()).await;

    let output = result.expect("assign_hotkey should succeed");
    assert_eq!(output.id, expected_id);

    // daemon が受信したリクエストの内容を検証（REQ-IPC-05: hotkey=Some("Ctrl+Alt+3"), clear_hotkey=false）
    let received = daemon
        .received_request
        .await
        .expect("should receive request");
    assert!(
        matches!(
            &received,
            IpcRequest::EditRecord {
                hotkey: Some(combo),
                clear_hotkey: false,
                ..
            } if combo == "Ctrl+Alt+3"
        ),
        "EditRecord must have hotkey=Some(\"Ctrl+Alt+3\"), clear_hotkey=false: {received:?}"
    );
}

// ---------------------------------------------------------------------------
// IT12: assign_hotkey — HotkeyConflict
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn it12_assign_hotkey_conflict() {
    let record_id = new_valid_record_id();
    let uuid_str = record_id.to_string();

    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Error(IpcErrorCode::HotkeyConflict {
            reason: "hotkey conflict".to_owned(),
        }))
        .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = assign_hotkey(state, uuid_str, "Ctrl+Alt+5".to_owned()).await;

    assert!(
        matches!(
            result,
            Err(GUIError::Ipc(IpcErrorCode::HotkeyConflict { .. }))
        ),
        "Expected Ipc(HotkeyConflict), got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// IT13: remove_hotkey 正常系 + clear_hotkey=true 検証
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn it13_remove_hotkey_success_and_verify_clear_hotkey() {
    let record_id = new_valid_record_id();
    let expected_id = record_id.to_string();
    let uuid_str = expected_id.clone();

    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Edited { id: record_id }).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = remove_hotkey(state, uuid_str).await;

    let output = result.expect("remove_hotkey should succeed");
    assert_eq!(output.id, expected_id);

    // daemon 送信リクエストの clear_hotkey=true を確認（REQ-IPC-06）
    let received = daemon
        .received_request
        .await
        .expect("should receive request");
    assert!(
        matches!(
            received,
            IpcRequest::EditRecord {
                clear_hotkey: true,
                ..
            }
        ),
        "EditRecord must have clear_hotkey=true: {received:?}"
    );
}

// ---------------------------------------------------------------------------
// IT14: get_vault_status — protection_mode のみ返却（records は含まれない）
// ---------------------------------------------------------------------------

/// `get_vault_status` は `Records { records: [2件], protection_mode: EncryptedLocked }` を受け取るが
/// 返却値には `protection_mode` のみ含まれる（R1-GUI-13）。
#[cfg(unix)]
#[tokio::test]
async fn it14_get_vault_status_returns_only_protection_mode() {
    use shikomi_core::ipc::RecordSummary;

    let record_id = new_valid_record_id();
    let dummy_summary = RecordSummary {
        id: record_id,
        kind: RecordKind::Text,
        label: shikomi_core::RecordLabel::try_new("dummy".to_owned()).unwrap(),
        value_preview: Some("preview".to_owned()),
        value_masked: false,
        hotkey: None,
    };

    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::Records {
        records: vec![dummy_summary],
        protection_mode: ProtectionModeBanner::EncryptedLocked,
    })
    .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = get_vault_status(state).await;

    let output = result.expect("get_vault_status should succeed");
    assert_eq!(output.vault_status, ProtectionModeBanner::EncryptedLocked);
    // VaultStatusOutput には entries フィールドが存在しない（型レベルで保証）
    // → コンパイル成功自体が R1-GUI-13 を検証する
}

// ---------------------------------------------------------------------------
// IT15: encrypt_vault — disclosure 24 語
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn it15_encrypt_vault_disclosure_24_words() {
    use shikomi_core::ipc::SerializableSecretBytes;

    let disclosure: Vec<SerializableSecretBytes> = (0..24)
        .map(|i| {
            let word = format!("word{i:02}");
            SerializableSecretBytes::new(SecretBytes::from_vec(word.into_bytes()))
        })
        .collect();

    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Encrypted { disclosure }).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = encrypt_vault(state, "StrongPass123!".to_owned()).await;

    let output = result.expect("encrypt_vault should succeed");
    assert_eq!(
        output.disclosure.len(),
        24,
        "disclosure must contain exactly 24 words"
    );
}

// ---------------------------------------------------------------------------
// IT16: decrypt_vault — confirmed=true 正常系
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn it16_decrypt_vault_confirmed_true_success() {
    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::Decrypted).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = decrypt_vault(state, "correct-password".to_owned(), true).await;

    result.expect("decrypt_vault(confirmed=true) should succeed");
}

// ---------------------------------------------------------------------------
// IT17: unlock_vault — recovery=None 検証
// ---------------------------------------------------------------------------

/// `unlock_vault` が `Unlock { recovery: None }` を送信することを検証する（REQ-IPC-10）。
#[cfg(unix)]
#[tokio::test]
async fn it17_unlock_vault_success_and_verify_recovery_none() {
    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::Unlocked).await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();
    let result = unlock_vault(state, "correct-password".to_owned()).await;

    result.expect("unlock_vault should succeed");

    // daemon 送信リクエストの recovery=None を確認（REQ-IPC-10: password 経路のみ）
    let received = daemon
        .received_request
        .await
        .expect("should receive request");
    assert!(
        matches!(received, IpcRequest::Unlock { recovery: None, .. }),
        "Unlock request must have recovery=None: {received:?}"
    );
}
