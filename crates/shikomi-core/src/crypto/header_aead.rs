//! `HeaderAeadKey` — vault ヘッダ独立 AEAD タグ検証用鍵 (KEK_pw 流用)。
//!
//! Sub-0 凍結: ヘッダ AEAD タグの鍵は `KekPw` 流用。`kdf_params` 改竄は
//! KDF 出力変化として AEAD タグ不一致で間接検出される。
//! 鍵経路を型レベルで明示するために独立型として定義する。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/crypto-types.md`

use core::fmt;

use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

use crate::crypto::key::{Kek, KekKindPw, KEY_LEN};

/// vault ヘッダ独立 AEAD タグ検証用鍵 (32 byte)。
///
/// `KekPw` から `from_kek_pw` 経由でのみ派生する。`KekRecovery` を渡そうとすると
/// 関数シグネチャの型不一致でコンパイルエラーになる (Sub-0 凍結の鍵経路を型で明示)。
///
/// `Drop` 時に内部 32B を zeroize。`Clone` / `Display` / `Serialize` / `PartialEq` / `Eq` は未実装。
///
/// ```compile_fail
/// use shikomi_core::crypto::{HeaderAeadKey, Kek, KekKindRecovery};
/// let recovery = Kek::<KekKindRecovery>::from_array([0u8; 32]);
/// // KekRecovery を渡すとコンパイルエラー (契約 C-6: ヘッダ AEAD = KEK_pw 流用のみ)
/// let _ = HeaderAeadKey::from_kek_pw(&recovery);
/// ```
pub struct HeaderAeadKey {
    inner: SecretBox<Zeroizing<[u8; KEY_LEN]>>,
}

impl HeaderAeadKey {
    /// `KekPw` のバイトをコピーして `HeaderAeadKey` を派生する。
    ///
    /// 元の `KekPw` は呼出側スコープで生き続ける (参照のみ取得)。
    /// `HeaderAeadKey` は独立した `SecretBox` を保有し、`Drop` で独立に zeroize される。
    #[must_use]
    pub fn from_kek_pw(kek: &Kek<KekKindPw>) -> Self {
        let bytes = *kek.expose_within_crate();
        Self {
            inner: SecretBox::new(Box::new(Zeroizing::new(bytes))),
        }
    }

    /// crate 内部からのみ生バイト参照を取り出す (ヘッダ AEAD 検証関数 = Sub-D 用)。
    pub(crate) fn expose_within_crate(&self) -> &[u8; KEY_LEN] {
        self.inner.expose_secret()
    }
}

impl fmt::Debug for HeaderAeadKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED HEADER AEAD KEY]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_kek_pw_constructs_without_panic() {
        let kek = Kek::<KekKindPw>::from_array([0u8; KEY_LEN]);
        let _ = HeaderAeadKey::from_kek_pw(&kek);
    }

    #[test]
    fn debug_returns_fixed_redacted_string() {
        let kek = Kek::<KekKindPw>::from_array([0xCDu8; KEY_LEN]);
        let h = HeaderAeadKey::from_kek_pw(&kek);
        assert_eq!(format!("{h:?}"), "[REDACTED HEADER AEAD KEY]");
    }

    #[test]
    fn from_kek_pw_copies_bytes_independently_from_source_kek() {
        let kek = Kek::<KekKindPw>::from_array([0x5Au8; KEY_LEN]);
        let h = HeaderAeadKey::from_kek_pw(&kek);
        // 派生後も元 KEK は使用可能 (参照のみ取得しているため)
        assert_eq!(kek.expose_within_crate(), &[0x5Au8; KEY_LEN]);
        // 派生先も同じバイト列を持つ
        assert_eq!(h.expose_within_crate(), &[0x5Au8; KEY_LEN]);
    }
}
