//! IPC リクエスト → レスポンスの pure 写像。
//!
//! ハンドラは I/O / `Mutex` ロックを行わない（呼び出し側 = `IpcServer::handle_connection`
//! の責務）。`&dyn VaultRepository` と `&mut Vault` を引数で受ける。
//!
//! ## サブモジュール構成
//!
//! 旧 `handler.rs` 単一ファイルが 600 行を超えてレビュー却下対象（500 行ルール）となったため、
//! variant ごと + 写像層に分割した（外部レビュー §UX-2 対応）:
//!
//! - `list` / `add` / `edit` / `remove`: 各 `IpcRequest` variant の処理
//! - `error_mapping`: `DomainError` / `PersistenceError` → `IpcErrorCode` 写像
//! - `tests`: pure 写像のユニットテスト群（mock repo + fixed vault）
//!
//! 設計根拠: docs/features/daemon-ipc/detailed-design/daemon-runtime.md
//! §`handler::handle_request` 関数 / §Result → IpcErrorCode 写像の規約

mod add;
mod edit;
mod error_mapping;
mod list;
mod remove;

#[cfg(test)]
mod tests;

use shikomi_core::ipc::{IpcErrorCode, IpcRequest, IpcResponse};
use shikomi_core::Vault;
use shikomi_infra::persistence::VaultRepository;

/// `IpcRequest` を `IpcResponse` に写像する pure 関数。
pub fn handle_request<R: VaultRepository + ?Sized>(
    repo: &R,
    vault: &mut Vault,
    request: IpcRequest,
) -> IpcResponse {
    match request {
        IpcRequest::Handshake { .. } => {
            // ハンドシェイクは別経路（`handshake::negotiate`）で扱う。
            // 防御的に Internal を返す。
            IpcResponse::Error(IpcErrorCode::Internal {
                reason: "handshake should be handled separately".to_owned(),
            })
        }
        IpcRequest::ListRecords => list::handle_list(vault),
        IpcRequest::AddRecord {
            kind,
            label,
            value,
            now,
        } => add::handle_add(repo, vault, kind, label, value, now),
        IpcRequest::EditRecord {
            id,
            label,
            value,
            now,
        } => edit::handle_edit(repo, vault, id, label, value, now),
        IpcRequest::RemoveRecord { id } => remove::handle_remove(repo, vault, id),
        // `#[non_exhaustive]` 防御的 wildcard（後続 V2 variant に対する多層防御、
        // cross-crate `non_exhaustive` のため明示分岐は実現不能）。
        _ => IpcResponse::Error(IpcErrorCode::Internal {
            reason: "unknown request variant".to_owned(),
        }),
    }
}
