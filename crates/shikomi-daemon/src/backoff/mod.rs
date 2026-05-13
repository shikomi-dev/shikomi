//! `backoff` モジュール — Sub-E (#43) 連続 unlock 失敗の指数バックオフ。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §`UnlockBackoff` (REQ-S11)

#[doc(hidden)]
pub mod unlock;

pub use unlock::{BackoffActive, UnlockBackoff};
