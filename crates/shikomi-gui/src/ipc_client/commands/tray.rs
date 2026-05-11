//! クリップボードカウントダウン Tauri Command（Sub-D #97）。
//!
//! | コマンド | IpcRequest | 正常応答 |
//! |---|---|---|
//! | `get_clipboard_countdown` | `GetClipboardStatus` | `ClipboardCountdownResult { remaining_secs }` |
//!
//! daemon 未接続（`AppState == None`）の場合は `{ remaining_secs: null }` を即返す。
//! エラーを SolidJS に伝搬しない（countdown polling がエラーパネルを誘発しないよう設計する）。
//!
//! 設計根拠: docs/features/shikomi-gui/system-tray/basic-design.md §3.3
//!          docs/features/shikomi-gui/system-tray/detailed-design.md §5

use serde::Serialize;
use shikomi_core::ipc::{IpcRequest, IpcResponse};
use tauri::State;

use crate::ipc_client::{round_trip_checked, AppState};

// ---------------------------------------------------------------------------
// 出力型
// ---------------------------------------------------------------------------

/// `get_clipboard_countdown` の戻り値。
///
/// `remaining_secs: null` → カウントダウン非アクティブ。
/// `remaining_secs: n` → 残 n 秒でカウントダウン中。
///
/// 設計根拠: docs/features/shikomi-gui/system-tray/detailed-design.md §5.1
#[derive(Debug, Serialize)]
pub struct ClipboardCountdownResult {
    /// クリップボード自動消去までの残秒（`null` は非アクティブ）。
    pub remaining_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// get_clipboard_countdown（REQ-TRAY-04）
// ---------------------------------------------------------------------------

/// クリップボード自動消去カウントダウン残秒を取得する。
///
/// `GUIError` を返さない（detailed-design.md §5.2 サイレントフォールバック設計）。
/// daemon 未接続または IPC エラーは `remaining_secs: null` として扱う。
///
/// 設計根拠: docs/features/shikomi-gui/system-tray/basic-design.md §3.3
#[tauri::command]
pub async fn get_clipboard_countdown(
    state: State<'_, AppState>,
) -> Result<ClipboardCountdownResult, ()> {
    let request = IpcRequest::GetClipboardStatus;
    let remaining_secs = match round_trip_checked(&state, &request).await {
        Ok(IpcResponse::ClipboardStatus { remaining_secs }) => remaining_secs,
        Ok(other) => {
            tracing::debug!(
                variant = other.variant_name(),
                "get_clipboard_countdown: unexpected IPC response"
            );
            None
        }
        Err(e) => {
            tracing::debug!(error = %e, "get_clipboard_countdown: IPC error (silent fallback)");
            None
        }
    };
    Ok(ClipboardCountdownResult { remaining_secs })
}
