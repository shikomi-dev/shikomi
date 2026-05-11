//! Tauri Commands — IPC ブリッジ層。
//!
//! 全 10 コマンドを再エクスポートする。`lib.rs::run()` の `generate_handler!` に渡す。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md §2.3

pub mod entries;
pub mod hotkey;
pub mod vault;

pub use entries::{add_entry, delete_entry, list_entries, update_entry};
pub use hotkey::{assign_hotkey, remove_hotkey};
pub use vault::{decrypt_vault, encrypt_vault, get_vault_status, unlock_vault};
