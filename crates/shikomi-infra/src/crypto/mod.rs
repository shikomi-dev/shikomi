//! shikomi-infra 暗号アダプタ層 (Sub-B / Sub-C 共有モジュール)。
//!
//! Clean Architecture の依存方向: 本モジュールは `shikomi-core` の暗号ドメイン型
//! (`Vek` / `Kek<_>` / `KdfSalt` / `NonceBytes` / `MasterPassword` / `RecoveryMnemonic` /
//! `CryptoError` / `WeakPasswordFeedback`) に**のみ**依存する。`shikomi-core` への
//! 逆依存は禁止 (no-I/O 制約継承、`docs/features/vault-encryption/detailed-design/index.md`)。
//!
//! ## サブモジュール
//!
//! - [`rng`]: `rand_core::OsRng` を用いた CSPRNG 単一エントリ点 (Sub-B、`rng.md`)
//! - [`kdf`]: Argon2id / BIP-39+PBKDF2+HKDF KDF アダプタ (Sub-B、`kdf.md`)
//! - [`password`]: zxcvbn ベース `PasswordStrengthGate` 実装 (Sub-B、`password.md`)

pub mod kdf;
pub mod password;
pub mod rng;

pub use kdf::{Argon2idAdapter, Argon2idParams, Bip39Pbkdf2Hkdf, HKDF_INFO};
pub use password::ZxcvbnGate;
pub use rng::Rng;
