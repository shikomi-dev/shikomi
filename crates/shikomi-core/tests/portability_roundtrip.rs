//! データポータビリティ serde ラウンドトリップ統合テスト（TC-UT-180〜187）。
//!
//! `serde_json` は dev-dependency のため統合テストファイルに記述する。

use std::collections::HashSet;

use shikomi_core::portability::{
    ExportPayload, ExportRecord, ExportRecordPayload, ImportPayload, ImportValidationError,
    ImportValidator, EXPORT_FORMAT_VERSION,
};
use shikomi_core::{
    Hotkey, Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString,
};
use time::OffsetDateTime;

fn make_record(kind: RecordKind, label: &str, value: &str) -> Record {
    let id = RecordId::new(uuid::Uuid::now_v7()).unwrap();
    let label = RecordLabel::try_new(label.to_owned()).unwrap();
    let payload = RecordPayload::Plaintext(SecretString::from_string(value.to_owned()));
    Record::new(id, kind, label, payload, OffsetDateTime::UNIX_EPOCH)
}

// --- TC-UT-180: Redacted JSON 表現 ---
#[test]
fn tc_ut_180_redacted_serializes_to_tagged_union() {
    let r = ExportRecordPayload::Redacted;
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["kind"], "redacted");
    assert!(
        json.get("value").is_none(),
        "Redacted must not contain 'value' field"
    );
}

// --- TC-UT-181: Plaintext JSON 表現 ---
#[test]
fn tc_ut_181_plaintext_serializes_to_tagged_union() {
    let p = ExportRecordPayload::Plaintext {
        value: "hello".to_owned(),
    };
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["kind"], "plaintext");
    assert_eq!(json["value"], "hello");
}

// --- TC-UT-182: ExportRecord::try_from — hotkey=Some でフィールドが正しくマッピングされる ---
#[test]
fn tc_ut_182_export_record_try_from_record_with_hotkey() {
    let id = RecordId::new(uuid::Uuid::now_v7()).unwrap();
    let label = RecordLabel::try_new("my-label".to_owned()).unwrap();
    let payload = RecordPayload::Plaintext(SecretString::from_string("my-value".to_owned()));
    let hotkey = Hotkey::parse("ctrl+1").unwrap();
    let record = Record::new(
        id,
        RecordKind::Text,
        label,
        payload,
        OffsetDateTime::UNIX_EPOCH,
    )
    .with_hotkey(hotkey);

    let export = ExportRecord::try_from((&record, false)).unwrap();

    assert_eq!(export.id, record.id().to_string());
    assert_eq!(export.kind, RecordKind::Text);
    assert_eq!(export.label, "my-label");
    // Hotkey::as_str() 正規化文字列変換の検証: "ctrl+1" に正規化される
    assert_eq!(export.hotkey, Some("ctrl+1".to_owned()));
    assert!(!export.created_at.is_empty());
    assert!(!export.updated_at.is_empty());
}

// --- TC-UT-183: ExportRecord::try_from — hotkey=None / Plaintext ---
#[test]
fn tc_ut_183_export_record_try_from_record_hotkey_none() {
    let record = make_record(RecordKind::Text, "my-label", "my-value");
    let export = ExportRecord::try_from((&record, false)).unwrap();

    let json = serde_json::to_value(&export).unwrap();
    assert_eq!(json["kind"], "text");
    assert_eq!(json["label"], "my-label");
    assert_eq!(json["payload"]["kind"], "plaintext");
    assert_eq!(json["payload"]["value"], "my-value");
    assert!(json["hotkey"].is_null(), "hotkey should be null when None");
}

// --- TC-UT-184: ExportPayload::new — format_version=1 が含まれる ---
#[test]
fn tc_ut_184_export_payload_contains_format_version_one() {
    let payload = ExportPayload::new(vec![], "test-vault".to_owned(), OffsetDateTime::UNIX_EPOCH);
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["format_version"], EXPORT_FORMAT_VERSION);
    assert_eq!(json["vault_name"], "test-vault");
    assert!(json["records"].as_array().unwrap().is_empty());
}

// --- TC-UT-185: ExportPayload → JSON → ImportPayload serde ラウンドトリップ ---
#[test]
fn tc_ut_185_export_payload_roundtrip_via_json() {
    let record = make_record(RecordKind::Text, "rt-label", "rt-value");
    let export_record = ExportRecord::try_from((&record, false)).unwrap();

    let export_payload = ExportPayload::new(
        vec![export_record.clone()],
        "roundtrip-vault".to_owned(),
        OffsetDateTime::UNIX_EPOCH,
    );

    let json_str = serde_json::to_string(&export_payload).unwrap();
    let import_payload: ImportPayload = serde_json::from_str(&json_str).unwrap();

    assert_eq!(import_payload.format_version, EXPORT_FORMAT_VERSION);
    assert_eq!(import_payload.vault_name, "roundtrip-vault");
    assert_eq!(import_payload.records.len(), 1);
    assert_eq!(import_payload.records[0].id, export_record.id);
    assert_eq!(import_payload.records[0].label, export_record.label);
    assert_eq!(import_payload.records[0].payload, export_record.payload);
}

// --- TC-UT-186b: ImportValidator — JSON パース経由: format_version=2 は UnknownFormatVersion ---
// JSON 文字列からデシリアライズした ImportPayload での統合検証（import.rs TC-UT-186 の JSON 経路補完）
#[test]
fn tc_ut_186b_format_version_two_is_rejected_via_json_parse() {
    let json_str = r#"{"format_version":2,"exported_at":"1970-01-01T00:00:00Z","vault_name":"v","records":[]}"#;
    let payload: ImportPayload = serde_json::from_str(json_str).unwrap();
    let result = ImportValidator::validate(&payload, &HashSet::new());
    assert!(matches!(
        result,
        Err(ImportValidationError::UnknownFormatVersion { found: 2 })
    ));
}

// --- TC-UT-187b: ImportValidator — JSON パース経由: format_version=1 は受理 ---
// JSON 文字列からデシリアライズした ImportPayload での統合検証（import.rs TC-UT-187 の JSON 経路補完）
#[test]
fn tc_ut_187b_format_version_one_is_accepted_via_json_parse() {
    let json_str = r#"{"format_version":1,"exported_at":"1970-01-01T00:00:00Z","vault_name":"v","records":[]}"#;
    let payload: ImportPayload = serde_json::from_str(json_str).unwrap();
    let report = ImportValidator::validate(&payload, &HashSet::new()).unwrap();
    assert!(report.conflicting_ids.is_empty());
}
