//! `IpcRequest::AddRecord` の処理。

use shikomi_core::ipc::{IpcErrorCode, IpcResponse, SerializableSecretBytes};
use shikomi_core::{Hotkey, Record, RecordId, RecordKind, RecordLabel, RecordPayload, Vault};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;
use uuid::Uuid;

use super::error_mapping::{map_domain_error, map_persistence_error};

pub(super) fn handle_add<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    kind: RecordKind,
    label: RecordLabel,
    value: SerializableSecretBytes,
    now: OffsetDateTime,
    hotkey: Option<String>,
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

    // ホットキー設定（ドメイン層操作）
    if let Some(hotkey_str) = hotkey {
        match Hotkey::parse(&hotkey_str) {
            Ok(hotkey) => {
                if let Err(err) = vault.assign_hotkey(&record_id, hotkey) {
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
    IpcResponse::Added { id: record_id }
}
