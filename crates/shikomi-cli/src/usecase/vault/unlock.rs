//! `vault unlock` usecase（Sub-F #44、F-F3）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F3
//!
//! 工程4 Bug-F-001 解消: `--recovery` 24 語入力経路を `input::mnemonic::prompt`
//! 経由で本実装。stub の `UsageError` 返却を完全削除し EC-F3 を満たす。

use std::io::Write;

use super::read_master_password;
use crate::cli::UnlockArgs;
use crate::error::CliError;
use crate::input::mnemonic;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault unlock` を実行する (パスワード or recovery 24 語経路)。
///
/// # Errors
/// `--recovery` 経路で stdin が非 TTY なら `CliError::NonInteractivePassword`
/// (C-38)、24 語入力不正なら `CliError::UsageError`。IPC 失敗 / daemon 側
/// V2 エラー / TTY 入力失敗時に `CliError`。
pub fn execute(
    repo: &IpcVaultRepository,
    args: &UnlockArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let master_password = read_master_password("master password: ")?;
    let recovery = if args.recovery {
        Some(mnemonic::prompt(
            "recovery words (24 BIP-39 words separated by spaces): ",
        )?)
    } else {
        None
    };
    repo.unlock(master_password, recovery)?;
    if !quiet {
        let rendered = success::render_unlocked(locale);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(rendered.as_bytes());
    }
    Ok(())
}
