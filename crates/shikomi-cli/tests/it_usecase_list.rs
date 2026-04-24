//! 結合テスト — `usecase::list::list_records`
//!
//! 対応 REQ: REQ-CLI-001, REQ-CLI-007, REQ-CLI-010
//! 対応 Issue: #20
//! 対応 TC: TC-IT-001 / TC-IT-002 / TC-IT-003
//! 設計書: `docs/features/cli-vault-commands/test-design/integration.md §4.1`

mod common;

use shikomi_cli::error::CliError;
use shikomi_cli::usecase::list::list_records;
use shikomi_cli::view::ValueView;
use shikomi_core::{
    Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
    VaultVersion,
};
use shikomi_infra::persistence::VaultRepository;
use uuid::Uuid;

use common::{fixed_time, fresh_repo};

// -------------------------------------------------------------------
// TC-IT-001: list_records — 空 vault（records 0 件）は Ok(Vec::new())
// -------------------------------------------------------------------

#[test]
fn tc_it_001_list_records_empty_vault_returns_empty_vec() {
    let (dir, repo) = fresh_repo();
    // 空 vault を初期化（record 0 件）
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_time()).unwrap();
    let vault = Vault::new(header);
    repo.save(&vault).expect("save empty vault");

    let result = list_records(&repo, dir.path()).expect("list_records");
    assert!(
        result.is_empty(),
        "empty vault should yield empty Vec, got {result:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-002: list_records — 3 件 mixed (Text x 2, Secret x 1)
// -------------------------------------------------------------------

#[test]
fn tc_it_002_list_records_mixed_kinds_returns_three_views_with_correct_masking() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, now).unwrap();
    let mut vault = Vault::new(header);

    // Text x 2
    for (label, value) in [
        ("url-1", "https://example.com/a"),
        ("url-2", "https://example.com/b"),
    ] {
        let record = Record::new(
            RecordId::new(Uuid::now_v7()).unwrap(),
            RecordKind::Text,
            RecordLabel::try_new(label.to_owned()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string(value.to_owned())),
            now,
        );
        vault.add_record(record).unwrap();
    }
    // Secret x 1
    let record = Record::new(
        RecordId::new(Uuid::now_v7()).unwrap(),
        RecordKind::Secret,
        RecordLabel::try_new("pw".to_owned()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("SECRET_TEST_VALUE".to_owned())),
        now,
    );
    vault.add_record(record).unwrap();
    repo.save(&vault).expect("save");

    let views = list_records(&repo, dir.path()).expect("list_records");
    assert_eq!(views.len(), 3, "expected 3 views, got {}", views.len());

    // Secret は 1 件、Text は 2 件、Secret は Masked のみ
    let secret_views: Vec<_> = views
        .iter()
        .filter(|v| matches!(v.kind, RecordKind::Secret))
        .collect();
    assert_eq!(secret_views.len(), 1);
    assert!(matches!(secret_views[0].value, ValueView::Masked));

    let text_views: Vec<_> = views
        .iter()
        .filter(|v| matches!(v.kind, RecordKind::Text))
        .collect();
    assert_eq!(text_views.len(), 2);
    for v in text_views {
        assert!(
            matches!(v.value, ValueView::Plain(_)),
            "Text kind should yield Plain"
        );
    }
}

// -------------------------------------------------------------------
// TC-IT-003: list_records — exists()=false（vault.db 不在）で VaultNotInitialized
// -------------------------------------------------------------------

#[test]
fn tc_it_003_list_records_vault_db_missing_returns_vault_not_initialized() {
    let (dir, repo) = fresh_repo();
    // save() を呼ばない → vault.db は存在しない
    let err = list_records(&repo, dir.path()).expect_err("expected error");
    assert!(
        matches!(err, CliError::VaultNotInitialized(_)),
        "expected VaultNotInitialized, got {err:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-002 補: list_records の返却値に secret 原文が含まれないこと
// (secret マスキングの横串検証、REQ-CLI-007)
// -------------------------------------------------------------------

#[test]
fn tc_it_002b_list_records_debug_representation_excludes_secret_value() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, now).unwrap();
    let mut vault = Vault::new(header);
    vault
        .add_record(Record::new(
            RecordId::new(Uuid::now_v7()).unwrap(),
            RecordKind::Secret,
            RecordLabel::try_new("pw".to_owned()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("SECRET_TEST_VALUE".to_owned())),
            now,
        ))
        .unwrap();
    repo.save(&vault).unwrap();

    let views = list_records(&repo, dir.path()).unwrap();
    let dbg = format!("{views:?}");
    assert!(
        !dbg.contains("SECRET_TEST_VALUE"),
        "RecordView Debug must not leak secret: {dbg}"
    );
}
