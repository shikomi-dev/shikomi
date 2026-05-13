//! Wayland (XDG Portal `org.freedesktop.portal.GlobalShortcuts`) backend
//! for `HotkeyBackend` trait.
//!
//! Linux Wayland セッションでは `global-hotkey` crate (XGrabKey 系) が動作しないため、
//! XDG Desktop Portal の `GlobalShortcuts` 経由でショートカットを登録・受信する。
//!
//! ## アーキテクチャ
//!
//! ashpd の API は async/await ベース、`HotkeyBackend` trait は sync。
//! 両者を橋渡しするため、別 OS スレッドで `tokio::runtime::current_thread`
//! を走らせ、`std::sync::mpsc` で同期コマンドを送る (既存 `global_hotkey.rs`
//! と同一の橋渡しパターン)。
//!
//! ```text
//! caller (tokio task)                    worker thread (own tokio rt)
//!     ├── register("Ctrl+Alt+1") ─────► BackendCmd::Register { combo, reply }
//!     │                                    ├── ashpd: bind_shortcuts([...])
//!     │   ◄────────────────────────────── reply.send(Ok)
//!     │
//!     └── event_stream() poll ────────────────────────────┐
//!                                                          │
//!     worker is also listening to:                         │
//!         ashpd Activated signal                           │
//!         └── HotkeyEvent { combo } ──► event_tx ──────────┘
//! ```
//!
//! 設計根拠: Issue #160 / #162 (v0.1.2 仕様)

use std::sync::Arc;

use ashpd::desktop::global_shortcuts::{
    Activated, BindShortcutsOptions, GlobalShortcuts, NewShortcut,
};
use ashpd::desktop::CreateSessionOptions;
use futures_util::{stream::BoxStream, StreamExt};
use tokio::sync::Mutex as TokioMutex;

use super::{HotkeyBackend, HotkeyError, HotkeyEvent};

// ---------------------------------------------------------------------------
// XdgPortalBackend (public)
// ---------------------------------------------------------------------------

pub struct XdgPortalBackend {
    cmd_tx: std::sync::mpsc::SyncSender<BackendCmd>,
    event_rx: Arc<TokioMutex<tokio::sync::mpsc::UnboundedReceiver<HotkeyEvent>>>,
}

impl XdgPortalBackend {
    /// 新しい XDG Portal バックエンドを構築する。
    ///
    /// 内部で OS スレッドを起動し、専用の tokio current_thread runtime で
    /// ashpd `GlobalShortcuts::create_session()` を実行する。session 確立まで
    /// 同期的に待機（最大 5 秒タイムアウト）。
    ///
    /// # Errors
    /// セッション作成失敗 / D-Bus 接続失敗 / Portal 未対応。
    pub fn new() -> Result<Self, HotkeyError> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel::<BackendCmd>(32);
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), HotkeyError>>(1);

        // OS スレッドで独立した tokio runtime を回す
        std::thread::Builder::new()
            .name("shikomi-xdg-portal".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(HotkeyError::Unavailable {
                            reason: format!("tokio runtime build failed: {e}"),
                        }));
                        return;
                    }
                };
                rt.block_on(worker_main(cmd_rx, event_tx, ready_tx));
            })
            .map_err(|e| HotkeyError::Unavailable {
                reason: format!("thread spawn failed: {e}"),
            })?;

        // session 確立を待つ (5s)
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => {
                tracing::info!("XdgPortalBackend: session ready");
                Ok(Self {
                    cmd_tx,
                    event_rx: Arc::new(TokioMutex::new(event_rx)),
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(HotkeyError::Unavailable {
                reason: "XDG Portal session creation timed out (5s)".into(),
            }),
        }
    }

    fn send_cmd_and_wait<F>(&self, make: F) -> Result<(), HotkeyError>
    where
        F: FnOnce(std::sync::mpsc::SyncSender<Result<(), HotkeyError>>) -> BackendCmd,
    {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let cmd = make(reply_tx);
        self.cmd_tx
            .send(cmd)
            .map_err(|_| HotkeyError::Unavailable {
                reason: "XDG Portal worker thread is gone".into(),
            })?;
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| HotkeyError::Unavailable {
                reason: "XDG Portal reply timed out".into(),
            })?
    }
}

impl HotkeyBackend for XdgPortalBackend {
    fn register(&self, combo: &str) -> Result<(), HotkeyError> {
        let combo_owned = combo.to_owned();
        self.send_cmd_and_wait(|reply| BackendCmd::Register {
            combo: combo_owned,
            reply,
        })
    }

    fn unregister(&self, combo: &str) -> Result<(), HotkeyError> {
        let combo_owned = combo.to_owned();
        self.send_cmd_and_wait(|reply| BackendCmd::Unregister {
            combo: combo_owned,
            reply,
        })
    }

    fn event_stream(&self) -> BoxStream<'static, HotkeyEvent> {
        let rx_arc = Arc::clone(&self.event_rx);
        Box::pin(futures_util::stream::unfold(rx_arc, |rx_arc| async move {
            let event = rx_arc.lock().await.recv().await?;
            Some((event, rx_arc))
        }))
    }
}

impl Drop for XdgPortalBackend {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(BackendCmd::Shutdown);
    }
}

// ---------------------------------------------------------------------------
// Worker thread
// ---------------------------------------------------------------------------

enum BackendCmd {
    Register {
        combo: String,
        reply: std::sync::mpsc::SyncSender<Result<(), HotkeyError>>,
    },
    Unregister {
        combo: String,
        reply: std::sync::mpsc::SyncSender<Result<(), HotkeyError>>,
    },
    Shutdown,
}

async fn worker_main(
    cmd_rx: std::sync::mpsc::Receiver<BackendCmd>,
    event_tx: tokio::sync::mpsc::UnboundedSender<HotkeyEvent>,
    ready_tx: std::sync::mpsc::SyncSender<Result<(), HotkeyError>>,
) {
    // 同期 mpsc を tokio に橋渡しするため、worker 内で別タスクで recv して
    // tokio チャンネルに転送する（unsafe ポインタ trick を回避）。
    let (cmd_tokio_tx, mut cmd_tokio_rx) = tokio::sync::mpsc::unbounded_channel::<BackendCmd>();
    std::thread::Builder::new()
        .name("shikomi-xdg-cmd-bridge".into())
        .spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                if cmd_tokio_tx.send(cmd).is_err() {
                    break;
                }
            }
        })
        .expect("cmd bridge thread spawn failed");

    // ashpd: アプリ ID を host portal registry に登録（GlobalShortcuts CreateSession の必須前提）。
    // AppID は `/usr/share/applications/dev.shikomi.daemon.desktop` の basename と一致させる必要がある
    // (xdg-desktop-portal が App info 検索に使う規約 + 2 セグメント以上の制約)。
    // `.desktop` ファイルは tauri.conf.json の bundle.linux.{deb,rpm}.files で同梱。
    match ashpd::AppID::try_from("dev.shikomi.daemon") {
        Ok(app_id) => match ashpd::register_host_app(app_id).await {
            Ok(()) => tracing::info!("XdgPortalBackend: register_host_app(dev.shikomi.daemon) ok"),
            Err(e) => {
                tracing::warn!(error = %e, "XdgPortalBackend: register_host_app failed (dev 環境では .desktop 未配置の可能性)")
            }
        },
        Err(e) => tracing::warn!(error = %e, "XdgPortalBackend: AppID parse failed"),
    }

    // ashpd: GlobalShortcuts proxy + session 作成
    let gs = match GlobalShortcuts::new().await {
        Ok(g) => g,
        Err(e) => {
            let _ = ready_tx.send(Err(HotkeyError::Unavailable {
                reason: format!("GlobalShortcuts::new failed: {e}"),
            }));
            return;
        }
    };

    let session = match gs.create_session(CreateSessionOptions::default()).await {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(HotkeyError::Unavailable {
                reason: format!("create_session failed: {e}"),
            }));
            return;
        }
    };

    let activated_stream = match gs.receive_activated().await {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(HotkeyError::Unavailable {
                reason: format!("receive_activated failed: {e}"),
            }));
            return;
        }
    };
    let mut activated_stream = std::pin::pin!(activated_stream);

    // 現在登録済みコンボの集合（BindShortcuts は全置換なので、増減を管理）
    let mut bound: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    let _ = ready_tx.send(Ok(()));

    // メインループ: cmd 受信 と Activated signal を同時に待つ
    loop {
        tokio::select! {
            // ホットキー押下イベント
            Some(activated) = activated_stream.next() => {
                handle_activated(activated, &event_tx);
            }

            // ブリッジスレッド経由で sync mpsc を tokio mpsc に橋渡しした recv
            cmd = cmd_tokio_rx.recv() => {
                let Some(cmd) = cmd else {
                    tracing::info!("XdgPortalBackend: command channel closed, exiting");
                    break;
                };
                match cmd {
                    BackendCmd::Register { combo, reply } => {
                        bound.insert(combo);
                        let result = bind_all(&gs, &session, &bound).await;
                        let _ = reply.send(result);
                    }
                    BackendCmd::Unregister { combo, reply } => {
                        bound.remove(&combo);
                        let result = bind_all(&gs, &session, &bound).await;
                        let _ = reply.send(result);
                    }
                    BackendCmd::Shutdown => {
                        tracing::info!("XdgPortalBackend: shutdown signal received");
                        break;
                    }
                }
            }
        }
    }
}

fn handle_activated(
    activated: Activated,
    event_tx: &tokio::sync::mpsc::UnboundedSender<HotkeyEvent>,
) {
    let combo = activated.shortcut_id().to_owned();
    tracing::debug!(combo = %combo, "XdgPortalBackend: shortcut activated");
    if event_tx.send(HotkeyEvent { combo }).is_err() {
        tracing::warn!("XdgPortalBackend: event_tx closed");
    }
}

/// 現在 `bound` 集合に含まれる全ショートカットを Portal に再登録する。
///
/// `BindShortcuts` は呼び出しごとに引数の集合で「全置換」される（部分更新の API がない）。
/// `shortcut_id` は HotkeyEvent.combo で照合するため正規化済み文字列を直接使う。
async fn bind_all(
    gs: &GlobalShortcuts,
    session: &ashpd::desktop::Session<GlobalShortcuts>,
    bound: &std::collections::BTreeSet<String>,
) -> Result<(), HotkeyError> {
    let shortcuts: Vec<NewShortcut> = bound
        .iter()
        .map(|combo| {
            NewShortcut::new(combo.clone(), format!("shikomi: {combo}"))
                .preferred_trigger(Some(combo.as_str()))
        })
        .collect();

    gs.bind_shortcuts(session, &shortcuts, None, BindShortcutsOptions::default())
        .await
        .map_err(|e| HotkeyError::RegisterFailed {
            combo: bound.iter().next_back().cloned().unwrap_or_default(),
            reason: format!("XDG Portal BindShortcuts failed: {e}"),
        })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Availability check (使う側 = backend/mod.rs::detect() が呼ぶ)
// ---------------------------------------------------------------------------

/// Wayland セッションか否かを `WAYLAND_DISPLAY` env で判定する。
pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}
