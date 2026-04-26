//! `AeadKey` trait — 鍵バイトをクロージャインジェクション経由で AEAD adapter に貸す。
//!
//! Sub-B Rev2 で凍結した「鍵バイト型は `pub(crate)` 維持」契約を破壊せず、
//! shikomi-infra::crypto::aead から鍵バイトに借用越境する正規経路を提供する
//! (verify_aead_decrypt の caller-asserted マーカーと完全同型思想)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/nonce-and-aead.md` §`AeadKey` trait

use crate::crypto::header_aead::HeaderAeadKey;
use crate::crypto::key::{Kek, KekKindPw, KekKindRecovery, Vek};

/// AEAD 鍵 (32B = 256bit) クロージャインジェクション trait。
///
/// **dyn-safe ではない** (`impl FnOnce<R>` のため `&dyn AeadKey` 不可)。これは意図的:
/// trait オブジェクト化で attacker-readable 経路 + 最適化機会喪失を構造禁止する
/// (`nonce-and-aead.md` §`AeadKey` trait 設計判断)。
///
/// 実装は `Vek` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` / `HeaderAeadKey`。
/// いずれも内部 `expose_within_crate()` (`pub(crate)`) を `f` に渡すだけのワンライナー。
pub trait AeadKey {
    /// 鍵バイト 32B にクロージャ `f` を適用し、`f` の戻り値を返す。
    ///
    /// 鍵バイトは shikomi-core 側の `SecretBox<Zeroizing<[u8; 32]>>` 内に
    /// 留まり、`f` 実行中のみ借用が貸し出される。`f` 実行後の bytes 借用は
    /// 失効するが、鍵自体の zeroize は呼出元 `Self` の Drop で実施される
    /// (本メソッド内では再 zeroize しない、性能劣化回避)。
    fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R;
}

impl AeadKey for Vek {
    fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        f(self.expose_within_crate())
    }
}

impl AeadKey for Kek<KekKindPw> {
    fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        f(self.expose_within_crate())
    }
}

impl AeadKey for Kek<KekKindRecovery> {
    fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        f(self.expose_within_crate())
    }
}

impl AeadKey for HeaderAeadKey {
    fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        f(self.expose_within_crate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vek_with_secret_bytes_invokes_closure_with_32b_array() {
        let v = Vek::from_array([0x42u8; 32]);
        let captured = v.with_secret_bytes(|bytes| bytes[0]);
        assert_eq!(captured, 0x42);
    }

    #[test]
    fn kek_pw_with_secret_bytes_passes_underlying_bytes() {
        let k = Kek::<KekKindPw>::from_array([0xABu8; 32]);
        let sum: u32 = k.with_secret_bytes(|bytes| bytes.iter().map(|&b| u32::from(b)).sum());
        assert_eq!(sum, 0xAB * 32);
    }

    #[test]
    fn kek_recovery_with_secret_bytes_passes_underlying_bytes() {
        let k = Kek::<KekKindRecovery>::from_array([0x77u8; 32]);
        let v = k.with_secret_bytes(|bytes| bytes[31]);
        assert_eq!(v, 0x77);
    }

    #[test]
    fn header_aead_key_with_secret_bytes_passes_kek_pw_derived_bytes() {
        let kek = Kek::<KekKindPw>::from_array([0x5Au8; 32]);
        let h = HeaderAeadKey::from_kek_pw(&kek);
        let v = h.with_secret_bytes(|bytes| bytes[0]);
        assert_eq!(v, 0x5A);
    }
}
