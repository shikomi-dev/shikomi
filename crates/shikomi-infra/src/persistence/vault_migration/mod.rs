//! `vault_migration` — 平文 ⇄ 暗号化 vault マイグレーション service (Sub-D 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//!
//! ## モジュール構成
//!
//! - [`error`]: `MigrationError` (Sub-D 新規、`#[non_exhaustive]`)
//! - [`header`]: `VaultEncryptedHeader` / `HeaderAeadEnvelope` / `KdfParams`
//! - [`record`]: `EncryptedRecord` (永続化形)
//! - [`recovery`]: `RecoveryDisclosure` / `RecoveryWords` (REQ-S13 型レベル強制)
//! - [`confirmation`]: `DecryptConfirmation` (C-20 二段確認証跡)
//! - [`service`]: `VaultMigration` 6 メソッド (encrypt/decrypt/unlock x2/rekey/change_password)
//! - [`storage`]: SQLite 永続化形式 ↔ `VaultEncryptedHeader` のエンコード / デコード
//!
//! ## Clean Architecture
//!
//! - shikomi-core 側型のみ消費 (`Vek` / `MasterPassword` / `RecoveryMnemonic` /
//!   `Aad` / `WrappedVek` / `Record` / `Vault`)。
//! - shikomi-core への新規依存追加なし (`aes-gcm` / `argon2` / `bip39` は
//!   shikomi-infra::crypto::{aead, kdf} アダプタ経由のみ)。
//! - TC-D-S01 / TC-D-S04 sub-d-static-checks.sh で grep 検証。

pub mod confirmation;
pub mod error;
pub mod header;
pub mod record;
pub mod recovery;
pub mod service;
pub mod storage;

// 公開 re-export (shikomi-infra crate ルートから利用するエントリ)。
pub use confirmation::DecryptConfirmation;
pub use error::MigrationError;
pub use header::{HeaderAeadEnvelope, KdfParams, VaultEncryptedHeader};
pub use record::EncryptedRecord;
pub use recovery::{RecoveryDisclosure, RecoveryWords};
pub use service::VaultMigration;
