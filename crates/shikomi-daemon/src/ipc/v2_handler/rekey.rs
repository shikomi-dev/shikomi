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

/// **テスト専用 fault-injection seam** (Pegasus 工程5 致命指摘 ④ TC-E-I06c).
///
/// `debug_assertions` ビルドでのみ実体を持つ `AtomicBool`。`true` を
/// `Ordering::SeqCst` で store した状態で `handle_rekey` / `handle_rotate_recovery`
/// が atomic write 後に再 unlock を試みる経路に到達した時、**`unlock_with_password`
/// の成否に関わらず** 強制的に `cache_relocked = false` 経路に分岐する。本フラグは
/// Lie-Then-Surprise 経路 (vault.db 正常 / daemon cache のみ Locked) を実機経路
/// として発火させ、後続の `ListRecords` が `IpcErrorCode::VaultLocked` を返すことを
/// テストで機械固定する用途に限定。
///
/// `cfg(debug_assertions)` で囲み **release ビルド (本番) には実体がコンパイル
/// されない**ため、攻撃面ゼロ。`shikomi-daemon` は `publish = false` の internal
/// crate であり、`pub` でも外部 crate からはアクセスできない。
///
/// 設計判断: `#[cfg(test)]` でなく `#[cfg(debug_assertions)]` を採用したのは、
/// integration test (tests/ 配下) は cfg(test) では daemon lib を build せず、
/// `pub` 公開シンボルのみアクセス可能なため。`debug_assertions` は dev profile
/// (= cargo test/run) でのみ true になり、release build では false。
#[cfg(debug_assertions)]
pub static FORCE_RELOCK_FAILURE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
    let (records_count, disclosure) = match ctx
        .migration
        .rekey_with_recovery_rotation(password_str.clone())
    {
        Ok(pair) => pair,
        Err(err) => return IpcResponse::Error(migration_error_to_ipc(err)),
    };

    // cache を新 VEK で再構築 (lock → 再 unlock)。
    // **ペガサス工程5 致命指摘解消**: 旧実装は再 unlock 失敗時に `tracing::warn!` のみで
    // `Rekeyed` を成功として返却 (Lie-Then-Surprise)。`cache_relocked: bool` を IPC 応答
    // に含め Sub-F CLI/GUI が「鍵情報の再キャッシュに失敗、もう一度 unlock してください」
    // を表示できる経路を確保 (Fail Kindly)。
    if let Err(err) = ctx.cache.lock().await {
        return IpcResponse::Error(IpcErrorCode::Internal {
            reason: format!("cache-lock-failed: {err}"),
        });
    }
    // テスト fault-injection: TC-E-I06c で `cache_relocked: false` 経路を実機経路として
    // 発火させる。release ビルドには `FORCE_RELOCK_FAILURE` 自体がコンパイルされない
    // (`#[cfg(debug_assertions)]`)、攻撃面ゼロ。
    #[cfg(debug_assertions)]
    let force_failure = FORCE_RELOCK_FAILURE.load(std::sync::atomic::Ordering::SeqCst);
    #[cfg(not(debug_assertions))]
    let force_failure = false;

    let cache_relocked = if force_failure {
        tracing::warn!(
            target: "shikomi_daemon::ipc::v2_handler",
            "rekey: relock forced to fail (test fault-injection); \
             responding with cache_relocked=false (Pegasus 工程5 TC-E-I06c)"
        );
        false
    } else {
        match ctx.migration.unlock_with_password(password_str) {
            Ok(new_vek) => match ctx.cache.unlock(new_vek).await {
                Ok(()) => true,
                Err(err) => {
                    tracing::warn!(
                        target: "shikomi_daemon::ipc::v2_handler",
                        "rekey: cache.unlock failed after atomic save: {err:?}; \
                         responding with cache_relocked=false (Pegasus 工程5)"
                    );
                    false
                }
            },
            Err(err) => {
                tracing::warn!(
                    target: "shikomi_daemon::ipc::v2_handler",
                    "rekey: cache re-unlock failed after atomic save: {err:?}; \
                     responding with cache_relocked=false (Pegasus 工程5)"
                );
                false
            }
        }
    };

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
        cache_relocked,
    }
}

fn secret_bytes_to_string(secret: &SerializableSecretBytes) -> String {
    secret.to_lossy_string_for_handler()
}
