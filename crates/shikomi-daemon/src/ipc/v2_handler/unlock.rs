//! `handle_unlock` — Sub-E (#43) §F-E1 IPC `Unlock` ハンドラ。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §F-E1: `vault unlock`
//!
//! 処理ステップ (設計書通り):
//! 1. `backoff.check()?` でバックオフ中なら `BackoffActive` で即拒否
//! 2. `cache.is_unlocked().await` が true なら `Internal { reason: "already-unlocked" }`
//! 3. `migration.unlock_with_password(password)?` または `unlock_with_recovery(words)?`
//!    - 失敗時 `Crypto(WrongPassword)` のみ `record_failure` → 5 連で指数バックオフ発動
//!    - 他の `Crypto(_)` variant は backoff カウント対象外 (§F-E1 step 4 服部指摘)
//!    - `RecoveryRequired` → `IpcErrorCode::RecoveryRequired` 透過 (§C-27)
//! 4. `cache.unlock(vek).await` で `Unlocked` 遷移、`backoff.record_success()`
//! 5. `IpcResponse::Unlocked` 応答 (VEK 自体は IPC に乗せない)

use shikomi_core::error::CryptoError;
use shikomi_core::ipc::secret_bytes::SerializableSecretBytes;
use shikomi_core::ipc::{IpcErrorCode, IpcResponse};
use shikomi_infra::persistence::vault_migration::MigrationError;
use shikomi_infra::persistence::VaultRepository;

use super::error_mapping::migration_error_to_ipc;
use super::V2Context;

/// `IpcRequest::Unlock` を処理する。
pub async fn handle_unlock<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
    master_password: SerializableSecretBytes,
    recovery: Option<Vec<SerializableSecretBytes>>,
) -> IpcResponse {
    // 1. backoff check (C-26)
    {
        let backoff_guard = ctx.backoff.lock().await;
        if let Err(active) = backoff_guard.check() {
            return IpcResponse::Error(IpcErrorCode::BackoffActive {
                wait_secs: active.wait_secs,
            });
        }
    }

    // 2. 既に Unlocked なら拒否 (二重 unlock 試行の防御的拒否)
    if ctx.cache.is_unlocked().await {
        return IpcResponse::Error(IpcErrorCode::Internal {
            reason: "already-unlocked".to_owned(),
        });
    }

    // 3. unlock 実行: recovery 経路 or password 経路
    let unlock_result: Result<shikomi_core::crypto::Vek, MigrationError> = match recovery {
        Some(recovery_words) => {
            // recovery 24 語を [String; 24] に変換
            let words: [String; 24] = match secret_bytes_vec_to_words(recovery_words) {
                Ok(w) => w,
                Err(reason) => {
                    return IpcResponse::Error(IpcErrorCode::Internal { reason });
                }
            };
            ctx.migration.unlock_with_recovery(words)
        }
        None => {
            // パスワード経路
            let password_str = secret_bytes_to_string(&master_password);
            ctx.migration.unlock_with_password(password_str)
        }
    };

    match unlock_result {
        Ok(vek) => {
            // 4. cache に VEK を格納 (Locked → Unlocked)
            if let Err(err) = ctx.cache.unlock(vek).await {
                return IpcResponse::Error(IpcErrorCode::Internal {
                    reason: format!("cache-unlock-failed: {err}"),
                });
            }
            // backoff カウンタリセット
            let mut backoff_guard = ctx.backoff.lock().await;
            backoff_guard.record_success();
            // 5. Unlocked 応答
            IpcResponse::Unlocked
        }
        Err(err) => {
            // 失敗種別ごとの backoff カウント判定 (§F-E1 step 4)
            //
            // **`Crypto(WrongPassword)` のみ** カウント。他は backoff 対象外:
            // - `AeadTagMismatch`: vault.db 改竄経路、L2 DoS 防衛で除外
            // - `NonceLimitExceeded`: 内部状態起因、ユーザ入力起因でない
            // - `KdfFailed`: 暗号ライブラリ内部エラー、再試行余地なし
            // - `InvalidMnemonic`: recovery 入力検証失敗 (MSG-S12)、独立経路
            // - `RecoveryRequired`: パスワード経路で `MasterPassword::new` 失敗等、
            //   `IpcErrorCode::RecoveryRequired` 透過 (§C-27)
            if matches!(err, MigrationError::Crypto(CryptoError::WrongPassword)) {
                let mut backoff_guard = ctx.backoff.lock().await;
                backoff_guard.record_failure();
            }
            IpcResponse::Error(migration_error_to_ipc(err))
        }
    }
}

/// `SerializableSecretBytes` の中身バイト列を UTF-8 文字列として取り出す。
///
/// IPC 経由で受け取ったパスワードは `SecretBytes` (=`Vec<u8>`) として運ばれるため、
/// `String` 化する必要がある (`MasterPassword::new` シグネチャ要件)。
fn secret_bytes_to_string(secret: &SerializableSecretBytes) -> String {
    let bytes = secret.inner().expose_secret();
    String::from_utf8_lossy(bytes).to_string()
}

/// `Vec<SerializableSecretBytes>` (24 個) を `[String; 24]` に変換する。
fn secret_bytes_vec_to_words(
    vec: Vec<SerializableSecretBytes>,
) -> Result<[String; 24], String> {
    if vec.len() != 24 {
        return Err(format!("recovery-words-must-be-24, got {}", vec.len()));
    }
    let words: Vec<String> = vec.iter().map(secret_bytes_to_string).collect();
    let arr: [String; 24] = words.try_into().map_err(|_| "recovery-words-conversion-failed".to_owned())?;
    Ok(arr)
}
