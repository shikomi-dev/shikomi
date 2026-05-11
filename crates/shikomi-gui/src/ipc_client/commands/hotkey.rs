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
use crate::ipc_client::{round_trip_checked, AppState};

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

    let request = IpcRequest::EditRecord {
        id: record_id,
        label: None,
        value: None,
        now,
        hotkey: Some(combo),
        clear_hotkey: false,
    };
    match round_trip_checked(&state, &request).await? {
        IpcResponse::Edited { id } => Ok(HotkeyOutput { id: id.to_string() }),
        IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
        other => Err(GUIError::UnexpectedResponse(format!(
            "expected Edited, got {}",
            other.variant_name()
        ))),
    }
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

    let request = IpcRequest::EditRecord {
        id: record_id,
        label: None,
        value: None,
        now,
        hotkey: None,
        clear_hotkey: true,
    };
    match round_trip_checked(&state, &request).await? {
        IpcResponse::Edited { id } => Ok(HotkeyOutput { id: id.to_string() }),
        IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
        other => Err(GUIError::UnexpectedResponse(format!(
            "expected Edited, got {}",
            other.variant_name()
        ))),
    }
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
            .is_some_and(|c| c.is_ascii_digit() && c != '0');

    if valid {
        Ok(())
    } else {
        Err(GUIError::InvalidInput(
            "hotkey must be Ctrl+Alt+[1-9]".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::validate_hotkey_combo;

    // TC-GUI-IPC-UT03 — 境界値: Ctrl+Alt+0 は範囲外
    #[test]
    fn ut03_ctrl_alt_0_is_rejected() {
        assert!(
            validate_hotkey_combo("Ctrl+Alt+0").is_err(),
            "Ctrl+Alt+0 (digit 0) must be rejected"
        );
    }

    // TC-GUI-IPC-UT04 — 大文字小文字: ctrl+alt+1 は拒否
    #[test]
    fn ut04_lowercase_ctrl_alt_is_rejected() {
        assert!(
            validate_hotkey_combo("ctrl+alt+1").is_err(),
            "lowercase combo must be rejected"
        );
    }

    // TC-GUI-IPC-UT05 — 境界値最小: Ctrl+Alt+1 は受理
    #[test]
    fn ut05_ctrl_alt_1_is_accepted() {
        assert!(
            validate_hotkey_combo("Ctrl+Alt+1").is_ok(),
            "Ctrl+Alt+1 (minimum valid) must be accepted"
        );
    }

    // TC-GUI-IPC-UT06 — 境界値最大: Ctrl+Alt+9 は受理
    #[test]
    fn ut06_ctrl_alt_9_is_accepted() {
        assert!(
            validate_hotkey_combo("Ctrl+Alt+9").is_ok(),
            "Ctrl+Alt+9 (maximum valid) must be accepted"
        );
    }

    // 追加境界値: "Ctrl+Alt+" only (missing digit) → rejected
    #[test]
    fn ut03b_ctrl_alt_no_digit_is_rejected() {
        assert!(validate_hotkey_combo("Ctrl+Alt+").is_err());
    }

    // 追加境界値: "Ctrl+Alt+10" (two digits) → rejected
    #[test]
    fn ut03c_ctrl_alt_two_digits_is_rejected() {
        assert!(validate_hotkey_combo("Ctrl+Alt+10").is_err());
    }
}
