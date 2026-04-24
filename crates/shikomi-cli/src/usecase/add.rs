//! `add` UseCase — 新規レコードを追加する。vault 未作成なら平文 vault を初期化。

use shikomi_core::{
    ProtectionMode, Record, RecordId, RecordPayload, Vault, VaultHeader, VaultVersion,
};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::CliError;
use crate::input::AddInput;

/// 新規レコードを vault に追加する。vault 未作成なら平文モードで自動作成。
///
/// `now` は呼び出し側（`run()`）で `OffsetDateTime::now_utc()` を評価して渡す
/// （UseCase の純粋性維持のため）。
///
/// # Errors
/// - 暗号化モード検出: `CliError::EncryptionUnsupported`
/// - ドメイン整合性エラー（id 重複 / モード不一致等）: `CliError::Domain`
/// - 永続化エラー: `CliError::Persistence`
pub fn add_record(
    repo: &dyn VaultRepository,
    input: AddInput,
    now: OffsetDateTime,
) -> Result<RecordId, CliError> {
    let mut vault = if repo.exists()? {
        let loaded = repo.load()?;
        if loaded.protection_mode() == ProtectionMode::Encrypted {
            return Err(CliError::EncryptionUnsupported);
        }
        loaded
    } else {
        let header =
            VaultHeader::new_plaintext(VaultVersion::CURRENT, now).map_err(CliError::Domain)?;
        Vault::new(header)
    };

    let id = RecordId::new(Uuid::now_v7()).map_err(CliError::Domain)?;
    let payload = RecordPayload::Plaintext(input.value);
    let record = Record::new(id.clone(), input.kind, input.label, payload, now);

    vault.add_record(record).map_err(CliError::Domain)?;
    repo.save(&vault)?;

    Ok(id)
}
