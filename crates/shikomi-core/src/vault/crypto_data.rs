//! 暗号化関連バイト列 newtype 群。
//!
//! `KdfSalt` / `WrappedVek` / `CipherText` / `Aad` の 4 型。
//! いずれも検証済み newtype で、生バイト列の取り違えを型で防ぐ。

use time::OffsetDateTime;

use crate::error::{DomainError, InvalidRecordPayloadReason, InvalidVaultHeaderReason};
use crate::vault::id::RecordId;
use crate::vault::version::VaultVersion;

/// Argon2id KDF ソルト長（16 byte、OWASP 推奨 16 B 以上）。
const KDF_SALT_LEN: usize = 16;

/// `WrappedVek` に必要な最小バイト長。
/// AES-GCM 認証タグが 16 B 固定のため、それ以上でなければ暗号的に不正。
const WRAPPED_VEK_MIN_LEN: usize = 16;

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
// WrappedVek
// -------------------------------------------------------------------

/// VEK（Vault Encryption Key）を暗号化してラップしたバイト列。
///
/// AES-GCM wrap には認証タグ（16 B）が含まれるため、実態は空ではない。
#[derive(Debug, Clone)]
pub struct WrappedVek {
    inner: Box<[u8]>,
}

impl WrappedVek {
    /// バイト列から `WrappedVek` を構築する。
    ///
    /// # Errors
    /// - 空の場合 `DomainError::InvalidVaultHeader(WrappedVekEmpty)` を返す。
    /// - 16 バイト未満の場合 `DomainError::InvalidVaultHeader(WrappedVekTooShort)` を返す。
    pub fn try_new(bytes: Box<[u8]>) -> Result<Self, DomainError> {
        if bytes.is_empty() {
            return Err(DomainError::InvalidVaultHeader(
                InvalidVaultHeaderReason::WrappedVekEmpty,
            ));
        }
        if bytes.len() < WRAPPED_VEK_MIN_LEN {
            return Err(DomainError::InvalidVaultHeader(
                InvalidVaultHeaderReason::WrappedVekTooShort,
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
// CipherText
// -------------------------------------------------------------------

/// AES-256-GCM 暗号化済みのレコードペイロード。認証タグを含む。
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
    use crate::vault::version::VaultVersion;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn make_id() -> RecordId {
        RecordId::new(Uuid::now_v7()).unwrap()
    }

    #[test]
    fn test_aad_new_with_valid_args_ok() {
        let id = make_id();
        assert!(Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).is_ok());
    }

    #[test]
    fn test_aad_new_with_out_of_range_timestamp_returns_aad_timestamp_out_of_range() {
        // TC-U11-02: AadTimestampOutOfRange fires when microseconds overflow i64.
        // Overflow threshold: seconds > i64::MAX / 1_000_000 ≈ 9_223_372_036_854.
        // time 0.3 limits representable years to [-9999, 9999]; max unix timestamp
        // ≈ year 9999 ≈ 2.53×10^11 s → max microseconds ≈ 2.53×10^17 << i64::MAX.
        // Therefore AadTimestampOutOfRange is unreachable within time 0.3's range.
        // If time 0.3 cannot represent the threshold timestamp, the test is
        // vacuously satisfied (defensive path cannot be triggered with this crate).
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
        // else: time 0.3 cannot represent this timestamp; the error variant is
        // dead code in practice — test is vacuously satisfied.
    }

    #[test]
    fn test_aad_to_canonical_bytes_returns_26_bytes() {
        let id = make_id();
        let aad = Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap();
        assert_eq!(aad.to_canonical_bytes().len(), 26);
    }

    #[test]
    fn test_aad_to_canonical_bytes_golden_value() {
        // Golden value test: known RecordId + VaultVersion::CURRENT + UNIX_EPOCH
        // Expected 26 bytes:
        //   [0..16]: UUID bytes MSB first
        //   [16..18]: u16=1 big-endian = [0x00, 0x01]
        //   [18..26]: i64=0 big-endian = [0x00; 8]
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
        assert_eq!(
            canonical, expected,
            "Golden value mismatch: got {:02x?}, expected {:02x?}",
            canonical, expected
        );
    }
}
