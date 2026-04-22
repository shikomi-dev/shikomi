//! 結合テスト: vault ライフサイクル (TC-I01, TC-I02)
//! REQ-007 / AC-01, AC-02, AC-06

use shikomi_core::{
    DomainError, Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault,
    VaultConsistencyReason, VaultHeader, VaultVersion,
};
use time::OffsetDateTime;

fn make_id() -> RecordId {
    RecordId::new(uuid::Uuid::now_v7()).unwrap()
}

fn make_plaintext_header() -> VaultHeader {
    VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap()
}

fn make_plaintext_record(label: &str) -> Record {
    Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new(label.to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
        OffsetDateTime::UNIX_EPOCH,
    )
}

/// TC-I01: Plaintext vault ライフサイクル
#[test]
fn test_plaintext_vault_lifecycle_add_find_remove() {
    let mut vault = Vault::new(make_plaintext_header());

    let r1 = make_plaintext_record("record-1");
    let r2 = make_plaintext_record("record-2");
    let id1 = r1.id().clone();
    let id2 = r2.id().clone();

    // add 2 records
    vault.add_record(r1).unwrap();
    vault.add_record(r2).unwrap();
    assert_eq!(vault.records().len(), 2);

    // find existing
    let found = vault.find_record(&id1);
    assert!(found.is_some());
    assert_eq!(found.unwrap().label().as_str(), "record-1");

    // remove one
    vault.remove_record(&id2).unwrap();
    assert_eq!(vault.records().len(), 1);
    assert!(vault.find_record(&id2).is_none());
}

/// TC-I02: Vault モード不整合検知
#[test]
fn test_plaintext_vault_rejects_encrypted_payload() {
    use shikomi_core::{Aad, CipherText, NonceBytes, RecordPayloadEncrypted};

    let mut vault = Vault::new(make_plaintext_header());

    let id = make_id();
    let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
    let cipher = CipherText::try_new(vec![1u8; 32].into_boxed_slice()).unwrap();
    let aad = Aad::new(
        id.clone(),
        VaultVersion::CURRENT,
        OffsetDateTime::UNIX_EPOCH,
    )
    .unwrap();
    let enc = RecordPayloadEncrypted::new(nonce, cipher, aad).unwrap();
    let record = Record::new(
        id,
        RecordKind::Secret,
        RecordLabel::try_new("enc".to_string()).unwrap(),
        RecordPayload::Encrypted(enc),
        OffsetDateTime::UNIX_EPOCH,
    );

    let err = vault.add_record(record).unwrap_err();
    assert!(matches!(
        err,
        DomainError::VaultConsistencyError(VaultConsistencyReason::ModeMismatch { .. })
    ));
}
