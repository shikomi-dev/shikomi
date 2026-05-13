//! `cache` モジュール — Sub-E (#43) VEK lifecycle 管理。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §`VekCache` / §`VaultUnlockState` 型遷移 / §`IdleTimer` / §`OsLockSignal` trait
//!
//! ## モジュール構成
//!
//! - [`vek`] — `VekCache` + `VaultUnlockState` (Locked/Unlocked 型遷移、`with_vek`
//!   クロージャインジェクション、C-22 / C-23)
//! - [`lifecycle`] — `IdleTimer` (15min 自動 lock、C-24) + `OsLockSignal` trait
//!   (OS スクリーンロック / サスペンド購読、C-25)

#[doc(hidden)]
pub mod lifecycle;
#[doc(hidden)]
pub mod vek;

pub use lifecycle::{IdleTimer, LockEvent, OsLockSignal};
pub use vek::{CacheError, VaultUnlockState, VekCache};
