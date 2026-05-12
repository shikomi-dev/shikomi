//! データポータビリティ用エラー型。
//!
//! `ImportValidationError`: import バリデーション失敗の詳細理由。
//! `ExportError`: `ExportRecordPayload::from_record` が返すエラー。

use thiserror::Error;

// -------------------------------------------------------------------
// ExportError
// -------------------------------------------------------------------

/// `ExportRecordPayload::from_record` が返すエラー。
///
/// `DataPortabilityError::VaultLocked`（CLI UseCase 層のエラー）とは別に、
/// domain 層専用のエラーとして定義する。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExportError {
    /// vault が暗号化ロック済みの状態で export を試みた。
    ///
    /// `RecordPayload::Encrypted` を持つレコードを変換しようとした場合に発生する（Fail Fast）。
    #[error("vault is locked; cannot export encrypted payload")]
    VaultLocked,
}

// -------------------------------------------------------------------
// ImportValidationError
// -------------------------------------------------------------------

/// `ImportValidator::validate` が返すバリデーションエラー。
///
/// `DataPortabilityError::ValidationFailed(ImportValidationError)` として
/// CLI UseCase 層に伝播する。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportValidationError {
    /// import ファイルの `format_version` が未知のバージョン（> `EXPORT_FORMAT_VERSION`）。
    #[error("unknown format_version: found {found}, max supported is 1")]
    UnknownFormatVersion {
        /// ファイル内に記録されていたバージョン番号。
        found: u32,
    },

    /// import ファイル内に同一 ID のレコードが複数存在する。
    #[error("duplicate record id in import file: {id}")]
    DuplicateIdInFile {
        /// 重複していた最初のレコード ID 文字列。
        id: String,
    },

    /// `payload.kind == "redacted"` のレコードを import しようとした。
    #[error("cannot import: payload is redacted for record id={id}")]
    RedactedPayload {
        /// 該当レコードの ID 文字列。
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_error_vault_locked_display() {
        let e = ExportError::VaultLocked;
        assert!(e.to_string().contains("vault is locked"));
    }

    #[test]
    fn import_validation_error_unknown_format_version_display() {
        let e = ImportValidationError::UnknownFormatVersion { found: 99 };
        assert!(e.to_string().contains("99"));
    }

    #[test]
    fn import_validation_error_redacted_payload_display() {
        let e = ImportValidationError::RedactedPayload {
            id: "some-id".to_owned(),
        };
        assert!(e.to_string().contains("redacted"));
        assert!(e.to_string().contains("some-id"));
    }
}
