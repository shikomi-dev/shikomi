//! 結合テスト — system-tray get_clipboard_countdown（TC-TRAY-IT01〜IT06）。
//!
//! 設計根拠: docs/features/shikomi-gui/system-tray/test-design.md §6
//!          docs/features/shikomi-gui/system-tray/basic-design.md §3.3
//!          docs/features/shikomi-gui/system-tray/detailed-design.md §5
//!
//! ## テストケース一覧
//!
//! | TC | 内容 |
//! |---|---|
//! | IT01 | AppState=None → Ok(remaining_secs: None)（daemon 未接続サイレントフォールバック） |
//! | IT02 | MockDaemon ClipboardStatus { Some(20) } → Ok(remaining_secs: Some(20)) |
//! | IT03 | MockDaemon ClipboardStatus { None } → Ok(remaining_secs: None) |
//! | IT04 | MockDaemon 接続切断（IPC エラー）→ Ok(remaining_secs: None)（エラー非伝搬） |
//! | IT05 | remaining_secs: Some(15) → JSON シリアライズ `{ "remaining_secs": 15 }`（数値） |
//! | IT06 | remaining_secs: None → JSON シリアライズ `{ "remaining_secs": null }` |

mod common;

use shikomi_core::ipc::IpcResponse;
use shikomi_gui::ipc_client::{commands::tray::get_clipboard_countdown, AppState, GuiIpcClient};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// ヘルパ
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn build_none_app() -> tauri::App<tauri::test::MockRuntime> {
    mock_builder()
        .manage(Mutex::new(None::<GuiIpcClient>) as AppState)
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app with None AppState")
}

#[cfg(unix)]
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
// TC-TRAY-IT01: AppState=None → Ok(remaining_secs: None)（サイレントフォールバック）
// ---------------------------------------------------------------------------

/// daemon 未接続（`AppState = None`）の場合、`get_clipboard_countdown` は
/// `GUIError` を返さず `Ok(remaining_secs: None)` を返す。
///
/// 設計根拠: detailed-design.md §5.2（countdown polling がエラーパネルを誘発しない）
#[cfg(unix)]
#[tokio::test]
async fn it01_get_clipboard_countdown_not_connected_returns_none() {
    let app = build_none_app();
    let state = app.state::<AppState>();
    let result = get_clipboard_countdown(state).await;

    // GUIError を返さない（Ok の確認）
    let output = result.expect("get_clipboard_countdown should return Ok even when not connected");
    // remaining_secs は None（カウントダウン非アクティブ扱い）
    assert_eq!(output.remaining_secs, None);
}

// ---------------------------------------------------------------------------
// TC-TRAY-IT02: MockDaemon ClipboardStatus { Some(20) } → Ok(remaining_secs: Some(20))
// ---------------------------------------------------------------------------

/// MockDaemon が `ClipboardStatus { remaining_secs: Some(20) }` を返すとき、
/// `get_clipboard_countdown` は `Ok(ClipboardCountdownResult { remaining_secs: Some(20) })` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it02_get_clipboard_countdown_active_remaining_20() {
    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::ClipboardStatus {
        remaining_secs: Some(20),
    })
    .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();

    let result = get_clipboard_countdown(state).await;

    let output = result.expect("get_clipboard_countdown should succeed");
    assert_eq!(output.remaining_secs, Some(20));
}

// ---------------------------------------------------------------------------
// TC-TRAY-IT03: MockDaemon ClipboardStatus { None } → Ok(remaining_secs: None)
// ---------------------------------------------------------------------------

/// MockDaemon が `ClipboardStatus { remaining_secs: None }` を返すとき、
/// `get_clipboard_countdown` は `Ok(ClipboardCountdownResult { remaining_secs: None })` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it03_get_clipboard_countdown_inactive_none() {
    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::ClipboardStatus {
        remaining_secs: None,
    })
    .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();

    let result = get_clipboard_countdown(state).await;

    let output = result.expect("get_clipboard_countdown should succeed");
    assert_eq!(output.remaining_secs, None);
}

// ---------------------------------------------------------------------------
// TC-TRAY-IT04: IPC エラー（接続切断）→ Ok(remaining_secs: None)（エラー非伝搬）
// ---------------------------------------------------------------------------

/// MockDaemon が Handshake 後に接続を強制切断した場合、
/// `get_clipboard_countdown` は `Err` を返さず `Ok(remaining_secs: None)` を返す。
///
/// 設計根拠: detailed-design.md §4.2（IPC エラー時は `tracing::debug!` のみ、エラー非伝搬）
#[cfg(unix)]
#[tokio::test]
async fn it04_get_clipboard_countdown_ipc_error_silent_fallback() {
    let daemon = common::mock_daemon::MockDaemon::spawn_disconnect_after_handshake().await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();

    let result = get_clipboard_countdown(state).await;

    // IPC エラーでも Ok を返す（countdown ポーリングがエラーパネルを誘発しない）
    let output = result.expect("get_clipboard_countdown should return Ok even on IPC error");
    assert_eq!(output.remaining_secs, None);
}

// ---------------------------------------------------------------------------
// TC-TRAY-IT05: remaining_secs: Some(15) → JSON `{ "remaining_secs": 15 }`
// ---------------------------------------------------------------------------

/// `remaining_secs: Some(15)` の場合、JSON シリアライズで `{ "remaining_secs": 15 }`（数値）となる。
///
/// 設計根拠: detailed-design.md §5.1 シリアライズ契約 / §9 SolidJS ペイロード型凍結
#[cfg(unix)]
#[tokio::test]
async fn it05_serialize_remaining_secs_some_is_number() {
    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::ClipboardStatus {
        remaining_secs: Some(15),
    })
    .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();

    let output = get_clipboard_countdown(state)
        .await
        .expect("get_clipboard_countdown should succeed");

    let json = serde_json::to_value(&output).expect("serialization should succeed");
    // SolidJS 側が受け取る型: `remaining_secs` は number（null ではない）
    assert_eq!(json["remaining_secs"], serde_json::json!(15u64));
}

// ---------------------------------------------------------------------------
// TC-TRAY-IT06: remaining_secs: None → JSON `{ "remaining_secs": null }`
// ---------------------------------------------------------------------------

/// `remaining_secs: None` の場合、JSON シリアライズで `{ "remaining_secs": null }` となる。
///
/// 設計根拠: detailed-design.md §5.1 シリアライズ契約 / §9 SolidJS ペイロード型凍結
#[cfg(unix)]
#[tokio::test]
async fn it06_serialize_remaining_secs_none_is_null() {
    let daemon = common::mock_daemon::MockDaemon::spawn(IpcResponse::ClipboardStatus {
        remaining_secs: None,
    })
    .await;
    let app = build_connected_app(&daemon.socket_path).await;
    let state = app.state::<AppState>();

    let output = get_clipboard_countdown(state)
        .await
        .expect("get_clipboard_countdown should succeed");

    let json = serde_json::to_value(&output).expect("serialization should succeed");
    // SolidJS 側が受け取る型: `remaining_secs` は null（数値ではない）
    assert_eq!(json["remaining_secs"], serde_json::Value::Null);
}
