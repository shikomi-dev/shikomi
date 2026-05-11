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

use shikomi_core::Vault;

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

// -------------------------------------------------------------------
// ユニットテスト（TC-HD-DU01〜DU03）
// -------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shikomi_core::secret::SecretString;
    use shikomi_core::{
        Hotkey, Record, RecordId, RecordKind, RecordLabel, RecordPayload, Vault, VaultHeader,
        VaultVersion,
    };
    use time::OffsetDateTime;
    use uuid::Uuid;

    use super::*;
    use crate::hotkey::backend::{BackendEnum, MockBackend};
    use crate::hotkey::notifier::NullNotifier;

    fn fixed_now() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
    }

    fn make_id() -> RecordId {
        RecordId::new(Uuid::now_v7()).unwrap()
    }

    fn empty_vault() -> Vault {
        let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_now()).unwrap();
        Vault::new(header)
    }

    fn vault_with_hotkeys(combos: &[&str]) -> Vault {
        let mut vault = empty_vault();
        for (i, combo) in combos.iter().enumerate() {
            let id = make_id();
            let label = RecordLabel::try_new(format!("label-{i}")).unwrap();
            let payload = RecordPayload::Plaintext(SecretString::from_string(format!("val-{i}")));
            let record = Record::new(id.clone(), RecordKind::Text, label, payload, fixed_now());
            vault.add_record(record).unwrap();
            vault
                .assign_hotkey(&id, Hotkey::parse(combo).unwrap())
                .unwrap();
        }
        vault
    }

    fn make_backend_mock() -> (
        Arc<BackendEnum>,
        tokio::sync::mpsc::Sender<crate::hotkey::backend::HotkeyEvent>,
    ) {
        let (mock, sender) = MockBackend::new_with_sender();
        (Arc::new(BackendEnum::Mock(mock)), sender)
    }

    fn get_mock_ref(backend: &BackendEnum) -> &MockBackend {
        match backend {
            BackendEnum::Mock(m) => m,
            _ => panic!("expected MockBackend"),
        }
    }

    // ── TC-HD-DU01-a: register_all が vault エントリを全件登録する ──────

    #[test]
    fn tc_hd_du01_a_register_all_registers_all_entries() {
        let vault = vault_with_hotkeys(&["ctrl+alt+1", "ctrl+alt+2"]);
        let (backend, _sender) = make_backend_mock();
        let notifier = Arc::new(NullNotifier);
        let _manager = HotkeyManager::new(Arc::clone(&backend), &vault, notifier);

        let registered = get_mock_ref(&backend).registered();
        assert!(registered.contains("alt+ctrl+1"));
        assert!(registered.contains("alt+ctrl+2"));
    }

    // ── TC-HD-DU01-b: 1 件が失敗しても他は登録される ───────────────────

    #[test]
    fn tc_hd_du01_b_register_all_skips_failed_entry() {
        let vault = vault_with_hotkeys(&["ctrl+alt+1", "ctrl+alt+2"]);
        let (mock, _sender) = MockBackend::new_with_sender();
        mock.set_fail_on_register("alt+ctrl+1");
        let backend = Arc::new(BackendEnum::Mock(mock));
        let notifier = Arc::new(NullNotifier);
        let _manager = HotkeyManager::new(Arc::clone(&backend), &vault, notifier);

        let registered = get_mock_ref(&backend).registered();
        assert!(
            !registered.contains("alt+ctrl+1"),
            "failed combo should not be registered"
        );
        assert!(
            registered.contains("alt+ctrl+2"),
            "successful combo should be registered"
        );
    }

    // ── TC-HD-DU02: register_one / unregister_one ─────────────────────

    #[test]
    fn tc_hd_du02_a_register_one_adds_combo() {
        let (backend, _sender) = make_backend_mock();
        let notifier = Arc::new(NullNotifier);
        let vault = empty_vault();
        let manager = HotkeyManager::new(Arc::clone(&backend), &vault, notifier);

        manager.register_one("alt+ctrl+1").unwrap();
        assert!(get_mock_ref(&backend).registered().contains("alt+ctrl+1"));
    }

    #[test]
    fn tc_hd_du02_b_unregister_one_removes_combo() {
        let (backend, _sender) = make_backend_mock();
        let notifier = Arc::new(NullNotifier);
        let vault = empty_vault();
        let manager = HotkeyManager::new(Arc::clone(&backend), &vault, notifier);

        manager.register_one("alt+ctrl+1").unwrap();
        manager.unregister_one("alt+ctrl+1").unwrap();
        assert!(!get_mock_ref(&backend).registered().contains("alt+ctrl+1"));
    }

    #[test]
    fn tc_hd_du02_c_unregister_one_of_unregistered_is_ok() {
        let (backend, _sender) = make_backend_mock();
        let notifier = Arc::new(NullNotifier);
        let vault = empty_vault();
        let manager = HotkeyManager::new(Arc::clone(&backend), &vault, notifier);

        // 登録されていない combo を unregister_one
        let result = manager.unregister_one("alt+ctrl+9");
        assert!(
            result.is_ok(),
            "unregister_one of unregistered combo should be Ok"
        );
    }

    // ── TC-HD-DU03: Drop が全コンボを解除する ─────────────────────────

    #[test]
    fn tc_hd_du03_drop_unregisters_all() {
        let vault = vault_with_hotkeys(&["ctrl+alt+1", "ctrl+alt+2"]);
        let (backend, _sender) = make_backend_mock();
        let notifier = Arc::new(NullNotifier);
        let manager = HotkeyManager::new(Arc::clone(&backend), &vault, notifier);

        assert_eq!(get_mock_ref(&backend).registered().len(), 2);
        drop(manager);
        assert!(
            get_mock_ref(&backend).registered().is_empty(),
            "all combos should be unregistered after drop"
        );
    }
}
