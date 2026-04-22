//! vault 集約ルートと関連型の公開インターフェース。

pub mod crypto_data;
pub mod header;
pub mod id;
pub mod nonce;
pub mod protection_mode;
pub mod record;
pub mod version;

pub use crypto_data::{Aad, CipherText, KdfSalt, WrappedVek};
pub use header::{VaultHeader, VaultHeaderEncrypted, VaultHeaderPlaintext};
pub use id::RecordId;
pub use nonce::{NonceBytes, NonceCounter};
pub use protection_mode::ProtectionMode;
pub use record::{Record, RecordKind, RecordLabel, RecordPayload, RecordPayloadEncrypted};
pub use version::VaultVersion;

use crate::error::{DomainError, VaultConsistencyReason};
use crate::secret::SecretBytes;

// -------------------------------------------------------------------
// VekProvider trait
// -------------------------------------------------------------------

/// VEK（Vault Encryption Key）の再生成・再暗号化を担うプロバイダ trait。
///
/// 実装は `shikomi-infra` に置く（Dependency Inversion）。
/// `shikomi-core` はこの trait シグネチャのみを所有し、暗号実装に依存しない。
pub trait VekProvider {
    /// プロバイダが保持する新 VEK への参照を返す。
    ///
    /// `Vault::rekey_with` が呼び出し元（`shikomi-infra`）の提供する VEK を受け取るために使う。
    fn new_vek(&self) -> &SecretBytes;

    /// 全レコードを新 VEK で再暗号化する（in-place）。
    ///
    /// 部分失敗した場合は `DomainError::VaultConsistencyError(RekeyPartialFailure)` を返す。
    /// 呼び出し元は `SQLite` トランザクションでアトミック更新を保証すること。
    ///
    /// # Errors
    /// 再暗号化失敗時に `DomainError` を返す。
    fn reencrypt_all(
        &mut self,
        records: &mut [Record],
        new_vek: &SecretBytes,
    ) -> Result<(), DomainError>;

    /// 新 VEK をパスワード由来の KEK でラップした `WrappedVek` を返す。
    ///
    /// # Errors
    /// KDF / 暗号化失敗時に `DomainError` を返す。
    fn derive_new_wrapped_pw(&self, vek: &SecretBytes) -> Result<WrappedVek, DomainError>;

    /// 新 VEK をリカバリ由来の KEK でラップした `WrappedVek` を返す。
    ///
    /// # Errors
    /// KDF / 暗号化失敗時に `DomainError` を返す。
    fn derive_new_wrapped_recovery(&self, vek: &SecretBytes) -> Result<WrappedVek, DomainError>;
}

// -------------------------------------------------------------------
// Vault
// -------------------------------------------------------------------

/// vault 集約ルート。
///
/// ヘッダの保護モードと全レコードペイロードの整合性を `add_record` / `update_record`
/// で常に保証する（Fail Fast）。
/// レコード ID の一意性も集約自身が強制する。
pub struct Vault {
    header: VaultHeader,
    records: Vec<Record>,
}

impl Vault {
    /// 空のレコードリストで `Vault` を構築する。
    ///
    /// 空集合はヘッダと常に整合するため、この構築は失敗しない。
    #[must_use]
    pub fn new(header: VaultHeader) -> Self {
        Self {
            header,
            records: Vec::new(),
        }
    }

    /// ヘッダが表す保護モードを返す。
    #[must_use]
    pub fn protection_mode(&self) -> ProtectionMode {
        self.header.protection_mode()
    }

    /// vault ヘッダへの参照を返す。
    #[must_use]
    pub fn header(&self) -> &VaultHeader {
        &self.header
    }

    /// 全レコードのスライスを返す。
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// 指定した ID を持つレコードへの参照を返す。存在しない場合は `None`。
    #[must_use]
    pub fn find_record(&self, id: &RecordId) -> Option<&Record> {
        self.records.iter().find(|r| r.id() == id)
    }

    /// レコードを追加する。
    ///
    /// # Errors
    /// - 保護モードとペイロードが一致しない: `DomainError::VaultConsistencyError(ModeMismatch)`
    /// - 同一 ID のレコードが既に存在する: `DomainError::VaultConsistencyError(DuplicateId)`
    pub fn add_record(&mut self, record: Record) -> Result<(), DomainError> {
        let vault_mode = self.protection_mode();
        let record_mode = record.payload().variant_mode();
        if vault_mode != record_mode {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::ModeMismatch {
                    vault_mode,
                    record_mode,
                },
            ));
        }
        if self.records.iter().any(|r| r.id() == record.id()) {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::DuplicateId(record.id().clone()),
            ));
        }
        self.records.push(record);
        Ok(())
    }

    /// 指定した ID のレコードを削除して返す。
    ///
    /// # Errors
    /// 該当 ID が存在しない場合 `DomainError::VaultConsistencyError(RecordNotFound)` を返す。
    pub fn remove_record(&mut self, id: &RecordId) -> Result<Record, DomainError> {
        let pos = self
            .records
            .iter()
            .position(|r| r.id() == id)
            .ok_or_else(|| {
                DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(
                    id.clone(),
                ))
            })?;
        Ok(self.records.remove(pos))
    }

    /// 指定した ID のレコードに `updater` クロージャを適用して更新する。
    ///
    /// `updater` には元レコードの `clone` を渡す。
    /// `updater` 失敗・モード不一致の場合も `self.records` は変更されない（Fail Fast）。
    ///
    /// # Errors
    /// - 該当 ID が存在しない: `DomainError::VaultConsistencyError(RecordNotFound)`
    /// - updater が `Err` を返した場合: その `DomainError`
    /// - 更新後のペイロードモードが vault と不一致: `DomainError::VaultConsistencyError(ModeMismatch)`
    pub fn update_record<F>(&mut self, id: &RecordId, updater: F) -> Result<(), DomainError>
    where
        F: FnOnce(Record) -> Result<Record, DomainError>,
    {
        let pos = self
            .records
            .iter()
            .position(|r| r.id() == id)
            .ok_or_else(|| {
                DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(
                    id.clone(),
                ))
            })?;

        // clone を updater に渡す。updater 失敗・モード不一致でも self.records[pos] は untouched。
        let new_record = updater(self.records[pos].clone())?;

        let vault_mode = self.protection_mode();
        let record_mode = new_record.payload().variant_mode();
        if vault_mode != record_mode {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::ModeMismatch {
                    vault_mode,
                    record_mode,
                },
            ));
        }

        self.records[pos] = new_record;
        Ok(())
    }

    /// VEK を再生成し、全レコードを再暗号化する（rekey）。
    ///
    /// 平文 vault に対しては失敗する。
    /// 再暗号化は `VekProvider` に委譲し、`shikomi-core` は暗号実装に依存しない。
    ///
    /// # Errors
    /// - 平文 vault に対する rekey: `DomainError::VaultConsistencyError(RekeyInPlaintextMode)`
    /// - 再暗号化失敗: `DomainError::VaultConsistencyError(RekeyPartialFailure)` 等
    pub fn rekey_with<P: VekProvider>(&mut self, provider: &mut P) -> Result<(), DomainError> {
        if self.protection_mode() != ProtectionMode::Encrypted {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::RekeyInPlaintextMode,
            ));
        }

        // VEK を provider から取得（clone で借用競合を回避）
        let new_vek: SecretBytes = provider.new_vek().clone();

        let new_wrapped_pw = provider.derive_new_wrapped_pw(&new_vek)?;
        let new_wrapped_recovery = provider.derive_new_wrapped_recovery(&new_vek)?;
        provider.reencrypt_all(&mut self.records, &new_vek)?;

        // ヘッダの wrapped VEK を更新
        if let VaultHeader::Encrypted(ref mut enc) = self.header {
            enc.replace_wrapped_veks(new_wrapped_pw, new_wrapped_recovery);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{DomainError, VaultConsistencyReason};
    use crate::secret::{SecretBytes, SecretString};
    use crate::vault::crypto_data::{Aad, CipherText, KdfSalt, WrappedVek};
    use crate::vault::id::RecordId;
    use crate::vault::nonce::NonceBytes;
    use crate::vault::record::{
        Record, RecordKind, RecordLabel, RecordPayload, RecordPayloadEncrypted,
    };
    use crate::vault::version::VaultVersion;
    use time::OffsetDateTime;

    // --- Helpers ---

    fn make_plaintext_header() -> VaultHeader {
        VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap()
    }

    fn make_encrypted_header() -> VaultHeader {
        let salt = KdfSalt::try_new(&[0u8; 16]).unwrap();
        let wrapped = WrappedVek::try_new(vec![0u8; 48].into_boxed_slice()).unwrap();
        VaultHeader::new_encrypted(
            VaultVersion::CURRENT,
            OffsetDateTime::UNIX_EPOCH,
            salt,
            wrapped.clone(),
            wrapped,
        )
        .unwrap()
    }

    fn make_id() -> RecordId {
        RecordId::new(uuid::Uuid::now_v7()).unwrap()
    }

    fn make_plaintext_record() -> Record {
        Record::new(
            make_id(),
            RecordKind::Text,
            RecordLabel::try_new("label".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    fn make_encrypted_record(id: Option<RecordId>) -> Record {
        let record_id = id.unwrap_or_else(make_id);
        let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
        let cipher = CipherText::try_new(vec![1u8; 32].into_boxed_slice()).unwrap();
        let aad = Aad::new(
            record_id.clone(),
            VaultVersion::CURRENT,
            OffsetDateTime::UNIX_EPOCH,
        )
        .unwrap();
        let enc = RecordPayloadEncrypted::new(nonce, cipher, aad).unwrap();
        Record::new(
            record_id,
            RecordKind::Secret,
            RecordLabel::try_new("secret label".to_string()).unwrap(),
            RecordPayload::Encrypted(enc),
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    // DummyVekProvider for rekey tests
    struct DummyVekProvider {
        should_fail: bool,
        vek: SecretBytes,
        wrapped: WrappedVek,
    }

    impl DummyVekProvider {
        fn new(should_fail: bool) -> Self {
            Self {
                should_fail,
                vek: SecretBytes::from_boxed_slice(vec![0u8; 32].into_boxed_slice()),
                wrapped: WrappedVek::try_new(vec![0u8; 48].into_boxed_slice()).unwrap(),
            }
        }
    }

    impl VekProvider for DummyVekProvider {
        fn new_vek(&self) -> &SecretBytes {
            &self.vek
        }

        fn reencrypt_all(
            &mut self,
            _records: &mut [Record],
            _new_vek: &SecretBytes,
        ) -> Result<(), DomainError> {
            if self.should_fail {
                Err(DomainError::VaultConsistencyError(
                    VaultConsistencyReason::RekeyPartialFailure,
                ))
            } else {
                Ok(())
            }
        }

        fn derive_new_wrapped_pw(&self, _vek: &SecretBytes) -> Result<WrappedVek, DomainError> {
            if self.should_fail {
                Err(DomainError::VaultConsistencyError(
                    VaultConsistencyReason::RekeyPartialFailure,
                ))
            } else {
                Ok(self.wrapped.clone())
            }
        }

        fn derive_new_wrapped_recovery(
            &self,
            _vek: &SecretBytes,
        ) -> Result<WrappedVek, DomainError> {
            if self.should_fail {
                Err(DomainError::VaultConsistencyError(
                    VaultConsistencyReason::RekeyPartialFailure,
                ))
            } else {
                Ok(self.wrapped.clone())
            }
        }
    }

    // --- TC-U07: Vault ---

    #[test]
    fn test_vault_new_plaintext_has_empty_records() {
        let vault = Vault::new(make_plaintext_header());
        assert!(vault.records().is_empty());
    }

    #[test]
    fn test_vault_new_encrypted_has_empty_records() {
        let vault = Vault::new(make_encrypted_header());
        assert!(vault.records().is_empty());
    }

    #[test]
    fn test_add_record_plaintext_to_plaintext_vault_ok() {
        let mut vault = Vault::new(make_plaintext_header());
        vault.add_record(make_plaintext_record()).unwrap();
        assert_eq!(vault.records().len(), 1);
    }

    #[test]
    fn test_add_record_encrypted_payload_to_plaintext_vault_returns_mode_mismatch() {
        let mut vault = Vault::new(make_plaintext_header());
        let record = make_encrypted_record(None);
        let err = vault.add_record(record).unwrap_err();
        assert!(matches!(
            err,
            DomainError::VaultConsistencyError(VaultConsistencyReason::ModeMismatch { .. })
        ));
    }

    #[test]
    fn test_add_record_plaintext_payload_to_encrypted_vault_returns_mode_mismatch() {
        let mut vault = Vault::new(make_encrypted_header());
        let err = vault.add_record(make_plaintext_record()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::VaultConsistencyError(VaultConsistencyReason::ModeMismatch { .. })
        ));
    }

    #[test]
    fn test_add_record_duplicate_id_returns_duplicate_id_error() {
        let mut vault = Vault::new(make_plaintext_header());
        let id = make_id();
        let r1 = Record::new(
            id.clone(),
            RecordKind::Text,
            RecordLabel::try_new("l1".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("v".to_string())),
            OffsetDateTime::UNIX_EPOCH,
        );
        let r2 = Record::new(
            id,
            RecordKind::Text,
            RecordLabel::try_new("l2".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("v".to_string())),
            OffsetDateTime::UNIX_EPOCH,
        );
        vault.add_record(r1).unwrap();
        let err = vault.add_record(r2).unwrap_err();
        assert!(matches!(
            err,
            DomainError::VaultConsistencyError(VaultConsistencyReason::DuplicateId(_))
        ));
    }

    #[test]
    fn test_remove_record_existing_returns_record_and_vault_is_empty() {
        let mut vault = Vault::new(make_plaintext_header());
        let record = make_plaintext_record();
        let id = record.id().clone();
        vault.add_record(record).unwrap();
        let removed = vault.remove_record(&id).unwrap();
        assert_eq!(removed.id(), &id);
        assert!(vault.records().is_empty());
    }

    #[test]
    fn test_remove_record_nonexistent_returns_record_not_found() {
        let mut vault = Vault::new(make_plaintext_header());
        let unknown_id = make_id();
        let err = vault.remove_record(&unknown_id).unwrap_err();
        assert!(matches!(
            err,
            DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(_))
        ));
    }

    #[test]
    fn test_find_record_existing_returns_some() {
        let mut vault = Vault::new(make_plaintext_header());
        let record = make_plaintext_record();
        let id = record.id().clone();
        vault.add_record(record).unwrap();
        assert!(vault.find_record(&id).is_some());
    }

    #[test]
    fn test_find_record_nonexistent_returns_none() {
        let vault = Vault::new(make_plaintext_header());
        let unknown_id = make_id();
        assert!(vault.find_record(&unknown_id).is_none());
    }

    #[test]
    fn test_protection_mode_plaintext_vault_returns_plaintext() {
        let vault = Vault::new(make_plaintext_header());
        assert_eq!(vault.protection_mode(), ProtectionMode::Plaintext);
    }

    #[test]
    fn test_records_returns_slice_with_all_records() {
        let mut vault = Vault::new(make_plaintext_header());
        vault.add_record(make_plaintext_record()).unwrap();
        vault.add_record(make_plaintext_record()).unwrap();
        assert_eq!(vault.records().len(), 2);
    }

    #[test]
    fn test_update_record_applies_updater_and_persists_changes() {
        let mut vault = Vault::new(make_plaintext_header());
        let record = make_plaintext_record();
        let id = record.id().clone();
        vault.add_record(record).unwrap();
        let new_label = RecordLabel::try_new("updated".to_string()).unwrap();
        vault
            .update_record(&id, |r| {
                r.with_updated_label(new_label, OffsetDateTime::UNIX_EPOCH)
            })
            .unwrap();
        let found = vault.find_record(&id).unwrap();
        assert_eq!(found.label().as_str(), "updated");
    }

    #[test]
    fn test_rekey_with_on_plaintext_vault_returns_rekey_in_plaintext_mode_error() {
        let mut vault = Vault::new(make_plaintext_header());
        let mut provider = DummyVekProvider::new(false);
        let err = vault.rekey_with(&mut provider).unwrap_err();
        assert!(matches!(
            err,
            DomainError::VaultConsistencyError(VaultConsistencyReason::RekeyInPlaintextMode)
        ));
    }

    #[test]
    fn test_rekey_with_succeeding_provider_on_encrypted_vault_returns_ok() {
        let mut vault = Vault::new(make_encrypted_header());
        let record = make_encrypted_record(None);
        vault.add_record(record).unwrap();
        let mut provider = DummyVekProvider::new(false);
        assert!(vault.rekey_with(&mut provider).is_ok());
    }

    #[test]
    fn test_rekey_with_failing_provider_returns_rekey_partial_failure() {
        let mut vault = Vault::new(make_encrypted_header());
        let record = make_encrypted_record(None);
        vault.add_record(record).unwrap();
        let mut provider = DummyVekProvider::new(true);
        let err = vault.rekey_with(&mut provider).unwrap_err();
        assert!(matches!(
            err,
            DomainError::VaultConsistencyError(VaultConsistencyReason::RekeyPartialFailure)
        ));
    }
}
