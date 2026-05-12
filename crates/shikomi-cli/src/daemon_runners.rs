//! `shikomi daemon` サブコマンドの dispatcher 群。
//!
//! 工程5 ペガサス指摘 (lib.rs 500 行ルール超過) 解消のため、`lib.rs` から
//! `run_daemon_subcommand` と `probe_daemon_running` を本ファイルに切り出した。
//! `lib.rs` はコンポジションルート + IPC handshake + vault サブコマンド経路に
//! 責務を集約する設計に整理。
//!
//! 設計根拠:
//! - docs/features/daemon-default-mode/autostart/detailed-design/index.md
//!   §run_daemon_subcommand

use crate::cli::DaemonSubcommand;
use crate::error::ExitCode;
use crate::io;
use crate::presenter::{self, Locale};
use crate::{autostart, eprint_stderr};
use io::ipc_vault_repository::IpcVaultRepository;

/// daemon サブコマンドを処理する（RepositoryHandle 不要のため early-return 経路）。
///
/// 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/index.md
/// §run_daemon_subcommand
pub(crate) fn run_daemon_subcommand(
    sub: &DaemonSubcommand,
    no_ipc: bool,
    locale: Locale,
    quiet: bool,
) -> ExitCode {
    match sub {
        DaemonSubcommand::Install => {
            let backend = autostart::detect();
            match backend.install() {
                Ok(()) => {
                    tracing::info!(
                        target: "shikomi_cli::autostart",
                        "autostart install: backend={}",
                        backend.name()
                    );
                    if !quiet {
                        print!("{}", presenter::success::render_autostart_installed(locale));
                        if let Some(hint) = backend.install_hint() {
                            println!("{hint}");
                        }
                    }
                    ExitCode::Success
                }
                Err(ref err) => {
                    let msg = presenter::error::render_autostart_install_error(err, locale);
                    eprint_stderr(&msg);
                    ExitCode::UserError
                }
            }
        }
        DaemonSubcommand::Uninstall => {
            let backend = autostart::detect();
            match backend.uninstall() {
                Ok(()) => {
                    tracing::info!(
                        target: "shikomi_cli::autostart",
                        "autostart uninstall: backend={}",
                        backend.name()
                    );
                    if !quiet {
                        print!(
                            "{}",
                            presenter::success::render_autostart_uninstalled(locale)
                        );
                    }
                    ExitCode::Success
                }
                Err(ref err) => {
                    let msg = presenter::error::render_autostart_uninstall_error(err, locale);
                    eprint_stderr(&msg);
                    ExitCode::UserError
                }
            }
        }
        DaemonSubcommand::Status => {
            // IPC probe（no_ipc=true の場合は省略）
            let daemon_line = if no_ipc {
                "daemon: unknown (--no-ipc)".to_owned()
            } else {
                probe_daemon_running()
            };

            let backend = autostart::detect();
            let autostart_line = if backend.is_registered() {
                "autostart: enabled"
            } else {
                "autostart: disabled"
            };

            println!("{daemon_line}");
            println!("{autostart_line}");
            // Status は常に Success（REQ-DDM-012 「情報提供のみ、副作用なし」）
            ExitCode::Success
        }
    }
}

/// daemon IPC 接続を 200ms タイムアウトで probe し、稼働状態を文字列で返す。
///
/// 設計根拠: index.md §Status 処理手順 — IPC probe 200ms タイムアウト
pub(crate) fn probe_daemon_running() -> String {
    let socket_path = match IpcVaultRepository::default_socket_path() {
        Ok(p) => p,
        Err(_) => return "daemon: not running".to_owned(),
    };

    // tokio runtime で 200ms タイムアウト付き接続を試みる
    let connected = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|rt| {
            rt.block_on(async {
                matches!(
                    tokio::time::timeout(
                        std::time::Duration::from_millis(200),
                        io::ipc_client::IpcClient::connect(&socket_path),
                    )
                    .await,
                    Ok(Ok(_))
                )
            })
        })
        .unwrap_or(false);

    if connected {
        "daemon: running".to_owned()
    } else {
        "daemon: not running".to_owned()
    }
}
