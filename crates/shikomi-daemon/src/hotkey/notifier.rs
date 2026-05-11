//! OS 通知送信の抽象インターフェース（R1-HK-13 / R1-HK-14）。
//!
//! `Notifier` trait によりテスト時に `MockNotifier` に差し替え可能。

// -------------------------------------------------------------------
// NotifyLevel
// -------------------------------------------------------------------

/// 通知の緊急度レベル。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLevel {
    /// 低優先度（情報通知）。vault ロック中などの情報提供。
    Low,
    /// 通常優先度（警告・エラー）。クリップボード書き込み失敗など。
    Normal,
}

// -------------------------------------------------------------------
// NotifyError
// -------------------------------------------------------------------

/// `Notifier::notify` が返すエラー型。
#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    /// OS 通知システムが使用不可（ヘッドレス環境等）。
    #[error("notification system unavailable: {reason}")]
    Unavailable { reason: String },
}

// -------------------------------------------------------------------
// Notifier trait
// -------------------------------------------------------------------

/// OS 通知を送信するインターフェース。
///
/// `Send + Sync + 'static` 境界: `HotkeyEventLoop` が `Arc<dyn Notifier>` として
/// tokio spawn タスク内で共有するために必要。
pub trait Notifier: Send + Sync + 'static {
    /// 通知を送信する。
    ///
    /// # Errors
    /// OS 通知システムが利用不可の場合 `NotifyError::Unavailable`。
    fn notify(&self, level: NotifyLevel, title: &str, body: &str) -> Result<(), NotifyError>;
}

// -------------------------------------------------------------------
// NotifyRustNotifier
// -------------------------------------------------------------------

/// `notify-rust` crate を使用する本番 OS 通知実装。
///
/// 送信失敗は `tracing::warn!` でログのみ（通知システムの不在がアプリ動作を止めない）。
pub struct NotifyRustNotifier;

impl Notifier for NotifyRustNotifier {
    fn notify(&self, level: NotifyLevel, title: &str, body: &str) -> Result<(), NotifyError> {
        use notify_rust::Notification;

        let mut notification = Notification::new();
        notification.summary(title).body(body);

        // urgency は Linux (D-Bus) のみサポート。macOS / Windows は設定不可。
        #[cfg(target_os = "linux")]
        {
            let urgency = match level {
                NotifyLevel::Low => notify_rust::Urgency::Low,
                NotifyLevel::Normal => notify_rust::Urgency::Normal,
            };
            notification.urgency(urgency);
        }
        // macOS / Windows では urgency を設定せず level を無視する（best-effort）
        #[cfg(not(target_os = "linux"))]
        let _ = level;

        let result = notification.show();

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    title, body,
                    "OS notification failed (best-effort, continuing)"
                );
                Err(NotifyError::Unavailable {
                    reason: e.to_string(),
                })
            }
        }
    }
}

// -------------------------------------------------------------------
// NullNotifier
// -------------------------------------------------------------------

/// ヘッドレス環境 / 通知未対応環境向けの noop 実装。
///
/// `HotkeyManager` の初期化失敗フォールバックとして使用する。
pub struct NullNotifier;

impl Notifier for NullNotifier {
    fn notify(&self, _level: NotifyLevel, title: &str, body: &str) -> Result<(), NotifyError> {
        tracing::debug!(title, body, "NullNotifier: notification suppressed");
        Ok(())
    }
}

// -------------------------------------------------------------------
// MockNotifier (テスト用)
// -------------------------------------------------------------------

/// テスト用の記録型 Notifier。送信履歴を `Vec` に蓄積する。
#[cfg(test)]
#[derive(Default)]
pub struct MockNotifier {
    pub records: std::sync::Mutex<Vec<(NotifyLevel, String, String)>>,
}

#[cfg(test)]
impl MockNotifier {
    /// 空の `MockNotifier` を構築する。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 記録された通知の件数を返す。
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

#[cfg(test)]
impl Notifier for MockNotifier {
    fn notify(&self, level: NotifyLevel, title: &str, body: &str) -> Result<(), NotifyError> {
        self.records
            .lock()
            .unwrap()
            .push((level, title.to_owned(), body.to_owned()));
        Ok(())
    }
}
