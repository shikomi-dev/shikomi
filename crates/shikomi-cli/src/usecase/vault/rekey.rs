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
use crate::accessibility::{audio_tts, braille_brf, output_target, print_pdf, umask};
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
        render_rekey(&outcome, args.output, locale)?;
    }
    Ok(())
}

fn render_rekey(
    outcome: &crate::io::ipc_vault_repository::RekeyOutcome,
    output: OutputTarget,
    locale: Locale,
) -> Result<(), CliError> {
    let resolved = output_target::resolve(output);
    match resolved {
        OutputTarget::Screen => {
            // Issue #75 Bug-F-002 §経路復活: `success::render_rekeyed_with_fallback_notice`
            // を `cache_relocked == false` 時に呼出 (`cli-subcommands.md` §Bug-F-002 解消の
            // SSoT 通り presenter 層に責務移譲、C-31/C-36 articulate を実体化)。
            // `cache_relocked == true` 経路は警告不要のため `render_rekeyed` のままを使う。
            let rendered = if outcome.cache_relocked {
                success::render_rekeyed(outcome.records_count, &outcome.words, locale)
            } else {
                success::render_rekeyed_with_fallback_notice(
                    outcome.records_count,
                    &outcome.words,
                    locale,
                )
            };
            write_to_stdout(&rendered)
        }
        OutputTarget::Braille => {
            umask::with_secure_umask(|| braille_brf::write_to_stdout(&outcome.words))?;
            // 24 語の本体は BRF で出力済み。MSG-S20 連結 (cache_relocked == false) は
            // Screen 経路で stderr ではなく stdout に出してしまうと印字機が読めるため、
            // BRF 出力の純粋性を尊重して MSG-S20 は **省略** する (Phase 6 trade-off、
            // Phase 7 で BRF 末尾に「次操作前に再 unlock」の点字表現を検討)。
            Ok(())
        }
        OutputTarget::Audio => audio_tts::speak(&outcome.words),
        OutputTarget::Print => {
            // Phase 7: PDF 経路。MSG-S20 連結は Braille と同様 PDF 純粋性を尊重して
            // 省略 (操作成功 = ExitCode::Success は維持、cache_relocked 警告は将来
            // PDF 末尾ページに追記する設計 minor で検討)。
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
