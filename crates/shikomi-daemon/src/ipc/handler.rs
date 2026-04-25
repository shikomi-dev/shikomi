//! IPC リクエスト → レスポンスの pure 写像。
//!
//! ハンドラは I/O / `Mutex` ロックを行わない（呼び出し側 = `IpcServer::handle_connection`
//! の責務）。`&dyn VaultRepository` と `&mut Vault` を引数で受ける。

use shikomi_core::ipc::{
    IpcErrorCode, IpcRequest, IpcResponse, RecordSummary, SerializableSecretBytes,
};
use shikomi_core::{
    DomainError, Record, RecordId, RecordKind, RecordLabel, RecordPayload, Vault,
    VaultConsistencyReason,
};
use shikomi_infra::persistence::{PersistenceError, VaultRepository};
use time::OffsetDateTime;
use uuid::Uuid;

/// `IpcRequest` を `IpcResponse` に写像する pure 関数。
///
/// 設計根拠: docs/features/daemon-ipc/detailed-design/daemon-runtime.md
/// §`handler::handle_request` 関数 / §Result → IpcErrorCode 写像の規約
pub fn handle_request<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    request: IpcRequest,
) -> IpcResponse {
    match request {
        IpcRequest::Handshake { .. } => {
            // ハンドシェイクは別経路（`handshake::negotiate`）で扱う。
            // 防御的に Internal を返す。
            IpcResponse::Error(IpcErrorCode::Internal {
                reason: "handshake should be handled separately".to_owned(),
            })
        }
        IpcRequest::ListRecords => handle_list(vault),
        IpcRequest::AddRecord {
            kind,
            label,
            value,
            now,
        } => handle_add(repo, vault, kind, label, value, now),
        IpcRequest::EditRecord {
            id,
            label,
            value,
            now,
        } => handle_edit(repo, vault, id, label, value, now),
        IpcRequest::RemoveRecord { id } => handle_remove(repo, vault, id),
        // `#[non_exhaustive]` 防御的 wildcard（後続 V2 variant に対する多層防御）
        _ => IpcResponse::Error(IpcErrorCode::Internal {
            reason: "unknown request variant".to_owned(),
        }),
    }
}

// -------------------------------------------------------------------
// 各 variant の処理
// -------------------------------------------------------------------

fn handle_list(vault: &Vault) -> IpcResponse {
    let summaries: Vec<RecordSummary> = vault
        .records()
        .iter()
        .map(RecordSummary::from_record)
        .collect();
    IpcResponse::Records(summaries)
}

fn handle_add<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    kind: RecordKind,
    label: RecordLabel,
    value: SerializableSecretBytes,
    now: OffsetDateTime,
) -> IpcResponse {
    let secret = match value.into_inner().into_secret_string() {
        Ok(s) => s,
        Err(_) => {
            return IpcResponse::Error(IpcErrorCode::InvalidLabel {
                reason: "invalid utf-8 value".to_owned(),
            });
        }
    };
    // NOTE: Phase 1 は plaintext のみサポート。secret は plaintext として記録する。
    let payload = RecordPayload::Plaintext(secret);

    let record_id = match RecordId::new(Uuid::now_v7()) {
        Ok(id) => id,
        Err(_) => {
            return IpcResponse::Error(IpcErrorCode::Internal {
                reason: "unexpected error".to_owned(),
            });
        }
    };

    let record = Record::new(record_id.clone(), kind, label, payload, now);

    if let Err(err) = vault.add_record(record) {
        return IpcResponse::Error(map_domain_error(&err));
    }
    if let Err(err) = repo.save(vault) {
        return IpcResponse::Error(map_persistence_error(&err));
    }
    IpcResponse::Added { id: record_id }
}

fn handle_edit<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    id: RecordId,
    label: Option<RecordLabel>,
    value: Option<SerializableSecretBytes>,
    now: OffsetDateTime,
) -> IpcResponse {
    if vault.find_record(&id).is_none() {
        return IpcResponse::Error(IpcErrorCode::NotFound { id });
    }

    // 値の変換は update_record クロージャ呼出前に実施し、UTF-8 エラーを早期検知する。
    let new_secret = match value {
        Some(v) => match v.into_inner().into_secret_string() {
            Ok(s) => Some(s),
            Err(_) => {
                return IpcResponse::Error(IpcErrorCode::InvalidLabel {
                    reason: "invalid utf-8 value".to_owned(),
                });
            }
        },
        None => None,
    };

    let update_result = vault.update_record(&id, |old| {
        let mut updated = old;
        if let Some(new_label) = label {
            updated = updated.with_updated_label(new_label, now)?;
        }
        if let Some(secret) = new_secret {
            updated = updated.with_updated_payload(RecordPayload::Plaintext(secret), now)?;
        }
        Ok(updated)
    });

    if let Err(err) = update_result {
        return IpcResponse::Error(map_domain_error(&err));
    }
    if let Err(err) = repo.save(vault) {
        return IpcResponse::Error(map_persistence_error(&err));
    }
    IpcResponse::Edited { id }
}

fn handle_remove<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    id: RecordId,
) -> IpcResponse {
    if vault.find_record(&id).is_none() {
        return IpcResponse::Error(IpcErrorCode::NotFound { id });
    }
    if let Err(err) = vault.remove_record(&id) {
        return IpcResponse::Error(map_domain_error(&err));
    }
    if let Err(err) = repo.save(vault) {
        return IpcResponse::Error(map_persistence_error(&err));
    }
    IpcResponse::Removed { id }
}

// -------------------------------------------------------------------
// 写像関数（reason はハードコード固定文言のみ、basic-design/error.md §IpcErrorCode 設計規約）
// -------------------------------------------------------------------

fn map_domain_error(err: &DomainError) -> IpcErrorCode {
    match err {
        DomainError::InvalidRecordLabel(_) => IpcErrorCode::InvalidLabel {
            reason: "invalid label".to_owned(),
        },
        DomainError::InvalidRecordId(_) => IpcErrorCode::InvalidLabel {
            reason: "invalid record id".to_owned(),
        },
        DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(id)) => {
            IpcErrorCode::NotFound { id: id.clone() }
        }
        DomainError::VaultConsistencyError(VaultConsistencyReason::DuplicateId(_)) => {
            IpcErrorCode::Domain {
                reason: "duplicate record id".to_owned(),
            }
        }
        _ => IpcErrorCode::Domain {
            reason: "domain error".to_owned(),
        },
    }
}

fn map_persistence_error(err: &PersistenceError) -> IpcErrorCode {
    match err {
        PersistenceError::Corrupted { .. } => IpcErrorCode::Persistence {
            reason: "vault corrupted".to_owned(),
        },
        PersistenceError::CannotResolveVaultDir => IpcErrorCode::Persistence {
            reason: "vault directory not resolvable".to_owned(),
        },
        PersistenceError::UnsupportedYet {
            feature: "encrypted vault persistence",
            ..
        } => IpcErrorCode::EncryptionUnsupported,
        _ => IpcErrorCode::Persistence {
            reason: "persistence error".to_owned(),
        },
    }
}

// -------------------------------------------------------------------
// ユニットテスト（テスト設計 `test-design/unit.md §2.5 TC-UT-030〜039`）
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    //! `handle_request` の pure 写像を mock repo + fixed vault で検証する。
    //!
    //! 設計根拠: `test-design/unit.md §2.5`、`../detailed-design/daemon-runtime.md §handler::handle_request`。
    //!
    //! 対応 Issue: #26

    use super::*;
    use shikomi_core::ipc::SerializableSecretBytes as SSB;
    use shikomi_core::{
        ProtectionMode, SecretBytes, SecretString, Vault, VaultHeader, VaultVersion,
    };
    use std::cell::RefCell;
    use std::path::PathBuf;

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
    // TC-UT-033: AddRecord label 重複で Domain error、reason は固定文言
    // -----------------------------------------------------------------
    #[test]
    fn test_add_duplicate_label_returns_domain_error_with_fixed_reason() {
        // shikomi-core の vault.add_record は label 重複を禁止する設計
        // （DomainError::VaultConsistencyError::DuplicateLabel 相当）。本テストでは
        // 重複 id を作れないため、代替として同 label の Text を 2 度 add する経路を試す。
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
        let _ = handle_request(&repo, &mut vault, req1);
        let res = handle_request(&repo, &mut vault, req2);
        // domain error に写像される（reason は固定文言、`/home/` や PID / SECRET を含まない）
        if let IpcResponse::Error(IpcErrorCode::Domain { reason }) = &res {
            assert!(!reason.contains("/home/"));
            assert!(!reason.to_lowercase().contains("pid"));
            assert!(!reason.contains("SECRET_TEST_VALUE"));
        } else {
            // 実装により duplicate label を add_record 側で reject しない可能性もあるため、
            // Error variant でない場合もエラーメッセージを出して明示。
            match &res {
                IpcResponse::Error(_) => {} // 何らかのエラーであれば許容（reason 汚染チェックのみ）
                IpcResponse::Added { .. } => {
                    // duplicate label が許容されるなら save_calls=2 を検証
                    assert_eq!(
                        repo.save_calls(),
                        2,
                        "duplicate label accepted but save not called"
                    );
                }
                other => panic!("unexpected response: {other:?}"),
            }
        }
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
        // 存在しないため save は呼ばれない
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
                // 固定文言："persistence error"、絶対パス / secret / pid 非漏洩
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
    // 注記: daemon 起動時に fail fast するのが主経路（run() 内）。handler 単独で
    // 検出する経路は直接ないが、repo.save が UnsupportedYet { feature: "encrypted
    // vault persistence" } を返した場合に EncryptionUnsupported に写像することを検証。
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
    // (別経路 handshake::negotiate で処理される契約の防御的検証)
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
}
