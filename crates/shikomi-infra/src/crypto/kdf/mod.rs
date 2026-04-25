//! KDF アダプタ — `Argon2idAdapter` (パスワード経路) / `Bip39Pbkdf2Hkdf` (リカバリ経路)。
//!
//! - `Argon2idAdapter`: マスターパスワード + 16B salt → `Kek<KekKindPw>` (Argon2id raw API)
//! - `Bip39Pbkdf2Hkdf`: 24 語 → `bip39` 経由で 64B seed → HKDF-SHA256 → `Kek<KekKindRecovery>`
//!
//! `pbkdf2` / `hmac` crate は本モジュールの直接依存ではなく、`bip39` / `hkdf` / `argon2`
//! 内部で利用される (`Cargo.toml` に直接追加しない、`kdf.md` §`bip39` / `pbkdf2` / `hkdf`
//! crate 呼出契約)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/kdf.md`

pub mod argon2id;
pub mod bip39_pbkdf2_hkdf;

#[cfg(test)]
mod kat;

pub use argon2id::{Argon2idAdapter, Argon2idParams};
pub use bip39_pbkdf2_hkdf::{Bip39Pbkdf2Hkdf, HKDF_INFO};
