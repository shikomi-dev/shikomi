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
