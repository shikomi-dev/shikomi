//! Sub-B (#40) BC-3 リリースブロッカ — KDF 性能ベンチ (criterion)。
//!
//! 設計書 `docs/features/vault-encryption/detailed-design/kdf.md`
//! §性能契約 (criterion ベンチ p95 1 秒) に対応する gating 用ベンチ。
//!
//! - **`argon2id_derive_kek_pw_frozen_owasp_2024_05`**: 採用経路
//!   `Argon2idAdapter::default().derive_kek_pw(&MasterPassword, &KdfSalt)` の
//!   単一呼出時間 (m=19_456 / t=2 / p=1 / output_len=32)。
//! - **`bip39_derive_kek_recovery_24_words`**: 採用経路
//!   `Bip39Pbkdf2Hkdf::default().derive_kek_recovery(&RecoveryMnemonic)` の
//!   単一呼出時間 (PBKDF2-HMAC-SHA512 2048 iter + HKDF-SHA256)。
//!
//! gating の閾値判定は本 bench の outside (`scripts/ci/bench-kdf-gating.sh`) で実施。
//! criterion 自体は median + 信頼区間を出力するのみで「閾値超過 = exit 非 0」の
//! 自動 fail はしない (gating script が `--output-format bencher` の median を
//! `750ms` の proxy 閾値と比較する)。

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use shikomi_core::crypto::{PasswordStrengthGate, WeakPasswordFeedback};
use shikomi_core::{KdfSalt, MasterPassword, RecoveryMnemonic};
use shikomi_infra::crypto::kdf::{Argon2idAdapter, Bip39Pbkdf2Hkdf};

/// 強度ゲートをテスト用に常時 Ok にする。本番経路は `ZxcvbnGate::default()` だが、
/// bench fixture では強度判定時間を含めずに `MasterPassword` 構築を計測対象外にする
/// ため bypass する (`b.iter_batched` の setup phase で構築コストを除外)。
struct AlwaysAcceptGate;

impl PasswordStrengthGate for AlwaysAcceptGate {
    fn validate(&self, _password: &str) -> Result<(), WeakPasswordFeedback> {
        Ok(())
    }
}

/// 24 語固定の `RecoveryMnemonic` ("abandon" × 23 + "art")。
/// BIP-39 24 語 zero-entropy mnemonic で wordlist + checksum を満たす。
fn fixture_recovery_mnemonic() -> RecoveryMnemonic {
    let words: [String; 24] = std::array::from_fn(|i| {
        if i == 23 {
            "art".to_string()
        } else {
            "abandon".to_string()
        }
    });
    RecoveryMnemonic::from_words(words)
}

/// Argon2id `FROZEN_OWASP_2024_05` の単一 derive 時間。
fn bench_argon2id_kek_pw_frozen(c: &mut Criterion) {
    let adapter = Argon2idAdapter::default();
    // 16 byte 固定の salt fixture。CSPRNG ではなく決定値で再現性を担保。
    let salt = KdfSalt::from_array([0x42u8; 16]);
    // zxcvbn 強度 ≥ 3 想定の bench 用 password fixture。
    let password_seed = "Tr0ub4dor&3-shikomi-bench-fixture";

    c.bench_function("argon2id_derive_kek_pw_frozen_owasp_2024_05", |b| {
        b.iter_batched(
            || {
                MasterPassword::new(password_seed.to_string(), &AlwaysAcceptGate)
                    .expect("AlwaysAcceptGate must accept any password (test-only fixture)")
            },
            |password| {
                adapter
                    .derive_kek_pw(&password, &salt)
                    .expect("derive_kek_pw must succeed for valid params + 16B salt")
            },
            BatchSize::SmallInput,
        );
    });
}

/// `Bip39Pbkdf2Hkdf` の単一 derive 時間 (BIP-39 24 語 → seed → HKDF-SHA256)。
fn bench_bip39_kek_recovery(c: &mut Criterion) {
    let adapter = Bip39Pbkdf2Hkdf;

    c.bench_function("bip39_derive_kek_recovery_24_words", |b| {
        b.iter_batched(
            fixture_recovery_mnemonic,
            |recovery| {
                adapter
                    .derive_kek_recovery(&recovery)
                    .expect("derive_kek_recovery must succeed for known-good 24-word mnemonic")
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_argon2id_kek_pw_frozen,
    bench_bip39_kek_recovery
);
criterion_main!(benches);
