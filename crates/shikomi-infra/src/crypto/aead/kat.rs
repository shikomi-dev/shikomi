//! Known Answer Tests (KAT) — AES-256-GCM 三軸 (key bias × plaintext 長 × AAD 長) 網羅。
//!
//! `#[cfg(test)]` 専用。本 KAT は **自己整合 KAT** に責務を絞る:
//! RustCrypto `aes-gcm` crate (`Aes256Gcm`) で encrypt → 同 key/nonce/aad で
//! decrypt して bit-exact match を確認する。実装パスが「encrypt_in_place_detached
//! / decrypt_in_place_detached の roundtrip 成立」「三軸の分散条件全てで動作」を
//! 検証することが目的。
//!
//! 本物の NIST CAVP `gcmEncryptExtIV256.rsp` 公式 expected ciphertext との
//! bit-exact 比較は **Sub-D の repository 結合テスト** で実施 (本 KAT は
//! AES-256-GCM が動く + 三軸網羅の動作証明に限定)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/nonce-and-aead.md`
//!         §`AesGcmAeadAdapter` §`aes-gcm` crate 呼出契約 KAT 行
//!         + Bug-C-003 凍結の三軸分散条件
//!
//! TC-C-I02 静的検査の `mod tests` ブロック判定経路に乗せるため、全 KAT
//! 関数を本ファイル内 `mod tests { ... }` に格納する
//! (`tests/docs/sub-c-static-checks.sh` の awk フィルタ仕様、Sub-B kat.rs と同型)。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use aes_gcm::aead::generic_array::GenericArray;
    use aes_gcm::{AeadInPlace, Aes256Gcm, Key, KeyInit};

    /// AES-256-GCM の `encrypt_in_place_detached` → `decrypt_in_place_detached`
    /// roundtrip + non-trivial 出力検証 (中身を直接担保するヘルパ)。
    ///
    /// 三軸の各 KAT 関数から呼び出され、**本ヘルパ内で AES-256-GCM が動く** sanity
    /// (= ciphertext != plaintext かつ tag != 全 0) を確認する。
    fn run_kat(key_bytes: &[u8; 32], nonce_bytes: &[u8; 12], aad: &[u8], plaintext: &[u8]) {
        let key: &Key<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = GenericArray::clone_from_slice(nonce_bytes);

        // encrypt
        let mut buf = plaintext.to_vec();
        let tag = cipher
            .encrypt_in_place_detached(&nonce, aad, buf.as_mut_slice())
            .expect("encrypt_in_place_detached must succeed within P_MAX/A_MAX");

        // sanity: ciphertext != plaintext (空 plaintext を除く)
        if !plaintext.is_empty() {
            assert_ne!(
                buf.as_slice(),
                plaintext,
                "ciphertext must not equal plaintext (KAT sanity)"
            );
        }
        // sanity: tag は全 0 ではない (非自明 GMAC 出力)
        assert!(
            tag.iter().any(|&b| b != 0),
            "auth tag must not be all zeros (GMAC running?)"
        );
        let ciphertext = buf.clone();

        // decrypt (bit-exact match)
        let mut decrypt_buf = ciphertext.clone();
        cipher
            .decrypt_in_place_detached(&nonce, aad, decrypt_buf.as_mut_slice(), &tag)
            .expect("decrypt_in_place_detached must succeed for matching key/nonce/aad/tag");
        assert_eq!(
            decrypt_buf.as_slice(),
            plaintext,
            "roundtrip must be bit-exact"
        );

        // tampered tag → decrypt 失敗 (AEAD 検証経路の sanity)
        let mut tampered_tag = tag;
        tampered_tag[0] ^= 0x01;
        let mut buf_for_bad = ciphertext.clone();
        let bad = cipher.decrypt_in_place_detached(
            &nonce,
            aad,
            buf_for_bad.as_mut_slice(),
            &tampered_tag,
        );
        assert!(bad.is_err(), "tampered tag must cause AEAD failure");
    }

    // -------------------------------------------------------------------
    // Bug-C-003 凍結の三軸分散条件: key bias × plaintext 長 × AAD 長
    //   key bias 軸: 全 0 / 全 0xFF / random / "test" 字列 padded
    //   plaintext 長 軸: 0 / 16 / 64 / 任意非整数倍 (13, 47)
    //   AAD 長 軸: 0 / 16 / 64
    // -------------------------------------------------------------------

    /// KAT-1: key bias 軸 = 全 0、plaintext 長 = 0 (空)、AAD 長 = 0 (空)。
    /// AES-GCM は plaintext 長 0 でも有効 (ciphertext は空、tag のみ生成される)。
    #[test]
    fn kat_1_key_all_zero_pt_empty_aad_empty() {
        run_kat(&[0u8; 32], &[0u8; 12], &[], &[]);
    }

    /// KAT-2: key bias 軸 = 全 0xFF、plaintext 長 = 16 (1 ブロック)、AAD 長 = 16。
    #[test]
    fn kat_2_key_all_ff_pt_16_aad_16() {
        run_kat(&[0xFFu8; 32], &[0x01u8; 12], &[0xAAu8; 16], &[0x55u8; 16]);
    }

    /// KAT-3: key bias 軸 = "random"-ish (UUID-like パターン)、plaintext 長 = 64
    /// (4 ブロック)、AAD 長 = 64。
    #[test]
    fn kat_3_key_random_pt_64_aad_64() {
        let key = [
            0x01u8, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10, 0x0f, 0x1e, 0x2d, 0x3c, 0x4b, 0x5a, 0x69, 0x78, 0x87, 0x96, 0xa5, 0xb4,
            0xc3, 0xd2, 0xe1, 0xf0,
        ];
        let nonce = [
            0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x12, 0x34, 0x56, 0x78,
        ];
        let aad: Vec<u8> = (0u8..64).collect();
        let pt: Vec<u8> = (0u8..64).map(|b| b.wrapping_mul(3)).collect();
        run_kat(&key, &nonce, &aad, &pt);
    }

    /// KAT-4: key bias 軸 = ASCII "test" 字列 padded、plaintext 長 = 13
    /// (任意非整数倍)、AAD 長 = 0。
    #[test]
    fn kat_4_key_ascii_test_pt_13_aad_empty() {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(b"test");
        let pt = b"hello, world!"; // 13 byte
        run_kat(&key, &[0x42u8; 12], &[], pt);
    }

    /// KAT-5: key bias 軸 = 全 0、plaintext 長 = 47 (任意非整数倍 = 2 ブロック + 15B)、
    /// AAD 長 = 16。
    #[test]
    fn kat_5_key_all_zero_pt_47_aad_16() {
        run_kat(&[0u8; 32], &[0xCCu8; 12], &[0x99u8; 16], &[0x33u8; 47]);
    }

    /// KAT-6: key bias 軸 = 全 0xFF、plaintext 長 = 0、AAD 長 = 64。
    /// AAD のみで GMAC が走る経路の sanity。
    #[test]
    fn kat_6_key_all_ff_pt_empty_aad_64() {
        run_kat(&[0xFFu8; 32], &[0xEEu8; 12], &[0x11u8; 64], &[]);
    }

    /// KAT-7: key bias 軸 = random-ish、plaintext 長 = 16 (1 ブロック)、AAD 長 = 0。
    #[test]
    fn kat_7_key_random_pt_16_aad_empty() {
        let key = [
            0xa3u8, 0xb1, 0xc5, 0xd7, 0xe9, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, 0x00, 0x12, 0x34,
        ];
        run_kat(&key, &[0x77u8; 12], &[], &[0x88u8; 16]);
    }

    /// KAT-8: key bias 軸 = ASCII "test" padded、plaintext 長 = 64、AAD 長 = 64。
    /// 三軸の組合せ網羅を補強。
    #[test]
    fn kat_8_key_ascii_test_pt_64_aad_64() {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(b"test");
        let aad: Vec<u8> = (0u8..64).map(|b| b.wrapping_add(0x10)).collect();
        let pt: Vec<u8> = (0u8..64).map(|b| b.wrapping_mul(7)).collect();
        run_kat(&key, &[0xABu8; 12], &aad, &pt);
    }

    /// KAT-9 (補): plaintext 長 = 16 (一括ブロック境界)、key 全 0、AAD 長 = 64。
    /// AAD と plaintext の長さ非対称な経路を補強。
    #[test]
    fn kat_9_key_all_zero_pt_16_aad_64() {
        run_kat(&[0u8; 32], &[0xBBu8; 12], &[0x77u8; 64], &[0x44u8; 16]);
    }
}
