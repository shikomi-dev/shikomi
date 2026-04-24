use super::*;
use crate::error::{DomainError, InvalidRecordLabelReason, InvalidRecordPayloadReason};
use crate::secret::SecretString;
use crate::vault::crypto_data::{Aad, CipherText};
use crate::vault::id::RecordId;
use crate::vault::nonce::NonceBytes;
use crate::vault::protection_mode::ProtectionMode;
use crate::vault::version::VaultVersion;
use time::OffsetDateTime;

fn make_id() -> RecordId {
    RecordId::new(uuid::Uuid::now_v7()).unwrap()
}

fn make_aad() -> Aad {
    let id = make_id();
    Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap()
}

// --- TC-U05: RecordLabel ---

#[test]
fn test_record_label_try_new_one_char_ok() {
    assert!(RecordLabel::try_new("A".to_string()).is_ok());
}

#[test]
fn test_record_label_try_new_255_graphemes_ok() {
    let s = "あ".repeat(255);
    assert!(RecordLabel::try_new(s).is_ok());
}

#[test]
fn test_record_label_try_new_256_graphemes_returns_too_long() {
    let s = "あ".repeat(256);
    let err = RecordLabel::try_new(s).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::TooLong {
            grapheme_count: 256
        })
    ));
}

#[test]
fn test_record_label_try_new_empty_returns_empty_error() {
    let err = RecordLabel::try_new("".to_string()).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::Empty)
    ));
}

#[test]
fn test_record_label_try_new_nul_char_returns_control_char_error() {
    let err = RecordLabel::try_new("\x00A".to_string()).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { position: 0 })
    ));
}

#[test]
fn test_record_label_try_new_us1f_returns_control_char_error() {
    let err = RecordLabel::try_new("\x1FA".to_string()).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
    ));
}

#[test]
fn test_record_label_try_new_del_returns_control_char_error() {
    let err = RecordLabel::try_new("\x7FA".to_string()).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
    ));
}

#[test]
fn test_record_label_try_new_tab_is_allowed() {
    assert!(RecordLabel::try_new("A\tB".to_string()).is_ok());
}

#[test]
fn test_record_label_try_new_newline_is_allowed() {
    assert!(RecordLabel::try_new("A\nB".to_string()).is_ok());
}

#[test]
fn test_record_label_try_new_carriage_return_is_allowed() {
    assert!(RecordLabel::try_new("A\rB".to_string()).is_ok());
}

#[test]
fn test_record_label_as_str_returns_stored_string() {
    let label = RecordLabel::try_new("Test".to_string()).unwrap();
    assert_eq!(label.as_str(), "Test");
}

// --- TC-U06: RecordPayload ---

#[test]
fn test_record_payload_plaintext_holds_secret_string() {
    let payload = RecordPayload::Plaintext(SecretString::from_string("s".to_string()));
    assert!(matches!(payload, RecordPayload::Plaintext(_)));
    assert_eq!(payload.variant_mode(), ProtectionMode::Plaintext);
}

#[test]
fn test_record_payload_encrypted_new_with_valid_args_ok() {
    let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
    let cipher = CipherText::try_new(vec![1u8; 32].into_boxed_slice()).unwrap();
    let aad = make_aad();
    assert!(RecordPayloadEncrypted::new(nonce, cipher, aad).is_ok());
}

#[test]
fn test_nonce_bytes_try_new_with_11_bytes_returns_nonce_length_error() {
    // RecordPayloadEncrypted requires NonceBytes; NonceBytes rejects 11-byte input
    let err = NonceBytes::try_new(&[0u8; 11]).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::NonceLength {
            expected: 12,
            got: 11
        })
    ));
}

#[test]
fn test_cipher_text_try_new_with_empty_bytes_returns_cipher_text_empty_error() {
    let err = CipherText::try_new(vec![].into_boxed_slice()).unwrap_err();
    assert!(matches!(
        err,
        DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::CipherTextEmpty)
    ));
}

#[test]
fn test_record_payload_plaintext_variant_mode_is_plaintext() {
    let payload = RecordPayload::Plaintext(SecretString::from_string("s".to_string()));
    assert_eq!(payload.variant_mode(), ProtectionMode::Plaintext);
}

#[test]
fn test_record_payload_encrypted_variant_mode_is_encrypted() {
    let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
    let cipher = CipherText::try_new(vec![1u8; 32].into_boxed_slice()).unwrap();
    let aad = make_aad();
    let enc = RecordPayloadEncrypted::new(nonce, cipher, aad).unwrap();
    let payload = RecordPayload::Encrypted(enc);
    assert_eq!(payload.variant_mode(), ProtectionMode::Encrypted);
}

// --- TC-U12: Record timestamp subsecond rounding ---

#[test]
fn test_record_new_truncates_created_at_to_microseconds() {
    // 123_456_789 ns = 0.123456789s → 789ns sub-µs component should be truncated
    let now = OffsetDateTime::from_unix_timestamp_nanos(123_456_789).unwrap();
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("label".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
        now,
    );
    assert_eq!(
        record.created_at().nanosecond() % 1_000,
        0,
        "789ns sub-µs should have been truncated to 000ns"
    );
}

#[test]
fn test_record_new_updated_at_equals_created_at_and_is_microsecond_precision() {
    let now = OffsetDateTime::from_unix_timestamp_nanos(123_456_789).unwrap();
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("label".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
        now,
    );
    assert_eq!(record.updated_at().nanosecond() % 1_000, 0);
    assert_eq!(record.updated_at(), record.created_at());
}

// --- TC-U13: Record::rehydrate — updated_at < created_at → InvalidUpdatedAt ---

#[test]
fn test_record_rehydrate_updated_at_before_created_at_returns_invalid_updated_at() {
    let created_at = OffsetDateTime::from_unix_timestamp(1_000_000).unwrap();
    let updated_at = OffsetDateTime::from_unix_timestamp(999_999).unwrap(); // 1秒前
    let result = Record::rehydrate(
        make_id(),
        RecordKind::Secret,
        RecordLabel::try_new("label".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
        created_at,
        updated_at,
    );
    assert!(
        matches!(
            result,
            Err(DomainError::VaultConsistencyError(
                crate::error::VaultConsistencyReason::InvalidUpdatedAt
            ))
        ),
        "updated_at < created_at は InvalidUpdatedAt を期待したが: {result:?}"
    );
}

// --- TC-U14: Record::rehydrate — サブマイクロ秒成分切り捨て ---

#[test]
fn test_record_rehydrate_truncates_subsecond_to_microseconds() {
    // 789 ns のサブマイクロ秒成分を持つタイムスタンプ
    let created_at = OffsetDateTime::from_unix_timestamp_nanos(1_000_000_000_123_456_789).unwrap();
    let updated_at = OffsetDateTime::from_unix_timestamp_nanos(1_000_000_001_987_654_321).unwrap();
    let record = Record::rehydrate(
        make_id(),
        RecordKind::Secret,
        RecordLabel::try_new("label".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
        created_at,
        updated_at,
    )
    .expect("rehydrate が失敗した");

    assert_eq!(
        record.created_at().nanosecond() % 1_000,
        0,
        "created_at のサブマイクロ秒成分が切り捨てられていない"
    );
    assert_eq!(
        record.updated_at().nanosecond() % 1_000,
        0,
        "updated_at のサブマイクロ秒成分が切り捨てられていない"
    );
    assert!(
        record.updated_at() >= record.created_at(),
        "切り捨て後も updated_at >= created_at を保持すべき"
    );
}

// --- text_preview の挙動検証（cli-vault-commands feature 用アクセサ） ---

#[test]
fn test_text_preview_text_kind_returns_prefix() {
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("l".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("hello world".to_string())),
        OffsetDateTime::UNIX_EPOCH,
    );
    assert_eq!(record.text_preview(5), Some("hello".to_string()));
}

#[test]
fn test_text_preview_max_chars_zero_returns_empty_string() {
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("l".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
        OffsetDateTime::UNIX_EPOCH,
    );
    assert_eq!(record.text_preview(0), Some(String::new()));
}

#[test]
fn test_text_preview_max_chars_exceeds_length_returns_whole_string() {
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("l".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("abc".to_string())),
        OffsetDateTime::UNIX_EPOCH,
    );
    assert_eq!(record.text_preview(100), Some("abc".to_string()));
}

#[test]
fn test_text_preview_multibyte_char_unit_truncation() {
    // 先頭 3 char を取る。grapheme ではなく char 単位なので "あいう" が返る。
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("l".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("あいうえお".to_string())),
        OffsetDateTime::UNIX_EPOCH,
    );
    assert_eq!(record.text_preview(3), Some("あいう".to_string()));
}

#[test]
fn test_text_preview_secret_kind_returns_none() {
    let record = Record::new(
        make_id(),
        RecordKind::Secret,
        RecordLabel::try_new("l".to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string("super-secret".to_string())),
        OffsetDateTime::UNIX_EPOCH,
    );
    assert_eq!(record.text_preview(10), None);
}

#[test]
fn test_text_preview_encrypted_variant_returns_none() {
    let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
    let ciphertext = CipherText::try_new(vec![0u8; 32].into_boxed_slice()).unwrap();
    let aad = make_aad();
    let enc = RecordPayloadEncrypted::new(nonce, ciphertext, aad).unwrap();
    let record = Record::new(
        make_id(),
        RecordKind::Text,
        RecordLabel::try_new("l".to_string()).unwrap(),
        RecordPayload::Encrypted(enc),
        OffsetDateTime::UNIX_EPOCH,
    );
    assert_eq!(record.text_preview(10), None);
}
