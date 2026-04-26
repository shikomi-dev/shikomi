//! `vault rotate-recovery` usecase（Sub-F #44 Phase 2、F-F7）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F7
//!
//! Phase 2 スコープ: master password 取得 → IPC `RotateRecovery` 1 往復 → 24 語
//! を Screen 経路で render。`cache_relocked: false` 経路は Phase 4 で集約。

use std::io::Write;

use super::read_master_password;
use crate::cli::{OutputArgs, OutputTarget};
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault rotate-recovery --output` を実行する。
///
/// # Errors
/// IPC 失敗 / daemon 側エラー / TTY 入力失敗時に `CliError`。
pub fn execute(
    repo: &IpcVaultRepository,
    args: &OutputArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let master_password = read_master_password("master password: ")?;
    let outcome = repo.rotate_recovery(master_password)?;
    if !quiet {
        render_rotate_recovery(&outcome, args.output, locale);
    }
    Ok(())
}

fn render_rotate_recovery(
    outcome: &crate::io::ipc_vault_repository::RotateRecoveryOutcome,
    output: OutputTarget,
    locale: Locale,
) {
    let rendered = match output {
        OutputTarget::Screen => {
            success::render_recovery_rotated(&outcome.words, outcome.cache_relocked, locale)
        }
        OutputTarget::Print | OutputTarget::Braille | OutputTarget::Audio => {
            success::render_recovery_rotated_with_fallback_notice(
                &outcome.words,
                outcome.cache_relocked,
                output,
                locale,
            )
        }
    };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(rendered.as_bytes());
}
