//! `handle_rotate_recovery` — Sub-E (#43) §F-E4 IPC `RotateRecovery` ハンドラ。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §F-E4: `rotate_recovery`
//!
//! 処理:
//! 1. `cache.is_unlocked()` 確認、`Locked` なら `IpcErrorCode::VaultLocked` (C-22)
//! 2. `migration.rekey_with_recovery_rotation(master_password)?` で **rekey + recovery
//!    rotation を 1 atomic write** で同時実行 (§F-E5 服部指摘の整合性破壊ウィンドウ
//!    封鎖)。戻り値の `RecoveryDisclosure` から 新 24 語を `disclose()` で 1 度限り取得
//! 3. 新 VEK は atomic write 内で生成された後、daemon は再 unlock で取得する経路
//!    (cache.lock → migration.unlock_with_password → cache.unlock)
//! 4. 新 24 語を `Vec<SerializableSecretBytes>` に変換して `IpcResponse::RecoveryRotated`
//!    で返却。daemon 側 zeroize 4 経路 (§F-E4 step 9):
//!    - (a) `RecoveryWords::Drop` で `String::zeroize()` 連鎖
//!    - (b) IPC フレーム送信完了後 `mem::drop` で Drop 連鎖
//!    - (c) `tracing` には Debug 出力含めない (`[REDACTED RECOVERY WORDS (24)]` 固定)
//!    - (d) `Err(_)` 経路でも構築済 disclosure は Drop 連鎖で zeroize

use shikomi_core::ipc::{IpcErrorCode, IpcResponse, SerializableSecretBytes};
use shikomi_core::SecretBytes;
use shikomi_infra::persistence::VaultRepository;

use super::error_mapping::migration_error_to_ipc;
use super::V2Context;

/// `IpcRequest::RotateRecovery` を処理する。
pub async fn handle_rotate_recovery<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
    master_password: SerializableSecretBytes,
) -> IpcResponse {
    // C-22: Locked 拒否
    if !ctx.cache.is_unlocked().await {
        return IpcResponse::Error(IpcErrorCode::VaultLocked);
    }

    let password_str = secret_bytes_to_string(&master_password);

    // rekey + recovery rotation atomic 実行 (§F-E5)
    let (_records_count, disclosure) = match ctx
        .migration
        .rekey_with_recovery_rotation(password_str.clone())
    {
        Ok(pair) => pair,
        Err(err) => return IpcResponse::Error(migration_error_to_ipc(err)),
    };

    // cache を新 VEK で再構築: lock → 再 unlock
    if let Err(err) = ctx.cache.lock().await {
        return IpcResponse::Error(IpcErrorCode::Internal {
            reason: format!("cache-lock-failed: {err}"),
        });
    }
    match ctx.migration.unlock_with_password(password_str) {
        Ok(new_vek) => {
            if let Err(err) = ctx.cache.unlock(new_vek).await {
                return IpcResponse::Error(IpcErrorCode::Internal {
                    reason: format!("cache-unlock-failed: {err}"),
                });
            }
        }
        Err(err) => {
            // ここに来た場合は atomic write は成功しているが daemon 側 cache 復旧失敗。
            // 次回 IPC で `vault unlock` 再試行が必要。warning ログのみ。
            tracing::warn!(
                target: "shikomi_daemon::ipc::v2_handler",
                "rotate_recovery: cache re-unlock failed after atomic save: {err:?}"
            );
        }
    }

    // 新 24 語を IPC 応答用に変換
    let words = disclosure.disclose();
    let words_vec: Vec<SerializableSecretBytes> = words
        .as_slice()
        .iter()
        .map(|w| SerializableSecretBytes::new(SecretBytes::from_vec(w.as_bytes().to_vec())))
        .collect();
    // words: RecoveryWords は Drop 連鎖で String::zeroize() (a)
    drop(words);

    IpcResponse::RecoveryRotated { words: words_vec }
}

fn secret_bytes_to_string(secret: &SerializableSecretBytes) -> String {
    secret.to_lossy_string_for_handler()
}
