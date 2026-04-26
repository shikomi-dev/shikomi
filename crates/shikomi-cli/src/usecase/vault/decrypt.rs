//! `vault decrypt` usecase（Sub-F #44 Phase 2、F-F2）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F2
//!
//! Phase 2 スコープ: master password + 確認入力 `DECRYPT` 取得 → IPC `Decrypt`
//! 1 往復。`subtle::ConstantTimeEq` 比較 + paste 抑制 (C-34) は Phase 5 で
//! `input::decrypt_confirmation::prompt` に集約する。Phase 2 は単純文字列比較で wire。

use std::io::Write;

use super::read_master_password;
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::io::terminal;
use crate::presenter::{success, Locale};

/// 確認入力で要求される確定文字列（Sub-D Rev2 凍結契約 + C-34）。
const CONFIRMATION_LITERAL: &str = "DECRYPT";

/// `vault decrypt` を実行する。
///
/// # Errors
/// 確認入力不一致 / IPC 失敗 / daemon 側エラー / TTY 入力失敗時に `CliError`。
pub fn execute(repo: &IpcVaultRepository, locale: Locale, quiet: bool) -> Result<(), CliError> {
    let master_password = read_master_password("master password: ")?;
    let confirmation = read_confirmation_literal(locale)?;
    let confirmed = confirmation == CONFIRMATION_LITERAL;
    if !confirmed {
        return Err(CliError::UsageError(
            "decrypt confirmation mismatch (expected literal `DECRYPT`)".to_owned(),
        ));
    }
    repo.decrypt(master_password, confirmed)?;
    if !quiet {
        let rendered = success::render_decrypted(locale);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(rendered.as_bytes());
    }
    Ok(())
}

/// `DECRYPT` 確認入力を 1 行読み込む（Phase 2 単純実装、Phase 5 で paste 抑制 + ConstantTimeEq）。
fn read_confirmation_literal(_locale: Locale) -> Result<String, CliError> {
    let prompt = "type `DECRYPT` to confirm: ";
    terminal::read_line(prompt).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<tty>"),
            source: e,
        })
    })
}
