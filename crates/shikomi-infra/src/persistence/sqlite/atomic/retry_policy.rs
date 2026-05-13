//! rename retry の振る舞いを抽象化する trait と実装群。

use std::time::Duration;

// -------------------------------------------------------------------
// RetryPolicy trait
// -------------------------------------------------------------------

/// rename retry の振る舞いを抽象化する trait（Phase 8、Issue #73）。
///
/// `cfg(windows)` 限定 rename retry で使用する。`NoSleepRetryPolicy` を差し込むことで
/// テスト時の実際の sleep を排除し CI を高速化する（`./classes.md` §3.4 参照）。
///
/// Unix では `finalize` 内で `should_retry` / `sleep_duration` は一切呼ばれない
/// （Unix の rename は即 fail fast、retry なし）。
///
/// `cfg(windows)` 限定ロジックで使用するため非 Windows ビルドでは dead_code になるが意図的。
#[allow(dead_code)]
pub(crate) trait RetryPolicy {
    /// 最大 retry 回数。超過したら `AtomicWriteFailed { stage: Rename }` で return。
    fn max_attempts(&self) -> u32;

    /// `attempt` 番目（1-indexed）の retry 前 sleep 量を返す。jitter を内部で生成してよい。
    fn sleep_duration(&self, attempt: u32) -> Duration;

    /// OS エラーコードが一過性エラーか否かを判定する。
    ///
    /// `cfg(windows)`:
    /// - `ERROR_ACCESS_DENIED (5)` / `ERROR_SHARING_VIOLATION (32)` /
    ///   `ERROR_LOCK_VIOLATION (33)` → `true`、それ以外 → `false`
    ///
    /// `cfg(not(windows))`: 常に `false`。
    fn should_retry(&self, raw_os_error: i32) -> bool;
}

// -------------------------------------------------------------------
// ExponentialBackoffRetryPolicy
// -------------------------------------------------------------------

/// 指数バックオフ retry の production default 実装。
///
/// `max_attempts = 5`、`base_ms = 50`、`jitter = ±25ms`（`OsRng` 一様乱数）。
///
/// | attempt | 中央値 | range       | 累積中央値 |
/// |---------|--------|-------------|----------|
/// | 1       | 50ms   | 25〜75ms   | 50ms     |
/// | 2       | 100ms  | 75〜125ms  | 150ms    |
/// | 3       | 200ms  | 175〜225ms | 350ms    |
/// | 4       | 400ms  | 375〜425ms | 750ms    |
/// | 5       | 800ms  | 775〜825ms | 1550ms   |
///
/// 最悪 ~1675ms / 平均 ~1550ms。SSoT:
/// `docs/features/vault-persistence/basic-design/security.md` §jitter。
pub(crate) struct ExponentialBackoffRetryPolicy;

impl Default for ExponentialBackoffRetryPolicy {
    fn default() -> Self {
        Self
    }
}

impl RetryPolicy for ExponentialBackoffRetryPolicy {
    fn max_attempts(&self) -> u32 {
        5
    }

    fn sleep_duration(&self, attempt: u32) -> Duration {
        #[cfg(windows)]
        {
            use rand_core::{OsRng, RngCore};

            const BASE_MS: u64 = 50;
            const JITTER_HALF_RANGE_MS: u64 = 25;
            // 0..=50 を一様抽選後 -25 シフトで [-25, +25]
            // HALF_RANGE_MS ≤ 127 の範囲で u8 へのキャストは安全
            const JITTER_RANGE: u8 = (JITTER_HALF_RANGE_MS * 2 + 1) as u8;

            let mut buf = [0u8; 1];
            OsRng.fill_bytes(&mut buf);
            let jitter_pos = u64::from(buf[0] % JITTER_RANGE);
            // attempt は 1..=max_attempts(=5) なので左シフト overflow なし
            let multiplier: u64 = 1u64 << (attempt.saturating_sub(1));
            let center_ms = BASE_MS.saturating_mul(multiplier);
            let delay_ms = center_ms + jitter_pos - JITTER_HALF_RANGE_MS;
            Duration::from_millis(delay_ms)
        }
        #[cfg(not(windows))]
        {
            let _ = attempt;
            Duration::ZERO
        }
    }

    fn should_retry(&self, raw_os_error: i32) -> bool {
        #[cfg(windows)]
        {
            matches!(raw_os_error, 5 | 32 | 33)
        }
        #[cfg(not(windows))]
        {
            let _ = raw_os_error;
            false
        }
    }
}

// -------------------------------------------------------------------
// NoSleepRetryPolicy（テスト専用）
// -------------------------------------------------------------------

/// テスト専用 `RetryPolicy`（sleep なし・CI 高速化）。
///
/// `sleep_duration` は常に `Duration::ZERO`。`should_retry` は
/// `ExponentialBackoffRetryPolicy` と同じ判定ロジックを使用。
/// TC-I29 / TC-I29-B の retry 発火テストで実際の sleep を排除する（`./classes.md` §3.4）。
#[cfg(test)]
pub(crate) struct NoSleepRetryPolicy {
    pub(crate) max_attempts: u32,
}

#[cfg(test)]
impl RetryPolicy for NoSleepRetryPolicy {
    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    fn sleep_duration(&self, _attempt: u32) -> Duration {
        Duration::ZERO
    }

    fn should_retry(&self, raw_os_error: i32) -> bool {
        #[cfg(windows)]
        {
            matches!(raw_os_error, 5 | 32 | 33)
        }
        #[cfg(not(windows))]
        {
            let _ = raw_os_error;
            false
        }
    }
}
