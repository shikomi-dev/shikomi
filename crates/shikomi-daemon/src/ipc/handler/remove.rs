//! `IpcRequest::RemoveRecord` の処理。

use shikomi_core::ipc::{IpcErrorCode, IpcResponse};
use shikomi_core::{RecordId, Vault};
use shikomi_infra::persistence::VaultRepository;

use super::error_mapping::{map_domain_error, map_persistence_error};

pub(super) fn handle_remove<R: VaultRepository + ?Sized>(
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
