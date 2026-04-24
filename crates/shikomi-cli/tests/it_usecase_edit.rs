//! 結合テスト — `usecase::edit::edit_record`
//!
//! 対応 REQ: REQ-CLI-003, REQ-CLI-009
//! 対応 Issue: #20
//! 対応 TC: TC-IT-020 / TC-IT-021 / TC-IT-022 / TC-IT-023
//! 設計書: `docs/features/cli-vault-commands/test-design/integration.md §4.3`
//!
//! NOTE: TC-IT-024（label/value 共に None で UsageError）は `edit_record` の事前条件
//! 検証ではなく `lib.rs::run_edit` 側の責務のため、本結合テストでは対応しない
//! （設計書 §4.3 注記参照）。

mod common;

use shikomi_cli::error::CliError;
use shikomi_cli::input::{AddInput, EditInput};
use shikomi_cli::usecase::{add::add_record, edit::edit_record, list::list_records};
use shikomi_cli::view::ValueView;
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use uuid::Uuid;

use common::{fixed_time, fixtures, fresh_repo};

// -------------------------------------------------------------------
// TC-IT-020: edit_record — label のみ更新 → value 不変
// -------------------------------------------------------------------

#[test]
fn tc_it_020_edit_record_updates_only_label_when_value_is_none() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    // 前提: 1 件存在
    let id = add_record(
        &repo,
        AddInput {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("OLD".to_owned()).unwrap(),
            value: SecretString::from_string("ORIG_VALUE".to_owned()),
        },
        now,
    )
    .unwrap();

    let edit_now = now + time::Duration::seconds(1);
    let input = EditInput {
        id: id.clone(),
        label: Some(RecordLabel::try_new("NEW".to_owned()).unwrap()),
        value: None,
    };
    let returned_id = edit_record(&repo, input, edit_now, dir.path()).expect("edit_record");
    assert_eq!(returned_id, id);

    // ラウンドトリップ: label が NEW に更新、value は ORIG_VALUE
    let views = list_records(&repo, dir.path()).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].label.as_str(), "NEW");
    match &views[0].value {
        ValueView::Plain(s) => assert_eq!(s, "ORIG_VALUE"),
        ValueView::Masked => panic!("Text kind should yield Plain"),
    }
}

// -------------------------------------------------------------------
// TC-IT-021: edit_record — label と value 両方更新
// -------------------------------------------------------------------

#[test]
fn tc_it_021_edit_record_updates_both_fields_when_both_are_some() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    let id = add_record(
        &repo,
        AddInput {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("OLD".to_owned()).unwrap(),
            value: SecretString::from_string("OLD_VAL".to_owned()),
        },
        now,
    )
    .unwrap();

    let edit_now = now + time::Duration::seconds(1);
    let input = EditInput {
        id: id.clone(),
        label: Some(RecordLabel::try_new("NEW".to_owned()).unwrap()),
        value: Some(SecretString::from_string("NEW_VAL".to_owned())),
    };
    edit_record(&repo, input, edit_now, dir.path()).expect("edit_record");

    let views = list_records(&repo, dir.path()).unwrap();
    assert_eq!(views[0].label.as_str(), "NEW");
    match &views[0].value {
        ValueView::Plain(s) => assert_eq!(s, "NEW_VAL"),
        ValueView::Masked => panic!("Text kind should yield Plain"),
    }
}

// -------------------------------------------------------------------
// TC-IT-022: edit_record — 存在しない id で RecordNotFound
// -------------------------------------------------------------------

#[test]
fn tc_it_022_edit_record_with_nonexistent_id_returns_record_not_found() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    // 1 件を追加しておく（vault 初期化のため）
    add_record(
        &repo,
        AddInput {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("L".to_owned()).unwrap(),
            value: SecretString::from_string("V".to_owned()),
        },
        now,
    )
    .unwrap();

    let missing_id = RecordId::new(Uuid::now_v7()).unwrap();
    let input = EditInput {
        id: missing_id.clone(),
        label: Some(RecordLabel::try_new("X".to_owned()).unwrap()),
        value: None,
    };
    let err = edit_record(&repo, input, now, dir.path()).expect_err("expected error");
    match err {
        CliError::RecordNotFound(id) => assert_eq!(id, missing_id),
        other => panic!("expected RecordNotFound, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-023: edit_record — 暗号化 vault で EncryptionUnsupported
// -------------------------------------------------------------------

#[test]
fn tc_it_023_edit_record_on_encrypted_vault_returns_encryption_unsupported() {
    let (dir, repo) = fresh_repo();
    fixtures::create_encrypted_vault(dir.path()).unwrap();

    let input = EditInput {
        id: RecordId::new(Uuid::now_v7()).unwrap(),
        label: Some(RecordLabel::try_new("L".to_owned()).unwrap()),
        value: None,
    };
    let err =
        edit_record(&repo, input, fixed_time(), dir.path()).expect_err("expected error");
    assert!(
        matches!(err, CliError::EncryptionUnsupported),
        "expected EncryptionUnsupported, got {err:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-050 (edit 部分): vault 未初期化で VaultNotInitialized
// -------------------------------------------------------------------

#[test]
fn tc_it_050a_edit_record_on_uninitialized_vault_returns_vault_not_initialized() {
    let (dir, repo) = fresh_repo();
    let input = EditInput {
        id: RecordId::new(Uuid::now_v7()).unwrap(),
        label: Some(RecordLabel::try_new("L".to_owned()).unwrap()),
        value: None,
    };
    let err =
        edit_record(&repo, input, fixed_time(), dir.path()).expect_err("expected error");
    assert!(
        matches!(err, CliError::VaultNotInitialized(_)),
        "expected VaultNotInitialized, got {err:?}"
    );
}
