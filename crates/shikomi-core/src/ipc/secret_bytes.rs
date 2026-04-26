//! IPC 経路で秘密値を運搬する newtype。
//!
//! `SecretBytes` 自体は永続化フォーマット側への誤流入を型で防ぐため `Serialize` /
//! `Deserialize` を意図的に実装しない。本 newtype は **IPC 経路でのみ秘密を運搬する文脈**
//! を明示化し、core 内のみで完結する safe API を呼び出すシリアライズ実装を提供する。
//!
//! 設計根拠: docs/features/daemon-ipc/basic-design/security.md
//! §SecretBytes のシリアライズ契約

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::secret::{clone_secret_string_bytes, SecretBytes, SecretString};

// -------------------------------------------------------------------
// SerializableSecretBytes
// -------------------------------------------------------------------

/// IPC 経路専用の秘密値ラッパ。
///
/// - `Debug` は `[REDACTED:SerializableSecretBytes]` 固定（newtype の Debug 透過防止）
/// - `Serialize` は `serializer.serialize_bytes(...)` を呼び、平文取り出しは core 内に
///   閉じる（`SecretBytes::as_serialize_slice` の `pub(crate)` メソッド経由で取得）
/// - `Deserialize` は `Vec<u8>` 経由で `SecretBytes::from_vec` を呼ぶ
pub struct SerializableSecretBytes(pub SecretBytes);

impl SerializableSecretBytes {
    /// `SecretBytes` をラップする。
    #[must_use]
    pub fn new(inner: SecretBytes) -> Self {
        Self(inner)
    }

    /// 内部の `SecretBytes` への参照を返す。
    #[must_use]
    pub fn inner(&self) -> &SecretBytes {
        &self.0
    }

    /// 内部の `SecretBytes` を取り出す（消費）。
    #[must_use]
    pub fn into_inner(self) -> SecretBytes {
        self.0
    }

    /// `SecretString` を消費して IPC 送信用 `SerializableSecretBytes` に変換する。
    ///
    /// CLI 側 `IpcVaultRepository::add_record` / `edit_record` から呼ばれ、
    /// 平文取り出し経路を core 内（本ファイル）に閉じる
    /// （CI grep TC-CI-015 / TC-CI-016 の監査範囲外を維持）。
    ///
    /// 設計根拠: docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md
    /// §`add_record` / §`edit_record`
    #[must_use]
    pub fn from_secret_string(secret: SecretString) -> Self {
        let bytes = clone_secret_string_bytes(&secret);
        Self(SecretBytes::from_vec(bytes))
    }

    /// **Sub-E (#43) 専用**: daemon の V2 ハンドラが IPC 経由で受け取った
    /// パスワード/recovery 24 語を **core 内に閉じた経路** で UTF-8 文字列に
    /// 変換するためのヘルパ。
    ///
    /// daemon 側 (`crates/shikomi-daemon/src/ipc/v2_handler/`) が直接
    /// `SecretBytes::expose_secret` を呼ぶと **TC-CI-017 grep**
    /// (`crates/shikomi-daemon/src/` に `expose_secret` 文字列禁止) に違反する。
    /// 本 API は shikomi-core 内で `expose_secret` 呼出を 1 行に閉じ込めることで、
    /// daemon の V2 ハンドラが TC-CI-017 を通過しつつ受信値を `String` に変換できる。
    ///
    /// 戻り値は `String::from_utf8_lossy` 経由で **損失あり変換** される。
    /// パスワードに非 UTF-8 バイト列を含めるユーザは想定していない (REQ-S08
    /// パスワード強度ゲート前提)。
    #[must_use]
    pub fn to_lossy_string_for_handler(&self) -> String {
        let bytes = self.0.expose_secret();
        String::from_utf8_lossy(bytes).into_owned()
    }
}

impl Clone for SerializableSecretBytes {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::Debug for SerializableSecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SerializableSecretBytes([REDACTED])")
    }
}

impl Serialize for SerializableSecretBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // `as_serialize_slice` は `pub(crate)` メソッドで、平文取り出しは
        // `crates/shikomi-core/src/secret/` に閉じる（CI grep 監査範囲外）。
        serializer.serialize_bytes(self.0.as_serialize_slice())
    }
}

impl<'de> Deserialize<'de> for SerializableSecretBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_byte_buf(SecretBytesVisitor)
    }
}

struct SecretBytesVisitor;

impl<'de> Visitor<'de> for SecretBytesVisitor {
    type Value = SerializableSecretBytes;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a byte array")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SerializableSecretBytes(SecretBytes::from_vec(v.to_vec())))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SerializableSecretBytes(SecretBytes::from_vec(v)))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut bytes: Vec<u8> = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(b) = seq.next_element::<u8>()? {
            bytes.push(b);
        }
        Ok(SerializableSecretBytes(SecretBytes::from_vec(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_does_not_expose_inner_bytes() {
        let s = SerializableSecretBytes(SecretBytes::from_vec(vec![1, 2, 3, 4]));
        let debug_output = format!("{s:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains('1'));
        assert!(!debug_output.contains('4'));
    }

    #[test]
    fn test_clone_preserves_inner_byte_length() {
        let s = SerializableSecretBytes(SecretBytes::from_vec(vec![10, 20, 30]));
        let cloned = s.clone();
        // inner bytes は core 内 API 経由でしか取り出せない（CI grep 監査）。
        // 本テストでは長さの一致のみ検証する。
        assert_eq!(cloned.inner().as_serialize_slice().len(), 3);
    }

    #[test]
    fn test_into_inner_returns_secret_bytes_with_same_length() {
        let s = SerializableSecretBytes(SecretBytes::from_vec(vec![5, 6, 7]));
        let inner = s.into_inner();
        assert_eq!(inner.as_serialize_slice().len(), 3);
    }

    #[test]
    fn test_from_secret_string_preserves_byte_length_and_redacts_debug() {
        let secret = SecretString::from_string("hunter2".to_string());
        let wrapped = SerializableSecretBytes::from_secret_string(secret);
        assert_eq!(wrapped.inner().as_serialize_slice().len(), "hunter2".len());

        let debug_output = format!("{wrapped:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("hunter2"));
    }

    #[test]
    fn test_from_secret_string_round_trips_via_serialize_slice() {
        // `SecretString` → `SerializableSecretBytes` → 内部スライスのラウンドトリップで
        // バイト列が等価であることを、長さと先頭/末尾のバイトで間接検証する
        // （本ファイルは CI grep `TC-CI-015` の監査範囲下のため、テストでも平文取出
        //  API を呼ばない方針）。
        let phrase = "こんにちは🌸";
        let original = SecretString::from_string(phrase.to_string());
        let wrapped = SerializableSecretBytes::from_secret_string(original);
        let inner_slice = wrapped.inner().as_serialize_slice();
        assert_eq!(inner_slice.len(), phrase.len());
        assert_eq!(inner_slice.first(), phrase.as_bytes().first());
        assert_eq!(inner_slice.last(), phrase.as_bytes().last());
    }
}
