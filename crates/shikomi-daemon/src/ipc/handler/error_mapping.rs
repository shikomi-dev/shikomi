//! `DomainError` / `PersistenceError` → `IpcErrorCode` の写像。
//!
//! reason はハードコード固定文言のみ（basic-design/error.md §IpcErrorCode 設計規約、
//! 漏洩 negative assertion は test-design/unit.md §TC-UT-038）。

use shikomi_core::ipc::IpcErrorCode;
use shikomi_core::{DomainError, VaultConsistencyReason};
use shikomi_infra::persistence::PersistenceError;

pub(super) fn map_domain_error(err: &DomainError) -> IpcErrorCode {
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
        // `DomainError` は `#[non_exhaustive]`（cross-crate）。wildcard fallback は
        // 必須の OS 制約ではないが、既知の variant を網羅した上で「不明」を `Domain`
        // に押し込めることで reason 漏洩 negative assertion を満たす。
        _ => IpcErrorCode::Domain {
            reason: "domain error".to_owned(),
        },
    }
}

pub(super) fn map_persistence_error(err: &PersistenceError) -> IpcErrorCode {
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
        // `PersistenceError` は `#[non_exhaustive]`（cross-crate）。reason は必ず固定文言
        // 「persistence error」に押し込め、source パス / OS error 文字列の漏洩を遮断する。
        _ => IpcErrorCode::Persistence {
            reason: "persistence error".to_owned(),
        },
    }
}
