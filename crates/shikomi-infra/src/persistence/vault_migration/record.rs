//! `EncryptedRecord` — shikomi-infra 側の暗号化レコード型 (Sub-D 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//! §`EncryptedRecord`
//!
//! shikomi-core 既存 `RecordPayloadEncrypted` (nonce + ciphertext + aad) と並列。
//! shikomi-infra 側では label / kind / created_at / updated_at まで含めた完全な
//! per-record 永続化形を保持し、`VaultMigration` の AEAD 経路で利用する。

use shikomi_core::{AuthTag, NonceBytes, RecordId, RecordKind, RecordLabel};
use time::OffsetDateTime;

/// AEAD 暗号化済みレコード (shikomi-infra 永続化形)。
///
/// 設計書 §`EncryptedRecord`:
/// - 派生: `Debug, Clone` (ciphertext / nonce / tag は秘密でない)。
/// - `Drop` 不要 (秘密保持なし)。
/// - AAD は `Aad::Record { id, version, created_at }` で 26B 正規化 (Sub-A `Aad`)。
#[derive(Debug, Clone)]
pub struct EncryptedRecord {
    /// レコード ID (UUIDv7)。
    pub id: RecordId,
    /// レコード種別 (`Text` / `Secret`)。
    pub kind: RecordKind,
    /// レコードラベル (1..=255 graphemes)。
    pub label: RecordLabel,
    /// AEAD 暗号文 (plaintext と同じ長さ、detached tag 方式)。
    pub payload_ciphertext: Vec<u8>,
    /// per-record AEAD nonce 12B。
    pub nonce: NonceBytes,
    /// per-record AEAD authentication tag 16B。
    pub tag: AuthTag,
    /// 作成時刻 (マイクロ秒精度)。
    pub created_at: OffsetDateTime,
    /// 最終更新時刻 (マイクロ秒精度)。
    pub updated_at: OffsetDateTime,
}

impl EncryptedRecord {
    /// 全フィールドを受け取って `EncryptedRecord` を構築する。
    /// 各フィールドは構築済型を渡すこと (検証は呼出側責務)。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: RecordId,
        kind: RecordKind,
        label: RecordLabel,
        payload_ciphertext: Vec<u8>,
        nonce: NonceBytes,
        tag: AuthTag,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) -> Self {
        Self {
            id,
            kind,
            label,
            payload_ciphertext,
            nonce,
            tag,
            created_at,
            updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{NonceBytes, RecordId, RecordKind, RecordLabel};
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn make_record() -> EncryptedRecord {
        EncryptedRecord::new(
            RecordId::new(Uuid::now_v7()).unwrap(),
            RecordKind::Secret,
            RecordLabel::try_new("test".to_string()).unwrap(),
            vec![0u8; 16],
            NonceBytes::from_random([0u8; 12]),
            AuthTag::from_array([0u8; 16]),
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn new_constructs_without_panic() {
        let _ = make_record();
    }

    #[test]
    fn clone_preserves_all_fields() {
        let r = make_record();
        let r2 = r.clone();
        assert_eq!(r.payload_ciphertext, r2.payload_ciphertext);
        assert_eq!(r.nonce.as_array(), r2.nonce.as_array());
        assert_eq!(r.tag.as_array(), r2.tag.as_array());
    }
}
