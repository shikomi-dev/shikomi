//! graceful shutdown シグナル受信。
//!
//! Unix: SIGTERM / SIGINT / Windows: CTRL_CLOSE_EVENT / CTRL_C_EVENT を待機し、
//! 受信時に `tokio::sync::watch::Sender::send(true)` で全 receiver に shutdown を伝達する。
//!
//! ## 設計選択: `Notify` ではなく `watch::channel<bool>`
//!
//! `Notify::notify_waiters()` は **「現在 pending な future にしか permit を渡さない」**
//! 仕様で、シグナル到達と receiver の `notified()` 登録の race window があると
//! 通知が消失する（BUG-DAEMON-IPC-002）。`watch::channel` は最新値を保持するため、
//! receiver が `changed().await` する前に `send(true)` が呼ばれても、次回 `changed()`
//! は即座に Ready で返る。**race window を構造的に消す**。

use tokio::sync::watch;

/// shutdown 通知の送受信ペアを生成する。
#[must_use]
pub fn channel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
    watch::channel(false)
}

/// シグナル受信を待ち、`tx.send(true)` で server に shutdown を通知する。
///
/// Unix: SIGTERM / SIGINT / Windows: ctrl_close / ctrl_c を `tokio::select!` で待機。
/// signal handler の install 失敗時は `tx` を drop することで receiver 側の
/// `changed()` が `Err` を返し、server は shutdown 経路に入る（fail-secure）。
pub async fn wait_for_signal(tx: watch::Sender<bool>) {
    tracing::info!(
        target: "shikomi_daemon::lifecycle",
        "wait_for_signal task started"
    );
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(target: "shikomi_daemon::lifecycle", "failed to install SIGTERM handler: {err}");
                return;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(target: "shikomi_daemon::lifecycle", "failed to install SIGINT handler: {err}");
                return;
            }
        };
        tracing::info!(
            target: "shikomi_daemon::lifecycle",
            "signal handlers installed (SIGTERM/SIGINT)"
        );
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!(target: "shikomi_daemon::lifecycle", "SIGTERM received");
            },
            _ = sigint.recv() => {
                tracing::info!(target: "shikomi_daemon::lifecycle", "SIGINT received");
            },
        }
    }
    #[cfg(windows)]
    {
        use tokio::signal::windows::{ctrl_c, ctrl_close};
        let mut close = match ctrl_close() {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(target: "shikomi_daemon::lifecycle", "failed to install ctrl_close handler: {err}");
                return;
            }
        };
        let mut ctrlc = match ctrl_c() {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(target: "shikomi_daemon::lifecycle", "failed to install ctrl_c handler: {err}");
                return;
            }
        };
        tracing::debug!(
            target: "shikomi_daemon::lifecycle",
            "signal handlers installed (CTRL_CLOSE/CTRL_C)"
        );
        tokio::select! {
            _ = close.recv() => {
                tracing::info!(target: "shikomi_daemon::lifecycle", "CTRL_CLOSE received");
            },
            _ = ctrlc.recv() => {
                tracing::info!(target: "shikomi_daemon::lifecycle", "CTRL_C received");
            },
        }
    }

    tracing::info!(target: "shikomi_daemon::lifecycle", "shutdown signal received");
    let _ = tx.send(true);
}
