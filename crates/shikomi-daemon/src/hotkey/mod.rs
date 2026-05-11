//! グローバルホットキー監視 + クリップボード投入モジュール（Issue #89）。
//!
//! ## モジュール構成
//!
//! - `backend/`: `HotkeyBackend` trait + `BackendEnum` + OS 別実装
//! - `clipboard`: `ClipboardWriter` trait + arboard 実装
//! - `clear_timer`: `ClearTimer`（30 秒自動クリア）
//! - `notifier`: `Notifier` trait + notify-rust 実装
//! - `event_loop`: `HotkeyEventLoop`
//! - `mod.rs`（本ファイル）: `HotkeyManager`
//!
//! 設計根拠: `docs/features/daemon-hotkey-clipboard/daemon/basic-design.md`

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use shikomi_core::{Hotkey, Vault};

use self::backend::{BackendEnum, HotkeyBackend, HotkeyError};
use self::notifier::{Notifier, NotifyLevel};

pub mod backend;
pub mod clear_timer;
pub mod clipboard;
pub mod event_loop;
pub mod notifier;

// -------------------------------------------------------------------
// HotkeyManager
// -------------------------------------------------------------------

/// vault 内の全ホットキーを OS バックエンドに登録・管理する RAII オブジェクト。
///
/// `Drop` 時に全登録済みホットキーを `backend.unregister` で解除する。
///
/// ## スレッド安全性
///
/// `registered` は `std::sync::Mutex` で保護される。`register_one` / `unregister_one`
/// は `&self` で呼び出せる（IPC ハンドラが `Arc<HotkeyManager>` として共有する）。
pub struct HotkeyManager {
    backend: Arc<BackendEnum>,
    registered: Mutex<HashSet<String>>,
    notifier: Arc<dyn Notifier>,
}

impl HotkeyManager {
    /// `HotkeyManager` を構築し、`vault` の全ホットキーエントリを OS に登録する。
    ///
    /// 登録失敗したホットキーは skip し、他は登録継続する（best-effort）。
    #[must_use]
    pub fn new(backend: Arc<BackendEnum>, vault: &Vault, notifier: Arc<dyn Notifier>) -> Self {
        let manager = Self {
            backend,
            registered: Mutex::new(HashSet::new()),
            notifier,
        };
        manager.register_all(vault);
        manager
    }

    /// テスト用: `NullBackend` + 空 vault で構築する（OS API 不使用）。
    ///
    /// `it_server_connection.rs` 等の統合テストが IpcServer のシグネチャを満たすために使う。
    #[doc(hidden)]
    #[must_use]
    pub fn new_null() -> Self {
        use self::backend::NullBackend;
        use self::notifier::NullNotifier;
        Self {
            backend: Arc::new(BackendEnum::Null(NullBackend)),
            registered: Mutex::new(HashSet::new()),
            notifier: Arc::new(NullNotifier),
        }
    }

    /// vault の全ホットキーエントリを OS バックエンドに一括登録する。
    ///
    /// 各エントリの登録失敗は `tracing::error!` + OS 通知でスキップ（他は継続）。
    pub fn register_all(&self, vault: &Vault) {
        for record in vault.hotkey_entries() {
            let Some(hotkey) = record.hotkey() else {
                continue;
            };
            let combo = hotkey.as_str().to_owned();
            self.do_register(combo);
        }
    }

    /// 単一ホットキーを OS に登録する（IPC `add`/`edit` ハンドラから呼ばれる）。
    ///
    /// # Errors
    /// OS 登録失敗 / バックエンド未対応。
    pub fn register_one(&self, combo: &str) -> Result<(), HotkeyError> {
        self.backend.register(combo)?;
        self.registered.lock().unwrap().insert(combo.to_owned());
        Ok(())
    }

    /// 単一ホットキーの OS 登録を解除する（IPC `edit`/`remove` ハンドラから呼ばれる）。
    ///
    /// # Errors
    /// OS 解除失敗 / バックエンド未対応。
    pub fn unregister_one(&self, combo: &str) -> Result<(), HotkeyError> {
        self.backend.unregister(combo)?;
        self.registered.lock().unwrap().remove(combo);
        Ok(())
    }

    /// 内部でホットキーを OS 登録し、結果をログ + 通知する。
    fn do_register(&self, combo: String) {
        match self.backend.register(&combo) {
            Ok(()) => {
                tracing::debug!(combo, "HotkeyManager: registered hotkey");
                self.registered.lock().unwrap().insert(combo);
            }
            Err(e) => {
                let body =
                    format!("ホットキー {combo} の登録に失敗しました。他のアプリと競合している可能性があります");
                tracing::error!(combo, error = %e, "HotkeyManager: failed to register hotkey");
                if let Err(ne) = self.notifier.notify(NotifyLevel::Normal, "shikomi", &body) {
                    tracing::warn!(error = %ne, "HotkeyManager: notification failed");
                }
            }
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        let registered = self.registered.lock().unwrap();
        for combo in registered.iter() {
            if let Err(e) = self.backend.unregister(combo) {
                tracing::warn!(combo, error = %e, "HotkeyManager: unregister failed on drop");
            }
        }
    }
}

// -------------------------------------------------------------------
// ヘルパ: Vault からのホットキー文字列取得
// -------------------------------------------------------------------

/// `Vault` から `Record` のホットキー文字列を取得するヘルパ。
///
/// `Vault::find_record` でレコードを参照し、そのホットキーコンボ文字列を返す。
/// レコード不在 / ホットキー未設定の場合は `None`。
pub fn get_record_hotkey_combo(vault: &Vault, id: &shikomi_core::RecordId) -> Option<String> {
    vault
        .find_record(id)
        .and_then(|r| r.hotkey())
        .map(|h| h.as_str().to_owned())
}

// -------------------------------------------------------------------
// クリップボード初期化ヘルパ
// -------------------------------------------------------------------

/// クリップボードを初期化する。
///
/// `SHIKOMI_DISABLE_CLIPBOARD=1` 環境変数が設定されている場合、または
/// `ArboardClipboardWriter::new()` が失敗した場合は `NullClipboardWriter` を返す。
pub fn init_clipboard() -> Arc<tokio::sync::Mutex<dyn clipboard::ClipboardWriter + Send>> {
    if std::env::var("SHIKOMI_DISABLE_CLIPBOARD").as_deref() == Ok("1") {
        tracing::info!("SHIKOMI_DISABLE_CLIPBOARD=1: clipboard disabled");
        return Arc::new(tokio::sync::Mutex::new(clipboard::NullClipboardWriter));
    }

    match clipboard::ArboardClipboardWriter::new() {
        Ok(writer) => {
            tracing::debug!("clipboard: initialized arboard clipboard");
            Arc::new(tokio::sync::Mutex::new(writer))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "clipboard: arboard init failed, falling back to NullClipboardWriter"
            );
            Arc::new(tokio::sync::Mutex::new(clipboard::NullClipboardWriter))
        }
    }
}
