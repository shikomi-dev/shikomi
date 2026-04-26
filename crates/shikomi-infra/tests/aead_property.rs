//! Sub-C (#41) test-design TC-C-P01 / TC-C-P02 — proptest 1000 ケース.
//!
//! Bug-C-001 顛末: 銀ちゃん impl PR #55 (`0507705`) は AES-GCM の roundtrip /
//! AAD swap / wrong-key 等を**単発 fixture テスト**で確認していた。
//! test-design.md §12.4 / §12.7 が要求するのは **proptest 1000 ケース**で、
//! 「単発 1 件」と「ランダム入力空間 1000 ケース」は意味論が異なる:
//! - 単発: 設計者が想定したケース 1 件で invariant が成立することを確認
//! - proptest: ランダム入力空間で**設計者が想定していない歪み**まで覆う
//!
//! テスト工程でマユリが Boy Scout として補強。`proptest` crate を workspace
//! dev-deps に追加し、本ファイルで以下 2 件を実装:
//!
//! - **TC-C-P01 (CC-7 / L1)**: 任意の 2 record (A/B) を組合せて encrypt し、
//!   AAD を A/B 入れ替えて decrypt → **同一組合せのみ復号成功**、入れ替え時は
//!   `Err(AeadTagMismatch)`。1000 ケース。
//! - **TC-C-P02 (CC-1 / CC-6)**: 任意の plaintext (0..=4096B) + 任意 `Vek` +
//!   任意 `Aad` + 任意 nonce で `encrypt_record → decrypt_record` 往復し、
//!   復元 plaintext が**元 plaintext と bit-exact 一致**。1000 ケース。

#![allow(clippy::unwrap_used, clippy::expect_used)] // proptest harness 内部、shrink 出力で許容

use proptest::prelude::*;
use shikomi_core::crypto::{Plaintext, Vek, Verified};
use shikomi_core::error::CryptoError;
use shikomi_core::vault::crypto_data::Aad;
use shikomi_core::{NonceBytes, RecordId, VaultVersion};
use shikomi_infra::crypto::aead::AesGcmAeadAdapter;
use time::OffsetDateTime;
use uuid::Uuid;

/// 任意 32B 配列 → `Vek` の strategy.
fn vek_strategy() -> impl Strategy<Value = Vek> {
    prop::array::uniform32(any::<u8>()).prop_map(Vek::from_array)
}

/// 任意 12B 配列 → `NonceBytes` の strategy.
fn nonce_strategy() -> impl Strategy<Value = NonceBytes> {
    prop::array::uniform12(any::<u8>()).prop_map(NonceBytes::from_random)
}

/// 任意の `Aad` の strategy.
/// `RecordId` は `Uuid::now_v7()` で生成（UUIDv7 必須、`RecordId::new` 検証通過）、
/// `vault_version` は 1..=5（現実的な範囲）、`created_at` は 2020〜2030 年範囲。
/// proptest が `RecordId` のランダムバリエーションを欲しがる場合は `now_v7` の
/// 連続呼出が timestamp + counter で異なる値を返すため、Strategy 内では
/// 1 ケース 1 UUID で十分。
fn aad_strategy() -> impl Strategy<Value = Aad> {
    // 2020-01-01T00:00:00Z = 1577836800 秒、2030-01-01T00:00:00Z = 1893456000 秒
    // VaultVersion::CURRENT == VaultVersion::MIN_SUPPORTED == 1 (本実装時点)。
    // 範囲外を proptest に渡すと UnsupportedVaultVersion でパニックするため、
    // 採用範囲のみに絞る。バージョン拡張時は VaultVersion::CURRENT を辿って自動拡張可。
    let secs_range = 1_577_836_800_i64..=1_893_456_000_i64;
    (
        secs_range,           // unix epoch seconds
        0_i64..1_000_000_i64, // microseconds within the second
    )
        .prop_map(|(secs, micros)| {
            let record_id = RecordId::new(Uuid::now_v7()).expect("UUIDv7 valid");
            let nanos = i128::from(secs) * 1_000_000_000 + i128::from(micros) * 1_000;
            let created_at = OffsetDateTime::from_unix_timestamp_nanos(nanos)
                .expect("nanos within OffsetDateTime range");
            // 本実装の有効範囲は 1 のみ (MIN_SUPPORTED..=CURRENT)、Version 値そのもののバリエーションは Sub-D 以降で拡張
            let version = VaultVersion::try_new(VaultVersion::CURRENT.value())
                .expect("CURRENT vault_version always valid");
            Aad::new(record_id, version, created_at).expect("Aad::new in-range")
        })
}

/// 任意 plaintext バイト列 (0..=4096B) の strategy.
fn plaintext_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=4096)
}

proptest! {
    // test-design.md §12.4 / §12.7 が要求する **1000 ケース** を明示。
    // proptest デフォルトは 256 ケースのため、ProptestConfig で上書きしないと
    // 設計と実態が乖離する (Bug-C-001 と同型の歪み再発を防ぐ)。
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// TC-C-P02 (CC-1 / CC-6 / property): encrypt → decrypt 往復不変条件.
    ///
    /// 任意の (plaintext, vek, aad, nonce) 組合せで:
    ///   adapter.encrypt_record → adapter.decrypt_record →
    ///   verified.expose_secret() == plaintext (bit-exact)
    ///
    /// テスト件数: ProptestConfig::cases = 1000 (1000 ケース)
    /// shrinking: 失敗時、proptest が minimal failing case を提示
    #[test]
    fn tc_c_p02_encrypt_decrypt_roundtrip_property(
        plaintext in plaintext_strategy(),
        vek in vek_strategy(),
        aad in aad_strategy(),
        nonce in nonce_strategy(),
    ) {
        let adapter = AesGcmAeadAdapter;

        let (ciphertext, tag) = adapter
            .encrypt_record(&vek, &nonce, &aad, &plaintext)
            .expect("encrypt should succeed for any valid input");

        let verified: Verified<Plaintext> = adapter
            .decrypt_record(&vek, &nonce, &aad, &ciphertext, &tag)
            .expect("decrypt with same (vek, nonce, aad) must succeed");

        // Verified<Plaintext> から元 plaintext を取り出し bit-exact 一致を確認
        let restored = verified.into_inner();
        prop_assert_eq!(restored.expose_secret(), plaintext.as_slice());
    }

    /// TC-C-P01 (CC-7 / L1 / property): AAD 入れ替え攻撃検出.
    ///
    /// 2 つの異なる (record_id, version, created_at) を持つ record A/B を
    /// 同一 VEK で encrypt。decrypt 時に AAD を入れ替えると `AeadTagMismatch`。
    /// 同一組合せでは復号成功。
    ///
    /// テスト件数: 1000 ケース
    #[test]
    fn tc_c_p01_aad_swap_attack_detected(
        plaintext_a in plaintext_strategy(),
        plaintext_b in plaintext_strategy(),
        vek in vek_strategy(),
        aad_a in aad_strategy(),
        aad_b in aad_strategy(),
        nonce_a in nonce_strategy(),
        nonce_b in nonce_strategy(),
    ) {
        // record A / B が異なる AAD を持つことを保証 (同一 AAD は本 TC の対象外)
        prop_assume!(aad_a.to_canonical_bytes() != aad_b.to_canonical_bytes());
        // 同一 nonce 衝突を避ける (AES-GCM は同一 (key, nonce) で 2 回 encrypt 禁止)
        prop_assume!(nonce_a.as_array() != nonce_b.as_array());

        let adapter = AesGcmAeadAdapter;

        let (ct_a, tag_a) = adapter
            .encrypt_record(&vek, &nonce_a, &aad_a, &plaintext_a)
            .expect("encrypt A");
        let (ct_b, tag_b) = adapter
            .encrypt_record(&vek, &nonce_b, &aad_b, &plaintext_b)
            .expect("encrypt B");

        // 1) 同一組合せ: A の (ct, tag, nonce, aad) で復号 → 成功
        let verified_a = adapter
            .decrypt_record(&vek, &nonce_a, &aad_a, &ct_a, &tag_a)
            .expect("same (nonce_a, aad_a) must succeed");
        let restored_a = verified_a.into_inner();
        prop_assert_eq!(restored_a.expose_secret(), plaintext_a.as_slice());

        let verified_b = adapter
            .decrypt_record(&vek, &nonce_b, &aad_b, &ct_b, &tag_b)
            .expect("same (nonce_b, aad_b) must succeed");
        let restored_b = verified_b.into_inner();
        prop_assert_eq!(restored_b.expose_secret(), plaintext_b.as_slice());

        // 2) AAD 入れ替え攻撃: A の ciphertext + tag に B の AAD を組合せ → AeadTagMismatch
        let result_swap_aad = adapter.decrypt_record(&vek, &nonce_a, &aad_b, &ct_a, &tag_a);
        prop_assert!(
            matches!(result_swap_aad, Err(CryptoError::AeadTagMismatch)),
            "AAD swap (A ciphertext + B aad) must return AeadTagMismatch, got: {:?}",
            result_swap_aad
        );

        // 3) nonce 入れ替え攻撃: A の ciphertext + tag に B の nonce を組合せ → AeadTagMismatch
        let result_swap_nonce = adapter.decrypt_record(&vek, &nonce_b, &aad_a, &ct_a, &tag_a);
        prop_assert!(
            matches!(result_swap_nonce, Err(CryptoError::AeadTagMismatch)),
            "nonce swap (A ciphertext + B nonce) must return AeadTagMismatch, got: {:?}",
            result_swap_nonce
        );
    }
}
