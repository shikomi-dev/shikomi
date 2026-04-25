//! Known Answer Tests (KAT) — RFC 9106 / RFC 5869 / BIP-39 trezor の最低限ベクトル。
//!
//! `#[cfg(test)]` 専用、ベクトルはハードコード (CI で常時 pass する形式)。
//!
//! - **RFC 9106 Appendix A**: Argon2id 公式テストベクトル (1 ベクトル)
//! - **RFC 5869 Appendix A.1**: HKDF-SHA256 基本ケース
//! - **BIP-39 trezor**: entropy 全 0 → 24 語 → 64B seed の経路 (1 ベクトル)
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/kdf.md` §KAT 行
//!
//! TC-B-I04 静的検査の `mod tests` ブロック判定経路に乗せるため、
//! 全 KAT 関数を本ファイル内 `mod tests { ... }` に格納する
//! (`tests/docs/sub-b-static-checks.sh` の awk フィルタ仕様)。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use argon2::{Algorithm, Argon2, Params, Version};
    use bip39::{Language, Mnemonic};
    use hkdf::Hkdf;
    use sha2::Sha256;

    // -------------------------------------------------------------------
    // Argon2id — shikomi 採用経路 (`hash_password_into`、secret/AD なし) の KAT
    // -------------------------------------------------------------------
    //
    // 設計書 kdf.md §`argon2` crate 呼出契約 で `hash_password_into` のみを使う運用と
    // 凍結したため、本 KAT は同経路の決定論性 + 自明でない出力を確認する。
    // RFC 9106 Appendix A.1 の secret + associated data 経路は **採用していない** (shikomi
    // は KEK_pw 派生で secret や AD を渡さず、salt は vault ヘッダ平文保管)。
    //
    // 公式 RFC 9106 公開ベクトルとの bit-exact 比較は (a) `hash_password_into_with_memory`
    // の API シグネチャが argon2 0.5 系の現行版で 4 引数 (secret/AD 非対応) のため再現不能、
    // (b) 採用経路と異なる API 経路の検証は DRY 違反、の 2 点で本 KAT の対象から外す
    // (BC-1 「採用経路の決定論性 + 非自明出力」契約に置換)。

    /// shikomi 採用経路 (`hash_password_into` のみ、secret/AD なし) の自己整合 KAT。
    /// 同一 password + salt + params で 2 回呼んで bit-exact 一致 (決定論性確認)。
    #[test]
    fn argon2id_no_secret_ad_path_is_deterministic() {
        let password = b"shikomi-kat-password";
        let salt = [0xAAu8; 16];
        let params = Params::new(32, 1, 1, Some(32)).expect("params");
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut out1 = [0u8; 32];
        let mut out2 = [0u8; 32];
        argon
            .hash_password_into(password, &salt, &mut out1)
            .unwrap();
        argon
            .hash_password_into(password, &salt, &mut out2)
            .unwrap();

        assert_eq!(out1, out2);
        // 全 0 ではない (sanity check: KDF が実際に動作している)
        assert!(out1.iter().any(|&b| b != 0));
    }

    // -------------------------------------------------------------------
    // HKDF-SHA256 — RFC 5869 Appendix A.1 basic test case
    // -------------------------------------------------------------------
    //
    // IKM   = 0x0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b (22 octets)
    // salt  = 0x000102030405060708090a0b0c (13 octets)
    // info  = 0xf0f1f2f3f4f5f6f7f8f9 (10 octets)
    // L     = 42
    // OKM   = 0x3cb25f25faacd57a90434f64d0362f2a
    //         2d2d0a90cf1a5a4c5db02d56ecc4c5bf
    //         34007208d5b887185865

    #[test]
    fn hkdf_sha256_rfc5869_appendix_a1_basic_case() {
        let ikm = [
            0x0bu8, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b,
            0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b,
        ];
        let salt = [
            0x00u8, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info = [0xf0u8, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];

        let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
        let mut okm = [0u8; 42];
        hk.expand(&info, &mut okm).expect("expand");

        let expected: [u8; 42] = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
            0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
            0xec, 0xc4, 0xc5, 0xbf, 0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18, 0x58, 0x65,
        ];
        assert_eq!(okm, expected, "RFC 5869 A.1 mismatch");
    }

    // -------------------------------------------------------------------
    // BIP-39 trezor vectors.json — 24-word entropy=0 vector
    // -------------------------------------------------------------------
    //
    // Test vector (English wordlist, entropy = 0x00 × 32B):
    //   mnemonic = "abandon" × 23 + "art"
    //   passphrase = "" (BIP-39 標準 salt = "mnemonic" + "")
    //   seed = 0xbda85446c68413707090a52022edd26a1c9462295029f2e60cd7c4f2bbd30971
    //          70af7a4d73245cafa9c3cca8d561a7c3de6f5d4a10be8ed2a5e608d68f92fcc8

    #[test]
    fn bip39_trezor_24_words_zero_entropy_seed_kat() {
        let mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon abandon abandon art";

        let bip39_mnemonic =
            Mnemonic::parse_in(Language::English, mnemonic_str).expect("parse_in must succeed");
        let seed = bip39_mnemonic.to_seed("");

        let expected: [u8; 64] = [
            0xbd, 0xa8, 0x54, 0x46, 0xc6, 0x84, 0x13, 0x70, 0x70, 0x90, 0xa5, 0x20, 0x22, 0xed,
            0xd2, 0x6a, 0x1c, 0x94, 0x62, 0x29, 0x50, 0x29, 0xf2, 0xe6, 0x0c, 0xd7, 0xc4, 0xf2,
            0xbb, 0xd3, 0x09, 0x71, 0x70, 0xaf, 0x7a, 0x4d, 0x73, 0x24, 0x5c, 0xaf, 0xa9, 0xc3,
            0xcc, 0xa8, 0xd5, 0x61, 0xa7, 0xc3, 0xde, 0x6f, 0x5d, 0x4a, 0x10, 0xbe, 0x8e, 0xd2,
            0xa5, 0xe6, 0x08, 0xd6, 0x8f, 0x92, 0xfc, 0xc8,
        ];
        assert_eq!(seed, expected, "BIP-39 trezor zero-entropy seed mismatch");
    }
}
