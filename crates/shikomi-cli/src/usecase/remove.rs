//! `remove` UseCase — 確認済みレコードを削除する。

use std::path::Path;

use shikomi_core::{DomainError, ProtectionMode, RecordId, VaultConsistencyReason};
use shikomi_infra::persistence::VaultRepository;

use crate::error::CliError;
use crate::input::ConfirmedRemoveInput;

/// 事前確認経由の `ConfirmedRemoveInput` を受け取り、レコードを削除する。
///
/// `ConfirmedRemoveInput` は確認を経ない経路では構築できないため、UseCase 側での
/// `debug_assert!` 等による再検証は不要（型で事前条件が強制される）。
///
/// # Errors
/// - vault 未作成: `CliError::VaultNotInitialized`
/// - 暗号化モード検出: `CliError::EncryptionUnsupported`
/// - 対象レコード不存在: `CliError::RecordNotFound`
/// - ドメイン / 永続化エラー
pub fn remove_record(
    repo: &dyn VaultRepository,
    input: ConfirmedRemoveInput,
    vault_dir: &Path,
) -> Result<RecordId, CliError> {
    if !repo.exists()? {
        return Err(CliError::VaultNotInitialized(vault_dir.to_path_buf()));
    }
    let mut vault = repo.load()?;
    if vault.protection_mode() == ProtectionMode::Encrypted {
        return Err(CliError::EncryptionUnsupported);
    }

    let id = input.id().clone();
    match vault.remove_record(&id) {
        Ok(_) => {}
        Err(DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(bad_id))) => {
            return Err(CliError::RecordNotFound(bad_id));
        }
        Err(other) => return Err(CliError::Domain(other)),
    }

    repo.save(&vault)?;
    Ok(id)
}
