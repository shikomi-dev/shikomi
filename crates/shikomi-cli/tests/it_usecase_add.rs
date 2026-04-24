//! 結合テスト — `usecase::add::add_record`
//!
//! 対応 REQ: REQ-CLI-002, REQ-CLI-007, REQ-CLI-009
//! 対応 Issue: #20
//! 対応 TC: TC-IT-010 / TC-IT-011 / TC-IT-012 / TC-IT-013
//! 設計書: `docs/features/cli-vault-commands/test-design/integration.md §4.2`

mod common;

use shikomi_cli::error::CliError;
use shikomi_cli::input::AddInput;
use shikomi_cli::usecase::{add::add_record, list::list_records};
use shikomi_cli::view::ValueView;
use shikomi_core::{RecordKind, RecordLabel, SecretString};

use common::{fixed_time, fixtures, fresh_repo};

// -------------------------------------------------------------------
// TC-IT-010: add_record — vault 未作成 + Text 入力 → 自動初期化 + Round trip
// -------------------------------------------------------------------

#[test]
fn tc_it_010_add_record_text_auto_initializes_vault_and_roundtrips() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    let input = AddInput {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("L".to_owned()).unwrap(),
        value: SecretString::from_string("V".to_owned()),
    };
    let id = add_record(&repo, input, now).expect("add_record");

    // ラウンドトリップ: list_records で 1 件、Plain("V")
    let views = list_records(&repo, dir.path()).expect("list_records");
    assert_eq!(views.len(), 1);
    assert_eq!(&views[0].id, &id);
    match &views[0].value {
        ValueView::Plain(s) => assert_eq!(s, "V"),
        ValueView::Masked => panic!("expected Plain for Text kind"),
    }
}

// -------------------------------------------------------------------
// TC-IT-011: add_record — Secret → 保存成功 + list で Masked + Debug に secret 不在
// -------------------------------------------------------------------

#[test]
fn tc_it_011_add_record_secret_persists_as_masked_view_without_leaking_value() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    let input = AddInput {
        kind: RecordKind::Secret,
        label: RecordLabel::try_new("S".to_owned()).unwrap(),
        value: SecretString::from_string("SECRET_TEST_VALUE".to_owned()),
    };
    let id = add_record(&repo, input, now).expect("add_record secret");

    let views = list_records(&repo, dir.path()).expect("list_records");
    assert_eq!(views.len(), 1);
    assert_eq!(&views[0].id, &id);
    assert!(
        matches!(views[0].value, ValueView::Masked),
        "Secret kind must become Masked view"
    );

    // Debug 表現に secret 値が含まれない
    let dbg = format!("{views:?}");
    assert!(
        !dbg.contains("SECRET_TEST_VALUE"),
        "Debug output must not leak secret: {dbg}"
    );
}

// -------------------------------------------------------------------
// TC-IT-012: add_record — 暗号化 vault で EncryptionUnsupported
// -------------------------------------------------------------------

#[test]
fn tc_it_012_add_record_on_encrypted_vault_returns_encryption_unsupported() {
    let (dir, repo) = fresh_repo();
    fixtures::create_encrypted_vault(dir.path()).expect("create encrypted vault");

    let input = AddInput {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("L".to_owned()).unwrap(),
        value: SecretString::from_string("V".to_owned()),
    };
    let err = add_record(&repo, input, fixed_time()).expect_err("expected error");
    assert!(
        matches!(err, CliError::EncryptionUnsupported),
        "expected EncryptionUnsupported, got {err:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-013: add_record — 不正ラベル（空文字）は RecordLabel::try_new の段階で拒否
// （AddInput の構築時点で domain 検証されるため、本結合テストでは
//   "try_new が DomainError を返す" ことを確認）
// -------------------------------------------------------------------

#[test]
fn tc_it_013_invalid_empty_label_is_rejected_at_record_label_try_new() {
    let result = RecordLabel::try_new(String::new());
    assert!(
        result.is_err(),
        "empty label should be rejected at try_new stage"
    );
    // run() では CliError::InvalidLabel(domain) でユーザー向けに整形される。
    // その検証は E2E テスト (TC-E2E-015) で実施する。
}
