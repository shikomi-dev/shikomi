//! CLI 層のエラー型と終了コード。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/data-structures.md
//! §`CliError` バリアント詳細、§`CliError` の `From` 実装

use std::path::PathBuf;

use shikomi_core::{DomainError, RecordId};
use shikomi_infra::persistence::PersistenceError;
use thiserror::Error;

// -------------------------------------------------------------------
// CliError
// -------------------------------------------------------------------

/// CLI 全体が返すエラー型。
///
/// i18n 併記は `presenter::error::render_error` の責務。`Display` は英語原文のみ。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CliError {
    /// clap usage error / フラグ併用違反（人間可読の英文を保持）
    #[error("{0}")]
    UsageError(String),

    /// `RecordLabel::try_new` 失敗
    #[error("invalid label: {0}")]
    InvalidLabel(DomainError),

    /// `RecordId::try_from_str` 失敗
    #[error("invalid record id: {0}")]
    InvalidId(DomainError),

    /// 対象レコードが vault に存在しない
    #[error("record not found: {0}")]
    RecordNotFound(RecordId),

    /// vault 未初期化（`list` / `edit` / `remove` のみ。`add` は自動作成）
    #[error("vault not initialized at {0}")]
    VaultNotInitialized(PathBuf),

    /// 非 TTY で `remove --yes` 未指定
    #[error("refusing to delete without --yes in non-interactive mode")]
    NonInteractiveRemove,

    /// 永続化層のエラー
    #[error("persistence error: {0}")]
    Persistence(PersistenceError),

    /// 予期しないドメインエラー（集約整合性等）
    #[error("domain error: {0}")]
    Domain(DomainError),

    /// 暗号化 vault 検出（Phase 1 未対応）
    #[error("this vault is encrypted; encryption is not yet supported in this CLI version")]
    EncryptionUnsupported,
}

impl From<PersistenceError> for CliError {
    fn from(e: PersistenceError) -> Self {
        // 暗号化 vault 検出は `PersistenceError::UnsupportedYet { feature: "encrypted vault persistence", .. }`
        // の形で infra 層から返る（`shikomi-infra::persistence::repository.rs` の
        // `load_inner` / `save_inner` が Encrypted モードで Fail Fast）。
        // CLI 層はこれを専用の `EncryptionUnsupported` バリアントへ写像し、
        // `ExitCode::EncryptionUnsupported (3)` に繋げる（REQ-CLI-009 / 受入基準 8）。
        match e {
            PersistenceError::UnsupportedYet {
                feature: "encrypted vault persistence",
                ..
            } => Self::EncryptionUnsupported,
            other => Self::Persistence(other),
        }
    }
}

// -------------------------------------------------------------------
// ExitCode
// -------------------------------------------------------------------

/// CLI の終了コード。`std::process::Termination` を実装し、`main() -> ExitCode` から返せる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// 成功
    Success = 0,
    /// ユーザ入力エラー（フラグ不足・不正値・vault 未作成 等）
    UserError = 1,
    /// システムエラー（I/O / 権限 / SQLite / 内部バグ）
    SystemError = 2,
    /// 暗号化 vault 検出（Phase 1 未対応）
    EncryptionUnsupported = 3,
}

impl std::process::Termination for ExitCode {
    fn report(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self as u8)
    }
}

impl From<&CliError> for ExitCode {
    fn from(err: &CliError) -> Self {
        match err {
            CliError::UsageError(_)
            | CliError::InvalidLabel(_)
            | CliError::InvalidId(_)
            | CliError::RecordNotFound(_)
            | CliError::VaultNotInitialized(_)
            | CliError::NonInteractiveRemove => Self::UserError,
            CliError::Persistence(_) | CliError::Domain(_) => Self::SystemError,
            CliError::EncryptionUnsupported => Self::EncryptionUnsupported,
        }
    }
}

// -------------------------------------------------------------------
// テスト
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::error::InvalidRecordLabelReason;

    #[test]
    fn test_exit_code_usage_error_maps_to_user_error() {
        let err = CliError::UsageError("x".to_owned());
        assert_eq!(ExitCode::from(&err), ExitCode::UserError);
    }

    #[test]
    fn test_exit_code_invalid_label_maps_to_user_error() {
        let err = CliError::InvalidLabel(DomainError::InvalidRecordLabel(
            InvalidRecordLabelReason::Empty,
        ));
        assert_eq!(ExitCode::from(&err), ExitCode::UserError);
    }

    #[test]
    fn test_exit_code_non_interactive_remove_maps_to_user_error() {
        assert_eq!(
            ExitCode::from(&CliError::NonInteractiveRemove),
            ExitCode::UserError
        );
    }

    #[test]
    fn test_exit_code_encryption_unsupported_maps_to_exit_3() {
        assert_eq!(ExitCode::from(&CliError::EncryptionUnsupported) as u8, 3);
    }

    #[test]
    fn test_display_invalid_label_does_not_contain_secret_marker() {
        // CliError::Display はラベル検証エラー文を英語で返す。secret 値は含まれない。
        let err = CliError::InvalidLabel(DomainError::InvalidRecordLabel(
            InvalidRecordLabelReason::Empty,
        ));
        let msg = err.to_string();
        assert!(msg.contains("invalid label"));
        assert!(!msg.contains("SECRET_TEST_VALUE"));
    }

    /// BUG-001 回帰: `PersistenceError::UnsupportedYet { feature: "encrypted vault persistence", .. }`
    /// は `CliError::EncryptionUnsupported`（exit 3）に写像されなければならない。
    /// 以前は無条件に `CliError::Persistence(...)` に包まれ、exit 2 になっていた。
    #[test]
    fn test_from_persistence_encrypted_vault_maps_to_encryption_unsupported() {
        let pe = PersistenceError::UnsupportedYet {
            feature: "encrypted vault persistence",
            tracking_issue: None,
        };
        let cli_err: CliError = pe.into();
        assert!(matches!(cli_err, CliError::EncryptionUnsupported));
        assert_eq!(ExitCode::from(&cli_err), ExitCode::EncryptionUnsupported);
    }

    /// 他の `UnsupportedYet`（将来の未実装機能）は `CliError::Persistence` に留める。
    #[test]
    fn test_from_persistence_other_unsupported_maps_to_persistence() {
        let pe = PersistenceError::UnsupportedYet {
            feature: "some other future feature",
            tracking_issue: None,
        };
        let cli_err: CliError = pe.into();
        assert!(matches!(cli_err, CliError::Persistence(_)));
        assert_eq!(ExitCode::from(&cli_err), ExitCode::SystemError);
    }
}
