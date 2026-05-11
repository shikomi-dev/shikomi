//! テスト用 `MockNotifier`。
//!
//! `Notifier` trait のテスト用実装。送信された通知の履歴を `Mutex<Vec>` に記録する。
//! `#[cfg(test)]` ガードを持たず `tests/` 配下に物理分離して配置（本番コードへの混入禁止）。
//!
//! 設計根拠: `docs/features/daemon-hotkey-clipboard/daemon/test-design.md §3`

use shikomi_daemon::hotkey::notifier::{Notifier, NotifyError, NotifyLevel};

/// テスト用の記録型 Notifier。送信履歴を `Vec` に蓄積する。
#[derive(Default)]
pub struct MockNotifier {
    records: std::sync::Mutex<Vec<(NotifyLevel, String, String)>>,
}

impl MockNotifier {
    /// 空の `MockNotifier` を構築する。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 送信された通知の履歴を返す（`(level, title, body)` のリスト）。
    #[must_use]
    pub fn notifications(&self) -> Vec<(NotifyLevel, String, String)> {
        self.records.lock().unwrap().clone()
    }

    /// 通知が送信された件数を返す。
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.lock().unwrap().len()
    }

    /// 通知履歴が空かどうかを返す。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.lock().unwrap().is_empty()
    }
}

impl Notifier for MockNotifier {
    fn notify(&self, level: NotifyLevel, title: &str, body: &str) -> Result<(), NotifyError> {
        self.records
            .lock()
            .unwrap()
            .push((level, title.to_owned(), body.to_owned()));
        Ok(())
    }
}
