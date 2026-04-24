//! 結合テスト — `usecase::remove::remove_record`
//!
//! 対応 REQ: REQ-CLI-004, REQ-CLI-009, REQ-CLI-010
//! 対応 Issue: #20
//! 対応 TC: TC-IT-030 / TC-IT-031 / TC-IT-033
//! 設計書: `docs/features/cli-vault-commands/test-design/integration.md §4.4`
//!
//! NOTE: TC-IT-032（`bool` フィールド撤廃の型契約）は `unit.md` §TC-UT-110 の
//! doc-test 相当で検証。本結合テストでは対応しない（設計書 §4.4 注記）。

mod common;

use shikomi_cli::error::CliError;
use shikomi_cli::input::{AddInput, ConfirmedRemoveInput};
use shikomi_cli::usecase::{add::add_record, list::list_records, remove::remove_record};
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use uuid::Uuid;

use common::{fixed_time, fixtures, fresh_repo};

// -------------------------------------------------------------------
// TC-IT-030: remove_record — 既存 id を削除するとレコードが消える
// -------------------------------------------------------------------

#[test]
fn tc_it_030_remove_record_removes_existing_record_and_roundtrips_empty() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    let id = add_record(
        &repo,
        AddInput {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("L".to_owned()).unwrap(),
            value: SecretString::from_string("V".to_owned()),
        },
        now,
    )
    .unwrap();

    let input = ConfirmedRemoveInput::new(id.clone());
    let returned_id = remove_record(&repo, input, dir.path()).expect("remove_record");
    assert_eq!(returned_id, id);

    let views = list_records(&repo, dir.path()).unwrap();
    assert!(views.is_empty(), "vault should be empty after remove");
}

// -------------------------------------------------------------------
// TC-IT-031: remove_record — 存在しない id で RecordNotFound
// -------------------------------------------------------------------

#[test]
fn tc_it_031_remove_record_with_nonexistent_id_returns_record_not_found() {
    let (dir, repo) = fresh_repo();
    let now = fixed_time();

    // vault 初期化のため 1 件追加
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

    let missing = RecordId::new(Uuid::now_v7()).unwrap();
    let input = ConfirmedRemoveInput::new(missing.clone());
    let err = remove_record(&repo, input, dir.path()).expect_err("expected error");
    match err {
        CliError::RecordNotFound(id) => assert_eq!(id, missing),
        other => panic!("expected RecordNotFound, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-033: remove_record — 暗号化 vault で EncryptionUnsupported
// -------------------------------------------------------------------

#[test]
fn tc_it_033_remove_record_on_encrypted_vault_returns_encryption_unsupported() {
    let (dir, repo) = fresh_repo();
    fixtures::create_encrypted_vault(dir.path()).unwrap();

    let input = ConfirmedRemoveInput::new(RecordId::new(Uuid::now_v7()).unwrap());
    let err = remove_record(&repo, input, dir.path()).expect_err("expected error");
    assert!(
        matches!(err, CliError::EncryptionUnsupported),
        "expected EncryptionUnsupported, got {err:?}"
    );
}

// -------------------------------------------------------------------
// TC-IT-050 (remove 部分): vault 未初期化で VaultNotInitialized
// -------------------------------------------------------------------

#[test]
fn tc_it_050b_remove_record_on_uninitialized_vault_returns_vault_not_initialized() {
    let (dir, repo) = fresh_repo();
    let input = ConfirmedRemoveInput::new(RecordId::new(Uuid::now_v7()).unwrap());
    let err = remove_record(&repo, input, dir.path()).expect_err("expected error");
    assert!(
        matches!(err, CliError::VaultNotInitialized(_)),
        "expected VaultNotInitialized, got {err:?}"
    );
}
