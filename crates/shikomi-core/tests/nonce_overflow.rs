//! 結合テスト: `NonceCounter` 上限到達検知 (TC-I05)
//!
//! Sub-A 凍結後の API:
//! - `NonceCounter::resume(count: u64) -> Self`
//! - `NonceCounter::increment(&mut self) -> Result<(), DomainError>`
//! - 上限 `NonceCounter::LIMIT = 1u64 << 32` (NIST SP 800-38D §8.3 random nonce birthday bound)

use shikomi_core::{DomainError, NonceCounter};

/// TC-I05: `NonceCounter` が `LIMIT` 到達で `NonceLimitExceeded` を返す。
#[test]
fn test_nonce_counter_increment_at_limit_returns_nonce_limit_exceeded_from_public_api() {
    let mut counter = NonceCounter::resume(NonceCounter::LIMIT);
    let err = counter.increment().unwrap_err();
    assert!(
        matches!(err, DomainError::NonceLimitExceeded),
        "Expected NonceLimitExceeded, got: {err:?}"
    );
    // 加算されないこと (Fail Fast: 失敗時は状態不変)
    assert_eq!(counter.current(), NonceCounter::LIMIT);
}

/// TC-I05 補足: `LIMIT - 1` では成功し、`current()` が `LIMIT` に到達する。
#[test]
fn test_nonce_counter_increment_at_limit_minus_one_succeeds_from_public_api() {
    let mut counter = NonceCounter::resume(NonceCounter::LIMIT - 1);
    assert!(
        counter.increment().is_ok(),
        "increment() at LIMIT - 1 must succeed"
    );
    assert_eq!(counter.current(), NonceCounter::LIMIT);
}
