//! TC-E-I07 / TC-E-I09: handshake 許可リスト境界 + バイパス拒否 (C-28 / C-29).
//!
//! 設計書 §14.4 / §14.6 SSoT:
//! - TC-E-I07: V1 client が V2 専用 variant 送信 → `ProtocolDowngrade` (C-28)
//! - TC-E-I09: PreHandshake で `Handshake` 以外を全拒否 (C-29)

use crate::common::{fresh_repo, tighten_perms_unix};
use crate::helpers::{
    encrypt_existing_vault, handshake_v1, run_dispatch, secret, seed_plaintext_vault,
    STRONG_PASSWORD,
};
use shikomi_core::ipc::{IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use shikomi_daemon::ipc::v2_handler::{check_request_allowed, ClientState};
use tokio::sync::Mutex;

// =====================================================================
// TC-E-I07: V1 client が V2 専用 variant 送信 → ProtocolDowngrade (C-28)
// =====================================================================

#[tokio::test]
async fn tc_e_i07_v1_client_protocol_downgrade() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // V1 client が Unlock (V2 専用) を送信
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v1(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    assert!(
        matches!(resp, IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)),
        "V1 client + V2-only must return ProtocolDowngrade, got {resp:?}"
    );
    assert!(
        !cache.is_unlocked().await,
        "V2-only rejection must NOT mutate cache"
    );

    // V1 client + V1 サブセット (ListRecords) は許可される (C-28 反対側)
    assert!(
        check_request_allowed(handshake_v1(), &IpcRequest::ListRecords).is_ok(),
        "V1 client + V1 subset (ListRecords) must be allowed"
    );
}

// =====================================================================
// TC-E-I09: handshake バイパス拒否 (C-29)
// =====================================================================

#[tokio::test]
async fn tc_e_i09_pre_handshake_rejects_all_non_handshake_variants() {
    let cases = vec![
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
        IpcRequest::ListRecords,
        IpcRequest::Lock,
        IpcRequest::RotateRecovery {
            master_password: secret(STRONG_PASSWORD),
        },
    ];

    for req in cases {
        let result = check_request_allowed(ClientState::PreHandshake, &req);
        match result {
            Err(IpcErrorCode::ProtocolDowngrade) => {}
            other => panic!(
                "PreHandshake + {} must reject with ProtocolDowngrade, got {:?}",
                req.variant_name(),
                other
            ),
        }
    }

    // PreHandshake で Handshake variant のみ通る
    let handshake = IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V2,
    };
    assert!(
        check_request_allowed(ClientState::PreHandshake, &handshake).is_ok(),
        "PreHandshake must allow Handshake variant"
    );
}
