//! `handle_request` の pure 写像を mock repo + fixed vault で検証する。
//!
//! 設計根拠: `test-design/unit.md §2.5`、`../../../detailed-design/daemon-runtime.md §handler::handle_request`。
//!
//! 対応 Issue: #26

use super::handle_request;
use shikomi_core::ipc::SerializableSecretBytes as SSB;
use shikomi_core::ipc::{IpcErrorCode, IpcRequest, IpcResponse};
use shikomi_core::{
    ProtectionMode, Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretBytes,
    SecretString, Vault, VaultHeader, VaultVersion,
};
use shikomi_infra::persistence::{PersistenceError, VaultRepository};
use std::cell::RefCell;
use std::path::PathBuf;
use time::OffsetDateTime;
use uuid::Uuid;

// -----------------------------------------------------------------
// Mock VaultRepository
// -----------------------------------------------------------------

/// save の成否を切替え可能な mock repo。save 呼出回数をカウント。
struct MockRepo {
    save_should_fail_with: RefCell<Option<PersistenceError>>,
    save_calls: RefCell<usize>,
}

impl MockRepo {
    fn ok() -> Self {
        Self {
            save_should_fail_with: RefCell::new(None),
            save_calls: RefCell::new(0),
        }
    }

    fn failing(err: PersistenceError) -> Self {
        Self {
            save_should_fail_with: RefCell::new(Some(err)),
            save_calls: RefCell::new(0),
        }
    }

    fn save_calls(&self) -> usize {
        *self.save_calls.borrow()
    }
}

impl VaultRepository for MockRepo {
    fn load(&self) -> Result<Vault, PersistenceError> {
        Err(PersistenceError::CannotResolveVaultDir)
    }
    fn save(&self, _vault: &Vault) -> Result<(), PersistenceError> {
        *self.save_calls.borrow_mut() += 1;
        if let Some(err) = self.save_should_fail_with.borrow_mut().take() {
            return Err(err);
        }
        Ok(())
    }
    fn exists(&self) -> Result<bool, PersistenceError> {
        Ok(true)
    }
}

fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
}

fn empty_vault() -> Vault {
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_time()).unwrap();
    Vault::new(header)
}

fn text_label(s: &str) -> RecordLabel {
    RecordLabel::try_new(s.to_owned()).unwrap()
}

fn secret_bytes(b: &[u8]) -> SSB {
    SSB::new(SecretBytes::from_vec(b.to_vec()))
}

fn add_text_record(vault: &mut Vault, label: &str, value: &str) -> RecordId {
    let id = RecordId::new(Uuid::now_v7()).unwrap();
    let rec = Record::new(
        id.clone(),
        RecordKind::Text,
        text_label(label),
        RecordPayload::Plaintext(SecretString::from_string(value.to_owned())),
        fixed_time(),
    );
    vault.add_record(rec).unwrap();
    id
}

// -----------------------------------------------------------------
// TC-UT-030: ListRecords 空 vault
// -----------------------------------------------------------------
#[test]
fn test_list_empty_vault_returns_empty_records() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let res = handle_request(&repo, &mut vault, IpcRequest::ListRecords);
    match res {
        IpcResponse::Records(v) => assert!(v.is_empty()),
        other => panic!("expected Records, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// TC-UT-031: ListRecords 複数（Text + Secret）— Secret は value_preview=None / masked=true
// -----------------------------------------------------------------
#[test]
fn test_list_mixed_records_secret_masked_and_text_previewed() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    add_text_record(&mut vault, "t1", "hello");
    let secret_id = RecordId::new(Uuid::now_v7()).unwrap();
    let secret = Record::new(
        secret_id,
        RecordKind::Secret,
        text_label("s1"),
        RecordPayload::Plaintext(SecretString::from_string("SECRET_TEST_VALUE".to_owned())),
        fixed_time(),
    );
    vault.add_record(secret).unwrap();

    let res = handle_request(&repo, &mut vault, IpcRequest::ListRecords);
    let IpcResponse::Records(summaries) = res else {
        panic!("expected Records");
    };
    assert_eq!(summaries.len(), 2);
    let secret_summary = summaries
        .iter()
        .find(|s| s.kind == RecordKind::Secret)
        .unwrap();
    assert_eq!(secret_summary.value_preview, None);
    assert!(secret_summary.value_masked);
    let text_summary = summaries
        .iter()
        .find(|s| s.kind == RecordKind::Text)
        .unwrap();
    assert!(text_summary.value_preview.is_some());
    assert!(!text_summary.value_masked);
}

// -----------------------------------------------------------------
// TC-UT-032: AddRecord 正常系 — Added を返し、save が 1 回呼ばれる
// -----------------------------------------------------------------
#[test]
fn test_add_text_record_returns_added_and_calls_save_once() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: text_label("L"),
        value: secret_bytes(b"V"),
        now: fixed_time(),
    };
    let res = handle_request(&repo, &mut vault, req);
    assert!(matches!(res, IpcResponse::Added { .. }));
    assert_eq!(repo.save_calls(), 1);
    assert_eq!(vault.records().len(), 1);
}

// -----------------------------------------------------------------
// TC-UT-033: AddRecord 同一 label を 2 回 → 両方 Added（label 重複は許容、ID は別）
//
// **仕様根拠**: `shikomi_core::Vault::add_record` は **DuplicateId** のみ拒否し、
// label 重複は許容する（同じ label の Text/Secret を別レコードとして共存可能）。
// 旧 TC-UT-033 は「Domain error or Added どちらでも通る」非決定的アサートだったため
// ペテルギウス指摘 §2 で却下、本契約に沿った単一期待値版へ書き直した。
// -----------------------------------------------------------------
#[test]
fn test_add_two_records_with_same_label_both_succeed_with_distinct_ids() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let req1 = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: text_label("dup"),
        value: secret_bytes(b"a"),
        now: fixed_time(),
    };
    let req2 = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: text_label("dup"),
        value: secret_bytes(b"b"),
        now: fixed_time(),
    };
    let res1 = handle_request(&repo, &mut vault, req1);
    let res2 = handle_request(&repo, &mut vault, req2);

    let id1 = match res1 {
        IpcResponse::Added { id } => id,
        other => panic!("expected Added on first add, got {other:?}"),
    };
    let id2 = match res2 {
        IpcResponse::Added { id } => id,
        other => panic!("expected Added on second add, got {other:?}"),
    };
    assert_ne!(id1, id2, "daemon must generate distinct RecordIds per add");
    assert_eq!(repo.save_calls(), 2);
    assert_eq!(vault.records().len(), 2);
    let labels: Vec<&str> = vault.records().iter().map(|r| r.label().as_str()).collect();
    assert_eq!(labels, vec!["dup", "dup"]);
}

// -----------------------------------------------------------------
// TC-UT-034: EditRecord 存在しない id → NotFound
// -----------------------------------------------------------------
#[test]
fn test_edit_nonexistent_id_returns_not_found() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let ghost_id = RecordId::new(Uuid::now_v7()).unwrap();
    let req = IpcRequest::EditRecord {
        id: ghost_id.clone(),
        label: Some(text_label("X")),
        value: None,
        now: fixed_time(),
    };
    let res = handle_request(&repo, &mut vault, req);
    match res {
        IpcResponse::Error(IpcErrorCode::NotFound { id }) => assert_eq!(id, ghost_id),
        other => panic!("expected NotFound, got {other:?}"),
    }
    assert_eq!(repo.save_calls(), 0);
}

// -----------------------------------------------------------------
// TC-UT-035: EditRecord label 更新 → Edited + save 呼出
// -----------------------------------------------------------------
#[test]
fn test_edit_label_returns_edited_and_saves() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let id = add_text_record(&mut vault, "old", "v");
    let req = IpcRequest::EditRecord {
        id: id.clone(),
        label: Some(text_label("new")),
        value: None,
        now: fixed_time() + time::Duration::seconds(1),
    };
    let res = handle_request(&repo, &mut vault, req);
    match res {
        IpcResponse::Edited { id: rid } => assert_eq!(rid, id),
        other => panic!("expected Edited, got {other:?}"),
    }
    assert_eq!(repo.save_calls(), 1);
}

// -----------------------------------------------------------------
// TC-UT-036: RemoveRecord 存在しない id → NotFound
// -----------------------------------------------------------------
#[test]
fn test_remove_nonexistent_id_returns_not_found() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let ghost_id = RecordId::new(Uuid::now_v7()).unwrap();
    let res = handle_request(
        &repo,
        &mut vault,
        IpcRequest::RemoveRecord {
            id: ghost_id.clone(),
        },
    );
    match res {
        IpcResponse::Error(IpcErrorCode::NotFound { id }) => assert_eq!(id, ghost_id),
        other => panic!("expected NotFound, got {other:?}"),
    }
    assert_eq!(repo.save_calls(), 0);
}

// -----------------------------------------------------------------
// TC-UT-037: RemoveRecord 正常 → Removed + save 呼出 + vault から消える
// -----------------------------------------------------------------
#[test]
fn test_remove_existing_record_returns_removed_and_saves() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let id = add_text_record(&mut vault, "bye", "v");
    let res = handle_request(
        &repo,
        &mut vault,
        IpcRequest::RemoveRecord { id: id.clone() },
    );
    match res {
        IpcResponse::Removed { id: rid } => assert_eq!(rid, id),
        other => panic!("expected Removed, got {other:?}"),
    }
    assert_eq!(repo.save_calls(), 1);
    assert!(vault.find_record(&id).is_none());
}

// -----------------------------------------------------------------
// TC-UT-038: Persistence 失敗 → IpcErrorCode::Persistence、reason は固定文言
// -----------------------------------------------------------------
#[test]
fn test_add_when_save_fails_returns_persistence_with_fixed_reason() {
    let err = PersistenceError::Io {
        path: PathBuf::from("/home/secret-user/shikomi/vault.db"),
        source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "leaked /home path"),
    };
    let repo = MockRepo::failing(err);
    let mut vault = empty_vault();
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: text_label("L"),
        value: secret_bytes(b"V"),
        now: fixed_time(),
    };
    let res = handle_request(&repo, &mut vault, req);
    match &res {
        IpcResponse::Error(IpcErrorCode::Persistence { reason }) => {
            assert_eq!(reason, "persistence error");
            assert!(!reason.contains("/home/"));
            assert!(!reason.contains("leaked"));
            assert!(!reason.to_lowercase().contains("pid"));
        }
        other => panic!("expected Persistence error, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// TC-UT-039: 暗号化 vault ハンドラ防御的検査
// -----------------------------------------------------------------
#[test]
fn test_add_when_repo_returns_unsupported_encrypted_returns_encryption_unsupported() {
    let repo = MockRepo::failing(PersistenceError::UnsupportedYet {
        feature: "encrypted vault persistence",
        tracking_issue: None,
    });
    let mut vault = empty_vault();
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: text_label("L"),
        value: secret_bytes(b"V"),
        now: fixed_time(),
    };
    let res = handle_request(&repo, &mut vault, req);
    assert!(matches!(
        res,
        IpcResponse::Error(IpcErrorCode::EncryptionUnsupported)
    ));
}

// -----------------------------------------------------------------
// TC-UT-039+: Handshake variant は pure handler で Internal を返す
// -----------------------------------------------------------------
#[test]
fn test_handshake_variant_through_handler_returns_internal() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    let req = IpcRequest::Handshake {
        client_version: shikomi_core::ipc::IpcProtocolVersion::V1,
    };
    let res = handle_request(&repo, &mut vault, req);
    match res {
        IpcResponse::Error(IpcErrorCode::Internal { reason }) => {
            assert!(reason.contains("handshake"));
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

/// ProtectionMode は変更しないことを確認（防御的）。
#[test]
fn test_list_does_not_modify_vault_protection_mode() {
    let repo = MockRepo::ok();
    let mut vault = empty_vault();
    assert_eq!(vault.protection_mode(), ProtectionMode::Plaintext);
    let _ = handle_request(&repo, &mut vault, IpcRequest::ListRecords);
    assert_eq!(vault.protection_mode(), ProtectionMode::Plaintext);
}
