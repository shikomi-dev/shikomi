//! CSPRNG 単一エントリ点 — `Rng` (`rand_core::OsRng` + `getrandom` バックエンド)。
//!
//! Sub-0 凍結文言「`shikomi-infra::crypto::Rng::generate_kdf_salt() -> KdfSalt`」を
//! Clean Architecture 整合的に物理実装する。shikomi-core は no-I/O 制約のため
//! CSPRNG を直接呼ばず、本モジュールが OS syscall (Linux: `getrandom(2)`、
//! macOS: `getentropy(2)`、Windows: `BCryptGenRandom`) を集約する。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/rng.md`

use rand_core::{OsRng, RngCore};
use shikomi_core::{KdfSalt, NonceBytes, Vek};
use zeroize::Zeroizing;

// バッファ長定数。`KdfSalt` 16B / `Vek` 32B / `NonceBytes` 12B / mnemonic entropy 32B。
const KDF_SALT_LEN: usize = 16;
const VEK_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const MNEMONIC_ENTROPY_LEN: usize = 32;

/// CSPRNG 単一エントリ点。`#[derive(Clone, Copy, Default)]` の無状態 struct。
///
/// 内部に `OsRng` インスタンスを保持しない (`OsRng` は構造体としてゼロサイズ、
/// `RngCore` 実装の thin wrapper)。各メソッド呼出時にローカル `OsRng` を構築 →
/// `fill_bytes` → drop の単発パターンで `&self` を維持する。
///
/// 設計判断: `rng.md` §設計判断の補足 §なぜ `Rng` を `pub struct Rng;` の無状態 struct にするか。
#[derive(Debug, Clone, Copy, Default)]
pub struct Rng;

impl Rng {
    /// Argon2id KDF ソルト 16B を生成する (Sub-D `vault encrypt` / `change-password` 用)。
    ///
    /// 中間バッファを `Zeroizing<[u8; 16]>` で囲み、戻り値 `KdfSalt` (内部 `[u8; 16]` コピー保持)
    /// 構築後にスコープ抜けで Drop → zeroize される。
    ///
    /// # Panics
    ///
    /// `OsRng::fill_bytes` は失敗時 panic する (`getrandom` crate の挙動、shikomi 対象 OS では
    /// 事実上発生しない、`rng.md` §`OsRng` 呼出パターン §エラー処理)。
    #[must_use]
    pub fn generate_kdf_salt(&self) -> KdfSalt {
        let mut buf: Zeroizing<[u8; KDF_SALT_LEN]> = Zeroizing::new([0u8; KDF_SALT_LEN]);
        OsRng.fill_bytes(buf.as_mut_slice());
        // `KdfSalt::from_array` は型レベルで 16B 固定 (Sub-A Boy Scout で AuthTag::from_array
        // と同型 API を追加、`try_new(&[u8])` は永続化復元専用)。Result/expect 経路を回避する
        // ことで TC-B-I04「production の unwrap/expect 禁止」契約を満たす。
        KdfSalt::from_array(*buf)
    }

    /// VEK 32B を生成する (Sub-D `vault encrypt` / `vault rekey` 用)。
    ///
    /// 中間バッファを `Zeroizing<[u8; 32]>` で囲み、`Vek::from_array(*buf)` で値ムーブ後に
    /// 元バッファは Drop → zeroize される (`Vek` 内部は `SecretBox<Zeroizing<[u8; 32]>>`)。
    ///
    /// # Panics
    ///
    /// `OsRng::fill_bytes` は失敗時 panic (前述同)。
    #[must_use]
    pub fn generate_vek(&self) -> Vek {
        let mut buf: Zeroizing<[u8; VEK_LEN]> = Zeroizing::new([0u8; VEK_LEN]);
        OsRng.fill_bytes(buf.as_mut_slice());
        Vek::from_array(*buf)
    }

    /// per-record AEAD nonce 12B を生成する (Sub-C AEAD 暗号化のたびに呼出)。
    ///
    /// random nonce 戦略 (NIST SP 800-38D §8.3 birthday bound)。`NonceCounter` で
    /// 暗号化回数を別途監視し、$2^{32}$ 到達時に rekey 強制。
    ///
    /// # Panics
    ///
    /// `OsRng::fill_bytes` は失敗時 panic (前述同)。
    #[must_use]
    pub fn generate_nonce_bytes(&self) -> NonceBytes {
        let mut buf: Zeroizing<[u8; NONCE_LEN]> = Zeroizing::new([0u8; NONCE_LEN]);
        OsRng.fill_bytes(buf.as_mut_slice());
        NonceBytes::from_random(*buf)
    }

    /// BIP-39 24 語生成元のエントロピー 256 bit (32B) を生成する。
    ///
    /// Sub-D の `vault recovery-show` 初回フローで呼出 → `bip39::Mnemonic::from_entropy` で
    /// 24 語化 → `RecoveryMnemonic::from_words` で型化される。
    /// 戻り値 `Zeroizing<[u8; 32]>` は呼出側スコープ終了で Drop → zeroize。
    ///
    /// # Panics
    ///
    /// `OsRng::fill_bytes` は失敗時 panic (前述同)。
    #[must_use]
    pub fn generate_mnemonic_entropy(&self) -> Zeroizing<[u8; MNEMONIC_ENTROPY_LEN]> {
        let mut buf: Zeroizing<[u8; MNEMONIC_ENTROPY_LEN]> =
            Zeroizing::new([0u8; MNEMONIC_ENTROPY_LEN]);
        OsRng.fill_bytes(buf.as_mut_slice());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 構築コストゼロ (Default で取得可能、テスト容易性の根拠)。
    /// `Rng` は unit struct のため `Rng::default()` を直接呼ぶと
    /// clippy::default_constructed_unit_structs に拒否される。
    /// `<Rng as Default>::default()` 経由で Default trait の実装存在を担保する。
    #[test]
    fn rng_default_constructs_without_panic() {
        let _ = Rng;
        let _ = <Rng as Default>::default();
    }

    /// `KdfSalt` 16B を返す (型レベル検証のため、内部値の確率的検証は KAT で実施しない)。
    #[test]
    fn generate_kdf_salt_returns_16_bytes() {
        let rng = Rng;
        let salt = rng.generate_kdf_salt();
        assert_eq!(salt.as_array().len(), 16);
    }

    /// 連続呼出で異なる値が出る (CSPRNG が動作している sanity check)。
    /// 確率的 false negative は $2^{-128}$ で無視可能。
    #[test]
    fn generate_kdf_salt_yields_distinct_values_on_consecutive_calls() {
        let rng = Rng;
        let a = rng.generate_kdf_salt();
        let b = rng.generate_kdf_salt();
        assert_ne!(a.as_array(), b.as_array());
    }

    /// `Vek` を構築できる (内部値は `expose_within_crate` が `pub(crate)` のため
    /// 外部から比較不可、構築成否のみ確認)。
    #[test]
    fn generate_vek_constructs_without_panic() {
        let _ = Rng.generate_vek();
    }

    /// `NonceBytes` 12B を返す。
    #[test]
    fn generate_nonce_bytes_returns_12_bytes() {
        let n = Rng.generate_nonce_bytes();
        assert_eq!(n.as_array().len(), 12);
    }

    /// 連続呼出で異なる nonce (random nonce 戦略の sanity)。
    #[test]
    fn generate_nonce_bytes_yields_distinct_values_on_consecutive_calls() {
        let rng = Rng;
        let a = rng.generate_nonce_bytes();
        let b = rng.generate_nonce_bytes();
        assert_ne!(a.as_array(), b.as_array());
    }

    /// mnemonic entropy 32B を返す。
    #[test]
    fn generate_mnemonic_entropy_returns_32_bytes() {
        let e = Rng.generate_mnemonic_entropy();
        assert_eq!(e.len(), 32);
    }

    /// 連続呼出で異なるエントロピー。
    #[test]
    fn generate_mnemonic_entropy_yields_distinct_values_on_consecutive_calls() {
        let rng = Rng;
        let a = rng.generate_mnemonic_entropy();
        let b = rng.generate_mnemonic_entropy();
        assert_ne!(*a, *b);
    }
}
