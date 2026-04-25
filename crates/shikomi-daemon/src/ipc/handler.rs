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
