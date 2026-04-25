//! `shikomi-daemon` — 常駐プロセス（IPC サーバ）。
//!
//! 本 crate は `[lib] + [[bin]]` の 2 ターゲット構成。lib の公開項目は全て
//! `#[doc(hidden)]` で `cargo doc` から隠し、外部契約化しない（`publish = false`）。
//!
//! 設計根拠: docs/features/daemon-ipc/detailed-design/composition-root.md / daemon-runtime.md

#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod ipc;
#[doc(hidden)]
pub mod lifecycle;
#[doc(hidden)]
pub mod panic_hook;
#[doc(hidden)]
pub mod permission;

pub use error::DaemonExit;

use std::process::ExitCode;
use std::sync::Arc;

use shikomi_core::ProtectionMode;
use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
use tokio::sync::{Mutex, Notify};
use tracing_subscriber::EnvFilter;

use crate::ipc::server::IpcServer;
use crate::ipc::transport::ListenerEnum;
use crate::lifecycle::{shutdown, single_instance::SingleInstanceLock, socket_path};

// -------------------------------------------------------------------
// run() — daemon コンポジションルート
// -------------------------------------------------------------------

/// daemon プロセスのエントリ async fn。
///
/// 戻り値: `std::process::ExitCode`（`DaemonExit` 経由で 0 / 1 / 2 / 3 を返す）。
///
/// 設計根拠: docs/features/daemon-ipc/detailed-design/composition-root.md §処理順序
#[must_use]
pub async fn run() -> ExitCode {
    panic_hook::install();

    init_tracing();

    let socket_dir = match socket_path::resolve_socket_dir() {
        Ok(d) => d,
        Err(err) => {
            tracing::error!(target: "shikomi_daemon::lifecycle", "cannot resolve socket dir: {err}");
            return DaemonExit::SystemError.into();
        }
    };

    // シングルインスタンス先取り（OS 別、3 段階）
    let mut single_instance = match SingleInstanceLock::acquire(&socket_dir) {
        Ok(lock) => lock,
        Err(err) => {
            tracing::error!(
                target: "shikomi_daemon::lifecycle",
                "single instance acquisition failed: {err}"
            );
            return single_instance_exit_code(&err).into();
        }
    };

    // vault dir 解決 + repo 構築 + load
    let vault_dir = match resolve_vault_dir() {
        Ok(p) => p,
        Err(err) => {
            tracing::error!(target: "shikomi_daemon::lifecycle", "cannot resolve vault dir: {err}");
            return DaemonExit::SystemError.into();
        }
    };

    let repo = match SqliteVaultRepository::from_directory(&vault_dir) {
        Ok(r) => r,
        Err(err) => {
            tracing::error!(target: "shikomi_daemon::lifecycle", "failed to construct repository: {err}");
            return DaemonExit::SystemError.into();
        }
    };

    let vault = match repo.load() {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(target: "shikomi_daemon::lifecycle", "failed to load vault: {err}");
            return DaemonExit::SystemError.into();
        }
    };

    if vault.protection_mode() == ProtectionMode::Encrypted {
        tracing::error!(
            target: "shikomi_daemon::lifecycle",
            "vault is encrypted; daemon does not support encrypted vaults yet (Issue #26 scope-out)"
        );
        return DaemonExit::EncryptionUnsupported.into();
    }

    let listener = match single_instance.take_listener() {
        Some(l) => l,
        None => {
            tracing::error!(
                target: "shikomi_daemon::lifecycle",
                "internal: listener missing after acquisition"
            );
            return DaemonExit::SystemError.into();
        }
    };

    let repo = Arc::new(repo);
    let vault = Arc::new(Mutex::new(vault));
    let shutdown_signal = Arc::new(Notify::new());

    // listening ログ（観測性、tests/IT-010 で検証）
    log_listening(&listener);

    // shutdown 受信タスク
    let shutdown_for_signal = Arc::clone(&shutdown_signal);
    let signal_task = tokio::spawn(async move {
        shutdown::wait_for_signal(shutdown_for_signal).await;
    });

    // server 実行
    let mut server = IpcServer::new(listener, Arc::clone(&repo), Arc::clone(&vault));
    let server_result = server
        .start_with_shutdown(Arc::clone(&shutdown_signal))
        .await;

    // signal task の解放（shutdown 通知済みなら戻ってくる）
    signal_task.abort();

    // 明示的に共有資源を drop し、Drop 順序を可視化（lock → vault → repo → single_instance）
    drop(server);
    drop(vault);
    drop(repo);
    drop(single_instance);

    match server_result {
        Ok(()) => {
            tracing::info!(target: "shikomi_daemon::lifecycle", "graceful shutdown complete");
            DaemonExit::Success.into()
        }
        Err(err) => {
            tracing::error!(target: "shikomi_daemon::lifecycle", "server error: {err}");
            DaemonExit::SystemError.into()
        }
    }
}

// -------------------------------------------------------------------
// 補助関数
// -------------------------------------------------------------------

fn init_tracing() {
    let filter =
        EnvFilter::try_from_env("SHIKOMI_DAEMON_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(true)
        .try_init();
}

/// vault ディレクトリを解決する。
///
/// CLI と同じく `dirs::data_dir()` 直下を使う（`SHIKOMI_VAULT_DIR` の env 上書きは
/// `SqliteVaultRepository::new()` 経路で吸収済みだが、本 daemon は明示パスで構築する）。
fn resolve_vault_dir() -> Result<std::path::PathBuf, std::io::Error> {
    if let Ok(v) = std::env::var("SHIKOMI_VAULT_DIR") {
        return Ok(std::path::PathBuf::from(v));
    }
    dirs::data_dir()
        .map(|base| base.join("shikomi"))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "data_dir is not available on this platform",
            )
        })
}

fn single_instance_exit_code(err: &lifecycle::single_instance::SingleInstanceError) -> DaemonExit {
    use lifecycle::single_instance::SingleInstanceError;
    match err {
        SingleInstanceError::AlreadyRunning { .. } => DaemonExit::SingleInstanceUnavailable,
        _ => DaemonExit::SystemError,
    }
}

fn log_listening(listener: &ListenerEnum) {
    match listener {
        #[cfg(unix)]
        ListenerEnum::Unix { socket_path, .. } => {
            tracing::info!(
                target: "shikomi_daemon::lifecycle",
                "shikomi-daemon listening on {}",
                socket_path.display()
            );
        }
        #[cfg(windows)]
        ListenerEnum::Windows { pipe_name, .. } => {
            tracing::info!(
                target: "shikomi_daemon::lifecycle",
                "shikomi-daemon listening on {pipe_name}"
            );
        }
    }
}
