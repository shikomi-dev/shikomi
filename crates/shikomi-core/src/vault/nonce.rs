//! nonce 管理 — `NonceBytes` (96 bit per-record nonce) と `NonceCounter` (暗号化回数監視)。
//!
//! Sub-A 凍結 (`docs/features/vault-encryption/detailed-design/nonce-and-aead.md`):
//!
//! - `NonceBytes` (12 byte): per-record AEAD nonce。**完全 random 12B** が CSPRNG 由来で
//!   呼び出し側 (`shikomi-infra::crypto::Rng`) から `[u8; 12]` として供給される。
//!   `from_random([u8; 12])` で構築 (失敗しない、型レベルで長さ強制 = 契約 C-10)。
//!   既存 `try_new(&[u8])` は永続化からの復元用に維持する。
//!
//! - `NonceCounter`: 「**この VEK で何回暗号化したか**」を u64 で数える単独カウンタ。
//!   nonce 値生成には**関与しない**。上限 `1u64 << 32` (NIST SP 800-38D §8.3 random nonce
//!   birthday bound)。`increment` で加算 → 上限到達で `Err(NonceLimitExceeded)` (契約 C-9)。
//!
//! Boy Scout Rule: 旧 `next() -> NonceBytes` API は削除。旧 `random_prefix: [u8; 8]`
//! フィールドも削除 (random nonce 採用で prefix 共有不要)。

use crate::error::{DomainError, InvalidRecordPayloadReason};

/// `NonceBytes` の固定長 (NIST SP 800-38D §5.2.1.1、96 bit)。
const NONCE_LEN: usize = 12;

// -------------------------------------------------------------------
// NonceBytes
// -------------------------------------------------------------------

/// AES-256-GCM の per-record nonce (96 bit = 12 byte)。
///
/// Sub-0 凍結: random nonce 戦略 (NIST SP 800-38D §8.3 birthday bound)。
/// `shikomi-infra::crypto::Rng::generate_nonce_bytes()` が CSPRNG から `[u8; 12]` を
/// 取得して `from_random` に渡す。`shikomi-core` 側は CSPRNG を呼ばない (no-I/O 制約)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonceBytes {
    inner: [u8; NONCE_LEN],
}

impl NonceBytes {
    /// CSPRNG 由来の 12 byte 配列から `NonceBytes` を構築する (契約 C-10)。
    ///
    /// 引数が `[u8; 12]` のため**型レベルで長さが強制され失敗しない**。
    /// `from_random` という関数名で「これは CSPRNG 由来である」という呼出側契約を明示する
    /// (ad-hoc な `[0u8; 12]` 等の決定論値構築はテスト用途以外で禁止、
    /// CI grep ルールは Sub-B / Sub-D で確定)。
    #[must_use]
    pub fn from_random(bytes: [u8; NONCE_LEN]) -> Self {
        Self { inner: bytes }
    }

    /// バイトスライスから `NonceBytes` を構築する (永続化からの復元用)。
    ///
    /// # Errors
    /// `bytes.len() != 12` の場合 `DomainError::InvalidRecordPayload(NonceLength)` を返す。
    pub fn try_new(bytes: &[u8]) -> Result<Self, DomainError> {
        if bytes.len() != NONCE_LEN {
            return Err(DomainError::InvalidRecordPayload(
                InvalidRecordPayloadReason::NonceLength {
                    expected: NONCE_LEN,
                    got: bytes.len(),
                },
            ));
        }
        let mut inner = [0u8; NONCE_LEN];
        inner.copy_from_slice(bytes);
        Ok(Self { inner })
    }

    /// 内包する 12 バイト配列への参照を返す。
    #[must_use]
    pub fn as_array(&self) -> &[u8; NONCE_LEN] {
        &self.inner
    }
}

// -------------------------------------------------------------------
// NonceCounter
// -------------------------------------------------------------------

/// 「この VEK での暗号化回数」を u64 で数える単独カウンタ。
///
/// nonce 値生成には**関与しない** (Sub-A 凍結で責務再定義、Boy Scout Rule)。
/// 上限到達で `increment` が `Err(NonceLimitExceeded)` を返し、呼出側は
/// `vault rekey` フロー (Sub-F) を起動する責務を負う (契約 C-9)。
#[derive(Debug)]
pub struct NonceCounter {
    count: u64,
}

impl NonceCounter {
    /// 上限値 (NIST SP 800-38D §8.3 random nonce birthday bound = `2^32`)。
    pub const LIMIT: u64 = 1u64 << 32;

    /// カウンタを 0 から開始する新規 `NonceCounter` を生成する。
    #[must_use]
    pub fn new() -> Self {
        Self { count: 0 }
    }

    /// 永続化したカウンタ値から `NonceCounter` を再開する (vault unlock 時)。
    #[must_use]
    pub fn resume(count: u64) -> Self {
        Self { count }
    }

    /// 暗号化 1 回ごとに呼び出して加算する (契約 C-9)。
    ///
    /// # Errors
    /// `count >= LIMIT` の場合 `DomainError::NonceLimitExceeded` を返し、加算しない。
    /// この状態では vault の `rekey` が必要。
    pub fn increment(&mut self) -> Result<(), DomainError> {
        if self.count >= Self::LIMIT {
            return Err(DomainError::NonceLimitExceeded);
        }
        self.count += 1;
        Ok(())
    }

    /// 現在のカウント値を返す (永続化用)。
    #[must_use]
    pub fn current(&self) -> u64 {
        self.count
    }
}

impl Default for NonceCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // NonceBytes
    // -----------------------------------------------------------------

    #[test]
    fn nonce_bytes_from_random_constructs_without_panic() {
        let _ = NonceBytes::from_random([0u8; NONCE_LEN]);
    }

    #[test]
    fn nonce_bytes_from_random_preserves_input_bytes() {
        let bytes = [0xABu8; NONCE_LEN];
        let n = NonceBytes::from_random(bytes);
        assert_eq!(n.as_array(), &bytes);
    }

    #[test]
    fn nonce_bytes_try_new_with_12_bytes_ok() {
        assert!(NonceBytes::try_new(&[0u8; NONCE_LEN]).is_ok());
    }

    #[test]
    fn nonce_bytes_try_new_with_wrong_length_returns_nonce_length_error() {
        let err = NonceBytes::try_new(&[0u8; 11]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::NonceLength {
                expected: 12,
                got: 11,
            })
        ));
    }

    // -----------------------------------------------------------------
    // NonceCounter (C-9)
    // -----------------------------------------------------------------

    #[test]
    fn nonce_counter_new_starts_at_zero() {
        let c = NonceCounter::new();
        assert_eq!(c.current(), 0);
    }

    #[test]
    fn nonce_counter_resume_starts_at_given_value() {
        let c = NonceCounter::resume(42);
        assert_eq!(c.current(), 42);
    }

    #[test]
    fn nonce_counter_increment_advances_by_one() {
        let mut c = NonceCounter::new();
        c.increment().unwrap();
        assert_eq!(c.current(), 1);
    }

    #[test]
    fn nonce_counter_limit_constant_is_2_pow_32() {
        assert_eq!(NonceCounter::LIMIT, 1u64 << 32);
    }

    #[test]
    fn nonce_counter_increment_just_below_limit_succeeds() {
        let mut c = NonceCounter::resume(NonceCounter::LIMIT - 1);
        assert!(c.increment().is_ok());
        assert_eq!(c.current(), NonceCounter::LIMIT);
    }

    #[test]
    fn nonce_counter_increment_at_limit_returns_nonce_limit_exceeded() {
        let mut c = NonceCounter::resume(NonceCounter::LIMIT);
        let err = c.increment().unwrap_err();
        assert!(matches!(err, DomainError::NonceLimitExceeded));
        // 加算されないこと
        assert_eq!(c.current(), NonceCounter::LIMIT);
    }

    #[test]
    fn nonce_counter_default_starts_at_zero() {
        let c = NonceCounter::default();
        assert_eq!(c.current(), 0);
    }
}
