//! shikomi-core — ドメインロジック層
//!
//! vault ドメイン型・暗号ドメイン型・秘密値ラッパ・ドメインエラーを提供する
//! pure Rust / no-I/O クレート。外部 API / OS / DB / CSPRNG (`rand_core::OsRng` /
//! `getrandom`) には一切アクセスしない。
//!
//! 暗号鍵階層と Fail-Secure 型は `crypto` モジュール、vault 集約とレコードは `vault`
//! モジュール、秘密値ラッパは `secret` モジュールに分離する (Clean Architecture)。

pub mod crypto;
pub mod error;
pub mod ipc;
pub mod secret;
pub mod vault;

// --- ドメインエラー ---
pub use error::{
    CryptoError, DomainError, InvalidRecordIdReason, InvalidRecordLabelReason,
    InvalidRecordPayloadReason, InvalidVaultHeaderReason, KdfErrorKind, VaultConsistencyReason,
};

// --- 秘密値ラッパ ---
pub use secret::{SecretBytes, SecretString};

// --- 暗号ドメイン型 (Sub-A 新規) ---
pub use crypto::{
    verify_aead_decrypt, CryptoOutcome, HeaderAeadKey, Kek, KekKind, KekKindPw, KekKindRecovery,
    MasterPassword, PasswordStrengthGate, Plaintext, RecoveryMnemonic, Vek, Verified,
    WeakPasswordFeedback,
};

// --- vault 集約・ヘッダ ---
pub use vault::{ProtectionMode, Vault, VaultHeader, VaultVersion};

// --- レコード ---
pub use vault::{Record, RecordId, RecordKind, RecordLabel, RecordPayload, RecordPayloadEncrypted};

// --- 暗号化関連データ型 ---
pub use vault::{Aad, AuthTag, CipherText, KdfSalt, NonceBytes, NonceCounter, WrappedVek};

// --- VekProvider trait ---
pub use vault::VekProvider;
