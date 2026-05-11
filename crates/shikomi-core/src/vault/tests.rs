use super::*;
use crate::crypto::Vek;
use crate::error::{DomainError, VaultConsistencyReason};
use crate::secret::SecretString;
use crate::vault::crypto_data::{Aad, AuthTag, CipherText, KdfSalt, WrappedVek};
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

fn make_wrapped_vek() -> WrappedVek {
    WrappedVek::new(
        vec![0u8; 32],
        NonceBytes::from_random([0u8; 12]),
        AuthTag::from_array([0u8; 16]),
    )
    .unwrap()
}

fn make_encrypted_header() -> VaultHeader {
    let salt = KdfSalt::try_new(&[0u8; 16]).unwrap();
    VaultHeader::new_encrypted(
        VaultVersion::CURRENT,
        OffsetDateTime::UNIX_EPOCH,
        salt,
        make_wrapped_vek(),
        make_wrapped_vek(),
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
    vek: Vek,
    wrapped: WrappedVek,
}

impl DummyVekProvider {
    fn new(should_fail: bool) -> Self {
        Self {
            should_fail,
            vek: Vek::from_array([0u8; 32]),
            wrapped: make_wrapped_vek(),
        }
    }
}

impl VekProvider for DummyVekProvider {
    fn new_vek(&self) -> &Vek {
        &self.vek
    }

    fn reencrypt_all(&mut self, _records: &mut [Record]) -> Result<(), DomainError> {
        if self.should_fail {
            Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::RekeyPartialFailure,
            ))
        } else {
            Ok(())
        }
    }

    fn derive_new_wrapped_pw(&self, _vek: &Vek) -> Result<WrappedVek, DomainError> {
        if self.should_fail {
            Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::RekeyPartialFailure,
            ))
        } else {
            Ok(self.wrapped.clone())
        }
    }

    fn derive_new_wrapped_recovery(&self, _vek: &Vek) -> Result<WrappedVek, DomainError> {
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

// ── TC-HD-U04: Vault::assign_hotkey ────────────────────────────────────────

use crate::vault::record::Hotkey;

fn make_hotkey(combo: &str) -> Hotkey {
    Hotkey::parse(combo).expect("valid combo")
}

fn make_plaintext_vault_with_record() -> (Vault, RecordId) {
    let mut vault = Vault::new(make_plaintext_header());
    let record = make_plaintext_record();
    let id = record.id().clone();
    vault.add_record(record).unwrap();
    (vault, id)
}

/// TC-HD-U04-a: 既存エントリに新規ホットキーを割り当て → Ok、hotkey が Some に
#[test]
fn tc_hd_u04_a_assign_hotkey_ok() {
    let (mut vault, id) = make_plaintext_vault_with_record();
    let hotkey = make_hotkey("ctrl+alt+1");
    vault.assign_hotkey(&id, hotkey.clone()).unwrap();
    let record = vault.find_record(&id).unwrap();
    assert_eq!(record.hotkey(), Some(&hotkey));
}

/// TC-HD-U04-b: 別エントリが同一ホットキー保持中に割り当て → `HotkeyConflict`
#[test]
fn tc_hd_u04_b_assign_conflicts_with_other_entry() {
    let mut vault = Vault::new(make_plaintext_header());
    let r1 = make_plaintext_record();
    let r2 = make_plaintext_record();
    let id1 = r1.id().clone();
    let id2 = r2.id().clone();
    vault.add_record(r1).unwrap();
    vault.add_record(r2).unwrap();

    vault
        .assign_hotkey(&id1, make_hotkey("ctrl+alt+1"))
        .unwrap();
    let err = vault
        .assign_hotkey(&id2, make_hotkey("ctrl+alt+1"))
        .unwrap_err();
    assert!(matches!(
        err,
        DomainError::VaultConsistencyError(VaultConsistencyReason::HotkeyConflict)
    ));
}

/// TC-HD-U04-c: 存在しない `RecordId` に割り当て → `RecordNotFound`
#[test]
fn tc_hd_u04_c_assign_to_nonexistent_id_returns_not_found() {
    let mut vault = Vault::new(make_plaintext_header());
    let unknown = make_id();
    let err = vault
        .assign_hotkey(&unknown, make_hotkey("ctrl+alt+1"))
        .unwrap_err();
    assert!(matches!(
        err,
        DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(_))
    ));
}

/// TC-HD-U04-d: 自エントリと同一ホットキーで上書き → Ok（競合なし）
#[test]
fn tc_hd_u04_d_reassign_same_hotkey_to_same_record_ok() {
    let (mut vault, id) = make_plaintext_vault_with_record();
    vault.assign_hotkey(&id, make_hotkey("ctrl+alt+1")).unwrap();
    // 同一 ID に同一ホットキーで上書き → 競合なし
    vault.assign_hotkey(&id, make_hotkey("ctrl+alt+1")).unwrap();
    let record = vault.find_record(&id).unwrap();
    assert_eq!(record.hotkey(), Some(&make_hotkey("ctrl+alt+1")));
}

// ── TC-HD-U05: Vault::clear_hotkey ────────────────────────────────────────

/// TC-HD-U05-a: ホットキー付きエントリのクリア → Ok、hotkey が None
#[test]
fn tc_hd_u05_a_clear_hotkey_removes_hotkey() {
    let (mut vault, id) = make_plaintext_vault_with_record();
    vault.assign_hotkey(&id, make_hotkey("ctrl+alt+1")).unwrap();
    vault.clear_hotkey(&id).unwrap();
    let record = vault.find_record(&id).unwrap();
    assert_eq!(record.hotkey(), None);
}

/// TC-HD-U05-b: ホットキーなしエントリのクリア → Ok（冪等）
#[test]
fn tc_hd_u05_b_clear_hotkey_idempotent() {
    let (mut vault, id) = make_plaintext_vault_with_record();
    // ホットキーなし状態でクリア
    vault.clear_hotkey(&id).unwrap();
    let record = vault.find_record(&id).unwrap();
    assert_eq!(record.hotkey(), None);
}

/// TC-HD-U05-c: 存在しない ID のクリア → `RecordNotFound`
#[test]
fn tc_hd_u05_c_clear_hotkey_nonexistent_id_returns_not_found() {
    let mut vault = Vault::new(make_plaintext_header());
    let unknown = make_id();
    let err = vault.clear_hotkey(&unknown).unwrap_err();
    assert!(matches!(
        err,
        DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(_))
    ));
}

// ── TC-HD-U06: Vault::find_by_hotkey ───────────────────────────────────────

/// TC-HD-U06-a: 登録済みホットキーで検索 → Some(&Record)
#[test]
fn tc_hd_u06_a_find_by_hotkey_returns_some() {
    let (mut vault, id) = make_plaintext_vault_with_record();
    vault.assign_hotkey(&id, make_hotkey("ctrl+alt+1")).unwrap();
    let result = vault.find_by_hotkey(&make_hotkey("ctrl+alt+1"));
    assert!(result.is_some());
    assert_eq!(result.unwrap().id(), &id);
}

/// TC-HD-U06-b: 未登録ホットキーで検索 → None
#[test]
fn tc_hd_u06_b_find_by_hotkey_unregistered_returns_none() {
    let vault = Vault::new(make_plaintext_header());
    let result = vault.find_by_hotkey(&make_hotkey("ctrl+alt+9"));
    assert!(result.is_none());
}
