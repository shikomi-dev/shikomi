//! `handle_change_password` — Sub-E (#43) §F-E3 IPC `ChangePassword` ハンドラ。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §F-E3: `change_password` (REQ-S10 O(1))
//!
//! 処理:
//! 1. `cache.is_unlocked()` 確認、`Locked` なら `IpcErrorCode::VaultLocked` (C-22)
//! 2. `migration.change_password(&old, &new)?` (Sub-D §F-D5、`wrapped_VEK_by_pw` のみ
//!    新 KEK で再 wrap、新 `kdf_salt` 生成、`nonce_counter` / `wrapped_VEK_by_recovery`
//!    は不変)
//! 3. **VEK 不変** のため `cache` の VEK は引き続き有効、再 unlock 不要
//! 4. `IpcResponse::PasswordChanged` 応答

use shikomi_core::ipc::{IpcErrorCode, IpcResponse, SerializableSecretBytes};
use shikomi_infra::persistence::VaultRepository;

use super::error_mapping::migration_error_to_ipc;
use super::V2Context;

/// `IpcRequest::ChangePassword` を処理する。
pub async fn handle_change_password<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
    old: SerializableSecretBytes,
    new: SerializableSecretBytes,
) -> IpcResponse {
    // C-22: Locked 拒否
    if !ctx.cache.is_unlocked().await {
        return IpcResponse::Error(IpcErrorCode::VaultLocked);
    }

    let old_str = secret_bytes_to_string(&old);
    let new_str = secret_bytes_to_string(&new);

    match ctx.migration.change_password(old_str, new_str) {
        Ok(()) => IpcResponse::PasswordChanged,
        Err(err) => IpcResponse::Error(migration_error_to_ipc(err)),
    }
}

fn secret_bytes_to_string(secret: &SerializableSecretBytes) -> String {
    secret.to_lossy_string_for_handler()
}
