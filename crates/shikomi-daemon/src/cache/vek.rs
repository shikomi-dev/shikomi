//! `VekCache` + `VaultUnlockState` — Sub-E (#43) VEK lifecycle 管理の中核。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §`VaultUnlockState` 型遷移 / §`VekCache`
//!
//! ## 不変条件
//!
//! - **C-22**: `Locked` 状態で read/write 系 IPC は型レベル拒否 (各ハンドラ入口の
//!   `match` で `Locked => Err(CacheError::VaultLocked)` 強制、ワイルドカード `_` 禁止)
//! - **C-23**: `Unlocked → Locked` 遷移時に `Vek` を即 zeroize (`mem::replace` で
//!   旧 state を取り出し Drop 連鎖、`Vek` の `Drop` 実装が zeroize)
//!
//! `Clone` / `Copy` / `Display` / `serde::Serialize` 未実装 (`Vek` の禁止トレイトが
//! 連鎖、誤コピー / 誤シリアライズを型レベル禁止)。

use std::sync::Arc;
use std::time::Instant;

use shikomi_core::crypto::Vek;
use thiserror::Error;
use tokio::sync::RwLock;

// -------------------------------------------------------------------
// VaultUnlockState
// -------------------------------------------------------------------

/// vault のロック状態 enum (型遷移)。
///
/// 設計書 §`VaultUnlockState` 型定義: `Locked` / `Unlocked { vek, last_used }`。
/// `#[non_exhaustive]` で将来拡張耐性、ワイルドカード `_` を呼出側 match で禁止する
/// (TC-E-S* grep gate で機械検証、Sub-D Rev3 ペテルギウス指摘継承)。
#[derive(Debug)]
#[non_exhaustive]
pub enum VaultUnlockState {
    /// 初期状態 / lock 後 / アイドルタイムアウト後 / OS シグナル後の状態。
    /// read/write IPC は本状態で `CacheError::VaultLocked` 拒否される。
    Locked,
    /// `Unlock` IPC + KDF 成功で遷移する状態。VEK と最終操作時刻を保持。
    /// `Drop` で `Vek` の zeroize 連鎖が発生する (C-23)。
    Unlocked {
        /// 復号済 Vault Encryption Key (32B、Sub-A 型階層)。
        vek: Vek,
        /// 最終 read/write IPC 実行時刻 (`IdleTimer` のポーリング起点、C-24)。
        last_used: Instant,
    },
}

// -------------------------------------------------------------------
// CacheError
// -------------------------------------------------------------------

/// `VekCache` の操作エラー。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CacheError {
    /// `Locked` 状態で `with_vek` / read/write IPC が呼ばれた (C-22)。
    /// `IpcErrorCode::VaultLocked` に透過 → MSG-S09 (c) キャッシュ揮発経路。
    #[error("vault is locked, unlock required")]
    VaultLocked,
    /// 多重 unlock 試行 (防御的、運用上は旧 VEK を破棄して新 VEK で上書きする経路を
    /// 採用、本 variant は opt-in 厳格モード用に予約)。
    #[error("vault is already unlocked")]
    AlreadyUnlocked,
}

// -------------------------------------------------------------------
// VekCache
// -------------------------------------------------------------------

/// VEK キャッシュ (`Arc<RwLock<VaultUnlockState>>`)。
///
/// 設計書 §`VekCache` 型定義 + メソッド群:
/// - `new` / `unlock` / `lock` / `with_vek` / `is_unlocked` / `last_used`
///
/// `Clone` 実装 (`Arc` のクローンのみ、内部 `RwLock` は共有)。daemon の
/// composition root で構築し、IPC ハンドラ / `IdleTimer` / `OsLockSignal`
/// 各経路に渡す。
#[derive(Clone)]
pub struct VekCache {
    state: Arc<RwLock<VaultUnlockState>>,
}

impl VekCache {
    /// `Locked` 状態で構築する (daemon 起動直後)。
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(VaultUnlockState::Locked)),
        }
    }

    /// `Locked → Unlocked { vek, last_used: Instant::now() }` 遷移。
    ///
    /// **既に `Unlocked` 状態の場合** は旧 `Vek` を Drop (zeroize) してから新 VEK で
    /// 上書きする (連続 unlock の防御的挙動、設計書 §`VekCache::unlock` 仕様)。
    ///
    /// # Errors
    ///
    /// 現状エラーケースは存在しない (`Result` を返すのは将来の拡張余地、設計書通り)。
    pub async fn unlock(&self, vek: Vek) -> Result<(), CacheError> {
        let mut guard = self.state.write().await;
        let old = std::mem::replace(
            &mut *guard,
            VaultUnlockState::Unlocked {
                vek,
                last_used: Instant::now(),
            },
        );
        // 旧 VEK (Locked なら no-op、Unlocked なら zeroize 連鎖) は scope 抜けで Drop
        drop(old);
        Ok(())
    }

    /// `Unlocked → Locked` 遷移、旧 `Vek` を `mem::replace` で取り出し Drop 連鎖
    /// zeroize (C-23)。`Locked` 状態で呼ばれた場合は no-op (再 lock 安全)。
    ///
    /// # Errors
    ///
    /// 現状エラーケースは存在しない。
    pub async fn lock(&self) -> Result<(), CacheError> {
        let mut guard = self.state.write().await;
        let old = std::mem::replace(&mut *guard, VaultUnlockState::Locked);
        drop(old);
        Ok(())
    }

    /// **クロージャインジェクション** で `Unlocked` 時のみ `f(&Vek)` を実行する。
    ///
    /// 設計書 §`VekCache::with_vek` 仕様: `Unlocked` の場合のみクロージャ `f(&vek)`
    /// を実行し、`last_used` を更新。`Locked` の場合 `Err(CacheError::VaultLocked)`。
    /// クロージャインジェクション (Sub-C `AeadKey::with_secret_bytes` と同型) で
    /// 借用越境、所有権は cache 内に保持し VEK が cache 境界外に漏出しない。
    ///
    /// # Errors
    ///
    /// - `Locked` 状態: `CacheError::VaultLocked`
    pub async fn with_vek<R, F>(&self, f: F) -> Result<R, CacheError>
    where
        F: FnOnce(&Vek) -> R,
    {
        let mut guard = self.state.write().await;
        match &mut *guard {
            VaultUnlockState::Locked => Err(CacheError::VaultLocked),
            VaultUnlockState::Unlocked { vek, last_used } => {
                let r = f(vek);
                *last_used = Instant::now();
                Ok(r)
            }
        }
    }

    /// 状態のみ確認 (IPC ハンドラの早期判定 / `last_used` 不更新)。
    pub async fn is_unlocked(&self) -> bool {
        matches!(*self.state.read().await, VaultUnlockState::Unlocked { .. })
    }

    /// `last_used` を取得 (`IdleTimer` のポーリング用)。
    /// `Locked` 状態なら `None` (idle タイムアウト判定対象外)。
    pub async fn last_used(&self) -> Option<Instant> {
        match &*self.state.read().await {
            VaultUnlockState::Locked => None,
            VaultUnlockState::Unlocked { last_used, .. } => Some(*last_used),
        }
    }
}

impl Default for VekCache {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------
// tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::crypto::Vek;

    fn dummy_vek() -> Vek {
        Vek::from_array([0xCDu8; 32])
    }

    #[tokio::test]
    async fn new_starts_locked() {
        let cache = VekCache::new();
        assert!(!cache.is_unlocked().await);
    }

    #[tokio::test]
    async fn unlock_transitions_to_unlocked() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();
        assert!(cache.is_unlocked().await);
    }

    #[tokio::test]
    async fn lock_after_unlock_returns_to_locked() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();
        cache.lock().await.unwrap();
        assert!(!cache.is_unlocked().await);
    }

    #[tokio::test]
    async fn with_vek_locked_returns_vault_locked_error() {
        let cache = VekCache::new();
        let result: Result<u8, CacheError> = cache.with_vek(|_| 0u8).await;
        assert!(matches!(result, Err(CacheError::VaultLocked)));
    }

    #[tokio::test]
    async fn with_vek_unlocked_invokes_closure_with_vek_bytes() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();
        let first_byte = cache
            .with_vek(|vek| {
                use shikomi_core::crypto::AeadKey;
                vek.with_secret_bytes(|bytes| bytes[0])
            })
            .await
            .unwrap();
        assert_eq!(first_byte, 0xCD);
    }

    #[tokio::test]
    async fn with_vek_updates_last_used_timestamp() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();
        let t0 = cache.last_used().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _: Result<(), CacheError> = cache.with_vek(|_| ()).await;
        let t1 = cache.last_used().await.unwrap();
        assert!(t1 > t0, "last_used must advance after with_vek call");
    }

    #[tokio::test]
    async fn unlock_when_already_unlocked_replaces_old_vek() {
        let cache = VekCache::new();
        cache.unlock(Vek::from_array([0x11u8; 32])).await.unwrap();
        cache.unlock(Vek::from_array([0x22u8; 32])).await.unwrap();
        let first_byte = cache
            .with_vek(|vek| {
                use shikomi_core::crypto::AeadKey;
                vek.with_secret_bytes(|bytes| bytes[0])
            })
            .await
            .unwrap();
        assert_eq!(first_byte, 0x22, "second unlock must replace first VEK");
    }

    #[tokio::test]
    async fn lock_on_locked_state_is_idempotent() {
        let cache = VekCache::new();
        cache.lock().await.unwrap();
        cache.lock().await.unwrap();
        assert!(!cache.is_unlocked().await);
    }

    #[tokio::test]
    async fn last_used_returns_none_when_locked() {
        let cache = VekCache::new();
        assert!(cache.last_used().await.is_none());
    }

    #[tokio::test]
    async fn cache_clone_shares_state() {
        let cache_a = VekCache::new();
        let cache_b = cache_a.clone();
        cache_a.unlock(dummy_vek()).await.unwrap();
        assert!(cache_b.is_unlocked().await, "clone must share inner Arc state");
    }
}
