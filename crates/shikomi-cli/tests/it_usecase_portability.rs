//! 結合テスト — `usecase::portability::export_records` / `import_records`
//!
//! 対応 REQ: REQ-DP-008 / REQ-DP-009 / REQ-DP-010
//! 対応 TC: TC-IT-DP-001〜004
//! 設計書: `docs/features/data-portability/cli/test-design.md §5.2`
//! 対応 Issue: #141

mod common;

use shikomi_cli::cli::{ExportArgs, ImportArgs, OnConflictArg};
use shikomi_cli::error::CliError;
use shikomi_cli::usecase::portability::{export::export_records, import::import_records};
use shikomi_core::{Hotkey, RecordKind, RecordLabel, RecordPayload, SecretString};
use shikomi_infra::persistence::VaultRepository;

use common::{fixed_time, fresh_repo};

// -------------------------------------------------------------------
// TC-IT-DP-001: export_records — vault 空でも ExportSummary { record_count: 0 }
// -------------------------------------------------------------------

/// TC-IT-DP-001: vault が存在しない状態でも 0 件 export として正常終了する。
///
/// REQ-DP-008 の「vault 不存在は 0 件 export として扱う」保証。
#[test]
fn tc_it_dp_001_export_records_empty_vault_returns_summary_with_zero_count() {
    let (dir, repo) = fresh_repo();
    let out = dir.path().join("out.json");
    let args = ExportArgs {
        output: out.clone(),
        export_secrets: false,
        force: false,
    };

    let summary = export_records(&repo, &args, dir.path(), fixed_time())
        .expect("export_records should succeed for empty vault");

    assert_eq!(summary.record_count, 0);
    assert!(
        out.exists(),
        "output file should be created even for empty vault"
    );

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("\"format_version\": 1"),
        "empty export should still contain format_version: 1"
    );
}

// -------------------------------------------------------------------
// TC-IT-DP-002: import_records — JSON パース失敗 → ImportDeserializationFailed
// -------------------------------------------------------------------

/// TC-IT-DP-002: 不正な JSON ファイルを import すると `ImportDeserializationFailed` が返る。
///
/// REQ-DP-009 の「JSON パース失敗時の適切なエラー返却」保証。
#[test]
fn tc_it_dp_002_import_records_broken_json_returns_deserialization_failed() {
    let (dir, repo) = fresh_repo();
    let broken = dir.path().join("broken.json");
    std::fs::write(&broken, "{invalid json").unwrap();

    let args = ImportArgs {
        input: broken,
        on_conflict: OnConflictArg::Error,
    };

    let result = import_records(&repo, &args, fixed_time());

    assert!(
        matches!(result, Err(CliError::ImportDeserializationFailed { .. })),
        "expected ImportDeserializationFailed, got: {result:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-DP-003: import_records — format_version:999 → ImportValidationFailed(UnknownFormatVersion)
// -------------------------------------------------------------------

/// TC-IT-DP-003: `format_version: 999` の JSON を import すると `ImportValidationFailed` が返る。
///
/// REQ-DP-009/010 の「不明フォーマットバージョン検出」保証。
#[test]
fn tc_it_dp_003_import_records_unknown_format_version_returns_validation_failed() {
    let (dir, repo) = fresh_repo();
    let v999 = dir.path().join("v999.json");
    std::fs::write(
        &v999,
        r#"{"format_version":999,"vault_name":"test","exported_at":"1970-01-01T00:00:00Z","records":[]}"#,
    )
    .unwrap();

    let args = ImportArgs {
        input: v999,
        on_conflict: OnConflictArg::Error,
    };

    let result = import_records(&repo, &args, fixed_time());

    assert!(
        matches!(
            result,
            Err(CliError::ImportValidationFailed(
                shikomi_core::portability::ImportValidationError::UnknownFormatVersion {
                    found: 999
                }
            ))
        ),
        "expected ImportValidationFailed(UnknownFormatVersion {{ found: 999 }}), got: {result:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-DP-004: import_records — hotkey フィールドが復元される（R1-DP-10）
// -------------------------------------------------------------------

/// TC-IT-DP-004: hotkey フィールドが export → import で正しく復元される。
///
/// REQ-DP-009/010（R1-DP-10 hotkey フィールド復元）の保証。
#[test]
fn tc_it_dp_004_import_records_hotkey_is_restored() {
    use shikomi_core::{Record, RecordId, Vault, VaultHeader, VaultVersion};
    use uuid::Uuid;

    // vault A にホットキー付きのレコードを追加
    let (dir_a, repo_a) = fresh_repo();
    let now = fixed_time();
    let id = RecordId::new(Uuid::now_v7()).unwrap();
    let hotkey = Hotkey::parse("ctrl+2").unwrap();
    let record = Record::new(
        id.clone(),
        RecordKind::Text,
        RecordLabel::try_new("hotkey-label".to_owned()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("hotkey-value".to_owned())),
        now,
    )
    .with_hotkey(hotkey);

    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, now).unwrap();
    let mut vault_a = Vault::new(header);
    vault_a.add_record(record).unwrap();
    repo_a.save(&vault_a).unwrap();

    // vault A を export
    let out = dir_a.path().join("with_hotkey.json");
    let export_args = ExportArgs {
        output: out.clone(),
        export_secrets: false,
        force: false,
    };
    export_records(&repo_a, &export_args, dir_a.path(), now).expect("export should succeed");

    // vault B に import
    let (_dir_b, repo_b) = fresh_repo();
    let import_args = ImportArgs {
        input: out,
        on_conflict: OnConflictArg::Error,
    };
    let summary = import_records(&repo_b, &import_args, now).expect("import should succeed");
    assert_eq!(summary.added, 1);

    // vault B のレコードの hotkey が "ctrl+2" に復元されていること
    let vault_b = repo_b.load().unwrap();
    let records: Vec<_> = vault_b.records().iter().collect();
    assert_eq!(records.len(), 1);
    let hotkey_str = records[0].hotkey().map(|h| h.as_str().to_owned());
    assert_eq!(
        hotkey_str,
        Some("ctrl+2".to_owned()),
        "hotkey should be restored as 'ctrl+2'"
    );
}
