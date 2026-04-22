//! 秘密値ラッパ型。
//!
//! `SecretString` / `SecretBytes` は内部を `secrecy::SecretBox` に格納し、
//! `Debug` / `Display` では `[REDACTED]` のみを出力する。
//! `serde::Serialize` は意図的に未実装（コンパイル時に誤シリアライズを防ぐ）。

use std::fmt;

use secrecy::{ExposeSecret, SecretBox};

// -------------------------------------------------------------------
// SecretString
// -------------------------------------------------------------------

/// ヒープ上の文字列を `secrecy::SecretBox` で保護するラッパ。
///
/// - `Debug` は `[REDACTED]` 固定（`Display` は未実装：ログ漏洩リスクを型で排除）
/// - `Clone` `は内部文字列を再ラップして生成する（SecretBox::clone` は使用不可）
/// - `serde::Serialize` は未実装
pub struct SecretString {
    inner: SecretBox<String>,
}

impl SecretString {
    /// `String` を `SecretString` に変換する。入力検証は呼び出し元の責務。
    #[must_use]
    pub fn from_string(s: String) -> Self {
        Self {
            inner: SecretBox::new(Box::new(s)),
        }
    }

    /// 保持している文字列への参照を返す。
    ///
    /// 返り値をログや永続化に流さないこと。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.inner.expose_secret().as_str()
    }
}

impl Clone for SecretString {
    fn clone(&self) -> Self {
        Self::from_string(self.expose_secret().to_owned())
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

// -------------------------------------------------------------------
// SecretBytes
// -------------------------------------------------------------------

/// ヒープ上のバイト列を `secrecy::SecretBox` で保護するラッパ。
///
/// - `Debug` は `[REDACTED]` 固定
/// - `Clone` は内部バイトを再ラップして生成する
/// - `serde::Serialize` は未実装
pub struct SecretBytes {
    inner: SecretBox<Vec<u8>>,
}

impl SecretBytes {
    /// `Box<[u8]>` を `SecretBytes` に変換する。
    #[must_use]
    pub fn from_boxed_slice(b: Box<[u8]>) -> Self {
        Self {
            inner: SecretBox::new(Box::new(b.into_vec())),
        }
    }

    /// 保持しているバイト列への参照を返す。
    ///
    /// 返り値をログや永続化に流さないこと。
    #[must_use]
    pub fn expose_secret(&self) -> &[u8] {
        self.inner.expose_secret().as_slice()
    }
}

impl Clone for SecretBytes {
    fn clone(&self) -> Self {
        Self::from_boxed_slice(self.expose_secret().to_vec().into_boxed_slice())
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_string_from_string_constructs_without_panic() {
        let _ = SecretString::from_string("password".to_string());
    }

    #[test]
    fn test_secret_string_debug_does_not_expose_value() {
        let s = SecretString::from_string("password".to_string());
        let debug_output = format!("{:?}", s);
        assert!(
            !debug_output.contains("password"),
            "Debug must not expose the secret"
        );
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn test_secret_string_expose_secret_returns_original_value() {
        let s = SecretString::from_string("password".to_string());
        assert_eq!(s.expose_secret(), "password");
    }

    #[test]
    fn test_secret_bytes_from_boxed_slice_constructs_without_panic() {
        let _ = SecretBytes::from_boxed_slice(vec![1u8, 2, 3].into_boxed_slice());
    }

    #[test]
    fn test_secret_bytes_debug_does_not_expose_value() {
        let b = SecretBytes::from_boxed_slice(vec![1u8, 2, 3].into_boxed_slice());
        let debug_output = format!("{:?}", b);
        assert!(
            !debug_output.contains("1, 2, 3"),
            "Debug must not expose bytes"
        );
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn test_secret_bytes_expose_secret_returns_original_bytes() {
        let b = SecretBytes::from_boxed_slice(vec![1u8, 2, 3].into_boxed_slice());
        assert_eq!(b.expose_secret(), &[1u8, 2, 3]);
    }
}
