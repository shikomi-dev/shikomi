//! `vault rekey` usecase（Sub-F #44、F-F6）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F6 / §設計判断: `cache_relocked: false` 経路の CLI 表示分岐
//!
//! Phase 4 責務集約:
//! - `success::render_rekeyed` は MSG-S07 + 24 語表示のみ。
//! - `outcome.cache_relocked == false` 観測時に
//!   `presenter::cache_relocked_warning::display` を **追加で** stdout に出力する
//!   責務を本 usecase が持つ（C-32 Lie-Then-Surprise 防止）。
//! - 終了コードは C-31 / C-36 に従い `Ok(())` のまま返す（operation 成功）。

use std::io::Write;

use super::read_master_password;
use crate::cli::{OutputArgs, OutputTarget};
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{cache_relocked_warning, success, Locale};

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
    let mut rendered = match output {
        OutputTarget::Screen => {
            success::render_rekeyed(outcome.records_count, &outcome.words, locale)
        }
        OutputTarget::Print | OutputTarget::Braille | OutputTarget::Audio => {
            // Phase 5 でアクセシビリティ経路に dispatch、現状は Screen fallback。
            success::render_rekeyed_with_fallback_notice(
                outcome.records_count,
                &outcome.words,
                output,
                locale,
            )
        }
    };
    if !outcome.cache_relocked {
        cache_relocked_warning::render_to(&mut rendered, locale);
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(rendered.as_bytes());
}
