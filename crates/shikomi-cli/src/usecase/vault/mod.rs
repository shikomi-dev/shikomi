//! `shikomi vault {subcommand}` サブコマンド usecase 群（Sub-F #44）。
//!
//! 各 usecase は以下の責務を負う:
//!
//! 1. ユーザ入力（master password 等）を **`input::password::prompt`** で取得
//!    （Phase 5 で C-38: stdin パイプ拒否 / TTY 強制経路に切替済）
//! 2. `IpcVaultRepository` の対応 round-trip を呼出
//! 3. presenter 経由で MSG-S{xx} 系の成功文言を render
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細（F-F1〜F-F7）/ §セキュリティ設計 §C-38

pub mod change_password;
pub mod decrypt;
pub mod encrypt;
pub mod lock;
pub mod rekey;
pub mod rotate_recovery;
pub mod unlock;

use crate::error::CliError;
use crate::input::password;
use shikomi_core::SecretString;

/// `master password: ` プロンプトで非エコー入力 1 行を取得する共通 helper。
///
/// Phase 5: `input::password::prompt` 経由に切替済 (C-38)。stdin が非 TTY の場合
/// `CliError::NonInteractivePassword` で fail fast し、`echo pw | shikomi vault unlock`
/// 経路を構造的に拒否する。
///
/// # Errors
/// - 非 TTY 時: `CliError::NonInteractivePassword`。
/// - TTY 読取失敗時: `CliError::Persistence`。
pub(crate) fn read_master_password(prompt_label: &str) -> Result<SecretString, CliError> {
    password::prompt(prompt_label)
}
