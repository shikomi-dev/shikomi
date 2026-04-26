//! `IdleTimer` + `OsLockSignal` trait — Sub-E (#43) VEK lifecycle 自動失効。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §`IdleTimer` / §`OsLockSignal` trait
//!
//! ## 不変条件
//!
//! - **C-24**: アイドル 15min タイムアウトで自動 lock。`IdleTimer` バックグラウンド
//!   task が 60 秒ポーリングで `now - last_used >= 15min` 検出 → `cache.lock()` 呼出
//! - **C-25**: OS スクリーンロック / サスペンド受信で即 lock。`OsLockSignal::next_lock_event`
//!   受信 → `cache.lock()` 呼出
//!
//! ## `OsLockSignal` の Send 戦略
//!
//! 設計書の代替方針 (§`OsLockSignal` trait): AFIT (`async fn in trait`) は
//! Rust 1.75+ で stable だが **`Send` future の自動推論不可** であり `tokio::spawn`
//! で `Send` 要求時に型エラーになる。本実装は **`async-trait` crate** の
//! `#[async_trait]` 属性で `Box<dyn>` 経由動的ディスパッチを採用、`Send + 'static`
//! future を明示保証する。1 回のヒープ確保コストはあるが、テスト容易性 + 既存 stable
//! Rust エディションでの保証された動作を優先する。
//!
//! 代替案 `#[trait_variant::make]` は Rust 1.75+ の AFIT を `Send` future 付きで
//! 自動派生できるが、本リポジトリの安定性を優先して `async-trait` 採用。

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::watch;

use super::vek::VekCache;

// -------------------------------------------------------------------
// LockEvent
// -------------------------------------------------------------------

/// OS シグナル購読の汎用イベント (`#[non_exhaustive]`)。
///
/// 各 OS (macOS / Windows / Linux) 固有のイベント名を本汎用 enum に正規化する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LockEvent {
    /// スクリーンロック検出。
    /// - macOS: `com.apple.screenIsLocked` / `com.apple.screensaver.didstart` 通知
    /// - Windows: `WM_WTSSESSION_CHANGE` (WTS_SESSION_LOCK)
    /// - Linux: D-Bus `org.freedesktop.login1.Session.Lock` シグナル
    ScreenLocked,
    /// システムサスペンド検出。
    /// - macOS: `NSWorkspaceWillSleepNotification`
    /// - Windows: `WM_POWERBROADCAST` (PBT_APMSUSPEND)
    /// - Linux: D-Bus `org.freedesktop.login1.Manager.PrepareForSleep`
    SystemSuspended,
}

// -------------------------------------------------------------------
// OsLockSignal trait
// -------------------------------------------------------------------

/// OS スクリーンロック / サスペンド購読の抽象化 trait (C-25)。
///
/// `Send + Sync + 'static` 制約は `tokio::spawn` で `Box<dyn OsLockSignal>` を
/// 渡すために必要。`async-trait` macro で `Box<dyn Future + Send + 'static>` 化される。
#[async_trait]
pub trait OsLockSignal: Send + Sync + 'static {
    /// 次の `LockEvent` を `await` する。シグナルストリームが終了したら
    /// `LockEvent::ScreenLocked` をフォールバックで返す (fail-secure: 不明状態は
    /// lock 側に倒す)。
    async fn next_lock_event(&mut self) -> LockEvent;
}

// -------------------------------------------------------------------
// IdleTimer
// -------------------------------------------------------------------

/// アイドル 15min タイムアウトで自動 lock する (C-24)。
///
/// 設計書 §`IdleTimer` 動作仕様: `tokio::time::sleep(60s)` をポーリング起点に、
/// `now - last_used >= 15min` を検出したら `cache.lock()` を呼ぶ。バックグラウンド
/// `tokio::spawn` で daemon の lifecycle に紐付ける。
///
/// 設計判断: cancellation token + reset 方式は複雑 → 60 秒ポーリングで KISS 達成
/// (最大遅延 60 秒は受容、Sub-0 凍結の「アイドル 15min」契約と整合)。
pub struct IdleTimer {
    cache: VekCache,
    threshold: Duration,
    poll_interval: Duration,
}

impl IdleTimer {
    /// 設計書凍結のアイドル閾値 (15 分)。
    pub const DEFAULT_THRESHOLD: Duration = Duration::from_secs(15 * 60);
    /// 設計書凍結のポーリング間隔 (60 秒)。
    pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(60);

    /// デフォルト閾値 (15min) + ポーリング間隔 (60s) で構築する。
    #[must_use]
    pub fn new(cache: VekCache) -> Self {
        Self {
            cache,
            threshold: Self::DEFAULT_THRESHOLD,
            poll_interval: Self::DEFAULT_POLL_INTERVAL,
        }
    }

    /// テスト用にカスタム閾値 / ポーリング間隔を指定する。
    #[must_use]
    pub fn with_thresholds(cache: VekCache, threshold: Duration, poll_interval: Duration) -> Self {
        Self {
            cache,
            threshold,
            poll_interval,
        }
    }

    /// `tokio::spawn(timer.run(shutdown_rx))` で起動する run loop。
    ///
    /// shutdown 信号 (`watch::Receiver<bool>`) で停止する。daemon の
    /// composition root が他のバックグラウンド task と同じ `shutdown_rx` を渡す。
    pub async fn run(self, mut shutdown_rx: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                () = tokio::time::sleep(self.poll_interval) => {
                    if let Some(last_used) = self.cache.last_used().await {
                        if last_used.elapsed() >= self.threshold {
                            // アイドル閾値 (15min) 超過 → 自動 lock (C-24)
                            if let Err(err) = self.cache.lock().await {
                                tracing::warn!(target: "shikomi_daemon::cache", "auto-lock failed: {err}");
                            } else {
                                tracing::info!(target: "shikomi_daemon::cache", "auto-lock by idle timeout (>= 15min)");
                            }
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::debug!(target: "shikomi_daemon::cache", "idle timer shutting down");
                        break;
                    }
                }
            }
        }
    }
}

/// `OsLockSignal::next_lock_event` を `tokio::spawn` で監視し、`LockEvent` 受信時に
/// `cache.lock()` を呼ぶ run loop (C-25)。
///
/// `signal: Box<dyn OsLockSignal>` で OS 固有実装を動的ディスパッチする
/// (composition root が `cfg(target_os = ...)` で切替注入する)。
pub async fn run_os_lock_signal_loop(
    cache: VekCache,
    mut signal: Box<dyn OsLockSignal>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            event = signal.next_lock_event() => {
                tracing::info!(
                    target: "shikomi_daemon::cache",
                    "received OS lock event: {event:?} → auto-lock"
                );
                if let Err(err) = cache.lock().await {
                    tracing::warn!(target: "shikomi_daemon::cache", "auto-lock on OS event failed: {err}");
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    tracing::debug!(target: "shikomi_daemon::cache", "OS lock signal loop shutting down");
                    break;
                }
            }
        }
    }
}

// -------------------------------------------------------------------
// MockLockSignal (test only)
// -------------------------------------------------------------------

/// テスト用 `OsLockSignal` 実装。`tokio::sync::mpsc` 経由で手動 `LockEvent` 注入。
///
/// 設計書 §採用方針 表「テスト用」行: integration test で `MockLockSignal` から
/// `LockEvent::ScreenLocked` を注入 → 100ms 以内に `VekCache` が `Locked` 遷移する
/// ことを確認する (C-25 受入条件)。
#[doc(hidden)]
pub struct MockLockSignal {
    rx: tokio::sync::mpsc::Receiver<LockEvent>,
}

impl MockLockSignal {
    /// `MockLockSignal` と注入用 `Sender` のペアを返す。
    #[must_use]
    pub fn new() -> (Self, tokio::sync::mpsc::Sender<LockEvent>) {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        (Self { rx }, tx)
    }
}

#[async_trait]
impl OsLockSignal for MockLockSignal {
    async fn next_lock_event(&mut self) -> LockEvent {
        // チャンネル close 時は ScreenLocked にフォールバック (fail-secure)
        self.rx.recv().await.unwrap_or(LockEvent::ScreenLocked)
    }
}

// -------------------------------------------------------------------
// OS-specific implementations
// -------------------------------------------------------------------

/// **macOS** OS スクリーンロック / サスペンド購読 (C-25)。
///
/// 完全実装は Sub-E 工程3 後続 commit で `DistributedNotificationCenter` 経由に
/// 拡張予定。本骨組みでは「未実装」を示す `pending` 経路として、シグナル待機を
/// 永久 pending にしてフォールバック動作 (`tokio::select!` で他経路が常に勝つ) で
/// 統合テストの邪魔をしない。
#[cfg(target_os = "macos")]
pub struct MacOsLockSignal;

#[cfg(target_os = "macos")]
impl MacOsLockSignal {
    #[must_use]
    pub fn new() -> Self {
        tracing::warn!(
            target: "shikomi_daemon::cache",
            "MacOsLockSignal: DistributedNotificationCenter integration is pending; \
             relying on IdleTimer + explicit Lock IPC for now"
        );
        Self
    }
}

#[cfg(target_os = "macos")]
impl Default for MacOsLockSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl OsLockSignal for MacOsLockSignal {
    async fn next_lock_event(&mut self) -> LockEvent {
        // 永久 pending: tokio::select! で他経路 (IdleTimer / shutdown) が必ず勝つ
        std::future::pending::<()>().await;
        LockEvent::ScreenLocked
    }
}

/// **Windows** WTS / WM_POWERBROADCAST 購読 (C-25、未実装スケルトン)。
#[cfg(target_os = "windows")]
pub struct WindowsLockSignal;

#[cfg(target_os = "windows")]
impl WindowsLockSignal {
    #[must_use]
    pub fn new() -> Self {
        tracing::warn!(
            target: "shikomi_daemon::cache",
            "WindowsLockSignal: WTSRegisterSessionNotification integration is pending; \
             relying on IdleTimer + explicit Lock IPC for now"
        );
        Self
    }
}

#[cfg(target_os = "windows")]
impl Default for WindowsLockSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl OsLockSignal for WindowsLockSignal {
    async fn next_lock_event(&mut self) -> LockEvent {
        std::future::pending::<()>().await;
        LockEvent::ScreenLocked
    }
}

/// **Linux** D-Bus `org.freedesktop.login1` 購読 (C-25、未実装スケルトン)。
///
/// 完全実装は `zbus` crate 追加 + `tech-stack.md` §4.7 同期更新が必要。本骨組みでは
/// pending future にフォールバックする。
#[cfg(target_os = "linux")]
pub struct LinuxLockSignal;

#[cfg(target_os = "linux")]
impl LinuxLockSignal {
    #[must_use]
    pub fn new() -> Self {
        tracing::warn!(
            target: "shikomi_daemon::cache",
            "LinuxLockSignal: org.freedesktop.login1 D-Bus integration is pending; \
             relying on IdleTimer + explicit Lock IPC for now"
        );
        Self
    }
}

#[cfg(target_os = "linux")]
impl Default for LinuxLockSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl OsLockSignal for LinuxLockSignal {
    async fn next_lock_event(&mut self) -> LockEvent {
        std::future::pending::<()>().await;
        LockEvent::ScreenLocked
    }
}

/// composition root から呼ばれる、OS 固有の `OsLockSignal` 実装ファクトリ。
/// `cfg(target_os = ...)` で切替、いずれにも該当しない OS は `MockLockSignal` (常時
/// pending) でフォールバック。
#[must_use]
pub fn make_default_os_lock_signal() -> Box<dyn OsLockSignal> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOsLockSignal::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsLockSignal::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxLockSignal::new())
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        let (mock, _tx) = MockLockSignal::new();
        Box::new(mock)
    }
}

// -------------------------------------------------------------------
// tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::crypto::Vek;

    fn dummy_vek() -> Vek {
        Vek::from_array([0xABu8; 32])
    }

    #[tokio::test]
    async fn idle_timer_locks_after_threshold_exceeded() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();

        let timer = IdleTimer::with_thresholds(
            cache.clone(),
            Duration::from_millis(50),
            Duration::from_millis(20),
        );
        let (tx, rx) = watch::channel(false);

        let handle = tokio::spawn(timer.run(rx));

        // 閾値 + ポーリングを十分超える時間待つ
        tokio::time::sleep(Duration::from_millis(150)).await;

        // shutdown
        let _ = tx.send(true);
        let _ = handle.await;

        assert!(
            !cache.is_unlocked().await,
            "cache must be locked after idle threshold exceeded"
        );
    }

    #[tokio::test]
    async fn idle_timer_keeps_unlock_when_active() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();

        let timer = IdleTimer::with_thresholds(
            cache.clone(),
            Duration::from_millis(200),
            Duration::from_millis(20),
        );
        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(timer.run(rx));

        // 50ms ごとに with_vek で last_used を更新する
        for _ in 0..3 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _: Result<(), CacheError> = cache.with_vek(|_| ()).await;
        }

        let _ = tx.send(true);
        let _ = handle.await;

        assert!(
            cache.is_unlocked().await,
            "cache must remain unlocked while last_used is advancing"
        );
    }

    #[tokio::test]
    async fn os_lock_signal_loop_locks_on_event() {
        let cache = VekCache::new();
        cache.unlock(dummy_vek()).await.unwrap();
        let (mock, tx_event) = MockLockSignal::new();
        let (tx_shutdown, rx_shutdown) = watch::channel(false);

        let handle = tokio::spawn(run_os_lock_signal_loop(
            cache.clone(),
            Box::new(mock),
            rx_shutdown,
        ));

        tx_event.send(LockEvent::ScreenLocked).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let _ = tx_shutdown.send(true);
        let _ = handle.await;

        assert!(
            !cache.is_unlocked().await,
            "ScreenLocked event must trigger cache.lock()"
        );
    }

    #[tokio::test]
    async fn make_default_os_lock_signal_returns_boxed_signal() {
        // OS によって具象型は変わるが、Box<dyn OsLockSignal> として常に取得可能
        let _signal = make_default_os_lock_signal();
    }
}
