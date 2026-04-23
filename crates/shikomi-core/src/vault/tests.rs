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
    // make_plaintext_record() uses UNIX_EPOCH as created_at; use 1s later as update time
    let later = OffsetDateTime::from_unix_timestamp(1).unwrap();
    vault
        .update_record(&id, |r| r.with_updated_label(new_label, later))
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
