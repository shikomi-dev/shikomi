//! 結合テスト — UseCase 横断パラメタライズ
//!
//! 対応 REQ: REQ-CLI-009, REQ-CLI-010
//! 対応 Issue: #20
//! 対応 TC: TC-IT-040 / TC-IT-050
//! 設計書: `docs/features/cli-vault-commands/test-design/integration.md §4.5`

mod common;

use shikomi_cli::error::CliError;
use shikomi_cli::input::{AddInput, ConfirmedRemoveInput, EditInput};
use shikomi_cli::usecase::{add::add_record, edit::edit_record, list::list_records, remove::remove_record};
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use uuid::Uuid;

use common::{fixed_time, fixtures, fresh_repo};

/// 暗号化 vault の内容（`vault.db` の SHA-256 ではなくサイズ + mtime）を採取する
/// シンプルな不変性観測ヘルパー。副作用ゼロの UseCase 呼び出し前後で値が一致する
/// ことを確認する目的。
fn snapshot(path: &std::path::Path) -> (u64, Vec<u8>) {
    let meta = std::fs::metadata(path).expect("metadata");
    let size = meta.len();
    let bytes = std::fs::read(path).expect("read vault.db");
    (size, bytes)
}

// -------------------------------------------------------------------
// TC-IT-040: 暗号化 vault に対して 4 UseCase 全てが EncryptionUnsupported
// + vault 内容が変化しないこと（契約: 副作用ゼロ）
// -------------------------------------------------------------------

#[test]
fn tc_it_040_all_usecases_on_encrypted_vault_return_encryption_unsupported_without_side_effects() {
    let (dir, repo) = fresh_repo();
    fixtures::create_encrypted_vault(dir.path()).unwrap();
    let vault_db = dir.path().join("vault.db");
    let before = snapshot(&vault_db);

    // list
    let err = list_records(&repo, dir.path()).expect_err("list should err");
    assert!(matches!(err, CliError::EncryptionUnsupported));

    // add
    let err = add_record(
        &repo,
        AddInput {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("L".to_owned()).unwrap(),
            value: SecretString::from_string("V".to_owned()),
        },
        fixed_time(),
    )
    .expect_err("add should err");
    assert!(matches!(err, CliError::EncryptionUnsupported));

    // edit
    let err = edit_record(
        &repo,
        EditInput {
            id: RecordId::new(Uuid::now_v7()).unwrap(),
            label: Some(RecordLabel::try_new("L".to_owned()).unwrap()),
            value: None,
        },
        fixed_time(),
        dir.path(),
    )
    .expect_err("edit should err");
    assert!(matches!(err, CliError::EncryptionUnsupported));

    // remove
    let err = remove_record(
        &repo,
        ConfirmedRemoveInput::new(RecordId::new(Uuid::now_v7()).unwrap()),
        dir.path(),
    )
    .expect_err("remove should err");
    assert!(matches!(err, CliError::EncryptionUnsupported));

    let after = snapshot(&vault_db);
    assert_eq!(
        before.0, after.0,
        "vault.db size changed under Encrypted Fail Fast path"
    );
    assert_eq!(
        before.1, after.1,
        "vault.db bytes changed under Encrypted Fail Fast path"
    );
}

// -------------------------------------------------------------------
// TC-IT-050: vault 未初期化で list / edit / remove 全て VaultNotInitialized
// （add は自動初期化のため対象外）
// -------------------------------------------------------------------

#[test]
fn tc_it_050_list_edit_remove_on_uninitialized_vault_all_return_vault_not_initialized() {
    let (dir, repo) = fresh_repo();

    let err = list_records(&repo, dir.path()).expect_err("list");
    assert!(matches!(err, CliError::VaultNotInitialized(_)));

    let err = edit_record(
        &repo,
        EditInput {
            id: RecordId::new(Uuid::now_v7()).unwrap(),
            label: Some(RecordLabel::try_new("L".to_owned()).unwrap()),
            value: None,
        },
        fixed_time(),
        dir.path(),
    )
    .expect_err("edit");
    assert!(matches!(err, CliError::VaultNotInitialized(_)));

    let err = remove_record(
        &repo,
        ConfirmedRemoveInput::new(RecordId::new(Uuid::now_v7()).unwrap()),
        dir.path(),
    )
    .expect_err("remove");
    assert!(matches!(err, CliError::VaultNotInitialized(_)));
}
