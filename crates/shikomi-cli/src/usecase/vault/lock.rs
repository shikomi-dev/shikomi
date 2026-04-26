//! `vault lock` usecase（Sub-F #44 Phase 2、F-F4）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F4

use std::io::Write;

use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault lock` を実行する（VEK 即 zeroize 要請）。
///
/// # Errors
/// IPC 失敗 / daemon 側エラー時に `CliError`。
pub fn execute(repo: &IpcVaultRepository, locale: Locale, quiet: bool) -> Result<(), CliError> {
    repo.lock()?;
    if !quiet {
        let rendered = success::render_locked(locale);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(rendered.as_bytes());
    }
    Ok(())
}
