//! ホットキーイベント受信ループ（R1-HK-07 / R1-HK-13 / R1-HK-14）。
//!
//! `HotkeyEventLoop::run` は `HotkeyBackend::event_stream` からイベントを受信し、
//! クリップボード投入および `ClearTimer` スケジューリングを行う。
//!
//! ## Mutex 保持時間最小化
//!
//! vault の Mutex はレコード検索 + ペイロード clone のみに限定し、
//! OS API 呼び出し（クリップボード・通知）は Mutex 外で実行する。
//! 詳細: `docs/features/daemon-hotkey-clipboard/daemon/detailed-design.md §4.1.5`

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use shikomi_core::{Hotkey, RecordKind, CLEAR_TIMEOUT_SECS};
use tokio::sync::{watch, Mutex};

use crate::cache::VekCache;
use crate::hotkey::backend::{BackendEnum, HotkeyBackend};
use crate::hotkey::clear_timer::ClearTimer;
use crate::hotkey::clipboard::ClipboardWriter;
use crate::hotkey::notifier::{Notifier, NotifyLevel};
use shikomi_core::Vault;

// -------------------------------------------------------------------
// HotkeyEventLoop
// -------------------------------------------------------------------

/// ホットキーイベント受信・クリップボード投入ループ。
///
/// `tokio::spawn(event_loop.run(shutdown_rx))` で起動する。
pub struct HotkeyEventLoop {
    backend: Arc<BackendEnum>,
    vault: Arc<Mutex<Vault>>,
    vek_cache: VekCache,
    clipboard: Arc<Mutex<dyn ClipboardWriter + Send>>,
    notifier: Arc<dyn Notifier>,
    clear_timer: ClearTimer,
    /// クリップボード投入開始時刻。GUI カウントダウン表示用（Sub-D #97）。
    ///
    /// `Some(t)`: 残秒計算の基点。`None`: カウントダウン非アクティブ。
    /// `Arc<Mutex<...>>` で `IpcServer` の `V2Context` と共有する。
    countdown_started_at: Arc<Mutex<Option<Instant>>>,
}

impl HotkeyEventLoop {
    /// `HotkeyEventLoop` を構築する。
    ///
    /// `clipboard`: `SHIKOMI_DISABLE_CLIPBOARD=1` の場合 `NullClipboardWriter` を渡す。
    /// `countdown_started_at`: GUI カウントダウン表示用の共有状態（`IpcServer` と共有）。
    pub fn new(
        backend: Arc<BackendEnum>,
        vault: Arc<Mutex<Vault>>,
        vek_cache: VekCache,
        clipboard: Arc<Mutex<dyn ClipboardWriter + Send>>,
        notifier: Arc<dyn Notifier>,
        countdown_started_at: Arc<Mutex<Option<Instant>>>,
    ) -> Self {
        Self {
            backend,
            vault,
            vek_cache,
            clipboard,
            notifier,
            clear_timer: ClearTimer::new(),
            countdown_started_at,
        }
    }

    /// イベントループを実行する（shutdown 受信まで継続）。
    ///
    /// `tokio::spawn` で起動することを想定。
    pub async fn run(mut self, mut shutdown_rx: watch::Receiver<bool>) {
        let mut stream = self.backend.event_stream();

        loop {
            tokio::select! {
                biased;
                changed = shutdown_rx.changed() => {
                    let _ = changed;
                    tracing::debug!("HotkeyEventLoop: shutdown received");
                    self.clear_timer.abort();
                    *self.countdown_started_at.lock().await = None;
                    return;
                }
                event = stream.next() => {
                    match event {
                        None => {
                            tracing::warn!("HotkeyEventLoop: event stream ended");
                            return;
                        }
                        Some(ev) => {
                            self.handle_event(&ev.combo).await;
                        }
                    }
                }
            }
        }
    }

    /// 単一ホットキーイベントの処理。
    ///
    /// 詳細設計: `detailed-design.md §4.2`
    async fn handle_event(&mut self, combo: &str) {
        // --- Step 1-4: vault Mutex 内でレコード検索 + ペイロード clone ---
        let result = {
            let vault_guard = self.vault.lock().await;
            let hotkey = match Hotkey::parse(combo) {
                Ok(h) => h,
                Err(_) => {
                    tracing::debug!(combo, "HotkeyEventLoop: failed to parse event combo");
                    return;
                }
            };

            let Some(record) = vault_guard.find_by_hotkey(&hotkey) else {
                tracing::debug!(
                    target: "shikomi::audit",
                    event = "hotkey_triggered",
                    combo,
                    result = "skipped:not_found",
                    "hotkey event: record not found"
                );
                return;
            };

            // vault がロック中の場合は OS 通知してスキップ（R1-HK-07 / R1-HK-13）
            // plaintext vault は保護なしのため、常にロック解除済み扱いとする。
            // VekCache は常に Locked 状態で起動するため plaintext vault で
            // is_unlocked() は false を返す点に注意（IPC handler の同パターンと統一）。
            let is_plaintext = matches!(
                vault_guard.protection_mode(),
                shikomi_core::ProtectionMode::Plaintext
            );
            if !is_plaintext && !self.vek_cache.is_unlocked().await {
                tracing::info!(
                    target: "shikomi::audit",
                    event = "hotkey_triggered",
                    record_id = %record.id(),
                    combo,
                    result = "skipped:vault_locked",
                    "hotkey event: vault is locked"
                );
                // vault Mutex を drop してから通知（Mutex 保持時間最小化）
                drop(vault_guard);
                self.send_notification(
                    NotifyLevel::Low,
                    "shikomi",
                    "vault がロック中です。shikomi vault unlock を実行してください",
                );
                return;
            }

            // ペイロード値を clone して取り出す（Mutex 保持時間最小化）
            // SEC-001: `text_preview` は Secret kind で None を返すため `clipboard_value` を使用。
            // Encrypted variant（Phase 2 以降）では None を返す。その場合は空文字を書かず
            // OS 通知してスキップする（Fail Fast: 空クリップボード書き込みを防ぐ）。
            let Some(payload_value) = record.clipboard_value() else {
                tracing::error!(
                    target: "shikomi::audit",
                    event = "hotkey_triggered",
                    record_id = %record.id(),
                    combo,
                    result = "skipped:encrypted_payload",
                    "clipboard_value unavailable (Encrypted variant?)"
                );
                drop(vault_guard);
                self.send_notification(
                    NotifyLevel::Normal,
                    "shikomi",
                    "クリップボードに投入できる値がありません",
                );
                return;
            };
            let record_kind = record.kind();
            let record_id = record.id().to_string();

            // vault Mutex はここで drop（ブロックを抜ける）
            drop(vault_guard);
            Some((payload_value, record_kind, record_id))
        };

        let Some((payload_value, record_kind, record_id)) = result else {
            return;
        };

        // --- Step 5: vault Mutex 外でクリップボード書き込み ---
        let write_result = {
            let mut cb = self.clipboard.lock().await;
            cb.write(&payload_value)
        };

        match write_result {
            Ok(()) => {
                tracing::info!(
                    target: "shikomi::audit",
                    event = "hotkey_triggered",
                    record_id,
                    combo,
                    result = "injected",
                    secret = matches!(record_kind, RecordKind::Secret),
                    "hotkey event: clipboard injected"
                );

                // --- Step 7: Secret レコードは ClearTimer をスケジュール ---
                if matches!(record_kind, RecordKind::Secret) {
                    // GUI カウントダウン用の投入時刻を記録（Sub-D #97）。
                    *self.countdown_started_at.lock().await = Some(Instant::now());
                    self.clear_timer.schedule(
                        Duration::from_secs(CLEAR_TIMEOUT_SECS),
                        Arc::clone(&self.clipboard),
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "shikomi::audit",
                    event = "hotkey_triggered",
                    record_id,
                    combo,
                    result = "error:clipboard",
                    error = %e,
                    "hotkey event: clipboard write failed"
                );
                // Step 6: クリップボード書き込み失敗時は OS 通知（R1-HK-14）
                self.send_notification(
                    NotifyLevel::Normal,
                    "shikomi",
                    "クリップボードへの書き込みに失敗しました",
                );
            }
        }
    }

    /// OS 通知を送信する（失敗は warn のみ、アプリ動作を止めない）。
    fn send_notification(&self, level: NotifyLevel, title: &str, body: &str) {
        if let Err(e) = self.notifier.notify(level, title, body) {
            tracing::warn!(error = %e, "HotkeyEventLoop: notification failed (best-effort)");
        }
    }
}
