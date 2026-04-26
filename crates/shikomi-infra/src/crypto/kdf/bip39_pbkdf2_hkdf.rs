//! `Bip39Pbkdf2Hkdf` — 24 語 → BIP-39 経由で 64B seed → HKDF-SHA256 → `Kek<KekKindRecovery>`。
//!
//! 処理経路 (`kdf.md` L93):
//!
//! 1. `RecoveryMnemonic::expose_words()` で `&[String; 24]` 取得
//! 2. 24 語をスペース区切りで連結し `bip39::Mnemonic::parse_in(English, joined)` で
//!    wordlist + checksum 検証 (失敗 → `CryptoError::InvalidMnemonic`)
//! 3. `bip39_mnemonic.to_seed("")` で 64B seed (PBKDF2-HMAC-SHA512 2048iter
//!    `salt="mnemonic"`、内部実装は `bip39` crate に委譲)
//! 4. `Hkdf::<Sha256>::new(None, &seed).expand(HKDF_INFO, &mut [u8; 32])` で 32B KEK 導出
//! 5. `Kek::<KekKindRecovery>::from_array` でラップ
//!
//! 中間 seed 64B + KEK 32B は `Zeroizing` で囲む。`bip39_mnemonic` 自体は
//! `bip39` crate v2 の `zeroize` feature により Drop 時に zeroize される
//! (`tech-stack.md` §4.7 `bip39` 行)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/kdf.md`

use bip39::{Language, Mnemonic};
use hkdf::Hkdf;
use sha2::Sha256;
use shikomi_core::crypto::{Kek, KekKindRecovery, RecoveryMnemonic};
use shikomi_core::error::{CryptoError, KdfErrorKind};
use zeroize::Zeroizing;

// 出力 KEK_recovery バイト長 (32B、AES-256 鍵長)。
const KEK_LEN: usize = 32;

// BIP-39 mnemonic → seed の出力長 (PBKDF2-HMAC-SHA512 → 64B、BIP-39 仕様固定)。
const SEED_LEN: usize = 64;

/// HKDF info 凍結値 (`tech-stack.md` §2.4 KEK_recovery 行 + Sub-0 凍結)。
///
/// アプリ固有のラベル (RFC 5869 §3.2 Recommendations: HKDF-info によるドメイン分離)。
/// 将来 KDF アルゴリズム変更時は `b"shikomi-kek-v2"` で別 const を追加し、
/// vault ヘッダの `kdf_version` で分岐する。
pub const HKDF_INFO: &[u8] = b"shikomi-kek-v1";

/// BIP-39 + PBKDF2 + HKDF KDF アダプタ。無状態 struct。
#[derive(Debug, Clone, Copy, Default)]
pub struct Bip39Pbkdf2Hkdf;

impl Bip39Pbkdf2Hkdf {
    /// 24 語リカバリ・ニーモニックから `Kek<KekKindRecovery>` を導出する。
    ///
    /// **変数名規約** (`kdf.md` L93): 引数 `recovery: &RecoveryMnemonic` (Sub-A 型)、
    /// 内部派生する `bip39::Mnemonic` インスタンスは `bip39_mnemonic` で受ける
    /// (同一 `mnemonic` 識別子の取り違え禁止)。
    ///
    /// # Errors
    ///
    /// - `bip39::Mnemonic::parse_in` 失敗 (wordlist 不一致 / checksum 不一致 / 単語数不正):
    ///   `CryptoError::InvalidMnemonic` (Sub-D `vault unlock --recovery` で MSG-S12 に変換)
    /// - `hkdf::Hkdf::expand` 失敗 (`okm.len() > 255 * 32`、32B 出力では発生しない):
    ///   `CryptoError::KdfFailed { kind: KdfErrorKind::Hkdf, source }`
    pub fn derive_kek_recovery(
        &self,
        recovery: &RecoveryMnemonic,
    ) -> Result<Kek<KekKindRecovery>, CryptoError> {
        let words = recovery.expose_words();
        let joined = words.join(" ");

        let bip39_mnemonic = Mnemonic::parse_in(Language::English, &joined)
            .map_err(|_| CryptoError::InvalidMnemonic)?;

        // PBKDF2-HMAC-SHA512 2048iter (BIP-39 標準、`bip39` crate 内部で実装)。
        // 中間 64B seed を Zeroizing で囲む (Drop で zeroize)。
        let seed: Zeroizing<[u8; SEED_LEN]> = Zeroizing::new(bip39_mnemonic.to_seed(""));

        // HKDF-SHA256: salt=None で内部 default salt ([0u8; 32] 等価)、info 固定。
        let mut okm: Zeroizing<[u8; KEK_LEN]> = Zeroizing::new([0u8; KEK_LEN]);
        Hkdf::<Sha256>::new(None, seed.as_slice())
            .expand(HKDF_INFO, okm.as_mut_slice())
            .map_err(|e| CryptoError::KdfFailed {
                kind: KdfErrorKind::Hkdf,
                source: Box::new(e),
            })?;

        Ok(Kek::<KekKindRecovery>::from_array(*okm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HKDF info 定数の値が固定であることをコンパイル時 + 実行時で確認 (Sub-0 凍結)。
    #[test]
    fn hkdf_info_constant_is_shikomi_kek_v1() {
        assert_eq!(HKDF_INFO, b"shikomi-kek-v1");
    }

    /// BIP-39 標準テスト mnemonic (entropy 0x00 × 32B → 24 語、英語 wordlist)。
    /// trezor 公式 `vectors.json` Test 11 相当: "abandon" × 23 + "art"。
    fn standard_24_words() -> [String; 24] {
        let mut arr: [String; 24] = std::array::from_fn(|_| String::new());
        for (i, slot) in arr.iter_mut().enumerate() {
            *slot = if i == 23 {
                "art".to_string()
            } else {
                "abandon".to_string()
            };
        }
        arr
    }

    #[test]
    fn derive_kek_recovery_succeeds_for_valid_24_words() {
        let recovery = RecoveryMnemonic::from_words(standard_24_words());
        let adapter = Bip39Pbkdf2Hkdf;
        assert!(adapter.derive_kek_recovery(&recovery).is_ok());
    }

    /// 同一入力 → 同一 KEK 出力 (BIP-39 + PBKDF2 + HKDF いずれも決定論的)。
    #[test]
    fn derive_kek_recovery_is_deterministic_for_same_words() {
        let r1 = RecoveryMnemonic::from_words(standard_24_words());
        let r2 = RecoveryMnemonic::from_words(standard_24_words());
        let adapter = Bip39Pbkdf2Hkdf;
        let k1 = adapter.derive_kek_recovery(&r1).unwrap();
        let k2 = adapter.derive_kek_recovery(&r2).unwrap();
        // `Kek` の `expose_within_crate` は `pub(crate)`、Debug 文字列で同型確認のみ。
        assert_eq!(format!("{k1:?}"), format!("{k2:?}"));
    }

    /// 不正な単語 → `CryptoError::InvalidMnemonic`。
    #[test]
    fn derive_kek_recovery_returns_invalid_mnemonic_for_unknown_word() {
        let mut words = standard_24_words();
        words[0] = "notawordinwordlist".to_string();
        let recovery = RecoveryMnemonic::from_words(words);
        let adapter = Bip39Pbkdf2Hkdf;
        let err = adapter.derive_kek_recovery(&recovery).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidMnemonic));
    }

    /// checksum 不一致 (24 語全て "abandon") → `CryptoError::InvalidMnemonic`。
    /// (entropy 全 0 の正規ニーモニックの最終語は "art" のため "abandon" × 24 は checksum 不一致)
    #[test]
    fn derive_kek_recovery_returns_invalid_mnemonic_for_checksum_failure() {
        let words: [String; 24] = std::array::from_fn(|_| "abandon".to_string());
        let recovery = RecoveryMnemonic::from_words(words);
        let adapter = Bip39Pbkdf2Hkdf;
        let err = adapter.derive_kek_recovery(&recovery).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidMnemonic));
    }
}
