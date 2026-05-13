//! インポート用ドメイン型。
//!
//! `ImportRecord`: `ExportRecord` の type alias（DRY / KISS）。
//! `ImportPayload`: import ファイルのルート JSON オブジェクト（`Deserialize` のみ）。
//! `ImportValidator`: import バリデーション責務を持つステートレス型。
//! `ImportValidationReport`: バリデーション結果。

use std::collections::HashSet;

use serde::Deserialize;

use super::error::ImportValidationError;
use super::export::{ExportRecord, EXPORT_FORMAT_VERSION};

// -------------------------------------------------------------------
// ImportRecord（type alias）
// -------------------------------------------------------------------

/// `ExportRecord` の type alias。
///
/// フィールド定義を 2 箇所で管理しない（DRY / KISS）。
/// バリデーション責務は `ImportValidator` が保持するため、独立型にする必要がない。
/// `deny_unknown_fields` は使用しない（将来バージョンの追加フィールドへの前方互換を保つ）。
pub type ImportRecord = ExportRecord;

// -------------------------------------------------------------------
// ImportPayload
// -------------------------------------------------------------------

/// import ファイルのルート JSON オブジェクト（`Deserialize` のみ）。
///
/// `ExportPayload` と同一フィールド定義だが独立型とする理由:
/// `ImportValidator::validate()` を呼び出すファサードとしてのバリデーション責務を保有するため。
#[derive(Debug, Clone, Deserialize)]
pub struct ImportPayload {
    /// フォーマットバージョン。
    pub format_version: u32,
    /// エクスポート実行時刻文字列（RFC 3339）。
    pub exported_at: String,
    /// vault 名。
    pub vault_name: String,
    /// インポートレコード一覧。
    pub records: Vec<ImportRecord>,
}

// -------------------------------------------------------------------
// ImportWarning
// -------------------------------------------------------------------

/// import 時の警告。エラーではなく継続可能な状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWarning {
    /// `records` が空の import ファイル。
    EmptyImport,
}

// -------------------------------------------------------------------
// ImportValidationReport
// -------------------------------------------------------------------

/// `ImportValidator::validate` の正常系戻り値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportValidationReport {
    /// 既存 vault と ID が衝突するレコードの ID 文字列一覧。
    ///
    /// 空の場合は衝突なし。戦略適用（skip / overwrite / error）は呼び出し側（UseCase）の責務。
    pub conflicting_ids: Vec<String>,
    /// 警告一覧。`records` が空の場合に `EmptyImport` 警告を含む。
    pub warnings: Vec<ImportWarning>,
}

// -------------------------------------------------------------------
// ImportValidator
// -------------------------------------------------------------------

/// import バリデーション責務を持つステートレス型。
///
/// バリデーション順序（`basic-design.md §REQ-DP-005`）:
/// 1. `format_version` 検証
/// 2. ファイル内 ID 重複検出
/// 3. `Redacted` payload 検出
/// 4. 既存 vault との衝突 ID 収集
pub struct ImportValidator;

impl ImportValidator {
    /// `ImportPayload` を検証し、`ImportValidationReport` を返す。
    ///
    /// # 処理順序
    /// 1. `format_version > EXPORT_FORMAT_VERSION` → `UnknownFormatVersion` エラー
    /// 2. `records` 内の `id` 重複検出 → `DuplicateIdInFile` エラー（最初の重複 ID のみ）
    /// 3. `payload.kind == "redacted"` 検出 → `RedactedPayload` エラー（最初の 1 件のみ）
    /// 4. `existing_ids` との衝突 ID 収集 → `conflicting_ids` に格納
    ///
    /// # Errors
    /// バリデーション失敗時に `ImportValidationError` を返す。
    pub fn validate(
        payload: &ImportPayload,
        existing_ids: &HashSet<String>,
    ) -> Result<ImportValidationReport, ImportValidationError> {
        // 1. format_version 検証
        if payload.format_version > EXPORT_FORMAT_VERSION {
            return Err(ImportValidationError::UnknownFormatVersion {
                found: payload.format_version,
            });
        }

        // 2. ファイル内 ID 重複検出
        let mut seen_ids: HashSet<&str> = HashSet::new();
        for record in &payload.records {
            if !seen_ids.insert(record.id.as_str()) {
                return Err(ImportValidationError::DuplicateIdInFile {
                    id: record.id.clone(),
                });
            }
        }

        // 3. Redacted payload 検出
        use super::export::ExportRecordPayload;
        for record in &payload.records {
            if record.payload == ExportRecordPayload::Redacted {
                return Err(ImportValidationError::RedactedPayload {
                    id: record.id.clone(),
                });
            }
        }

        // 4. 既存 vault との衝突 ID 収集
        let conflicting_ids: Vec<String> = payload
            .records
            .iter()
            .filter(|r| existing_ids.contains(r.id.as_str()))
            .map(|r| r.id.clone())
            .collect();

        // 5. 警告
        let mut warnings = Vec::new();
        if payload.records.is_empty() {
            warnings.push(ImportWarning::EmptyImport);
        }

        Ok(ImportValidationReport {
            conflicting_ids,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portability::export::{ExportRecord, ExportRecordPayload};
    use crate::RecordKind;

    fn make_plaintext_record(id: &str) -> ImportRecord {
        ExportRecord {
            id: id.to_owned(),
            kind: RecordKind::Text,
            label: "label".to_owned(),
            payload: ExportRecordPayload::Plaintext {
                value: "value".to_owned(),
            },
            created_at: "2026-05-12T00:00:00Z".to_owned(),
            updated_at: "2026-05-12T00:00:00Z".to_owned(),
            hotkey: None,
        }
    }

    fn make_redacted_record(id: &str) -> ImportRecord {
        ExportRecord {
            id: id.to_owned(),
            kind: RecordKind::Secret,
            label: "label".to_owned(),
            payload: ExportRecordPayload::Redacted,
            created_at: "2026-05-12T00:00:00Z".to_owned(),
            updated_at: "2026-05-12T00:00:00Z".to_owned(),
            hotkey: None,
        }
    }

    fn make_payload(records: Vec<ImportRecord>) -> ImportPayload {
        ImportPayload {
            format_version: 1,
            exported_at: "2026-05-12T00:00:00Z".to_owned(),
            vault_name: "test".to_owned(),
            records,
        }
    }

    // --- TC-UT-186: UnknownFormatVersion ---
    #[test]
    fn tc_ut_186_unknown_format_version_returns_error() {
        let payload = ImportPayload {
            format_version: 2,
            exported_at: "2026-05-12T00:00:00Z".to_owned(),
            vault_name: "test".to_owned(),
            records: vec![],
        };
        let result = ImportValidator::validate(&payload, &HashSet::new());
        assert!(matches!(
            result,
            Err(ImportValidationError::UnknownFormatVersion { found: 2 })
        ));
    }

    // --- TC-UT-188: DuplicateIdInFile ---
    #[test]
    fn tc_ut_188_duplicate_id_returns_error() {
        let payload = make_payload(vec![
            make_plaintext_record("id-1"),
            make_plaintext_record("id-1"),
        ]);
        let result = ImportValidator::validate(&payload, &HashSet::new());
        assert!(matches!(
            result,
            Err(ImportValidationError::DuplicateIdInFile { id }) if id == "id-1"
        ));
    }

    // --- TC-UT-189: RedactedPayload ---
    #[test]
    fn tc_ut_189_redacted_payload_returns_error() {
        let payload = make_payload(vec![make_redacted_record("id-1")]);
        let result = ImportValidator::validate(&payload, &HashSet::new());
        assert!(matches!(
            result,
            Err(ImportValidationError::RedactedPayload { id }) if id == "id-1"
        ));
    }

    // --- TC-UT-190: 衝突 ID の収集 ---
    #[test]
    fn tc_ut_190_conflicting_ids_are_collected() {
        let payload = make_payload(vec![
            make_plaintext_record("id-1"),
            make_plaintext_record("id-2"),
        ]);
        let existing: HashSet<String> = ["id-1".to_owned()].into();
        let report = ImportValidator::validate(&payload, &existing).unwrap();
        assert_eq!(report.conflicting_ids, vec!["id-1"]);
        assert!(report.warnings.is_empty());
    }

    // --- TC-UT-187: 正常系（衝突なし）---
    #[test]
    fn tc_ut_187_valid_payload_returns_empty_report() {
        let payload = make_payload(vec![make_plaintext_record("id-1")]);
        let report = ImportValidator::validate(&payload, &HashSet::new()).unwrap();
        assert!(report.conflicting_ids.is_empty());
        assert!(report.warnings.is_empty());
    }

    // --- TC-UT-191: 空 records → EmptyImport 警告 ---
    #[test]
    fn tc_ut_191_empty_records_produces_warning() {
        let payload = make_payload(vec![]);
        let report = ImportValidator::validate(&payload, &HashSet::new()).unwrap();
        assert!(report.warnings.contains(&ImportWarning::EmptyImport));
    }

    // --- TC-UT-196: include_secrets=true で書き出した Plaintext は ImportValidator で受理 ---
    #[test]
    fn tc_ut_196_plaintext_payload_from_export_secrets_is_accepted() {
        // {"kind":"plaintext"} は Redacted 判定されず Ok を返す
        let payload = make_payload(vec![ExportRecord {
            id: "id-1".to_owned(),
            kind: RecordKind::Secret,
            label: "label".to_owned(),
            payload: ExportRecordPayload::Plaintext {
                value: "plain-secret".to_owned(),
            },
            created_at: "2026-05-12T00:00:00Z".to_owned(),
            updated_at: "2026-05-12T00:00:00Z".to_owned(),
            hotkey: None,
        }]);
        let result = ImportValidator::validate(&payload, &HashSet::new());
        assert!(
            result.is_ok(),
            "plaintext payload should be accepted: {result:?}"
        );
    }

    // --- TC-UT-192: バリデーション順序: format_version=999 + ID 重複 → UnknownFormatVersion 優先 ---
    #[test]
    fn tc_ut_192_validation_order_format_version_beats_duplicate_id() {
        let payload = ImportPayload {
            format_version: 999,
            exported_at: "2026-05-12T00:00:00Z".to_owned(),
            vault_name: "test".to_owned(),
            records: vec![make_plaintext_record("id-1"), make_plaintext_record("id-1")],
        };
        let result = ImportValidator::validate(&payload, &HashSet::new());
        assert!(matches!(
            result,
            Err(ImportValidationError::UnknownFormatVersion { found: 999 })
        ));
    }

    // --- TC-UT-193: バリデーション順序: ID 重複 + Redacted payload → DuplicateIdInFile 優先 ---
    #[test]
    fn tc_ut_193_validation_order_duplicate_id_beats_redacted() {
        let payload = make_payload(vec![
            make_plaintext_record("id-1"),
            make_redacted_record("id-1"),
        ]);
        let result = ImportValidator::validate(&payload, &HashSet::new());
        assert!(matches!(
            result,
            Err(ImportValidationError::DuplicateIdInFile { id }) if id == "id-1"
        ));
    }
}
