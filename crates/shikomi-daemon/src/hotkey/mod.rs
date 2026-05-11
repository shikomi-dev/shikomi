//! グローバルホットキー監視 + クリップボード投入モジュール（Issue #89）。
//!
//! ## モジュール構成
//!
//! - `backend/`: `HotkeyBackend` trait + `BackendEnum` + OS 別実装
//! - `clipboard`: `ClipboardWriter` trait + arboard 実装（`ArboardClipboardWriter::init_shared` を含む）
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
    #[cfg(any(test, feature = "test-fixtures"))]
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
    /// `combo` は `Hotkey::parse` で正規化したうえで OS に登録する（P1-③ / H-003）。
    /// 正規化により `"ctrl+alt+1"` と `"alt+ctrl+1"` は同一コンボとして扱われる。
    ///
    /// # Errors
    /// コンボ文字列解析失敗 / OS 登録失敗 / バックエンド未対応。
    pub fn register_one(&self, combo: &str) -> Result<(), HotkeyError> {
        let hotkey = shikomi_core::Hotkey::parse(combo)
            .map_err(|_| HotkeyError::ParseFailed { combo: combo.to_owned() })?;
        let normalized = hotkey.as_str().to_owned();
        self.backend.register(&normalized)?;
        self.registered.lock().unwrap().insert(normalized);
        Ok(())
    }

    /// 単一ホットキーの OS 登録を解除する（IPC `edit`/`remove` ハンドラから呼ばれる）。
    ///
    /// `combo` は `Hotkey::parse` で正規化したうえで OS 解除する（P1-③ / H-003）。
    ///
    /// # Errors
    /// コンボ文字列解析失敗 / OS 解除失敗 / バックエンド未対応。
    pub fn unregister_one(&self, combo: &str) -> Result<(), HotkeyError> {
        let hotkey = shikomi_core::Hotkey::parse(combo)
            .map_err(|_| HotkeyError::ParseFailed { combo: combo.to_owned() })?;
        let normalized = hotkey.as_str().to_owned();
        self.backend.unregister(&normalized)?;
        self.registered.lock().unwrap().remove(&normalized);
        Ok(())
    }

    /// IPC `edit` / `add` 後の OS ホットキー状態を同期する（Tell, Don't Ask / P1-②）。
    ///
    /// - `clear == true`: `old_combo` を OS 解除して終了（`new_combo` は無視）
    /// - `new_combo.is_some()` && `!clear`:
    ///   - 正規化後 `old_combo != new_combo` の場合のみ旧コンボを解除（best-effort）
    ///   - 新コンボを OS 登録（Fail Fast: 失敗時は `Err` を返す）
    /// - それ以外: noop
    ///
    /// unregister 失敗は `tracing::warn!` のみ（OS が既に解除済みでも問題ない）。
    /// register 失敗は `Err` を返す（Fail Fast）。
    ///
    /// # Errors
    /// `clear == false` かつ `new_combo.is_some()` のとき OS 登録失敗。
    pub fn sync_hotkey(
        &self,
        old_combo: Option<&str>,
        new_combo: Option<&str>,
        clear: bool,
    ) -> Result<(), HotkeyError> {
        if clear {
            if let Some(old) = old_combo {
                if let Err(e) = self.unregister_one(old) {
                    tracing::warn!(combo = old, error = %e, "sync_hotkey: unregister_one on clear");
                }
            }
            return Ok(());
        }

        if let Some(new_raw) = new_combo {
            // 正規化後の新コンボ（比較用）
            let new_normalized = shikomi_core::Hotkey::parse(new_raw)
                .map(|h| h.as_str().to_owned())
                .unwrap_or_else(|_| new_raw.to_owned());

            // 正規化後に同一コンボなら変更なし（OS 再登録を防ぐ）
            if old_combo.is_some_and(|old| old == new_normalized) {
                return Ok(());
            }

            // 旧コンボ解除（best-effort）
            if let Some(old) = old_combo {
                if let Err(e) = self.unregister_one(old) {
                    tracing::warn!(combo = old, error = %e, "sync_hotkey: unregister_one old");
                }
            }

            // 新コンボ登録（Fail Fast）
            self.register_one(new_raw)?;
        }

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
