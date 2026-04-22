//! 結合テスト: NonceCounter オーバーフロー検知（TC-I05）
//! REQ-010 / AC-05, AC-06

use shikomi_core::{DomainError, NonceCounter};

/// TC-I05: NonceCounter が u32::MAX 到達で NonceOverflow を返す
#[test]
fn test_nonce_counter_overflow_at_max_from_public_api() {
    let mut counter = NonceCounter::resume([0u8; 8], u32::MAX);
    let err = counter.next().unwrap_err();
    assert!(
        matches!(err, DomainError::NonceOverflow),
        "Expected NonceOverflow, got: {:?}",
        err
    );
}

/// TC-I05 補足: u32::MAX - 1 では成功する（境界値直前）
#[test]
fn test_nonce_counter_next_at_max_minus_one_succeeds_from_public_api() {
    let mut counter = NonceCounter::resume([0u8; 8], u32::MAX - 1);
    assert!(
        counter.next().is_ok(),
        "next() at u32::MAX - 1 must succeed"
    );
}
