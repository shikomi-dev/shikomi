//! Sub-E (#43) IPC V2 ハンドラ群 + handshake 許可リスト + ClientState 管理。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §IPC V2 拡張 / §C-28 / §C-29
//!
//! ## モジュール構成
//!
//! - 本 [`mod.rs`]: `ClientState` / `check_request_allowed` / `V2Context`
//!   / `dispatch_v2` (V1+V2 統合 dispatch)
//! - [`unlock`]: `handle_unlock` (パスワード/recovery 経路、`UnlockBackoff` 連携)
//! - [`lock`]: `handle_lock` (`VekCache::lock`)
//! - [`change_password`]: `handle_change_password` (`VaultMigration::change_password`)
//! - [`rotate_recovery`]: `handle_rotate_recovery` (`rekey_with_recovery_rotation`、
//!   recovery 24 語 1 度限り開示)
//! - [`rekey`]: `handle_rekey` (rotate_recovery と内部実装同一、外向き名称分離)
//! - [`error_mapping`]: `MigrationError → IpcErrorCode` 写像 (§C-27)

pub mod change_password;
pub mod error_mapping;
pub mod lock;
pub mod rekey;
pub mod rotate_recovery;
pub mod unlock;

use shikomi_core::ipc::{IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse};
use shikomi_core::Vault;
use shikomi_infra::persistence::vault_migration::VaultMigration;
use shikomi_infra::persistence::VaultRepository;
use tokio::sync::Mutex;

use crate::backoff::UnlockBackoff;
use crate::cache::VekCache;

// -------------------------------------------------------------------
// ClientState
// -------------------------------------------------------------------

/// IPC クライアントの handshake 状態 (`#[non_exhaustive]`)。
///
/// 設計書 §C-29 handshake 必須契約: `PreHandshake` で `Handshake` 以外の variant を
/// 受信した場合は接続即切断 + `IpcErrorCode::ProtocolDowngrade` 返却。
///
/// 既存 `handshake::negotiate` が `Ok(())` を返した時点で `Handshake { version }` 状態に
/// 遷移する設計だが、本 enum は **PR レビュー段階で C-29 を構造化** する明示的な型として
/// 導入する。dispatch loop が状態を持ち、`PreHandshake` で他 variant が来たら拒否する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientState {
    /// 接続直後、handshake 未完了。`IpcRequest::Handshake` 以外は全て拒否される。
    PreHandshake,
    /// handshake 完了、client_version 確定。後続の variant は許可リスト判定の対象。
    Handshake {
        /// クライアントが申告したプロトコルバージョン。
        version: IpcProtocolVersion,
    },
}

// -------------------------------------------------------------------
// check_request_allowed
// -------------------------------------------------------------------

/// **handshake 許可リスト方式** (§C-28) + handshake 必須 (§C-29) の統合検証。
///
/// 設計書 §IPC V2 拡張 「許可リスト具体仕様」表:
///
/// | `client_version` | 受理する `IpcRequest` variant |
/// | --- | --- |
/// | `V1` | `Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord` |
/// | `V2` | V1 サブセット 5 件 + V2 新 variant 5 件 |
///
/// `IpcRequest::is_v2_only()` で V2 専用 variant を判定し、`(client_version, request)`
/// の組合せが許可リスト外なら `IpcErrorCode::ProtocolDowngrade` を返す。
///
/// # Errors
///
/// - `PreHandshake` 状態で `Handshake` 以外を受信: `ProtocolDowngrade` (C-29)
/// - V1 client が V2 専用 variant を送信: `ProtocolDowngrade` (C-28)
/// - `Unknown` バージョン: `ProtocolDowngrade` (fail-secure)
pub fn check_request_allowed(
    state: ClientState,
    request: &IpcRequest,
) -> Result<(), IpcErrorCode> {
    match state {
        ClientState::PreHandshake => {
            // C-29: handshake 前は Handshake variant のみ許可
            if matches!(request, IpcRequest::Handshake { .. }) {
                Ok(())
            } else {
                Err(IpcErrorCode::ProtocolDowngrade)
            }
        }
        ClientState::Handshake { version } => match version {
            IpcProtocolVersion::V1 => {
                // C-28: V1 client は V2 専用 variant を送信不可
                if request.is_v2_only() {
                    Err(IpcErrorCode::ProtocolDowngrade)
                } else {
                    Ok(())
                }
            }
            IpcProtocolVersion::V2 => {
                // V2 client は V1 サブセット + V2 新 variant の全てを送信可能
                Ok(())
            }
            IpcProtocolVersion::Unknown => {
                // fail-secure: 未知バージョンは全拒否
                Err(IpcErrorCode::ProtocolDowngrade)
            }
        },
    }
}

// -------------------------------------------------------------------
// V2Context
// -------------------------------------------------------------------

/// V2 dispatch のための依存集約。
///
/// daemon の composition root が構築し、各 IPC 接続のハンドラループに参照を渡す。
/// `Mutex<UnlockBackoff>` は connection 跨ぎで失敗カウンタを共有するため Arc<Mutex<_>>
/// で包む (composition root が `Arc::clone` で connection 数分配布)。
pub struct V2Context<'a, R: VaultRepository + ?Sized> {
    /// vault 永続化リポジトリ (V1 ハンドラ互換用)。
    pub repo: &'a R,
    /// 共有 `Vault` (V1 ハンドラの read/write 用)。
    pub vault: &'a Mutex<Vault>,
    /// VEK キャッシュ (`with_vek` クロージャ + `lock`/`unlock` 遷移)。
    pub cache: &'a VekCache,
    /// 連続 unlock 失敗の backoff (C-26、`Crypto(WrongPassword)` のみカウント)。
    pub backoff: &'a Mutex<UnlockBackoff>,
    /// Sub-D `VaultMigration` service (Sub-D Rev6 メソッド経由)。
    pub migration: &'a VaultMigration<'a>,
}

// -------------------------------------------------------------------
// dispatch_v2
// -------------------------------------------------------------------

/// `IpcRequest` を V1 + V2 を統合した経路で `IpcResponse` に写像する。
///
/// 1. `check_request_allowed(state, &request)` で許可リスト検証 (C-28 / C-29)
/// 2. V2 専用 variant は `v2_handler` 配下の各ハンドラに dispatch
/// 3. V1 read/write variant は **`Locked` 状態で拒否** (C-22)、`Unlocked` のみ既存
///    `handler::handle_request` に委譲
/// 4. `Handshake` variant は呼出側 `negotiate` で扱われる前提、本関数では Internal
///
/// # 引数
/// - `ctx`: 共有依存 (`V2Context`)
/// - `state`: 現在の `ClientState` (handshake 完了後 V1/V2 確定)
/// - `request`: 受信した `IpcRequest`
pub async fn dispatch_v2<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
    state: ClientState,
    request: IpcRequest,
) -> IpcResponse {
    // C-28 / C-29 許可リスト + handshake 必須検証
    if let Err(code) = check_request_allowed(state, &request) {
        return IpcResponse::Error(code);
    }

    match request {
        // ---------- V2 専用 variant ----------
        IpcRequest::Unlock {
            master_password,
            recovery,
        } => unlock::handle_unlock(ctx, master_password, recovery).await,
        IpcRequest::Lock => lock::handle_lock(ctx).await,
        IpcRequest::ChangePassword { old, new } => {
            change_password::handle_change_password(ctx, old, new).await
        }
        IpcRequest::RotateRecovery { master_password } => {
            rotate_recovery::handle_rotate_recovery(ctx, master_password).await
        }
        IpcRequest::Rekey { master_password } => rekey::handle_rekey(ctx, master_password).await,

        // ---------- V1 read/write variant: Locked 時に拒否 (C-22) ----------
        IpcRequest::ListRecords
        | IpcRequest::AddRecord { .. }
        | IpcRequest::EditRecord { .. }
        | IpcRequest::RemoveRecord { .. } => {
            if !ctx.cache.is_unlocked().await {
                return IpcResponse::Error(IpcErrorCode::VaultLocked);
            }
            // V1 ハンドラに委譲 (既存実装は無変更、`Vault` への mut 参照経由)。
            let mut vault = ctx.vault.lock().await;
            super::handler::handle_request(ctx.repo, &mut vault, request)
        }

        // ---------- Handshake は別経路 ----------
        IpcRequest::Handshake { .. } => IpcResponse::Error(IpcErrorCode::Internal {
            reason: "handshake should be handled separately".to_owned(),
        }),

        // ---------- 防御的: 将来拡張 variant ----------
        _ => IpcResponse::Error(IpcErrorCode::Internal {
            reason: "unknown request variant".to_owned(),
        }),
    }
}

// -------------------------------------------------------------------
// tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::ipc::IpcRequest;

    #[test]
    fn pre_handshake_allows_only_handshake() {
        let state = ClientState::PreHandshake;
        let handshake = IpcRequest::Handshake {
            client_version: IpcProtocolVersion::V2,
        };
        assert!(check_request_allowed(state, &handshake).is_ok());

        let list = IpcRequest::ListRecords;
        assert!(matches!(
            check_request_allowed(state, &list),
            Err(IpcErrorCode::ProtocolDowngrade)
        ));

        let lock = IpcRequest::Lock;
        assert!(matches!(
            check_request_allowed(state, &lock),
            Err(IpcErrorCode::ProtocolDowngrade)
        ));
    }

    #[test]
    fn v1_client_rejects_v2_only_variants() {
        let state = ClientState::Handshake {
            version: IpcProtocolVersion::V1,
        };
        // V1 サブセットは OK
        assert!(check_request_allowed(state, &IpcRequest::ListRecords).is_ok());
        // V2 専用 variant は拒否
        assert!(matches!(
            check_request_allowed(state, &IpcRequest::Lock),
            Err(IpcErrorCode::ProtocolDowngrade)
        ));
    }

    #[test]
    fn v2_client_accepts_both_v1_and_v2_variants() {
        let state = ClientState::Handshake {
            version: IpcProtocolVersion::V2,
        };
        assert!(check_request_allowed(state, &IpcRequest::ListRecords).is_ok());
        assert!(check_request_allowed(state, &IpcRequest::Lock).is_ok());
    }

    #[test]
    fn unknown_version_rejects_all() {
        let state = ClientState::Handshake {
            version: IpcProtocolVersion::Unknown,
        };
        assert!(matches!(
            check_request_allowed(state, &IpcRequest::ListRecords),
            Err(IpcErrorCode::ProtocolDowngrade)
        ));
    }
}
