//! TC-E-I01: Unlock 正常系 (REQ-S09 / EC-1).
//!
//! 設計書 §14.4 / §14.6 SSoT: パスワード経路で `Unlock` IPC 成功 → cache が
//! `Unlocked` に遷移、`UnlockBackoff::failures` が 0 にリセット。

use crate::common::{fresh_repo, tighten_perms_unix};
use crate::helpers::{
    encrypt_existing_vault, handshake_v2, run_dispatch, secret, seed_plaintext_vault,
    STRONG_PASSWORD,
};
use shikomi_core::ipc::{IpcRequest, IpcResponse};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use tokio::sync::Mutex;

#[tokio::test]
async fn tc_e_i01_unlock_round_trip() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 3);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    let resp = run_dispatch(
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

    assert!(
        matches!(resp, IpcResponse::Unlocked),
        "expected Unlocked, got {resp:?}"
    );
    assert!(cache.is_unlocked().await, "cache must be unlocked");
    assert_eq!(
        backoff.lock().await.failures(),
        0,
        "backoff counter must be reset on success"
    );
}
