//! 暗号ドメイン型のエントリ。鍵階層・Fail-Secure 型・パスワード認証境界を集約する。
//!
//! 本モジュールは pure Rust / no-I/O 制約を維持し、`rand_core::OsRng` や
//! `getrandom` への呼出を一切持たない。CSPRNG が必要な構築は呼び出し側
//! (`shikomi-infra::crypto::Rng`) から `[u8; N]` を受け取るコンストラクタで提供する。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/index.md`

pub mod aead_key;
pub mod header_aead;
pub mod key;
pub mod password;
pub mod recovery;
pub mod verified;

pub use aead_key::AeadKey;
pub use header_aead::HeaderAeadKey;
pub use key::{Kek, KekKind, KekKindPw, KekKindRecovery, Vek};
pub use password::{MasterPassword, PasswordStrengthGate, WeakPasswordFeedback};
pub use recovery::RecoveryMnemonic;
pub use verified::{
    verify_aead_decrypt, verify_aead_decrypt_to_plaintext, CryptoOutcome, Plaintext, Verified,
};
