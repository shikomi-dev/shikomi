//! `ZxcvbnGate` — `PasswordStrengthGate` の zxcvbn ベース具象実装 (Sub-B 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/password.md` §`ZxcvbnGate` 具象実装

pub mod zxcvbn_gate;

pub use zxcvbn_gate::ZxcvbnGate;
