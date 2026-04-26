//! `handle_lock` — Sub-E (#43) §F-E2 IPC `Lock` ハンドラ。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §F-E2: `vault lock`
//!
//! 処理: `cache.lock().await` で `Unlocked → Locked` 遷移、旧 `Vek` を Drop 連鎖で
//! zeroize (C-23)、`IpcResponse::Locked` 応答。`Locked` 状態で呼ばれた場合は no-op
//! (idempotent、再 lock 安全)。

use shikomi_core::ipc::{IpcErrorCode, IpcResponse};
use shikomi_infra::persistence::VaultRepository;

use super::V2Context;

/// `IpcRequest::Lock` を処理する。
pub async fn handle_lock<R: VaultRepository + ?Sized>(ctx: &V2Context<'_, R>) -> IpcResponse {
    match ctx.cache.lock().await {
        Ok(()) => IpcResponse::Locked,
        Err(err) => IpcResponse::Error(IpcErrorCode::Internal {
            reason: format!("cache-lock-failed: {err}"),
        }),
    }
}
