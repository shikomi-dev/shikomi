//! 結合テスト — `GuiIpcClient` 接続・Handshake (TC-GUI-IPC-IT01〜IT03, IT18)。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/test-design.md §6

mod common;

use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest, IpcResponse};
use shikomi_gui::ipc_client::{error::GUIError, exec_with_client, AppState, GuiIpcClient};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// TC-GUI-IPC-IT01: connect() — ソケット不存在 → DaemonNotRunning
// ---------------------------------------------------------------------------

/// daemon が起動していないパス（存在しないソケット）に接続すると `DaemonNotRunning` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it01_connect_no_socket_returns_daemon_not_running() {
    let tmp = tempfile::TempDir::new().unwrap();
    let non_existent = tmp.path().join("no_daemon.sock");
    // ソケットファイルを作成しない
    let result = GuiIpcClient::connect(&non_existent).await;
    assert!(
        matches!(result, Err(GUIError::DaemonNotRunning)),
        "Expected DaemonNotRunning, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// TC-GUI-IPC-IT02: connect() — V2 Handshake 成功 → Ok(GuiIpcClient)
// ---------------------------------------------------------------------------

/// MockDaemon が V2 Handshake を正常処理する場合、接続済み `GuiIpcClient` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it02_connect_v2_handshake_success() {
    let daemon =
        common::mock_daemon::MockDaemon::spawn(IpcResponse::Decrypted).await;
    let result = GuiIpcClient::connect(&daemon.socket_path).await;
    assert!(
        result.is_ok(),
        "V2 Handshake should succeed, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// TC-GUI-IPC-IT03: connect() — プロトコルバージョン不一致 → ProtocolVersionMismatch
// ---------------------------------------------------------------------------

/// MockDaemon が V1 を `server_version` として返す場合、`ProtocolVersionMismatch` を返す。
#[cfg(unix)]
#[tokio::test]
async fn it03_connect_version_mismatch_returns_error() {
    let daemon = common::mock_daemon::MockDaemon::spawn_with_server_version(
        IpcResponse::Decrypted,
        IpcProtocolVersion::V1,
    )
    .await;
    let result = GuiIpcClient::connect(&daemon.socket_path).await;
    assert!(
        matches!(result, Err(GUIError::ProtocolVersionMismatch { .. })),
        "Expected ProtocolVersionMismatch, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// TC-GUI-IPC-IT18: round_trip 中の強制切断 → ConnectionFailed + AppState リセット
// ---------------------------------------------------------------------------

/// MockDaemon が Handshake 後に接続を強制切断した場合、`ConnectionFailed` が返り
/// `exec_with_client` が `AppState` を `None` にリセットする（REQ-IPC-12 §5）。
#[cfg(unix)]
#[tokio::test]
async fn it18_disconnect_resets_app_state_to_none() {
    let daemon = common::mock_daemon::MockDaemon::spawn_disconnect_after_handshake().await;

    let client = GuiIpcClient::connect(&daemon.socket_path)
        .await
        .expect("initial connect should succeed");

    let app_state: AppState = Mutex::new(Some(client));

    // round_trip で切断 → ConnectionFailed
    let result = exec_with_client(&app_state, |client| async move {
        client
            .round_trip(&IpcRequest::ListRecords)
            .await
    })
    .await;

    assert!(
        matches!(result, Err(GUIError::ConnectionFailed(_))),
        "Expected ConnectionFailed after disconnect, got: {result:?}"
    );

    // AppState が None にリセットされていること（REQ-IPC-12 §detailed-design.md §5）
    let guard = app_state.lock().await;
    assert!(
        guard.is_none(),
        "AppState must be reset to None after ConnectionFailed"
    );
}
