//! `vault rekey` usecase（Sub-F #44 Phase 2、F-F6）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F6
//!
//! Phase 2 スコープ: master password 取得 → IPC `Rekey` 1 往復 → 24 語を
//! Screen 経路で render。`cache_relocked: false` 時の MSG-S07/S20 連結表示は
//! Phase 4 で `presenter::cache_relocked_warning::display` に集約する。

use std::io::Write;

use super::read_master_password;
use crate::cli::{OutputArgs, OutputTarget};
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault rekey --output` を実行する。
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
    let outcome = repo.rekey(master_password)?;
    if !quiet {
        render_rekey(&outcome, args.output, locale);
    }
    Ok(())
}

fn render_rekey(
    outcome: &crate::io::ipc_vault_repository::RekeyOutcome,
    output: OutputTarget,
    locale: Locale,
) {
    let rendered = match output {
        OutputTarget::Screen => success::render_rekeyed(
            outcome.records_count,
            &outcome.words,
            outcome.cache_relocked,
            locale,
        ),
        OutputTarget::Print | OutputTarget::Braille | OutputTarget::Audio => {
            // Phase 5 でアクセシビリティ経路に dispatch、Phase 2 は Screen fallback。
            success::render_rekeyed_with_fallback_notice(
                outcome.records_count,
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
