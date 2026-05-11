//! システムトレイ初期化（Sub-D #97）。
//!
//! `setup()` はアプリ起動時に一度だけ呼ばれ、以下を行う:
//! - `TrayIconBuilder` でトレイアイコン + ツールチップ + メニューを生成（REQ-TRAY-01）
//! - ウィンドウ close-to-tray ハンドラを登録（REQ-TRAY-02）
//! - `countdown::run` タスクを spawn（REQ-TRAY-05）
//!
//! 設計根拠: docs/features/shikomi-gui/system-tray/basic-design.md §2.1
//!          docs/features/shikomi-gui/system-tray/detailed-design.md §1

pub(crate) mod countdown;
pub(crate) mod menu;

use tauri::tray::TrayIconBuilder;
use tauri::{App, Manager as _};

/// システムトレイをセットアップする。
///
/// `lib.rs::run()` の `.setup()` フック内から呼び出す。
///
/// # Errors
///
/// `TrayIconBuilder::build` 失敗時に `tauri::Error` を返す（Fail Fast）。
/// トレイアイコン生成失敗はアプリ起動を中断する（REQ-TRAY-01）。
pub fn setup(app: &mut App) -> tauri::Result<()> {
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".to_owned()))?;

    let tray_menu = menu::build_menu(app)?;

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip("shikomi")
        .menu(&tray_menu)
        .on_menu_event(menu::handle_menu_event)
        .build(app)?;

    // close-to-tray ハンドラ登録（REQ-TRAY-02）
    register_close_to_tray_handler(app);

    // countdown ポーリングタスク起動（REQ-TRAY-05）
    let app_handle = app.handle().clone();
    let tray_id = tray.id().clone();
    tauri::async_runtime::spawn(countdown::run(app_handle, tray_id));

    Ok(())
}

/// ウィンドウ「×」ボタンをトレイ常駐にリダイレクトするハンドラを登録する（REQ-TRAY-02）。
///
/// `CloseRequested` を `prevent_default()` して `hide()` する。
/// `AppHandle::exit(0)` はトレイメニューの「終了」のみが呼ぶ唯一の終了経路（§2.2）。
fn register_close_to_tray_handler(app: &App) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("system_tray::setup: main window not found; close-to-tray not registered");
        return;
    };

    let window_clone = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_default();
            if let Err(e) = window_clone.hide() {
                tracing::warn!(error = %e, "system_tray: window.hide() failed");
            }
        }
    });
}
