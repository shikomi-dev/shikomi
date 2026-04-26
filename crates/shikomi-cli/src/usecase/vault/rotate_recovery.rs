//! `vault rotate-recovery` usecase（Sub-F #44、F-F7）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F7 / §設計判断: `cache_relocked: false` 経路の CLI 表示分岐
//!
//! Phase 4 責務集約: rekey usecase と同方針で MSG-S20 連結を本 usecase に集約。

use std::io::Write;

use super::read_master_password;
use crate::accessibility::{audio_tts, braille_brf, output_target, print_pdf, umask};
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
        render_rotate_recovery(&outcome, args.output, locale)?;
    }
    Ok(())
}

fn render_rotate_recovery(
    outcome: &crate::io::ipc_vault_repository::RotateRecoveryOutcome,
    output: OutputTarget,
    locale: Locale,
) -> Result<(), CliError> {
    let resolved = output_target::resolve(output);
    match resolved {
        OutputTarget::Screen => {
            let mut rendered = success::render_recovery_rotated(&outcome.words, locale);
            if !outcome.cache_relocked {
                cache_relocked_warning::render_to(&mut rendered, locale);
            }
            write_to_stdout(&rendered)
        }
        OutputTarget::Braille => {
            umask::with_secure_umask(|| braille_brf::write_to_stdout(&outcome.words))?;
            Ok(())
        }
        OutputTarget::Audio => audio_tts::speak(&outcome.words),
        OutputTarget::Print => {
            umask::with_secure_umask(|| print_pdf::write_to_stdout(&outcome.words))?;
            Ok(())
        }
    }
}

fn write_to_stdout(s: &str) -> Result<(), CliError> {
    let mut out = std::io::stdout().lock();
    out.write_all(s.as_bytes()).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<stdout>"),
            source: e,
        })
    })
}
