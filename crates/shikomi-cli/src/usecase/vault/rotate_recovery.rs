//! `vault rotate-recovery` usecase（Sub-F #44、F-F7）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F7 / §設計判断: `cache_relocked: false` 経路の CLI 表示分岐
//!
//! Phase 4 責務集約: rekey usecase と同方針で MSG-S20 連結を本 usecase に集約。

use std::io::Write;

use super::read_master_password;
use crate::cli::{OutputArgs, OutputTarget};
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{cache_relocked_warning, success, Locale};

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
    let mut rendered = match output {
        OutputTarget::Screen => success::render_recovery_rotated(&outcome.words, locale),
        OutputTarget::Print | OutputTarget::Braille | OutputTarget::Audio => {
            success::render_recovery_rotated_with_fallback_notice(&outcome.words, output, locale)
        }
    };
    if !outcome.cache_relocked {
        cache_relocked_warning::render_to(&mut rendered, locale);
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(rendered.as_bytes());
}
