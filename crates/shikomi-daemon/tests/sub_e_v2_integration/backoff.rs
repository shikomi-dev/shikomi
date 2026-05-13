//! TC-E-I02 / TC-E-I02b: 連続失敗 backoff (REQ-S11 / C-26).
//!
//! Bug-E-001 解決経路 (`9a25aa6`): `verify_header_aead` 失敗を `WrongPassword` に
//! 意味論再分類することで現実 brute force 経路でも 5 回連続失敗で backoff が発動。
//! TC-E-I02 は本来の意図 (5 連 wrong-password → 6 回目 BackoffActive) を機械固定、
//! TC-E-I02b は `UnlockBackoff::record_failure` 直接 5 回 → dispatch_v2 入口拒否を
//! 別経路で担保。

use crate::common::{fresh_repo, tighten_perms_unix};
use crate::helpers::{
    encrypt_existing_vault, handshake_v2, run_dispatch, secret, seed_plaintext_vault,
    STRONG_PASSWORD,
};
use shikomi_core::ipc::{IpcErrorCode, IpcRequest, IpcResponse};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use tokio::sync::Mutex;

// =====================================================================
// TC-E-I02: 5 回連続失敗で 6 回目 BackoffActive (REQ-S11 / C-26)
// =====================================================================
//
// 修正前 (Bug-E-001): 通常誤入力は AeadTagMismatch で failures==0 のまま
// 修正後 (`9a25aa6`): 通常誤入力は WrongPassword で failures がカウントされ、
//                     5 回後の 6 回目で BackoffActive 入口拒否

#[tokio::test]
async fn tc_e_i02_unlock_backoff_after_5_wrong_password_failures() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // 5 回連続でわざと違う強パスワードを送信。zxcvbn ゲートを通過する程度の長さで
    // KEK_pw 不一致 → verify_header_aead 失敗 → WrongPassword 変換 (Bug-E-001 修正) を狙う。
    let wrong_password = "incorrect horse battery staple wrong attempt phrase";
    for i in 0..5 {
        let resp = run_dispatch(
            &repo,
            &cache,
            &backoff,
            handshake_v2(),
            IpcRequest::Unlock {
                master_password: secret(wrong_password),
                recovery: None,
            },
        )
        .await;
        match resp {
            IpcResponse::Error(IpcErrorCode::Crypto { reason }) => {
                assert_eq!(
                    reason, "wrong-password",
                    "attempt {i}: expected wrong-password (Bug-E-001 修正後), got {reason}"
                );
            }
            other => panic!("attempt {i}: expected Crypto(wrong-password), got {other:?}"),
        }
    }

    assert_eq!(
        backoff.lock().await.failures(),
        5,
        "5 wrong-password failures must increment backoff counter to 5 (REQ-S11 / C-26)"
    );

    // 6 回目: ハンドラ入口で BackoffActive 即拒否 (VaultMigration には到達しない)
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD), // 6 回目は正パスワードでも届かない
            recovery: None,
        },
    )
    .await;
    match resp {
        IpcResponse::Error(IpcErrorCode::BackoffActive { wait_secs }) => {
            assert!(
                (30..=31).contains(&wait_secs),
                "5 failures should yield ~30s backoff, got {wait_secs}"
            );
        }
        other => panic!("expected BackoffActive(wait_secs=30), got {other:?}"),
    }

    assert!(
        !cache.is_unlocked().await,
        "cache must remain locked while backoff active"
    );
}

// =====================================================================
// TC-E-I02b: backoff 入口拒否動作 (handler entry 経路)
// =====================================================================

#[tokio::test]
async fn tc_e_i02b_backoff_blocks_unlock_at_handler_entry() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // backoff カウンタを直接 5 回上げる (C-26 発動状態を擬似的に作る)
    {
        let mut guard = backoff.lock().await;
        for _ in 0..UnlockBackoff::TRIGGER_FAILURES {
            guard.record_failure();
        }
        assert!(guard.check().is_err(), "backoff must be active");
    }

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
    match resp {
        IpcResponse::Error(IpcErrorCode::BackoffActive { wait_secs }) => {
            assert!(
                (30..=31).contains(&wait_secs),
                "5 failures should yield ~30s backoff, got {wait_secs}"
            );
        }
        other => panic!("expected BackoffActive, got {other:?}"),
    }
    assert!(
        !cache.is_unlocked().await,
        "backoff active state must NOT unlock cache"
    );
}
