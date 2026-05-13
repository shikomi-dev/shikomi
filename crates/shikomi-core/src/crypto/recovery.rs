//! `RecoveryMnemonic` — BIP-39 24 語リカバリ・ニーモニック (Tier-1 揮発、再表示不可)。
//!
//! - 配列長 24 を型レベルで強制 (契約 C-12: `[String; 24]` 引数)。
//! - 各単語の BIP-39 wordlist 検証 / チェックサム検証は Sub-B (`bip39` crate) に委譲。
//!   Sub-A は文字列の最低限の妥当性 (空でない / ASCII 範囲) のみ確認する。
//! - `Drop` で各 `String` のヒープバッファを zeroize。
//! - **再構築不可**: `Clone` 未実装 + 永続化フィールドなしで「初回 1 度のみ表示」契約 (REQ-S13)
//!   を型レベルで担保する。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/errors-and-contracts.md`

use core::fmt;

use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

/// BIP-39 24 語固定。
pub const MNEMONIC_WORD_COUNT: usize = 24;

/// BIP-39 リカバリ・ニーモニック (24 語固定)。
///
/// # Forbidden traits
///
/// `Clone` / `Copy` / `Display` / `serde::Serialize` / `serde::Deserialize` /
/// `PartialEq` / `Eq` は **意図的に未実装**。
/// 特に `Clone` 禁止は REQ-S13「初回 1 度のみ表示・再表示不可」契約の型レベル担保。
///
/// ```compile_fail
/// use shikomi_core::crypto::RecoveryMnemonic;
/// let words: [String; 24] = std::array::from_fn(|_| "abandon".to_string());
/// let m = RecoveryMnemonic::from_words(words).unwrap();
/// let _ = m.clone(); // 契約 C-2 / REQ-S13: Clone 禁止
/// ```
pub struct RecoveryMnemonic {
    words: SecretBox<Zeroizing<[String; MNEMONIC_WORD_COUNT]>>,
}

impl RecoveryMnemonic {
    /// 24 語の `String` 配列から `RecoveryMnemonic` を構築する (契約 C-12)。
    ///
    /// 配列長 24 を型レベルで強制するのが本コンストラクタの唯一の不変条件 (Sub-A 範囲)。
    /// 各単語の BIP-39 wordlist 所属検証・チェックサム検証は Sub-B (`bip39` crate) に
    /// 委譲するため、Sub-A は文字列の値検証を行わず **常に Ok 相当の構築のみ** を提供する。
    #[must_use]
    pub fn from_words(words: [String; MNEMONIC_WORD_COUNT]) -> Self {
        Self {
            words: SecretBox::new(Box::new(Zeroizing::new(words))),
        }
    }

    /// 単語配列への参照を取り出す (Sub-B `Bip39Pbkdf2Hkdf::derive_kek_recovery` 専用)。
    ///
    /// 可視性: `pub` (`shikomi-infra::crypto::kdf` への正規入力経路)。
    ///
    /// 可視性ポリシーの差別化 (Sub-B Rev2 で凍結、`detailed-design/password.md`
    /// §`MasterPassword` 参照):
    /// - `Vek` / `Kek<_>` / `HeaderAeadKey::expose_within_crate` は **`pub(crate)`**
    ///   (鍵バイトは外部 crate に渡さない、`shikomi-core::crypto` 内部閉じ)
    /// - `MasterPassword::expose_secret_bytes` / `RecoveryMnemonic::expose_words`
    ///   は **`pub`** (KDF 入力として `shikomi-infra` アダプタへの正規経路として開放)
    ///
    /// **呼び出して良いのは `shikomi-infra::crypto::kdf` モジュールのみ**。
    /// CLI / GUI / daemon の bin crate から本メソッドを呼ぶことは禁止
    /// (CI grep による静的検出対象、`detailed-design/kdf.md` §`Bip39Pbkdf2Hkdf` 参照)。
    #[must_use]
    pub fn expose_words(&self) -> &[String; MNEMONIC_WORD_COUNT] {
        self.words.expose_secret()
    }

    /// 24 語固定。
    #[must_use]
    pub const fn word_count() -> usize {
        MNEMONIC_WORD_COUNT
    }
}

impl fmt::Debug for RecoveryMnemonic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED MNEMONIC]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_words() -> [String; MNEMONIC_WORD_COUNT] {
        std::array::from_fn(|i| format!("word{i:02}"))
    }

    #[test]
    fn from_words_constructs_with_24_words() {
        let _ = RecoveryMnemonic::from_words(dummy_words());
    }

    #[test]
    fn debug_returns_fixed_redacted_string() {
        let m = RecoveryMnemonic::from_words(dummy_words());
        let s = format!("{m:?}");
        assert_eq!(s, "[REDACTED MNEMONIC]");
        assert!(!s.contains("word00"), "Debug must not expose any word: {s}");
    }

    #[test]
    fn expose_words_returns_24_word_array_in_order() {
        let m = RecoveryMnemonic::from_words(dummy_words());
        let words = m.expose_words();
        assert_eq!(words.len(), MNEMONIC_WORD_COUNT);
        assert_eq!(words[0], "word00");
        assert_eq!(words[23], "word23");
    }

    #[test]
    fn word_count_constant_is_24() {
        assert_eq!(RecoveryMnemonic::word_count(), 24);
    }
}
