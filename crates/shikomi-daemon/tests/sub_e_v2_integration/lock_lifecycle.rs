//! TC-E-I04 / TC-E-I05: lock 経路 + OS シグナル (C-23 / C-24 / C-25 / EC-2).
//!
//! 設計書 §14.4 / §14.6 SSoT:
//! - TC-E-I04: 明示 `IpcRequest::Lock` で cache が `Locked` 遷移、`Vek::Drop` 連鎖 zeroize
//! - TC-E-I05: `OsLockSignal::ScreenLocked` 受信から 100ms 以内に cache が `Locked` に到達

use crate::common::{fresh_repo, tighten_perms_unix};
use crate::helpers::{
    encrypt_existing_vault, handshake_v2, run_dispatch, secret, seed_plaintext_vault,
    STRONG_PASSWORD,
};
use shikomi_core::ipc::{IpcRequest, IpcResponse};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use std::time::Duration;
use tokio::sync::Mutex;

// =====================================================================
// TC-E-I04: 明示 Lock + Vek zeroize 連鎖 (C-23 / EC-2)
// =====================================================================

#[tokio::test]
async fn tc_e_i04_lock_explicit_after_unlock() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // unlock
    let _ = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    assert!(cache.is_unlocked().await);

    // 明示 Lock
    let resp = run_dispatch(&repo, &cache, &backoff, handshake_v2(), IpcRequest::Lock).await;
    assert!(matches!(resp, IpcResponse::Locked));
    assert!(
        !cache.is_unlocked().await,
        "explicit Lock must clear cache (C-23 zeroize 連鎖)"
    );
}

// =====================================================================
// TC-E-I05: OS シグナル 100ms 以内 lock (C-25)
// =====================================================================

#[tokio::test]
async fn tc_e_i05_os_lock_signal_within_100ms() {
    use shikomi_daemon::cache::lifecycle::{run_os_lock_signal_loop, MockLockSignal};
    use shikomi_daemon::cache::LockEvent;
    use tokio::sync::watch;

    let cache = VekCache::new();
    cache
        .unlock(shikomi_core::crypto::Vek::from_array([0x55u8; 32]))
        .await
        .unwrap();
    assert!(cache.is_unlocked().await);

    let (mock, tx_event) = MockLockSignal::new();
    let (tx_shutdown, rx_shutdown) = watch::channel(false);
    let handle = tokio::spawn(run_os_lock_signal_loop(
        cache.clone(),
        Box::new(mock),
        rx_shutdown,
    ));

    let t0 = tokio::time::Instant::now();
    tx_event.send(LockEvent::ScreenLocked).await.unwrap();

    // 100ms 以内に lock 観測
    let deadline = t0 + Duration::from_millis(100);
    let mut locked_within = false;
    while tokio::time::Instant::now() < deadline {
        if !cache.is_unlocked().await {
            locked_within = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let elapsed = t0.elapsed();

    let _ = tx_shutdown.send(true);
    let _ = handle.await;

    assert!(
        locked_within,
        "ScreenLocked must lock cache within 100ms (elapsed: {elapsed:?})"
    );
    assert!(!cache.is_unlocked().await);
}
