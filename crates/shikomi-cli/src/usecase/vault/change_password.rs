//! `vault change-password` usecase（Sub-F #44 Phase 2、F-F5）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F5
//!
//! Phase 2 スコープ: 旧 / 新パスワードを TTY 非エコー入力で取得 → IPC
//! `ChangePassword` 1 往復。新パスワード強度ゲート（zxcvbn 由来 `WeakPasswordFeedback`）
//! は daemon 側 `MasterPassword::new` で実施されるため CLI は透過。

use std::io::Write;

use super::read_master_password;
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::presenter::{success, Locale};

/// `vault change-password` を実行する。
///
/// # Errors
/// IPC 失敗 / daemon 側エラー / TTY 入力失敗時に `CliError`。
pub fn execute(repo: &IpcVaultRepository, locale: Locale, quiet: bool) -> Result<(), CliError> {
    let old = read_master_password("current master password: ")?;
    let new = read_master_password("new master password: ")?;
    repo.change_password(old, new)?;
    if !quiet {
        let rendered = success::render_password_changed(locale);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(rendered.as_bytes());
    }
    Ok(())
}
