//! シグナルマスク初期化（Unix 専用）。
//!
//! 親プロセス（例: `cargo test` の test runner、`std::process::Command` の親 thread）の
//! シグナルマスクが `SIGTERM` / `SIGINT` を block していると、`exec` 後の子プロセス
//! はそのマスクを継承してしまう（`fork(2)` / `execve(2)` の semantics）。
//! その結果、tokio が `signal-hook-registry` 経由で sigaction ハンドラを install しても、
//! kernel が当該シグナルを deliver せず、ハンドラが起動しない。
//!
//! 本モジュールは daemon 起動直後にプロセスのシグナルマスクから shutdown 系シグナルを
//! `SIG_UNBLOCK` で外し、tokio signal driver が確実に signal を受信できる状態を担保する。
//! バグ参照: BUG-DAEMON-IPC-002（cargo test 経由 spawn で SIGTERM が deliver されない）。

#[cfg(unix)]
use nix::sys::signal::{SigSet, SigmaskHow, Signal};

/// daemon が graceful shutdown をトリガしたいシグナルを sigmask から外す。
///
/// 失敗してもプロセスを継続させる（best-effort、警告ログのみ）。マスクが残ったままでも
/// tokio の signal driver は sigaction を上書き install しているため、稀ケースでは
/// 動作する可能性がある。ただし fail-secure としては「unblock 失敗 = shutdown が
/// 効かない可能性あり」を前提に warn ログで観測可能化する。
#[cfg(unix)]
pub fn unblock_shutdown_signals() {
    let mut set = SigSet::empty();
    set.add(Signal::SIGTERM);
    set.add(Signal::SIGINT);
    match nix::sys::signal::sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&set), None) {
        Ok(()) => {
            tracing::debug!(
                target: "shikomi_daemon::lifecycle",
                "shutdown signals unblocked (SIGTERM/SIGINT)"
            );
        }
        Err(err) => {
            tracing::warn!(
                target: "shikomi_daemon::lifecycle",
                "failed to unblock shutdown signals: {err}; SIGTERM may not be delivered if parent blocked them"
            );
        }
    }
}

/// Windows ビルドでは noop。Windows では Console Control Handler を使うため
/// シグナルマスクの概念がない。
#[cfg(windows)]
pub fn unblock_shutdown_signals() {}
