//! 暗号化関連バイト列 newtype 群。
//!
//! `KdfSalt` / `WrappedVek` / `AuthTag` / `CipherText` / `Aad` の 5 型。
//! いずれも検証済み newtype で、生バイト列の取り違えを型で防ぐ。
//!
//! Sub-A Boy Scout Rule: `WrappedVek` の内部構造を `(ciphertext, nonce, tag)` の
//! 3 フィールドに分離型化 (旧: `Box<[u8]>` 単一)。byte offset 演算を呼出側に漏らさない
//! (Tell, Don't Ask)。`AuthTag` を新規導入し AES-GCM 認証タグ 16B を独立型として表現する。

use time::OffsetDateTime;

use crate::error::{DomainError, InvalidRecordPayloadReason, InvalidVaultHeaderReason};
use crate::vault::id::RecordId;
use crate::vault::nonce::NonceBytes;
use crate::vault::version::VaultVersion;

/// Argon2id KDF ソルト長（16 byte、OWASP 推奨 16 B 以上）。
const KDF_SALT_LEN: usize = 16;

/// AES-GCM 認証タグ長 (NIST SP 800-38D §5.2.1.2、128 bit)。
const AUTH_TAG_LEN: usize = 16;

/// `WrappedVek::ciphertext` の最小バイト長 (VEK 32B が最小)。
/// AES-GCM 認証タグは別フィールド `AuthTag` に分離されるため本最小には含めない (契約 C-11)。
const WRAPPED_VEK_CIPHERTEXT_MIN_LEN: usize = 32;

// -------------------------------------------------------------------
// KdfSalt
// -------------------------------------------------------------------

/// Argon2id に渡す KDF ソルト（16 byte 固定）。
#[derive(Debug, Clone)]
pub struct KdfSalt {
    inner: [u8; KDF_SALT_LEN],
}

impl KdfSalt {
    /// バイトスライスから `KdfSalt` を構築する。
    ///
    /// # Errors
    /// `bytes.len() != 16` の場合 `DomainError::InvalidVaultHeader(KdfSaltLength)` を返す。
    pub fn try_new(bytes: &[u8]) -> Result<Self, DomainError> {
        if bytes.len() != KDF_SALT_LEN {
            return Err(DomainError::InvalidVaultHeader(
                InvalidVaultHeaderReason::KdfSaltLength {
                    expected: KDF_SALT_LEN,
                    got: bytes.len(),
                },
            ));
        }
        let mut inner = [0u8; KDF_SALT_LEN];
        inner.copy_from_slice(bytes);
        Ok(Self { inner })
    }

    /// 内包する 16 バイト配列への参照を返す。
    #[must_use]
    pub fn as_array(&self) -> &[u8; KDF_SALT_LEN] {
        &self.inner
    }
}

// -------------------------------------------------------------------
// AuthTag (新規、Sub-A)
// -------------------------------------------------------------------

/// AES-256-GCM 認証タグ (16 byte 固定)。
///
/// `WrappedVek` の `tag` フィールド・将来のヘッダ AEAD タグなどで利用する。
/// 旧来の「ciphertext + tag 連結」を分離型化することで byte offset 演算を呼出側から消す
/// (Sub-A Boy Scout Rule)。
#[derive(Debug, Clone)]
pub struct AuthTag {
    inner: [u8; AUTH_TAG_LEN],
}

impl AuthTag {
    /// バイトスライスから `AuthTag` を構築する。
    ///
    /// # Errors
    /// `bytes.len() != 16` の場合 `DomainError::InvalidVaultHeader(AuthTagLength)` を返す。
    pub fn try_new(bytes: &[u8]) -> Result<Self, DomainError> {
        if bytes.len() != AUTH_TAG_LEN {
            return Err(DomainError::InvalidVaultHeader(
                InvalidVaultHeaderReason::AuthTagLength { got: bytes.len() },
            ));
        }
        let mut inner = [0u8; AUTH_TAG_LEN];
        inner.copy_from_slice(bytes);
        Ok(Self { inner })
    }

    /// 16 byte 配列から直接構築する (Sub-C AEAD 完了直後の経路で使用)。
    #[must_use]
    pub fn from_array(bytes: [u8; AUTH_TAG_LEN]) -> Self {
        Self { inner: bytes }
    }

    /// 内包する 16 バイト配列への参照を返す。
    #[must_use]
    pub fn as_array(&self) -> &[u8; AUTH_TAG_LEN] {
        &self.inner
    }
}

// -------------------------------------------------------------------
// WrappedVek (Sub-A: ciphertext + nonce + tag の 3 フィールドに分離)
// -------------------------------------------------------------------

/// VEK を AES-256-GCM でラップしたデータ。`ciphertext` / `nonce` / `tag` を独立フィールドで保持。
///
/// 旧来の単一 `Box<[u8]>` フィールドを Sub-A で 3 フィールド構造に分離した (Boy Scout Rule)。
/// byte offset 演算は `WrappedVek::new` / `into_parts` に閉じる (Tell, Don't Ask)。
///
/// 永続化フォーマットは Sub-D で確定する (本 Sub-A はシリアライズ実装を持たない)。
#[derive(Debug, Clone)]
pub struct WrappedVek {
    ciphertext: Vec<u8>,
    nonce: NonceBytes,
    tag: AuthTag,
}

impl WrappedVek {
    /// 3 要素 (ciphertext / nonce / tag) から `WrappedVek` を構築する (契約 C-11)。
    ///
    /// # Errors
    ///
    /// - `ciphertext` が空: `DomainError::InvalidVaultHeader(WrappedVekEmpty)`
    /// - `ciphertext` の長さが 32 byte 未満: `DomainError::InvalidVaultHeader(WrappedVekTooShort)`
    pub fn new(ciphertext: Vec<u8>, nonce: NonceBytes, tag: AuthTag) -> Result<Self, DomainError> {
        if ciphertext.is_empty() {
            return Err(DomainError::InvalidVaultHeader(
                InvalidVaultHeaderReason::WrappedVekEmpty,
            ));
        }
        if ciphertext.len() < WRAPPED_VEK_CIPHERTEXT_MIN_LEN {
            return Err(DomainError::InvalidVaultHeader(
                InvalidVaultHeaderReason::WrappedVekTooShort,
            ));
        }
        Ok(Self {
            ciphertext,
            nonce,
            tag,
        })
    }

    /// `ciphertext` への参照を返す。
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// `nonce` への参照を返す。
    #[must_use]
    pub fn nonce(&self) -> &NonceBytes {
        &self.nonce
    }

    /// `tag` への参照を返す。
    #[must_use]
    pub fn tag(&self) -> &AuthTag {
        &self.tag
    }

    /// 3 要素を所有権付きで取り出す (永続化シリアライズ用)。
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, NonceBytes, AuthTag) {
        (self.ciphertext, self.nonce, self.tag)
    }
}

// -------------------------------------------------------------------
// CipherText
// -------------------------------------------------------------------

/// AES-256-GCM 暗号化済みのレコードペイロード。認証タグを含む (レコード境界での連結保持)。
///
/// レコード単位の AEAD 出力フォーマットは vault-persistence (Issue #7) で既に確定済。
/// 本 Sub-A では構造変更しない (`WrappedVek` のみ Boy Scout 分離)。
#[derive(Debug, Clone)]
pub struct CipherText {
    inner: Box<[u8]>,
}

impl CipherText {
    /// バイト列から `CipherText` を構築する。
    ///
    /// # Errors
    /// 空の場合 `DomainError::InvalidRecordPayload(CipherTextEmpty)` を返す。
    pub fn try_new(bytes: Box<[u8]>) -> Result<Self, DomainError> {
        if bytes.is_empty() {
            return Err(DomainError::InvalidRecordPayload(
                InvalidRecordPayloadReason::CipherTextEmpty,
            ));
        }
        Ok(Self { inner: bytes })
    }

    /// 内包するバイト列への参照を返す。
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }
}

// -------------------------------------------------------------------
// Aad
// -------------------------------------------------------------------

/// AEAD に渡す追加認証データ（Associated Authenticated Data）。
///
/// `to_canonical_bytes()` は 26 バイト固定長の決定論的バイト列を返す。
/// このレイアウトを破壊する変更は `VaultVersion` のメジャーアップとセットでのみ許可される。
#[derive(Debug, Clone)]
pub struct Aad {
    record_id: RecordId,
    vault_version: VaultVersion,
    record_created_at: OffsetDateTime,
    /// `record_created_at` を Unix epoch 起点のマイクロ秒（i64）に変換済みの値。
    /// `Aad::new` で検証・格納し、`to_canonical_bytes` でそのまま使用する。
    record_created_at_micros: i64,
}

impl Aad {
    /// `Aad` を構築する。
    ///
    /// `record_created_at` は `Record::new` によって既にマイクロ秒精度に切り捨て済みであること。
    ///
    /// # Errors
    /// `record_created_at` を i64 マイクロ秒に変換できない場合
    /// `DomainError::InvalidRecordPayload(AadTimestampOutOfRange)` を返す。
    pub fn new(
        record_id: RecordId,
        vault_version: VaultVersion,
        record_created_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let nanos: i128 = record_created_at.unix_timestamp_nanos();
        let micros_i128 = nanos / 1_000;
        // SAFETY(到達不能): time 0.3 の `OffsetDateTime` は約 ±9999 年の範囲に収まり、
        // i64 マイクロ秒（±292,000 年）を超えることは現実的にない。
        // 将来の time クレートの年範囲拡張に備えた防衛的コードとして維持する。
        let record_created_at_micros = i64::try_from(micros_i128).map_err(|_| {
            DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::AadTimestampOutOfRange)
        })?;
        Ok(Self {
            record_id,
            vault_version,
            record_created_at,
            record_created_at_micros,
        })
    }

    /// `record_id` への参照を返す。
    #[must_use]
    pub fn record_id(&self) -> &RecordId {
        &self.record_id
    }

    /// `vault_version` を返す。
    #[must_use]
    pub fn vault_version(&self) -> VaultVersion {
        self.vault_version
    }

    /// `record_created_at` を返す。
    #[must_use]
    pub fn record_created_at(&self) -> OffsetDateTime {
        self.record_created_at
    }

    /// AEAD に渡す 26 バイト固定長の決定論的バイト列を返す。
    ///
    /// レイアウト:
    /// - `[0..16]`  : `record_id` の `UUIDv7` バイト列（RFC 4122 バイナリ形式、MSB first）
    /// - `[16..18]` : `vault_version` の u16 値（big-endian）
    /// - `[18..26]` : `record_created_at` の Unix epoch 起点マイクロ秒（i64、big-endian two's complement）
    #[must_use]
    pub fn to_canonical_bytes(&self) -> [u8; 26] {
        let mut bytes = [0u8; 26];
        bytes[0..16].copy_from_slice(self.record_id.as_uuid().as_bytes());
        bytes[16..18].copy_from_slice(&self.vault_version.value().to_be_bytes());
        bytes[18..26].copy_from_slice(&self.record_created_at_micros.to_be_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::id::RecordId;
    use crate::vault::nonce::NonceBytes;
    use crate::vault::version::VaultVersion;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn make_id() -> RecordId {
        RecordId::new(Uuid::now_v7()).unwrap()
    }

    fn dummy_nonce() -> NonceBytes {
        NonceBytes::from_random([0u8; 12])
    }

    fn dummy_tag() -> AuthTag {
        AuthTag::from_array([0u8; 16])
    }

    // ---------------------------------------------------------------
    // AuthTag
    // ---------------------------------------------------------------

    #[test]
    fn auth_tag_from_array_constructs_without_panic() {
        let _ = AuthTag::from_array([0u8; 16]);
    }

    #[test]
    fn auth_tag_try_new_with_16_bytes_ok() {
        assert!(AuthTag::try_new(&[0u8; 16]).is_ok());
    }

    #[test]
    fn auth_tag_try_new_with_15_bytes_returns_auth_tag_length_error() {
        let err = AuthTag::try_new(&[0u8; 15]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidVaultHeader(InvalidVaultHeaderReason::AuthTagLength { got: 15 })
        ));
    }

    // ---------------------------------------------------------------
    // WrappedVek::new 境界値テスト (C-11)
    // ---------------------------------------------------------------

    #[test]
    fn wrapped_vek_new_with_empty_ciphertext_returns_wrapped_vek_empty() {
        let err = WrappedVek::new(vec![], dummy_nonce(), dummy_tag()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidVaultHeader(InvalidVaultHeaderReason::WrappedVekEmpty)
        ));
    }

    #[test]
    fn wrapped_vek_new_with_31_bytes_ciphertext_returns_wrapped_vek_too_short() {
        let err = WrappedVek::new(vec![0u8; 31], dummy_nonce(), dummy_tag()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidVaultHeader(InvalidVaultHeaderReason::WrappedVekTooShort)
        ));
    }

    #[test]
    fn wrapped_vek_new_with_32_bytes_ciphertext_ok() {
        assert!(WrappedVek::new(vec![0u8; 32], dummy_nonce(), dummy_tag()).is_ok());
    }

    #[test]
    fn wrapped_vek_into_parts_round_trips_with_new() {
        let w = WrappedVek::new(vec![0xAAu8; 32], dummy_nonce(), dummy_tag()).unwrap();
        let (ct, n, t) = w.into_parts();
        assert_eq!(ct, vec![0xAAu8; 32]);
        assert_eq!(n.as_array(), &[0u8; 12]);
        assert_eq!(t.as_array(), &[0u8; 16]);
    }

    #[test]
    fn wrapped_vek_field_accessors_return_references() {
        let w = WrappedVek::new(vec![1u8; 32], dummy_nonce(), dummy_tag()).unwrap();
        assert_eq!(w.ciphertext(), &[1u8; 32]);
        assert_eq!(w.nonce().as_array(), &[0u8; 12]);
        assert_eq!(w.tag().as_array(), &[0u8; 16]);
    }

    // ---------------------------------------------------------------
    // Aad テスト (既存維持)
    // ---------------------------------------------------------------

    #[test]
    fn test_aad_new_with_valid_args_ok() {
        let id = make_id();
        assert!(Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).is_ok());
    }

    #[test]
    fn test_aad_new_with_out_of_range_timestamp_returns_aad_timestamp_out_of_range() {
        let threshold = i64::MAX / 1_000_000 + 1;
        if let Ok(far_future) = OffsetDateTime::from_unix_timestamp(threshold) {
            let id = make_id();
            let err = Aad::new(id, VaultVersion::CURRENT, far_future).unwrap_err();
            assert!(matches!(
                err,
                DomainError::InvalidRecordPayload(
                    crate::error::InvalidRecordPayloadReason::AadTimestampOutOfRange
                )
            ));
        }
    }

    #[test]
    fn test_aad_to_canonical_bytes_returns_26_bytes() {
        let id = make_id();
        let aad = Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap();
        assert_eq!(aad.to_canonical_bytes().len(), 26);
    }

    #[test]
    fn test_aad_to_canonical_bytes_golden_value() {
        let uuid_bytes = [
            0x01u8, 0x8f, 0x12, 0x34, 0x56, 0x78, 0x7a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
            0x90, 0x12,
        ];
        let uuid = Uuid::from_bytes(uuid_bytes);
        let id = RecordId::new(uuid).unwrap();
        let aad = Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap();
        let canonical = aad.to_canonical_bytes();

        let expected: [u8; 26] = [
            0x01, 0x8f, 0x12, 0x34, 0x56, 0x78, 0x7a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
            0x90, 0x12, // UUID bytes [0..16]
            0x00, 0x01, // u16=1 BE [16..18]
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // i64=0 BE [18..26]
        ];
        assert_eq!(canonical, expected);
    }
}
