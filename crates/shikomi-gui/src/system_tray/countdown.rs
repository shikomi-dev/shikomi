//! クリップボード消去カウントダウンのポーリングタスク（Sub-D #97）。
//!
//! `run()` は 1 秒ごとに `get_clipboard_countdown` IPC を呼び出し、残秒に応じて
//! トレイアイコンのツールチップを更新する。
//!
//! 設計根拠: docs/features/shikomi-gui/system-tray/detailed-design.md §4
//!          docs/features/shikomi-gui/system-tray/detailed-design.md §9

use std::time::{Duration, Instant};

use shikomi_core::ipc::{IpcRequest, IpcResponse};
use shikomi_core::CLEAR_TIMEOUT_SECS;
use tauri::tray::TrayIconId;
use tauri::{AppHandle, Manager as _, Runtime};

use crate::ipc_client::{round_trip_checked, AppState};

// ---------------------------------------------------------------------------
// run — ポーリングループ（REQ-TRAY-05）
// ---------------------------------------------------------------------------

/// カウントダウンポーリングタスクのエントリポイント。
///
/// `setup()` から `tauri::async_runtime::spawn` で起動する。
/// トレイアイコンが見つからなくなった時点（アプリ終了途中）でループを終了する。
/// エラーは `tracing::warn!` でログし、タスクは継続する（best-effort）。
///
/// 設計根拠: docs/features/shikomi-gui/system-tray/detailed-design.md §4.3
pub async fn run<R: Runtime>(app_handle: AppHandle<R>, tray_id: TrayIconId) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let remaining = poll_remaining(&app_handle).await;

        let Some(tray) = app_handle.tray_by_id(&tray_id) else {
            // トレイが破棄されている（アプリ終了途中）→ ループを安全に終了する（REQ-TRAY-05 §4.3）
            tracing::debug!("countdown: tray icon not found; exiting loop");
            break;
        };

        let text = tooltip_text(remaining);
        if let Err(e) = tray.set_tooltip(Some(text.as_str())) {
            tracing::warn!(error = %e, "countdown: set_tooltip failed (best-effort)");
        }
    }
}

// ---------------------------------------------------------------------------
// poll_remaining — IPC 呼び出しで残秒を取得（§4.2 エラー方針）
// ---------------------------------------------------------------------------

/// `AppState` 経由で daemon に `GetClipboardStatus` を問い合わせ、残秒を返す。
///
/// - `AppState` が `None`（daemon 未接続）→ `None`（カウントダウン非アクティブ扱い）
/// - IPC 通信エラー → `tracing::debug!` のみ、前回値の代わりに `None` を返す
///   （§4.2: ツールチップのリセットより無変化を優先する必要があれば上位で対応）
async fn poll_remaining<R: Runtime>(app: &AppHandle<R>) -> Option<u64> {
    let state = app.state::<AppState>();
    match round_trip_checked(&state, &IpcRequest::GetClipboardStatus).await {
        Ok(IpcResponse::ClipboardStatus { remaining_secs }) => remaining_secs,
        Ok(other) => {
            tracing::debug!(
                variant = other.variant_name(),
                "countdown: unexpected IPC response"
            );
            None
        }
        Err(e) => {
            tracing::debug!(error = %e, "countdown: IPC error (best-effort)");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// tooltip_text — ツールチップ文字列生成（§9 / §10）
// ---------------------------------------------------------------------------

/// 残秒から表示するツールチップ文字列を返す。
///
/// | 状態 | 文字列 |
/// |------|--------|
/// | 非アクティブ（`None` または `0`） | `"shikomi"` |
/// | カウントダウン中（`Some(n)`, n > 0） | `"shikomi — クリップボードを自動消去まで {n} 秒"` |
///
/// 設計根拠: docs/features/shikomi-gui/system-tray/detailed-design.md §10
fn tooltip_text(remaining: Option<u64>) -> String {
    match remaining {
        Some(n) if n > 0 => format!("shikomi — クリップボードを自動消去まで {n} 秒"),
        _ => "shikomi".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// calc_remaining — 残秒計算純粋関数（§9、テスト用）
// ---------------------------------------------------------------------------

/// クリップボード投入開始時刻から残秒を計算する。
///
/// `now` を引数注入することで `Instant::now()` 依存を排除しテスト可能にする（§9）。
///
/// | 条件 | 戻り値 |
/// |------|--------|
/// | `elapsed >= CLEAR_TIMEOUT_SECS` | `None` |
/// | `elapsed < CLEAR_TIMEOUT_SECS` | `Some(CLEAR_TIMEOUT_SECS - elapsed)` |
fn calc_remaining(started_at: Instant, now: Instant) -> Option<u64> {
    let elapsed_secs = now.duration_since(started_at).as_secs();
    if elapsed_secs >= CLEAR_TIMEOUT_SECS {
        None
    } else {
        Some(CLEAR_TIMEOUT_SECS - elapsed_secs)
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // TC-TRAY-UT01: tooltip_text — カウントダウン中（残 15 秒）
    #[test]
    fn ut01_tooltip_text_countdown_active() {
        let text = tooltip_text(Some(15));
        assert_eq!(text, "shikomi — クリップボードを自動消去まで 15 秒");
    }

    // TC-TRAY-UT02: tooltip_text — 非アクティブ（None）
    #[test]
    fn ut02_tooltip_text_inactive_none() {
        let text = tooltip_text(None);
        assert_eq!(text, "shikomi");
    }

    // TC-TRAY-UT02b: tooltip_text — 非アクティブ（0 秒）
    #[test]
    fn ut02b_tooltip_text_inactive_zero() {
        let text = tooltip_text(Some(0));
        assert_eq!(text, "shikomi");
    }

    // TC-TRAY-UT03: calc_remaining — 残 20 秒
    #[test]
    fn ut03_calc_remaining_active() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(10);
        assert_eq!(calc_remaining(started_at, now), Some(20));
    }

    // TC-TRAY-UT04: calc_remaining — タイムアウト超過
    #[test]
    fn ut04_calc_remaining_expired() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(31);
        assert_eq!(calc_remaining(started_at, now), None);
    }

    // TC-TRAY-UT05: calc_remaining — 境界: elapsed == CLEAR_TIMEOUT_SECS（30 秒 = None）
    #[test]
    fn ut05_calc_remaining_boundary_exactly_timeout() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(30);
        assert_eq!(calc_remaining(started_at, now), None);
    }

    // TC-TRAY-UT06: calc_remaining — 直後（elapsed ≈ 0）
    #[test]
    fn ut06_calc_remaining_just_started() {
        let now = Instant::now();
        // elapsed = 0 → remaining = 30
        assert_eq!(calc_remaining(now, now), Some(30));
    }

    // TC-TRAY-UT07: tooltip_text — 境界最小（残 1 秒）
    #[test]
    fn ut07_tooltip_text_one_second_remaining() {
        let text = tooltip_text(Some(1));
        assert_eq!(text, "shikomi — クリップボードを自動消去まで 1 秒");
    }

    // TC-TRAY-UT09: calc_remaining — elapsed=29s → Some(1)（最小正値境界）
    #[test]
    fn ut09_calc_remaining_one_second_before_timeout() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(29);
        // elapsed=29 < CLEAR_TIMEOUT_SECS(30) → remaining = 1（最小正値境界）
        assert_eq!(calc_remaining(started_at, now), Some(1));
    }
}
