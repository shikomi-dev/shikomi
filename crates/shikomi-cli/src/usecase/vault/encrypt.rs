//! `vault encrypt` usecase（Sub-F #44 Phase 2、F-F1）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F1
//!
//! Phase 2 スコープ: master password 取得 → IPC `Encrypt` 1 往復 → 24 語を
//! Screen 経路で render。`--output {print,braille,audio}` は Phase 5 で実装。

use shikomi_core::ipc::SerializableSecretBytes;

use super::read_master_password;
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
        render_disclosure(&disclosure, args.output, locale);
    }
    Ok(())
}

/// 24 語を `--output` 経路で render する。Phase 2 は `Screen` 経路のみ実装。
fn render_disclosure(disclosure: &[SerializableSecretBytes], output: OutputTarget, locale: Locale) {
    let rendered = match output {
        OutputTarget::Screen => success::render_recovery_disclosure_screen(disclosure, locale),
        OutputTarget::Print | OutputTarget::Braille | OutputTarget::Audio => {
            // Phase 5 で `accessibility::{print_pdf, braille_brf, audio_tts}` に dispatch。
            // Phase 2 は Screen 経路にフォールバックして「未実装」案内を併記する。
            success::render_recovery_disclosure_screen_with_fallback_notice(
                disclosure, output, locale,
            )
        }
    };
    let mut out = std::io::stdout().lock();
    use std::io::Write;
    let _ = out.write_all(rendered.as_bytes());
}
