//! secret エントリ自動クリアタイマー（R1-HK-05）。
//!
//! `ClearTimer::schedule` が `CLEAR_TIMEOUT_SECS` 後にクリップボードをクリアするタスクを spawn する。
//! 再呼び出し時は既存タイマーを `abort()` して新しいタスクをスタートする。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::clipboard::{ClipboardError, ClipboardWriter};

// -------------------------------------------------------------------
// ClearTimer
// -------------------------------------------------------------------

/// secret エントリの自動クリアタスク管理。
///
/// | 状態 | 説明 |
/// |------|------|
/// | Idle | タイマー未設定（`handle == None`） |
/// | Running | カウントダウン中（`handle == Some(JoinHandle)`）|
///
/// RAII: `Drop` 時に実行中タスクを `abort()` する。
#[derive(Default)]
pub struct ClearTimer {
    handle: Option<JoinHandle<()>>,
}

impl ClearTimer {
    /// 未設定状態の `ClearTimer` を構築する。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 既存タイマーをキャンセルし、`duration` 後にクリップボードをクリアする新タスクを spawn する。
    ///
    /// `writer` は `Arc<Mutex<dyn ClipboardWriter + Send>>` として spawn タスク内で所有される。
    pub fn schedule(&mut self, duration: Duration, writer: Arc<Mutex<dyn ClipboardWriter + Send>>) {
        // 既存タイマーをキャンセル（再スケジュールの場合）
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }

        let handle = tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            let mut w = writer.lock().await;
            if let Err(e) = w.clear() {
                match e {
                    ClipboardError::Unavailable { .. } => {
                        tracing::debug!("ClearTimer: clipboard unavailable on clear");
                    }
                    ClipboardError::WriteFailed { reason } => {
                        tracing::warn!(reason, "ClearTimer: clipboard clear failed");
                    }
                }
            } else {
                tracing::info!(
                    target: "shikomi::audit",
                    event = "clipboard_cleared",
                    "ClearTimer: clipboard cleared after timeout"
                );
            }
        });

        self.handle = Some(handle);
    }

    /// 実行中タイマーを即時キャンセルする（shutdown 時等）。
    pub fn abort(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for ClearTimer {
    fn drop(&mut self) {
        self.abort();
    }
}

// -------------------------------------------------------------------
// ユニットテスト（TC-HD-DU04〜DU06）
// -------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Mutex;

    use super::*;
    use crate::hotkey::clipboard::{ClipboardError, ClipboardWriter};

    /// テスト用クリップボード実装（書き込み・クリア履歴を記録）
    struct TestClipboard {
        history: Vec<String>,
    }

    impl TestClipboard {
        fn new() -> Self {
            Self {
                history: Vec::new(),
            }
        }

        fn current(&self) -> Option<&str> {
            self.history.last().map(String::as_str)
        }
    }

    impl ClipboardWriter for TestClipboard {
        fn write(&mut self, value: &str) -> Result<(), ClipboardError> {
            self.history.push(value.to_owned());
            Ok(())
        }

        fn clear(&mut self) -> Result<(), ClipboardError> {
            self.history.push(String::new());
            Ok(())
        }
    }

    fn wrap(cb: TestClipboard) -> Arc<Mutex<dyn ClipboardWriter + Send>> {
        Arc::new(Mutex::new(cb))
    }

    // ── TC-HD-DU04: schedule が duration 後に clear() を呼ぶ ────────────

    /// TC-HD-DU04: start_paused + advance で 30 秒タイマー検証
    #[tokio::test(start_paused = true)]
    async fn tc_hd_du04_schedule_clears_after_duration() {
        let clipboard = Arc::new(Mutex::new(TestClipboard::new()));
        clipboard.lock().await.history.push("secret".to_owned());

        let mut timer = ClearTimer::new();
        timer.schedule(
            Duration::from_secs(30),
            Arc::clone(&clipboard) as Arc<Mutex<dyn ClipboardWriter + Send>>,
        );

        // start_paused=true により auto-advance が有効: 31 秒進めてタスクを発火させる
        tokio::time::sleep(Duration::from_secs(31)).await;

        let cb = clipboard.lock().await;
        assert_eq!(
            cb.current(),
            Some(""),
            "clipboard should be cleared after 30s"
        );
    }

    // ── TC-HD-DU05: 再 schedule が前のタイマーをキャンセルする ──────────

    /// TC-HD-DU05: reschedule でタイマー A が abort され、タイマー B のみ発火
    #[tokio::test(start_paused = true)]
    async fn tc_hd_du05_reschedule_cancels_previous_timer() {
        let cb_a = Arc::new(Mutex::new(TestClipboard::new()));
        let cb_b = Arc::new(Mutex::new(TestClipboard::new()));
        cb_a.lock().await.history.push("secret-a".to_owned());
        cb_b.lock().await.history.push("secret-b".to_owned());

        let mut timer = ClearTimer::new();

        // タイマー A を 30 秒でセット
        timer.schedule(
            Duration::from_secs(30),
            Arc::clone(&cb_a) as Arc<Mutex<dyn ClipboardWriter + Send>>,
        );

        // 15 秒経過後にタイマー B（30 秒）で上書き
        tokio::time::sleep(Duration::from_secs(15)).await;

        timer.schedule(
            Duration::from_secs(30),
            Arc::clone(&cb_b) as Arc<Mutex<dyn ClipboardWriter + Send>>,
        );

        // さらに 31 秒進める → タイマー B（起算から 31 秒後）が発火
        // タイマー A はすでに abort 済み: 発火しない
        tokio::time::sleep(Duration::from_secs(31)).await;

        // タイマー A はキャンセル → cb_a は変化なし
        let a = cb_a.lock().await;
        assert_eq!(a.current(), Some("secret-a"), "timer A should be cancelled");

        // タイマー B は発火 → cb_b はクリア
        let b = cb_b.lock().await;
        assert_eq!(b.current(), Some(""), "timer B should have cleared");
    }

    // ── TC-HD-DU06: MockClipboard の write/clear 動作確認 ────────────────

    #[test]
    fn tc_hd_du06_test_clipboard_write_and_clear() {
        let mut cb = TestClipboard::new();
        cb.write("hello").unwrap();
        assert_eq!(cb.current(), Some("hello"));
        cb.clear().unwrap();
        assert_eq!(cb.current(), Some(""));
    }
}
