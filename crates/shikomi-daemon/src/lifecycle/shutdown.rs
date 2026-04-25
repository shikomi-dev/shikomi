//! graceful shutdown シグナル受信。
//!
//! Unix: SIGTERM / SIGINT / Windows: CTRL_CLOSE_EVENT / CTRL_C_EVENT を待機し、
//! 受信時に `Notify::notify_waiters` で server に伝達する。

use std::sync::Arc;

use tokio::sync::Notify;

/// シグナル受信を待ち、`notify` で server に shutdown を通知する。
///
/// Unix: SIGTERM / SIGINT / Windows: ctrl_close / ctrl_c を `tokio::select!` で待機。
pub async fn wait_for_signal(notify: Arc<Notify>) {
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
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
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
        tokio::select! {
            _ = close.recv() => {},
            _ = ctrlc.recv() => {},
        }
    }

    tracing::info!(target: "shikomi_daemon::lifecycle", "shutdown signal received");
    notify.notify_waiters();
}
