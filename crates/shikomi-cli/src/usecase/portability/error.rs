//! データポータビリティ UseCase 内部エラー型。
//!
//! `DataPortabilityError` は UseCase 層の中間エラー型。
//! `From<DataPortabilityError> for CliError` で CLI エラーへ変換される
//! (`crates/shikomi-cli/src/error.rs §From<DataPortabilityError>` 参照)。
//!
//! 設計根拠: docs/features/data-portability/cli/detailed-design/usecase.md
//! §`usecase/portability/error.rs` の設計詳細

use std::path::PathBuf;

use shikomi_core::portability::ImportValidationError;
use thiserror::Error;

// -------------------------------------------------------------------
// DataPortabilityError
// -------------------------------------------------------------------

/// データポータビリティ UseCase 内部エラー型。
///
/// `From<DataPortabilityError> for CliError` (error.rs §Issue #141) で
/// 最終的な `CliError` に変換される。
#[derive(Debug, Error)]
pub enum DataPortabilityError {
    /// vault が暗号化ロック済みの状態で export / import を試みた。
    ///
    /// `SqliteVaultRepository::load()` が `ProtectionMode::Encrypted` を検出した場合、
    /// または `ExportError::VaultLocked` が伝播した場合に発生する。
    #[error("vault is locked")]
    VaultLocked,

    /// export 先ファイルが既に存在し `--force` 未指定。
    #[error("export output file already exists: {path}")]
    OutputFileExists {
        /// 既に存在する export 先ファイルパス。
        path: PathBuf,
    },

    /// `--on-conflict error` で衝突 ID を検出した。
    #[error("import conflict: {} record(s) already exist in vault", ids.len())]
    ConflictError {
        /// 衝突したレコード ID 文字列一覧。
        ids: Vec<String>,
    },

    /// `serde_json::from_reader` 等のデシリアライズ失敗。
    #[error("failed to parse import file: {reason}")]
    DeserializationFailed {
        /// デシリアライズ失敗の詳細（外部ライブラリのエラー文言）。
        reason: String,
    },

    /// `ImportValidator::validate` 失敗。
    #[error("import validation failed: {0}")]
    ValidationFailed(ImportValidationError),

    /// ファイル I/O エラー（`tempfile` 操作 / `persist` 失敗を含む）。
    #[error("io error: {0}")]
    IoError(std::io::Error),
}

impl From<std::io::Error> for DataPortabilityError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}
