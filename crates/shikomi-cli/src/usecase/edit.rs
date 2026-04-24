//! `edit` UseCase — 既存レコードの label / value を更新する。

use std::path::Path;

use shikomi_core::{DomainError, ProtectionMode, RecordId, RecordPayload, VaultConsistencyReason};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;

use crate::error::CliError;
use crate::input::EditInput;

/// 既存レコードの `label` / `value` を更新する。少なくとも一方が `Some` である前提（`run` 側で検証）。
///
/// # Errors
/// - vault 未作成: `CliError::VaultNotInitialized`
/// - 暗号化モード検出: `CliError::EncryptionUnsupported`
/// - 対象レコード不存在: `CliError::RecordNotFound`
/// - ドメイン / 永続化エラー
pub fn edit_record(
    repo: &dyn VaultRepository,
    input: EditInput,
    now: OffsetDateTime,
    vault_dir: &Path,
) -> Result<RecordId, CliError> {
    if !repo.exists()? {
        return Err(CliError::VaultNotInitialized(vault_dir.to_path_buf()));
    }
    let mut vault = repo.load()?;
    if vault.protection_mode() == ProtectionMode::Encrypted {
        return Err(CliError::EncryptionUnsupported);
    }

    // 先に存在チェックして RecordNotFound を user error で返す（ドメイン側も同等検出するが、
    // こちらで先に捕捉して CliError::RecordNotFound に写像する）。
    if vault.find_record(&input.id).is_none() {
        return Err(CliError::RecordNotFound(input.id));
    }

    let id_for_return = input.id.clone();
    let EditInput { id, label, value } = input;
    vault
        .update_record(&id, |mut record| {
            if let Some(new_label) = label {
                record = record.with_updated_label(new_label, now)?;
            }
            if let Some(new_value) = value {
                record = record.with_updated_payload(RecordPayload::Plaintext(new_value), now)?;
            }
            Ok(record)
        })
        .map_err(map_update_err)?;

    repo.save(&vault)?;
    Ok(id_for_return)
}

fn map_update_err(err: DomainError) -> CliError {
    match err {
        DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(id)) => {
            CliError::RecordNotFound(id)
        }
        other => CliError::Domain(other),
    }
}
