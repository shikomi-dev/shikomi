//! 結合テスト: SecretString / SecretBytes の非リーク確認（TC-I04）
//! REQ-008 / AC-04, AC-06

use shikomi_core::{SecretBytes, SecretString};

/// TC-I04: SecretString Debug フォーマットで秘密値が露出しない
#[test]
fn test_secret_string_debug_does_not_leak_value() {
    let s = SecretString::from_string("my-password".to_string());
    let debug_output = format!("{:?}", s);
    assert!(
        !debug_output.contains("my-password"),
        "SecretString::Debug must not contain the secret. Got: {}",
        debug_output
    );
    assert!(
        debug_output.contains("[REDACTED]"),
        "SecretString::Debug must contain [REDACTED]. Got: {}",
        debug_output
    );
}

/// TC-I04: SecretBytes Debug フォーマットで生バイトが露出しない
#[test]
fn test_secret_bytes_debug_does_not_leak_value() {
    let b = SecretBytes::from_boxed_slice(b"secret".to_vec().into_boxed_slice());
    let debug_output = format!("{:?}", b);
    // 生バイト値 "secret" や数値リテラルが含まれないことを確認
    assert!(
        !debug_output.contains("115"), // 's' = 115
        "SecretBytes::Debug must not contain raw byte values. Got: {}",
        debug_output
    );
    assert!(
        debug_output.contains("[REDACTED]"),
        "SecretBytes::Debug must contain [REDACTED]. Got: {}",
        debug_output
    );
}
