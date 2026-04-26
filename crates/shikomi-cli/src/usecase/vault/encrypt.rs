//! `vault encrypt` usecase（Sub-F #44、F-F1）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F1 / §アクセシビリティ代替経路
//!
//! Phase 6: `--output` 自動切替 + braille / audio 経路を accessibility モジュール
//! 経由で wire。`--output print` (PDF) は Phase 7 に分離 (printpdf 依存追加と同時)。

use std::io::Write;

use shikomi_core::ipc::SerializableSecretBytes;

use super::read_master_password;
use crate::accessibility::{audio_tts, braille_brf, output_target, umask};
use crate::cli::{EncryptArgs, OutputTarget};
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault encrypt --output` を実行する。
///
/// # Errors
/// IPC 失敗 / daemon 側エラー / TTY 入力失敗時に `CliError`。
pub fn execute(
    repo: &IpcVaultRepository,
    args: &EncryptArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let master_password = read_master_password("master password: ")?;
    let disclosure = repo.encrypt(master_password, args.accept_limits)?;
    if !quiet {
        render_disclosure(&disclosure, args.output, locale)?;
    }
    Ok(())
}

/// 24 語を `--output` 経路で render する。`SHIKOMI_ACCESSIBILITY` env による
/// 自動切替を `output_target::resolve` で適用してから dispatch する。
fn render_disclosure(
    disclosure: &[SerializableSecretBytes],
    output: OutputTarget,
    locale: Locale,
) -> Result<(), CliError> {
    let resolved = output_target::resolve(output);
    match resolved {
        OutputTarget::Screen => {
            let rendered = success::render_recovery_disclosure_screen(disclosure, locale);
            write_to_stdout(&rendered)
        }
        OutputTarget::Braille => {
            // umask(0o077) 内部適用 → BRF stdout 書出 → 旧 umask 復元 (RAII)。
            // ユーザの `> recovery.brf` リダイレクト先が 0600 相当で生成される。
            umask::with_secure_umask(|| braille_brf::write_to_stdout(disclosure))
        }
        OutputTarget::Audio => audio_tts::speak(disclosure),
        OutputTarget::Print => {
            // Phase 7 で PDF 本実装。現状は Screen + fallback notice を継続。
            let rendered = success::render_recovery_disclosure_screen_with_fallback_notice(
                disclosure, resolved, locale,
            );
            write_to_stdout(&rendered)
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
