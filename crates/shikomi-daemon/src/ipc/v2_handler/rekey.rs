//! `handle_rekey` — Sub-E (#43) §F-E5 IPC `Rekey` ハンドラ。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §F-E5: `rekey` (nonce overflow / 明示 rekey)
//!
//! 処理: `rotate_recovery` と内部実装は同一 (`rekey_with_recovery_rotation` 経由)、
//! 外向き IPC variant 名を `Rekey` として分離する (CLI `vault rekey` サブコマンドと
//! `vault rotate-recovery` の用途明示)。応答は `IpcResponse::Rekeyed { records_count, words }`
//! で再暗号化レコード数 + 新 24 語を含む。
//!
//! ユーザは rekey 完了直後に新 24 語を記録する責務 (旧 mnemonic は invalidated、
//! Sub-F MSG-S07 文言で誘導)。daemon 側 zeroize 経路は `rotate_recovery` と同等の
//! 4 段防衛 (§F-E4 step 9 (a)〜(d))。

use shikomi_core::ipc::{IpcErrorCode, IpcResponse, SerializableSecretBytes};
use shikomi_core::SecretBytes;
use shikomi_infra::persistence::VaultRepository;

use super::error_mapping::migration_error_to_ipc;
use super::V2Context;

/// `IpcRequest::Rekey` を処理する。
pub async fn handle_rekey<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
    master_password: SerializableSecretBytes,
) -> IpcResponse {
    // C-22: Locked 拒否
    if !ctx.cache.is_unlocked().await {
        return IpcResponse::Error(IpcErrorCode::VaultLocked);
    }

    let password_str = secret_bytes_to_string(&master_password);

    // rekey + recovery rotation atomic 実行 (§F-E5)
    let (records_count, disclosure) =
        match ctx.migration.rekey_with_recovery_rotation(password_str.clone()) {
            Ok(pair) => pair,
            Err(err) => return IpcResponse::Error(migration_error_to_ipc(err)),
        };

    // cache を新 VEK で再構築 (lock → 再 unlock)
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
            tracing::warn!(
                target: "shikomi_daemon::ipc::v2_handler",
                "rekey: cache re-unlock failed after atomic save: {err:?}"
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
    drop(words);

    IpcResponse::Rekeyed {
        records_count,
        words: words_vec,
    }
}

fn secret_bytes_to_string(secret: &SerializableSecretBytes) -> String {
    let bytes = secret.inner().expose_secret();
    String::from_utf8_lossy(bytes).to_string()
}
