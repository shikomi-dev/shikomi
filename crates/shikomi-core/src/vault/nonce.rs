//! nonce 管理（NonceBytes / `NonceCounter`）。
//!
//! AES-256-GCM の 96-bit IV は「8B CSPRNG prefix + 4B u32 counter (big-endian)」で構成する。
//! `NonceCounter` は pure Rust / no-I/O：乱数 prefix は構築時に呼び出し側が供給する。

use crate::error::{DomainError, InvalidRecordPayloadReason};

/// `NonceBytes` の固定長（NIST SP 800-38D §5.2.1.1、96 bit）。
const NONCE_LEN: usize = 12;
/// CSPRNG prefix の長さ。
const PREFIX_LEN: usize = 8;

// -------------------------------------------------------------------
// NonceBytes
// -------------------------------------------------------------------

/// AES-256-GCM の nonce（96 bit = 12 byte）。
///
/// レイアウト: `[0..8]` = CSPRNG prefix、`[8..12]` = u32 counter (big-endian)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonceBytes {
    inner: [u8; NONCE_LEN],
}

impl NonceBytes {
    /// バイトスライスから `NonceBytes` を構築する。
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

/// nonce を単調増加カウンタで管理する。
///
/// nonce = `random_prefix` (8B) ‖ `counter` (4B big-endian)。
/// カウンタが `u32::MAX` に達した次の `next()` 呼び出しで
/// `DomainError::NonceOverflow` を返し rekey を強制する。
///
/// 乱数生成は `shikomi-core` の責務外（pure Rust / no-I/O）。
/// `random_prefix` は呼び出し側（`shikomi-infra`）が OS CSPRNG から供給する。
pub struct NonceCounter {
    counter: u32,
    random_prefix: [u8; PREFIX_LEN],
}

impl NonceCounter {
    /// カウンタを 0 から開始する新規 `NonceCounter` を生成する。
    ///
    /// `random_prefix` は 8 バイトの CSPRNG 乱数。
    #[must_use]
    pub fn new(random_prefix: [u8; PREFIX_LEN]) -> Self {
        Self {
            counter: 0,
            random_prefix,
        }
    }

    /// 永続化したカウンタ値から `NonceCounter` を再開する。
    ///
    /// vault をロードして nonce を引き継ぐ際に使用する。
    #[must_use]
    pub fn resume(random_prefix: [u8; PREFIX_LEN], counter: u32) -> Self {
        Self {
            counter,
            random_prefix,
        }
    }

    /// 現在のカウンタ値から `NonceBytes` を生成し、カウンタをインクリメントする。
    ///
    /// # Errors
    /// カウンタが `u32::MAX` の場合 `DomainError::NonceOverflow` を返す。
    /// この状態では vault の rekey が必要（NIST SP 800-38D §8.3 準拠）。
    #[allow(clippy::should_implement_trait)] // 設計で `next()` と命名されており、Iterator は不適切
    pub fn next(&mut self) -> Result<NonceBytes, DomainError> {
        if self.counter == u32::MAX {
            return Err(DomainError::NonceOverflow);
        }
        let mut bytes = [0u8; NONCE_LEN];
        bytes[..PREFIX_LEN].copy_from_slice(&self.random_prefix);
        bytes[PREFIX_LEN..].copy_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        Ok(NonceBytes { inner: bytes })
    }

    /// 現在のカウンタ値を返す（永続化用）。
    #[must_use]
    pub fn current_counter(&self) -> u32 {
        self.counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_counter_new_starts_at_zero() {
        let counter = NonceCounter::new([0u8; 8]);
        assert_eq!(counter.current_counter(), 0u32);
    }

    #[test]
    fn test_nonce_counter_next_returns_12_byte_nonce() {
        let mut counter = NonceCounter::new([0u8; 8]);
        let nonce = counter.next().unwrap();
        assert_eq!(nonce.as_array().len(), 12);
    }

    #[test]
    fn test_nonce_counter_current_counter_increments_after_next() {
        let mut counter = NonceCounter::new([0u8; 8]);
        counter.next().unwrap();
        assert_eq!(counter.current_counter(), 1u32);
    }

    #[test]
    fn test_nonce_counter_resume_starts_at_given_counter() {
        let counter = NonceCounter::resume([0u8; 8], 42);
        assert_eq!(counter.current_counter(), 42u32);
    }

    #[test]
    fn test_nonce_counter_next_at_max_minus_one_succeeds() {
        let mut counter = NonceCounter::resume([0u8; 8], u32::MAX - 1);
        assert!(counter.next().is_ok());
    }

    #[test]
    fn test_nonce_counter_next_at_max_returns_nonce_overflow() {
        let mut counter = NonceCounter::resume([0u8; 8], u32::MAX);
        let err = counter.next().unwrap_err();
        assert!(matches!(err, DomainError::NonceOverflow));
    }

    #[test]
    fn test_nonce_counter_next_prefix_bytes_match_random_prefix() {
        let prefix = [0xABu8; 8];
        let mut counter = NonceCounter::new(prefix);
        let nonce = counter.next().unwrap();
        assert_eq!(&nonce.as_array()[0..8], &prefix);
    }

    #[test]
    fn test_nonce_counter_next_counter_bytes_are_big_endian() {
        let mut counter = NonceCounter::new([0u8; 8]);
        let n1 = counter.next().unwrap();
        let n2 = counter.next().unwrap();
        let n3 = counter.next().unwrap();
        assert_eq!(
            &n1.as_array()[8..12],
            &[0x00u8, 0x00, 0x00, 0x00],
            "1st: counter=0"
        );
        assert_eq!(
            &n2.as_array()[8..12],
            &[0x00u8, 0x00, 0x00, 0x01],
            "2nd: counter=1"
        );
        assert_eq!(
            &n3.as_array()[8..12],
            &[0x00u8, 0x00, 0x00, 0x02],
            "3rd: counter=2"
        );
    }
}
