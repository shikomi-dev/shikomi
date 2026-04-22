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
