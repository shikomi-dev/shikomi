//! `IpcRequest::EditRecord` の処理。

use shikomi_core::ipc::{IpcErrorCode, IpcResponse, SerializableSecretBytes};
use shikomi_core::{
    DomainError, Hotkey, RecordId, RecordLabel, RecordPayload, Vault, VaultConsistencyReason,
};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;

use super::error_mapping::{map_domain_error, map_persistence_error};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_edit<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    id: RecordId,
    label: Option<RecordLabel>,
    value: Option<SerializableSecretBytes>,
    now: OffsetDateTime,
    hotkey: Option<String>,
    clear_hotkey: bool,
) -> IpcResponse {
    // `--hotkey` と `--clear-hotkey` を同時指定した場合はエラー（矛盾入力）
    if clear_hotkey && hotkey.is_some() {
        return IpcResponse::Error(IpcErrorCode::HotkeyParseError {
            reason: "--hotkey と --clear-hotkey を同時指定することはできません".to_owned(),
        });
    }

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

    // ホットキー操作（ドメイン層）
    if clear_hotkey {
        if let Err(err) = vault.clear_hotkey(&id) {
            return IpcResponse::Error(map_domain_error(&err));
        }
    } else if let Some(hotkey_str) = hotkey {
        match Hotkey::parse(&hotkey_str) {
            Ok(hotkey) => {
                if let Err(err) = vault.assign_hotkey(&id, hotkey) {
                    return IpcResponse::Error(map_domain_error(&err));
                }
            }
            Err(_) => {
                return IpcResponse::Error(IpcErrorCode::HotkeyParseError {
                    reason: format!("invalid hotkey combo: {hotkey_str}"),
                });
            }
        }
    }

    if let Err(err) = repo.save(vault) {
        return IpcResponse::Error(map_persistence_error(&err));
    }
    IpcResponse::Edited { id }
}

fn map_update_err(err: DomainError) -> IpcErrorCode {
    match err {
        DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(id)) => {
            IpcErrorCode::NotFound { id }
        }
        other => map_domain_error(&other),
    }
}
