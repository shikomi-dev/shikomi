//! `vault unlock` usecase（Sub-F #44 Phase 2、F-F3）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F3
//!
//! Phase 2 スコープ: パスワード経路のみ wire。`--recovery` 24 語入力経路は
//! Phase 5 で `input::mnemonic::prompt` + bip39 検証に集約する。Phase 2 で
//! `--recovery` 指定時は `UsageError` で fail fast し、Phase 5 までの境界を明示する。

use std::io::Write;

use super::read_master_password;
use crate::cli::UnlockArgs;
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault unlock` を実行する。
///
/// # Errors
/// `--recovery` 指定時は Phase 5 まで `UsageError` を返す。IPC 失敗 / daemon 側
/// V2 エラー / TTY 入力失敗時に `CliError`。
pub fn execute(
    repo: &IpcVaultRepository,
    args: &UnlockArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    if args.recovery {
        return Err(CliError::UsageError(
            "--recovery path is not yet wired in this build (Phase 5)".to_owned(),
        ));
    }
    let master_password = read_master_password("master password: ")?;
    repo.unlock(master_password, None)?;
    if !quiet {
        let rendered = success::render_unlocked(locale);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(rendered.as_bytes());
    }
    Ok(())
}
