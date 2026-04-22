//! shikomi-core — ドメインロジック層
//!
//! vault ドメイン型・秘密値ラッパ・ドメインエラーを提供する pure Rust / no-I/O クレート。
//! 外部 API / OS / DB には一切アクセスしない。

pub mod error;
pub mod secret;
pub mod vault;

// --- ドメインエラー ---
pub use error::{
    DomainError, InvalidRecordIdReason, InvalidRecordLabelReason, InvalidRecordPayloadReason,
    InvalidVaultHeaderReason, VaultConsistencyReason,
};

// --- 秘密値ラッパ ---
pub use secret::{SecretBytes, SecretString};

// --- vault 集約・ヘッダ ---
pub use vault::{ProtectionMode, Vault, VaultHeader, VaultVersion};

// --- レコード ---
pub use vault::{Record, RecordId, RecordKind, RecordLabel, RecordPayload, RecordPayloadEncrypted};

// --- 暗号化関連データ型 ---
pub use vault::{Aad, CipherText, KdfSalt, NonceBytes, NonceCounter, WrappedVek};

// --- VekProvider trait ---
pub use vault::VekProvider;
