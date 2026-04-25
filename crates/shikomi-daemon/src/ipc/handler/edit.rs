//! `IpcRequest::EditRecord` の処理。

use shikomi_core::ipc::{IpcErrorCode, IpcResponse, SerializableSecretBytes};
use shikomi_core::{RecordId, RecordLabel, RecordPayload, Vault};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;

use super::error_mapping::{map_domain_error, map_persistence_error};

pub(super) fn handle_edit<R: VaultRepository + ?Sized>(
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
