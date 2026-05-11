//! shikomi-gui — GUI フロントエンド層（Tauri v2 + SolidJS）。
//!
//! ## モジュール構成
//!
//! - `ipc_client`: Tauri Commands ハンドラ群（Sub-B: IPC ブリッジ実装）
//! - `ipc_client::error`: `GUIError` enum（SolidJS への統一エラー型）
//!
//! ## 起動経路
//!
//! - バイナリ `shikomi-gui`: `main.rs` から `run()` を直接呼び出す
//! - `shikomi gui` CLI: `shikomi-gui` バイナリを `std::process::Command` で起動する
//!
//! 設計根拠: `docs/features/shikomi-gui/feature-spec.md` R1-GUI-01
//! `docs/architecture/tech-stack.md §2.6`

pub mod ipc_client;

use ipc_client::{
    commands::{
        add_entry, assign_hotkey, decrypt_vault, delete_entry, encrypt_vault, get_vault_status,
        list_entries, remove_hotkey, unlock_vault, update_entry,
    },
    AppState, GuiIpcClient,
};
use shikomi_infra::ipc::IpcEndpoint;

/// Tauri アプリケーションを起動する。
///
/// `shikomi-gui` バイナリの `main` および将来の CLI 直接呼び出し経路から使用する。
///
/// # Errors
///
/// Tauri ランタイムの初期化または起動に失敗した場合に `tauri::Error` を返す。
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(|app| {
            // AppState を初期化（None = daemon 未接続）
            app.manage::<AppState>(tokio::sync::Mutex::new(None));

            // daemon への初期接続（R1-GUI-02 / R1-GUI-03）
            // 接続失敗は UI パネルで通知し None のまま保持する（R1-GUI-03）
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match IpcEndpoint::default_for_current_user() {
                    Err(e) => {
                        tracing::warn!("failed to resolve daemon socket path: {e}");
                    }
                    Ok(socket_path) => match GuiIpcClient::connect(&socket_path).await {
                        Ok(client) => {
                            use tauri::Manager as _;
                            let state = app_handle.state::<AppState>();
                            *state.lock().await = Some(client);
                            tracing::info!("connected to daemon at {}", socket_path.display());
                        }
                        Err(e) => {
                            tracing::warn!(
                                "failed to connect to daemon at {}: {}",
                                socket_path.display(),
                                e
                            );
                        }
                    },
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_entries,
            add_entry,
            update_entry,
            delete_entry,
            assign_hotkey,
            remove_hotkey,
            get_vault_status,
            encrypt_vault,
            decrypt_vault,
            unlock_vault,
        ])
        .run(tauri::generate_context!())
}
