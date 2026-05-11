//! ホットキー操作系 Tauri Commands。
//!
//! | コマンド | IpcRequest | 正常応答 |
//! |---|---|---|
//! | `assign_hotkey` | `EditRecord { hotkey: Some, clear_hotkey: false }` | `Edited { id }` |
//! | `remove_hotkey` | `EditRecord { clear_hotkey: true }` | `Edited { id }` |
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md REQ-IPC-05〜06
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §3.5〜3.6

use serde::Serialize;
use shikomi_core::ipc::{IpcRequest, IpcResponse};
use shikomi_core::RecordId;
use tauri::State;
use time::OffsetDateTime;

use crate::ipc_client::error::GUIError;
use crate::ipc_client::{exec_with_client, AppState};

// ---------------------------------------------------------------------------
// 出力型
// ---------------------------------------------------------------------------

/// ホットキー操作の戻り値。
#[derive(Debug, Serialize)]
pub struct HotkeyOutput {
    /// 対象エントリの ID（UUIDv7 文字列）。
    pub id: String,
}

// ---------------------------------------------------------------------------
// assign_hotkey（REQ-IPC-05）
// ---------------------------------------------------------------------------

/// エントリにホットキーを割り当てる。
///
/// Rust 側バリデーション（R1-GUI-09, R1-GUI-19）:
/// - `combo` が `Ctrl+Alt+[1-9]` 形式以外 → `GUIError::InvalidInput`
///   JS 側セレクタ UI による事前制限とは独立した独自検証（バイパス対策）。
///
/// # Errors
/// `GUIError::InvalidInput`（形式違反・不正 UUID）/
/// `GUIError::Ipc(HotkeyConflict)` /
/// `GUIError::NotConnected` 等。
#[tauri::command]
pub async fn assign_hotkey(
    state: State<'_, AppState>,
    id: String,
    combo: String,
) -> Result<HotkeyOutput, GUIError> {
    // ホットキー形式検証（^Ctrl\+Alt\+[1-9]$）
    validate_hotkey_combo(&combo)?;

    let record_id = RecordId::try_from_str(&id)
        .map_err(|_| GUIError::InvalidInput("invalid record id format".to_owned()))?;

    let now = OffsetDateTime::now_utc();

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::EditRecord {
            id: record_id,
            label: None,
            value: None,
            now,
            hotkey: Some(combo),
            clear_hotkey: false,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Edited { id } => Ok(HotkeyOutput { id: id.to_string() }),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Edited, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// remove_hotkey（REQ-IPC-06）
// ---------------------------------------------------------------------------

/// エントリのホットキーを解除する。
///
/// # Errors
/// `GUIError::InvalidInput`（不正 UUID）/ `GUIError::NotConnected` /
/// `GUIError::Ipc(NotFound)` 等。
#[tauri::command]
pub async fn remove_hotkey(
    state: State<'_, AppState>,
    id: String,
) -> Result<HotkeyOutput, GUIError> {
    let record_id = RecordId::try_from_str(&id)
        .map_err(|_| GUIError::InvalidInput("invalid record id format".to_owned()))?;

    let now = OffsetDateTime::now_utc();

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::EditRecord {
            id: record_id,
            label: None,
            value: None,
            now,
            hotkey: None,
            clear_hotkey: true,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Edited { id } => Ok(HotkeyOutput { id: id.to_string() }),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Edited, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// 内部バリデーション
// ---------------------------------------------------------------------------

/// ホットキーコンボが `Ctrl+Alt+[1-9]` 形式かどうか検証する（R1-GUI-09）。
fn validate_hotkey_combo(combo: &str) -> Result<(), GUIError> {
    // パターン: "Ctrl+Alt+" + 1〜9 の 1 文字（合計 10 文字）
    let valid = combo.starts_with("Ctrl+Alt+")
        && combo.len() == "Ctrl+Alt+".len() + 1
        && combo
            .chars()
            .last()
            .map_or(false, |c| c.is_ascii_digit() && c != '0');

    if valid {
        Ok(())
    } else {
        Err(GUIError::InvalidInput(
            "hotkey must be Ctrl+Alt+[1-9]".to_owned(),
        ))
    }
}
