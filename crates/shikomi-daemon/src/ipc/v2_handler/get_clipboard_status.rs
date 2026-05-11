//! `GetClipboardStatus` IPC ハンドラ（Sub-D #97）。
//!
//! クリップボード自動消去カウントダウンの残秒を返す。
//! `countdown_started_at` が `None` または経過秒が `CLEAR_TIMEOUT_SECS` 以上の場合は
//! `remaining_secs: None` を返す（カウントダウン非アクティブ扱い）。
//!
//! 設計根拠: docs/features/shikomi-gui/system-tray/detailed-design.md §6.2

use shikomi_core::ipc::IpcResponse;
use shikomi_core::CLEAR_TIMEOUT_SECS;
use shikomi_infra::persistence::VaultRepository;
use std::time::Instant;

use crate::ipc::v2_handler::V2Context;

/// `GetClipboardStatus` リクエストを処理し `ClipboardStatus` レスポンスを返す。
///
/// 残秒計算ロジック（detailed-design.md §6.2）:
///
/// | 条件 | `remaining_secs` |
/// |------|-----------------|
/// | `countdown_started_at == None` | `None` |
/// | `elapsed >= CLEAR_TIMEOUT_SECS` | `None`（タイマー発火済み扱い） |
/// | `elapsed < CLEAR_TIMEOUT_SECS` | `Some(CLEAR_TIMEOUT_SECS - elapsed)` |
pub async fn handle_get_clipboard_status<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
) -> IpcResponse {
    let remaining_secs = calc_remaining_from_context(ctx).await;
    IpcResponse::ClipboardStatus { remaining_secs }
}

/// `countdown_started_at` から残秒を計算する。
fn calc_remaining(started_at: Instant, now: Instant) -> Option<u64> {
    let elapsed_secs = now.duration_since(started_at).as_secs();
    if elapsed_secs >= CLEAR_TIMEOUT_SECS {
        None
    } else {
        Some(CLEAR_TIMEOUT_SECS - elapsed_secs)
    }
}

async fn calc_remaining_from_context<R: VaultRepository + ?Sized>(
    ctx: &V2Context<'_, R>,
) -> Option<u64> {
    let guard = ctx.countdown_started_at.lock().await;
    match *guard {
        None => None,
        Some(started_at) => calc_remaining(started_at, Instant::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::calc_remaining;
    use std::time::{Duration, Instant};

    #[test]
    fn returns_none_when_elapsed_exceeds_timeout() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(31);
        assert_eq!(calc_remaining(started_at, now), None);
    }

    #[test]
    fn returns_none_when_elapsed_equals_timeout() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(30);
        assert_eq!(calc_remaining(started_at, now), None);
    }

    #[test]
    fn returns_remaining_secs_when_active() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(10);
        assert_eq!(calc_remaining(started_at, now), Some(20));
    }

    #[test]
    fn returns_full_timeout_when_just_started() {
        let now = Instant::now();
        let started_at = now;
        assert_eq!(calc_remaining(started_at, now), Some(30));
    }

    // TC-TRAY-UT09: elapsed=29s → Some(1)（最小正値境界。CLEAR_TIMEOUT_SECS=30 の前後境界）
    #[test]
    fn returns_one_when_29_seconds_elapsed() {
        let now = Instant::now();
        let started_at = now - Duration::from_secs(29);
        assert_eq!(calc_remaining(started_at, now), Some(1));
    }
}
