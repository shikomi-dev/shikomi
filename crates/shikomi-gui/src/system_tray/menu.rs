//! トレイメニュー構築とメニューイベントハンドラ（Sub-D #97）。
//!
//! メニュー項目: ウィンドウを開く / セパレータ / shikomi のサービスを再起動する /
//!             セパレータ / 終了（detailed-design.md §3.1）
//!
//! 設計根拠: docs/features/shikomi-gui/system-tray/basic-design.md §2.3
//!          docs/features/shikomi-gui/system-tray/detailed-design.md §3

use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::{App, AppHandle, Manager as _, Runtime};

/// トレイメニューを構築する。
///
/// | 順序 | ID | ラベル |
/// |------|-----|--------|
/// | 1 | `"open_window"` | 「ウィンドウを開く」 |
/// | 2 | — | セパレータ |
/// | 3 | `"restart_daemon"` | 「shikomi のサービスを再起動する」 |
/// | 4 | — | セパレータ |
/// | 5 | `"quit"` | 「終了」 |
///
/// # Errors
///
/// メニュー構築失敗時は `tauri::Error` を返す。
pub fn build_menu(app: &App) -> tauri::Result<Menu<tauri::Wry>> {
    let open_window = MenuItemBuilder::with_id("open_window", "ウィンドウを開く")
        .build(app)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let restart_daemon =
        MenuItemBuilder::with_id("restart_daemon", "shikomi のサービスを再起動する")
            .build(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", "終了").build(app)?;

    MenuBuilder::new(app)
        .item(&open_window)
        .item(&separator1)
        .item(&restart_daemon)
        .item(&separator2)
        .item(&quit)
        .build()
}

/// トレイメニューイベントを処理する。
///
/// `TrayIconBuilder::on_menu_event` に渡すハンドラ。
/// `message` フィールドを UI に表示しない（単一責務）。
///
/// | ID | 処理 |
/// |----|------|
/// | `"open_window"` | メインウィンドウを表示しフォーカス |
/// | `"restart_daemon"` | daemon を再起動しAppStateをリセット（§3.2） |
/// | `"quit"` | `AppHandle::exit(0)` で終了 |
pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "open_window" => handle_open_window(app),
        "restart_daemon" => handle_restart_daemon(app),
        "quit" => {
            tracing::info!("system_tray: quit requested via tray menu");
            app.exit(0);
        }
        id => {
            tracing::warn!(id, "system_tray: unknown menu item id");
        }
    }
}

// ---------------------------------------------------------------------------
// open_window ハンドラ（§3.2）
// ---------------------------------------------------------------------------

fn handle_open_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("system_tray: main window not found on open_window");
        return;
    };

    if let Err(e) = window.show() {
        tracing::warn!(error = %e, "system_tray: window.show() failed");
        return;
    }
    if let Err(e) = window.set_focus() {
        tracing::warn!(error = %e, "system_tray: window.set_focus() failed");
    }
}

// ---------------------------------------------------------------------------
// restart_daemon ハンドラ（§3.2）
// ---------------------------------------------------------------------------

fn handle_restart_daemon<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_shell::ShellExt as _;

    // AppState を None にリセット（既存接続を切断）
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        {
            let state = app_clone.state::<crate::ipc_client::AppState>();
            *state.lock().await = None;
            tracing::info!("system_tray: AppState reset for daemon restart");
        }

        // shikomi start を spawn（完了を待たない）
        // shell scope: { "name": "shikomi", "cmd": "shikomi", "args": ["^start$"] }
        // 設計根拠: basic-design.md §6.1
        match app_clone.shell().command("shikomi").args(["start"]).spawn() {
            Ok(_) => {
                tracing::info!("system_tray: shikomi start spawned");
            }
            Err(e) => {
                tracing::warn!(error = %e, "system_tray: shikomi start spawn failed");
            }
        }

        // 再接続試行（lib.rs の初期接続ロジックを再利用）
        reconnect_daemon(&app_clone).await;
    });
}

/// daemon への再接続を試みる。
///
/// 接続成功で `AppState` を `Some(client)` に更新する。
/// 接続失敗は `tracing::warn!` のみ（`DaemonConnectionPanel` が UI を担当）。
async fn reconnect_daemon<R: Runtime>(app: &AppHandle<R>) {
    use shikomi_infra::ipc::IpcEndpoint;

    // daemon 起動を少し待つ（best-effort、接続失敗は UI が通知する）
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let socket_path = match IpcEndpoint::default_for_current_user() {
        Err(e) => {
            tracing::warn!("system_tray: failed to resolve daemon socket path: {e}");
            return;
        }
        Ok(p) => p,
    };

    match crate::ipc_client::GuiIpcClient::connect(&socket_path).await {
        Ok(client) => {
            let state = app.state::<crate::ipc_client::AppState>();
            *state.lock().await = Some(client);
            tracing::info!(
                "system_tray: reconnected to daemon at {}",
                socket_path.display()
            );
        }
        Err(e) => {
            tracing::warn!(
                "system_tray: reconnect failed at {}: {}",
                socket_path.display(),
                e
            );
        }
    }
}
