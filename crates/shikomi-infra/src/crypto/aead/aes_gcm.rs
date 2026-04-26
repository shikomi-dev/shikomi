//! `AesGcmAeadAdapter` — AES-256-GCM AEAD アダプタ (RustCrypto `aes-gcm`)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/nonce-and-aead.md`
//!         §`AesGcmAeadAdapter`
//!
//! ## 採用 API
//!
//! - `aes_gcm::Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&[u8; 32]))`
//! - `AeadInPlace::encrypt_in_place_detached(&Nonce, &[u8] AAD, &mut [u8] buf) -> Result<Tag, Error>`
//! - `AeadInPlace::decrypt_in_place_detached(&Nonce, &[u8] AAD, &mut [u8] buf, &Tag) -> Result<(), Error>`
//!
//! ciphertext と tag は **物理的に分離**して扱う (`WrappedVek` の構造分離型化と整合、
//! Tell-Don't-Ask)。`Aead::encrypt` / `Aead::decrypt` の連結返却 API は使わない。
//!
//! ## 三段防御整合
//!
//! - 鍵バイトは `AeadKey::with_secret_bytes` クロージャ経由で借り受ける
//!   (TC-C-I01: `expose_within_crate` を adapter 内で呼ばない)
//! - tag 比較は `aes-gcm` 内部の constant-time 経路に委譲
//!   (TC-C-I04: 自前 `as_array() ==` 等を書かない)
//! - 中間 `buf` は `Zeroizing<Vec<u8>>` で囲む (TC-C-I02)
//! - AEAD タグ検証成功時のみ `verify_aead_decrypt_to_plaintext` 経由で
//!   `Verified<Plaintext>` を構築 (CC-3 / C-14)

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{AeadInPlace, Aes256Gcm, Key, KeyInit};
use shikomi_core::crypto::AeadKey;
use shikomi_core::error::CryptoError;
use shikomi_core::{
    verify_aead_decrypt_to_plaintext, Aad, AuthTag, NonceBytes, Plaintext, Vek, Verified,
    WrappedVek,
};
use zeroize::Zeroizing;

/// AES-256-GCM AEAD アダプタ。無状態 unit struct (`Default` 経由で構築)。
///
/// `aes-gcm` crate の `Aes256Gcm::new(key)` は鍵ごとに毎回構築される軽量操作で、
/// adapter 自体に state を持つ必要はない。Sub-D / Sub-E の複数呼出経路から
/// 並列に使用しても thread-safe。
#[derive(Debug, Clone, Copy, Default)]
pub struct AesGcmAeadAdapter;

impl AesGcmAeadAdapter {
    /// per-record 暗号化。`(ciphertext: Vec<u8>, tag: AuthTag)` を返す。
    ///
    /// AAD は `Aad::to_canonical_bytes()` の **26B 固定** (record_id 16B + version 2B BE
    /// + created_at_micros 8B BE)。
    ///
    /// **`NonceCounter::increment` は本関数で呼ばない** (Sub-D vault repository 層責務、
    /// `nonce-and-aead.md` §nonce_counter 統合契約)。
    ///
    /// # Errors
    ///
    /// `aes-gcm` 内部エラー (plaintext 過大 / AAD 過大) 時に
    /// `CryptoError::AeadTagMismatch` を返す (内部詳細秘匿、1 variant に収束)。
    pub fn encrypt_record(
        &self,
        key: &impl AeadKey,
        nonce: &NonceBytes,
        aad: &Aad,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, AuthTag), CryptoError> {
        let aad_bytes = aad.to_canonical_bytes();
        let nonce_ga = GenericArray::clone_from_slice(nonce.as_array());
        // 中間バッファ。`Zeroizing<Vec<u8>>` で囲み、関数スコープ抜けで Drop → zeroize
        // (TC-C-I02 / CC-5 / C-16)。
        let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(plaintext.to_vec());

        let tag_ga = key
            .with_secret_bytes(|bytes| {
                let cipher_key: &Key<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(bytes);
                let cipher = Aes256Gcm::new(cipher_key);
                cipher.encrypt_in_place_detached(&nonce_ga, &aad_bytes, buf.as_mut_slice())
            })
            .map_err(|_| CryptoError::AeadTagMismatch)?;

        let tag_array: [u8; 16] = tag_ga.into();
        Ok((buf.to_vec(), AuthTag::from_array(tag_array)))
    }

    /// per-record 復号 + AEAD タグ検証。タグ検証成功時のみ `Verified<Plaintext>` を返す。
    ///
    /// AAD は `encrypt_record` と同一値 (`Aad::to_canonical_bytes()` 26B)。
    /// AAD / nonce / ciphertext / tag のいずれかが不一致なら
    /// `Err(CryptoError::AeadTagMismatch)`。
    ///
    /// # Errors
    ///
    /// AEAD タグ検証失敗時に `CryptoError::AeadTagMismatch`。
    pub fn decrypt_record(
        &self,
        key: &impl AeadKey,
        nonce: &NonceBytes,
        aad: &Aad,
        ciphertext: &[u8],
        tag: &AuthTag,
    ) -> Result<Verified<Plaintext>, CryptoError> {
        let aad_bytes = aad.to_canonical_bytes();
        let nonce_ga = GenericArray::clone_from_slice(nonce.as_array());
        let tag_ga = GenericArray::clone_from_slice(tag.as_array());

        verify_aead_decrypt_to_plaintext(|| {
            let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(ciphertext.to_vec());
            key.with_secret_bytes(|bytes| {
                let cipher_key: &Key<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(bytes);
                let cipher = Aes256Gcm::new(cipher_key);
                cipher.decrypt_in_place_detached(&nonce_ga, &aad_bytes, buf.as_mut_slice(), &tag_ga)
            })
            .map_err(|_| CryptoError::AeadTagMismatch)?;
            // タグ検証成功直後に平文 bytes を取り出す。`buf` は本クロージャ抜けで Drop。
            Ok(buf.to_vec())
        })
    }

    /// VEK の wrap (KEK で AES-256-GCM 暗号化、AAD は空)。
    ///
    /// `wrap_vek` / `unwrap_vek` は AAD を持たない (vault ヘッダ独立 AEAD タグで別途
    /// 保護される設計、`nonce-and-aead.md` §`AesGcmAeadAdapter`)。
    ///
    /// # Errors
    ///
    /// `aes-gcm` 内部エラー時に `CryptoError::AeadTagMismatch` (内部詳細秘匿)。
    /// 構造的に到達しない `WrappedVek::new` の長さ検証エラーも同 variant に収束。
    pub fn wrap_vek(
        &self,
        kek: &impl AeadKey,
        nonce: &NonceBytes,
        vek: &Vek,
    ) -> Result<WrappedVek, CryptoError> {
        let nonce_ga = GenericArray::clone_from_slice(nonce.as_array());
        // VEK 平文 32B を中間バッファにコピー。`Zeroizing<Vec<u8>>` で囲む。
        let mut buf: Zeroizing<Vec<u8>> =
            vek.with_secret_bytes(|vek_bytes| Zeroizing::new(vek_bytes.to_vec()));

        let tag_ga = kek
            .with_secret_bytes(|kek_bytes| {
                let cipher_key: &Key<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(kek_bytes);
                let cipher = Aes256Gcm::new(cipher_key);
                // AAD は空 `&[]` (`wrap_vek` / `unwrap_vek` 規約)。
                cipher.encrypt_in_place_detached(&nonce_ga, &[], buf.as_mut_slice())
            })
            .map_err(|_| CryptoError::AeadTagMismatch)?;

        let tag_array: [u8; 16] = tag_ga.into();
        let auth_tag = AuthTag::from_array(tag_array);
        // VEK は型レベルで 32B 固定 → ciphertext も 32B → `WrappedVek::new` の
        // `WrappedVekTooShort` / `WrappedVekEmpty` は構造的に到達しない。
        // `map_err(|_| AeadTagMismatch)` は防御的 fallback (CC-10: unwrap/expect 禁止)。
        WrappedVek::new(buf.to_vec(), nonce.clone(), auth_tag)
            .map_err(|_| CryptoError::AeadTagMismatch)
    }

    /// VEK の unwrap (KEK で AEAD タグ検証付き復号)。AAD は空。
    ///
    /// 戻り値の `Verified<Plaintext>` 内の bytes を `Vek::from_array` の入力に
    /// 復元する経路は **Sub-D 責務** (本 adapter は AEAD 検証付き復号までを保証、
    /// 復号後の長さ検証は呼出側が実施)。
    ///
    /// # Errors
    ///
    /// AEAD タグ検証失敗時に `CryptoError::AeadTagMismatch`。
    pub fn unwrap_vek(
        &self,
        kek: &impl AeadKey,
        wrapped: &WrappedVek,
    ) -> Result<Verified<Plaintext>, CryptoError> {
        let nonce_ga = GenericArray::clone_from_slice(wrapped.nonce().as_array());
        let tag_ga = GenericArray::clone_from_slice(wrapped.tag().as_array());

        verify_aead_decrypt_to_plaintext(|| {
            let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(wrapped.ciphertext().to_vec());
            kek.with_secret_bytes(|kek_bytes| {
                let cipher_key: &Key<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(kek_bytes);
                let cipher = Aes256Gcm::new(cipher_key);
                cipher.decrypt_in_place_detached(&nonce_ga, &[], buf.as_mut_slice(), &tag_ga)
            })
            .map_err(|_| CryptoError::AeadTagMismatch)?;
            Ok(buf.to_vec())
        })
    }
}

// `aes_gcm::Error` → `CryptoError::AeadTagMismatch` の変換は `From` impl を
// **書かない** (orphan rule 違反: `aes_gcm::Error` も `CryptoError` も shikomi-infra
// に対して foreign)。各呼出地点で `.map_err(|_| CryptoError::AeadTagMismatch)`
// を inline する形に統一 (内部詳細秘匿、全 AEAD 経路で 1 variant に収束)。

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::crypto::{Kek, KekKindPw};
    use shikomi_core::{RecordId, VaultVersion};
    use time::OffsetDateTime;
    use uuid::Uuid;

    // -----------------------------------------------------------------
    // テスト用 helper
    // -----------------------------------------------------------------

    fn make_aad() -> Aad {
        let id = RecordId::new(Uuid::now_v7()).expect("v7 uuid");
        Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).expect("aad")
    }

    fn another_aad() -> Aad {
        let id = RecordId::new(Uuid::now_v7()).expect("v7 uuid");
        Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).expect("aad")
    }

    fn make_vek(byte: u8) -> Vek {
        Vek::from_array([byte; 32])
    }

    fn make_kek(byte: u8) -> Kek<KekKindPw> {
        Kek::<KekKindPw>::from_array([byte; 32])
    }

    fn make_nonce(byte: u8) -> NonceBytes {
        NonceBytes::from_random([byte; 12])
    }

    // -----------------------------------------------------------------
    // encrypt_record / decrypt_record roundtrip
    // -----------------------------------------------------------------

    #[test]
    fn encrypt_then_decrypt_roundtrip_bit_exact() {
        let adapter = AesGcmAeadAdapter;
        let vek = make_vek(0x11);
        let nonce = make_nonce(0x22);
        let aad = make_aad();
        let plaintext = b"the quick brown fox jumps over the lazy dog";

        let (ciphertext, tag) = adapter
            .encrypt_record(&vek, &nonce, &aad, plaintext)
            .expect("encrypt ok");

        // ciphertext は plaintext と同じ長さ (detached tag 方式)
        assert_eq!(ciphertext.len(), plaintext.len());
        // ciphertext は plaintext と一致してはならない
        assert_ne!(ciphertext.as_slice(), plaintext.as_slice());

        let verified = adapter
            .decrypt_record(&vek, &nonce, &aad, &ciphertext, &tag)
            .expect("decrypt ok");

        assert_eq!(verified.as_inner().expose_secret(), plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_returns_aead_tag_mismatch() {
        let adapter = AesGcmAeadAdapter;
        let vek1 = make_vek(0x11);
        let vek2 = make_vek(0x22);
        let nonce = make_nonce(0x33);
        let aad = make_aad();
        let plaintext = b"secret data";

        let (ciphertext, tag) = adapter
            .encrypt_record(&vek1, &nonce, &aad, plaintext)
            .expect("encrypt ok");

        let err = adapter
            .decrypt_record(&vek2, &nonce, &aad, &ciphertext, &tag)
            .expect_err("wrong key must fail");
        assert!(matches!(err, CryptoError::AeadTagMismatch));
    }

    #[test]
    fn decrypt_with_swapped_aad_returns_aead_tag_mismatch() {
        let adapter = AesGcmAeadAdapter;
        let vek = make_vek(0x55);
        let nonce = make_nonce(0x66);
        let aad_a = make_aad();
        let aad_b = another_aad();
        let plaintext = b"L1 protected payload";

        let (ciphertext, tag) = adapter
            .encrypt_record(&vek, &nonce, &aad_a, plaintext)
            .expect("encrypt ok");

        let err = adapter
            .decrypt_record(&vek, &nonce, &aad_b, &ciphertext, &tag)
            .expect_err("swapped aad must fail");
        assert!(matches!(err, CryptoError::AeadTagMismatch));
    }

    #[test]
    fn decrypt_with_swapped_nonce_returns_aead_tag_mismatch() {
        let adapter = AesGcmAeadAdapter;
        let vek = make_vek(0x77);
        let nonce_a = make_nonce(0x10);
        let nonce_b = make_nonce(0x20);
        let aad = make_aad();
        let plaintext = b"hello world";

        let (ciphertext, tag) = adapter
            .encrypt_record(&vek, &nonce_a, &aad, plaintext)
            .expect("encrypt ok");

        let err = adapter
            .decrypt_record(&vek, &nonce_b, &aad, &ciphertext, &tag)
            .expect_err("swapped nonce must fail");
        assert!(matches!(err, CryptoError::AeadTagMismatch));
    }

    #[test]
    fn decrypt_with_tampered_ciphertext_returns_aead_tag_mismatch() {
        let adapter = AesGcmAeadAdapter;
        let vek = make_vek(0x88);
        let nonce = make_nonce(0x99);
        let aad = make_aad();
        let plaintext = b"original data";

        let (mut ciphertext, tag) = adapter
            .encrypt_record(&vek, &nonce, &aad, plaintext)
            .expect("encrypt ok");
        // 1 bit 反転で AEAD 失敗を観測
        ciphertext[0] ^= 0x01;

        let err = adapter
            .decrypt_record(&vek, &nonce, &aad, &ciphertext, &tag)
            .expect_err("tampered ciphertext must fail");
        assert!(matches!(err, CryptoError::AeadTagMismatch));
    }

    // -----------------------------------------------------------------
    // wrap_vek / unwrap_vek roundtrip
    // -----------------------------------------------------------------

    #[test]
    fn wrap_then_unwrap_vek_roundtrip() {
        let adapter = AesGcmAeadAdapter;
        let kek = make_kek(0xAB);
        let nonce = make_nonce(0xCD);
        let original_bytes = [0x42u8; 32];
        let vek = Vek::from_array(original_bytes);

        let wrapped = adapter.wrap_vek(&kek, &nonce, &vek).expect("wrap ok");
        // ciphertext は VEK 32B と同じ長さ (detached tag 方式)
        assert_eq!(wrapped.ciphertext().len(), 32);
        assert_eq!(wrapped.nonce().as_array(), nonce.as_array());

        let verified = adapter.unwrap_vek(&kek, &wrapped).expect("unwrap ok");
        assert_eq!(verified.as_inner().expose_secret(), &original_bytes);
    }

    #[test]
    fn unwrap_vek_with_wrong_kek_returns_aead_tag_mismatch() {
        let adapter = AesGcmAeadAdapter;
        let kek1 = make_kek(0x01);
        let kek2 = make_kek(0x02);
        let nonce = make_nonce(0x03);
        let vek = make_vek(0x04);

        let wrapped = adapter.wrap_vek(&kek1, &nonce, &vek).expect("wrap ok");
        let err = adapter
            .unwrap_vek(&kek2, &wrapped)
            .expect_err("wrong kek must fail");
        assert!(matches!(err, CryptoError::AeadTagMismatch));
    }

    // -----------------------------------------------------------------
    // 構造性: ciphertext は plaintext と長さが等しい (detached tag)
    // -----------------------------------------------------------------

    #[test]
    fn encrypt_record_ciphertext_matches_plaintext_length() {
        let adapter = AesGcmAeadAdapter;
        let vek = make_vek(0x10);
        let nonce = make_nonce(0x20);
        let aad = make_aad();

        for &len in &[0usize, 1, 13, 16, 47, 64, 128] {
            let pt = vec![0x55u8; len];
            let (ct, _tag) = adapter
                .encrypt_record(&vek, &nonce, &aad, &pt)
                .expect("encrypt ok");
            assert_eq!(ct.len(), len, "ciphertext length mismatch for len={len}");
        }
    }

    // -----------------------------------------------------------------
    // KEK<KekKindRecovery> でも AeadKey impl 経由で動く (型一般化検証)
    // -----------------------------------------------------------------

    #[test]
    fn wrap_unwrap_vek_works_with_kek_recovery() {
        use shikomi_core::crypto::KekKindRecovery;
        let adapter = AesGcmAeadAdapter;
        let kek_recovery = Kek::<KekKindRecovery>::from_array([0x99u8; 32]);
        let nonce = make_nonce(0xAA);
        let vek_bytes = [0x33u8; 32];
        let vek = Vek::from_array(vek_bytes);

        let wrapped = adapter
            .wrap_vek(&kek_recovery, &nonce, &vek)
            .expect("wrap ok");
        let verified = adapter
            .unwrap_vek(&kek_recovery, &wrapped)
            .expect("unwrap ok");
        assert_eq!(verified.as_inner().expose_secret(), &vek_bytes);
    }
}
