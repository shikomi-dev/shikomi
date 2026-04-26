//! `UnlockBackoff` — REQ-S11 連続 unlock 失敗 5 回で指数バックオフ発動 (C-26)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §`UnlockBackoff`
//!
//! ## カウント対象
//!
//! 設計書 §F-E1 step 4 服部指摘契約: `MigrationError::Crypto(CryptoError::WrongPassword)`
//! の **本 variant のみ** を `record_failure` でカウント。他の `Crypto(_)` variant
//! (`AeadTagMismatch` / `NonceLimitExceeded` / `KdfFailed` / `InvalidMnemonic`) は
//! **明示的に backoff 対象外**:
//!
//! - (a) `AeadTagMismatch` で backoff 発動すると **L2 攻撃者が vault.db を 5 回連続
//!       破損 → 正規ユーザの unlock を DoS** する経路を開く
//! - (b) ディスク破損 / 実装バグでも 5 回再試行で誤バックオフ発動
//! - (c) backoff の本来目的は **パスワード違いに対する brute force レート制限**、
//!       それ以外のエラーは即返却で fail fast
//!
//! 呼出側 (Sub-E IPC `unlock` ハンドラ) は `match err: MigrationError` で
//! `Crypto(CryptoError::WrongPassword)` のみ `record_failure` を呼ぶ責務。
//!
//! ## バックオフ計算
//!
//! 設計書 L157: `5 → 30s, 6 → 60s, 7 → 120s, ... 最大 1 時間でクランプ`。
//! 数式: `BASE_BACKOFF * 2^(failures - (TRIGGER_FAILURES - 1))`、`MAX_BACKOFF` で
//! クランプ。`BASE_BACKOFF = 15s` / `TRIGGER_FAILURES = 5` / `MAX_BACKOFF = 1h`。

use std::time::{Duration, Instant};

use thiserror::Error;

// -------------------------------------------------------------------
// BackoffActive
// -------------------------------------------------------------------

/// バックオフ中である旨と次回試行可能までの残秒数を運搬するエラー型。
///
/// `IpcErrorCode::BackoffActive { wait_secs }` への透過用 (MSG-S09 (a) パスワード違い
/// カテゴリ + 待機時間の併記)。`wait_secs` はユーザ表示許容 (Sub-D Rev5 の nonce 数値
/// 非表示とは別経路、攻撃面ゼロ)。
#[derive(Debug, Clone, Copy, Error)]
#[error("unlock blocked by backoff for {wait_secs}s")]
pub struct BackoffActive {
    /// 次回試行可能までの待機秒数 (切り上げ、最低 1)。
    pub wait_secs: u32,
}

// -------------------------------------------------------------------
// UnlockBackoff
// -------------------------------------------------------------------

/// 連続 unlock 失敗 5 回で指数バックオフを発動する状態機械。
///
/// daemon プロセス内のメモリのみ保持、再起動でリセット (設計書 §`UnlockBackoff`
/// Fail-Secure 契約: daemon 再起動を回避策にできるが L1 同ユーザ別プロセスからの
/// brute force は IPC 経路で検出可能なため許容、Sub-0 §脅威モデル §4 L1 残存
/// リスクと整合)。
#[derive(Debug)]
#[non_exhaustive]
pub struct UnlockBackoff {
    failures: u32,
    next_allowed_at: Option<Instant>,
}

impl UnlockBackoff {
    /// バックオフ発動の閾値 (5 回連続失敗、設計書凍結)。
    pub const TRIGGER_FAILURES: u32 = 5;
    /// 最大バックオフ時間 (1 時間でクランプ、設計書凍結)。
    pub const MAX_BACKOFF: Duration = Duration::from_secs(60 * 60);
    /// バックオフのベース時間 (`BASE_BACKOFF * 2^k` で指数増、設計書 L157 「5 → 30s」
    /// 起点から逆算)。
    pub const BASE_BACKOFF: Duration = Duration::from_secs(15);

    /// 初期状態 (失敗 0 回、バックオフ未発動)。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            failures: 0,
            next_allowed_at: None,
        }
    }

    /// 失敗をカウントする (`Crypto(CryptoError::WrongPassword)` のみ呼出側責務)。
    ///
    /// `failures >= TRIGGER_FAILURES` で `next_allowed_at = now + 2^k * BASE_BACKOFF`
    /// (`k = failures - (TRIGGER_FAILURES - 1)`)。`failures = 5 → 30s` / `6 → 60s`
    /// / `7 → 120s` / ... `MAX_BACKOFF` (1h) でクランプ。
    pub fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= Self::TRIGGER_FAILURES {
            // exp = 1 (failures=5), 2 (failures=6), 3 (failures=7), ...
            let exp = self.failures - (Self::TRIGGER_FAILURES - 1);
            // multiplier = 2^exp。u32::MAX より大きい場合は飽和して MAX_BACKOFF でクランプされる。
            let multiplier: u64 = 1u64.checked_shl(exp).unwrap_or(u64::MAX);
            let backoff_secs = Self::BASE_BACKOFF
                .as_secs()
                .saturating_mul(multiplier)
                .min(Self::MAX_BACKOFF.as_secs());
            self.next_allowed_at = Some(Instant::now() + Duration::from_secs(backoff_secs));
        }
    }

    /// unlock 成功で失敗カウンタをリセットする (`record_failure` の対称操作)。
    pub fn record_success(&mut self) {
        self.failures = 0;
        self.next_allowed_at = None;
    }

    /// 現在 backoff 中か確認する。`next_allowed_at` が将来時刻なら `Err(BackoffActive)`、
    /// それ以外 (未発動 or 既に過ぎた) なら `Ok(())`。
    ///
    /// # Errors
    ///
    /// - バックオフ中: `BackoffActive { wait_secs }` (待機残秒数を切り上げで運搬)
    pub fn check(&self) -> Result<(), BackoffActive> {
        match self.next_allowed_at {
            Some(t) if t > Instant::now() => {
                let wait = t.saturating_duration_since(Instant::now());
                let wait_secs: u32 = wait
                    .as_secs()
                    .saturating_add(1)
                    .try_into()
                    .unwrap_or(u32::MAX);
                Err(BackoffActive { wait_secs })
            }
            _ => Ok(()),
        }
    }

    /// 現在の失敗カウンタ値 (テスト / 観測用、本値は IPC に絶対乗せない、
    /// 攻撃面隠蔽契約 §`UnlockBackoff` Fail-Secure 契約)。
    #[doc(hidden)]
    #[must_use]
    pub const fn failures(&self) -> u32 {
        self.failures
    }
}

impl Default for UnlockBackoff {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------
// tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_with_zero_failures_and_no_backoff() {
        let b = UnlockBackoff::new();
        assert_eq!(b.failures(), 0);
        assert!(b.check().is_ok());
    }

    #[test]
    fn record_failure_under_threshold_does_not_trigger_backoff() {
        let mut b = UnlockBackoff::new();
        for _ in 0..(UnlockBackoff::TRIGGER_FAILURES - 1) {
            b.record_failure();
            assert!(
                b.check().is_ok(),
                "backoff must not trigger before {} failures",
                UnlockBackoff::TRIGGER_FAILURES
            );
        }
        assert_eq!(b.failures(), UnlockBackoff::TRIGGER_FAILURES - 1);
    }

    #[test]
    fn record_failure_triggers_backoff_at_threshold() {
        let mut b = UnlockBackoff::new();
        for _ in 0..UnlockBackoff::TRIGGER_FAILURES {
            b.record_failure();
        }
        let active = b.check().expect_err("backoff must trigger at threshold");
        // 5 failures → 30s (BASE 15 * 2^1 = 30)
        // wait_secs は切り上げで 30 or 31 のはず
        assert!(
            active.wait_secs >= 30 && active.wait_secs <= 31,
            "5 failures should yield ~30s backoff, got {}",
            active.wait_secs
        );
    }

    #[test]
    fn backoff_grows_exponentially() {
        let mut b = UnlockBackoff::new();
        for _ in 0..6 {
            b.record_failure();
        }
        // 6 failures → 60s (BASE 15 * 2^2 = 60)
        let active = b.check().expect_err("backoff must trigger");
        assert!(
            active.wait_secs >= 60 && active.wait_secs <= 61,
            "6 failures should yield ~60s backoff, got {}",
            active.wait_secs
        );
    }

    #[test]
    fn backoff_clamps_at_max() {
        let mut b = UnlockBackoff::new();
        // 多数失敗で MAX_BACKOFF (1h = 3600s) クランプを確認
        for _ in 0..30 {
            b.record_failure();
        }
        let active = b.check().expect_err("backoff must trigger");
        assert!(
            active.wait_secs <= 3601,
            "backoff must clamp at MAX (3600s), got {}",
            active.wait_secs
        );
    }

    #[test]
    fn record_success_resets_counter_and_clears_backoff() {
        let mut b = UnlockBackoff::new();
        for _ in 0..UnlockBackoff::TRIGGER_FAILURES {
            b.record_failure();
        }
        assert!(b.check().is_err());
        b.record_success();
        assert_eq!(b.failures(), 0);
        assert!(b.check().is_ok(), "record_success must clear backoff");
    }
}
