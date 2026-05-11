//! `global-hotkey` crate を使用するクロスプラットフォームホットキーバックエンド実装。
//!
//! ## スレッドモデル
//!
//! `GlobalHotKeyManager` は一部 OS（macOS）でメインスレッド要件があるため、
//! 専用の OS バックグラウンドスレッドを spawn して以下を担う:
//!
//! 1. `GlobalHotKeyManager` の構築
//! 2. コマンドチャネル（`BackendCmd`）で register / unregister を受信して処理
//! 3. `GlobalHotKeyEvent::receiver()` をポーリングし、イベントを tokio mpsc で転送
//!
//! ポーリング間隔 10ms（ホットキーイベントの許容遅延として acceptable）。
//!
//! ## コンボ文字列 → `HotKey` マッピング
//!
//! shikomi 正規化フォーマット `"alt+ctrl+1"` を `HotKey::new(modifiers, code)` に変換する。
//! 変換不能なキーは `HotkeyError::ParseFailed` で返す。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::stream::BoxStream;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::{HotkeyBackend, HotkeyError, HotkeyEvent};

// -------------------------------------------------------------------
// BackendCmd（バックグラウンドスレッドへのコマンド）
// -------------------------------------------------------------------

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

// -------------------------------------------------------------------
// GlobalHotkeyBackend
// -------------------------------------------------------------------

/// `global-hotkey` crate バックエンド。
///
/// バックグラウンドスレッドが `GlobalHotKeyManager` を所有し、
/// コマンドチャネル経由で register / unregister を受け付ける。
pub struct GlobalHotkeyBackend {
    /// バックグラウンドスレッドへのコマンド送信チャネル。
    cmd_tx: std::sync::mpsc::SyncSender<BackendCmd>,
    /// tokio イベント受信チャネル（バックグラウンドスレッドから転送される）。
    event_rx: Arc<tokio::sync::Mutex<UnboundedReceiver<HotkeyEvent>>>,
    /// バックグラウンドスレッドのハンドル（Drop 時に Shutdown コマンドを送信）。
    _bg_thread: std::thread::JoinHandle<()>,
}

impl GlobalHotkeyBackend {
    /// バックエンドを初期化する。
    ///
    /// バックグラウンドスレッドを spawn して `GlobalHotKeyManager` を構築する。
    /// スレッド起動後、manager 構築結果を返す。
    ///
    /// # Errors
    /// `GlobalHotKeyManager::new()` 失敗時に `HotkeyError::Unavailable`。
    pub fn new() -> Result<Self, HotkeyError> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel::<BackendCmd>(32);
        let (event_tx, event_rx): (UnboundedSender<HotkeyEvent>, UnboundedReceiver<HotkeyEvent>) =
            mpsc::unbounded_channel();

        // manager 構築結果を受け取るチャネル
        let (init_tx, init_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        let bg_thread = std::thread::Builder::new()
            .name("shikomi-hotkey-backend".to_owned())
            .spawn(move || {
                background_thread_main(cmd_rx, event_tx, init_tx);
            })
            .map_err(|e| HotkeyError::Unavailable {
                reason: format!("failed to spawn hotkey thread: {e}"),
            })?;

        // バックグラウンドスレッドの初期化結果を待つ
        match init_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => {
                return Err(HotkeyError::Unavailable { reason });
            }
            Err(_) => {
                return Err(HotkeyError::Unavailable {
                    reason: "hotkey background thread disconnected during init".to_owned(),
                });
            }
        }

        Ok(Self {
            cmd_tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
            _bg_thread: bg_thread,
        })
    }

    /// コンボ文字列を送信してバックグラウンドスレッドの処理結果を受け取る汎用ヘルパ。
    fn send_cmd_and_wait<F>(&self, make_cmd: F) -> Result<(), HotkeyError>
    where
        F: FnOnce(std::sync::mpsc::SyncSender<Result<(), HotkeyError>>) -> BackendCmd,
    {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let cmd = make_cmd(reply_tx);
        self.cmd_tx
            .send(cmd)
            .map_err(|_| HotkeyError::Unavailable {
                reason: "hotkey backend thread is not running".to_owned(),
            })?;
        reply_rx.recv().map_err(|_| HotkeyError::Unavailable {
            reason: "hotkey backend thread disconnected".to_owned(),
        })?
    }
}

impl HotkeyBackend for GlobalHotkeyBackend {
    fn register(&self, combo: &str) -> Result<(), HotkeyError> {
        let combo = combo.to_owned();
        self.send_cmd_and_wait(|reply| BackendCmd::Register { combo, reply })
    }

    fn unregister(&self, combo: &str) -> Result<(), HotkeyError> {
        let combo = combo.to_owned();
        self.send_cmd_and_wait(|reply| BackendCmd::Unregister { combo, reply })
    }

    fn event_stream(&self) -> BoxStream<'static, HotkeyEvent> {
        let rx_arc = Arc::clone(&self.event_rx);
        Box::pin(futures_util::stream::unfold(rx_arc, |rx_arc| async move {
            let event = rx_arc.lock().await.recv().await?;
            Some((event, rx_arc))
        }))
    }
}

impl Drop for GlobalHotkeyBackend {
    fn drop(&mut self) {
        // Shutdown コマンドを送信してバックグラウンドスレッドを終了させる
        let _ = self.cmd_tx.send(BackendCmd::Shutdown);
    }
}

// -------------------------------------------------------------------
// バックグラウンドスレッド本体
// -------------------------------------------------------------------

/// バックグラウンドスレッドのメインループ。
///
/// `GlobalHotKeyManager` を所有し、コマンドとイベントをポーリングする。
fn background_thread_main(
    cmd_rx: std::sync::mpsc::Receiver<BackendCmd>,
    event_tx: UnboundedSender<HotkeyEvent>,
    init_tx: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    // GlobalHotKeyManager を構築
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => {
            let _ = init_tx.send(Ok(()));
            m
        }
        Err(e) => {
            let _ = init_tx.send(Err(e.to_string()));
            return;
        }
    };

    // コンボ文字列 → HotKey のマッピング（unregister のために保持）
    let mut hotkeys: HashMap<String, HotKey> = HashMap::new();
    // HotKey ID → コンボ文字列のマッピング（イベント → コンボ解決のために保持）
    let mut id_to_combo: HashMap<u32, String> = HashMap::new();

    let poll_interval = Duration::from_millis(10);

    loop {
        // コマンドを非ブロッキングで処理
        loop {
            match cmd_rx.try_recv() {
                Ok(BackendCmd::Register { combo, reply }) => {
                    let result = register_hotkey(&manager, &mut hotkeys, &mut id_to_combo, &combo);
                    let _ = reply.send(result);
                }
                Ok(BackendCmd::Unregister { combo, reply }) => {
                    let result =
                        unregister_hotkey(&manager, &mut hotkeys, &mut id_to_combo, &combo);
                    let _ = reply.send(result);
                }
                Ok(BackendCmd::Shutdown) => {
                    tracing::debug!("hotkey backend thread: shutdown received");
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    tracing::debug!("hotkey backend thread: cmd_rx disconnected, exiting");
                    return;
                }
            }
        }

        // GlobalHotKeyEvent をポーリング（10ms タイムアウト）
        let receiver = GlobalHotKeyEvent::receiver();
        let deadline = std::time::Instant::now() + poll_interval;
        while let Ok(event) = receiver.try_recv() {
            // Pressed イベントのみ転送（Released は無視）
            if matches!(event.state, global_hotkey::HotKeyState::Pressed) {
                if let Some(combo) = id_to_combo.get(&event.id) {
                    if event_tx
                        .send(HotkeyEvent {
                            combo: combo.clone(),
                        })
                        .is_err()
                    {
                        tracing::debug!("hotkey backend: event_tx disconnected, exiting");
                        return;
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
        }

        // 次のポーリングまで待機
        std::thread::sleep(poll_interval);
    }
}

// -------------------------------------------------------------------
// register / unregister ヘルパ
// -------------------------------------------------------------------

fn register_hotkey(
    manager: &GlobalHotKeyManager,
    hotkeys: &mut HashMap<String, HotKey>,
    id_to_combo: &mut HashMap<u32, String>,
    combo: &str,
) -> Result<(), HotkeyError> {
    // 既に登録済みの場合は idempotent に Ok を返す
    if hotkeys.contains_key(combo) {
        return Ok(());
    }

    let hotkey = parse_combo(combo)?;
    let id = hotkey.id();

    manager
        .register(hotkey)
        .map_err(|e| HotkeyError::RegisterFailed {
            combo: combo.to_owned(),
            reason: e.to_string(),
        })?;

    hotkeys.insert(combo.to_owned(), parse_combo(combo)?);
    id_to_combo.insert(id, combo.to_owned());
    Ok(())
}

fn unregister_hotkey(
    manager: &GlobalHotKeyManager,
    hotkeys: &mut HashMap<String, HotKey>,
    id_to_combo: &mut HashMap<u32, String>,
    combo: &str,
) -> Result<(), HotkeyError> {
    let Some(hotkey) = hotkeys.remove(combo) else {
        // 未登録の場合は idempotent に Ok を返す
        return Ok(());
    };
    let id = hotkey.id();
    id_to_combo.remove(&id);

    manager
        .unregister(hotkey)
        .map_err(|e| HotkeyError::UnregisterFailed {
            combo: combo.to_owned(),
            reason: e.to_string(),
        })?;

    Ok(())
}

// -------------------------------------------------------------------
// コンボ文字列パーサ
// -------------------------------------------------------------------

/// `"alt+ctrl+1"` などの shikomi 正規化フォーマットを `HotKey` に変換する。
fn parse_combo(combo: &str) -> Result<HotKey, HotkeyError> {
    let parts: Vec<&str> = combo.split('+').collect();
    if parts.is_empty() {
        return Err(HotkeyError::ParseFailed {
            combo: combo.to_owned(),
        });
    }

    // 最後のパートがキーコード、それ以前がモディファイア
    let (modifier_parts, key_parts) = parts.split_at(parts.len() - 1);
    let key_str = key_parts[0];

    let mut modifiers = Modifiers::empty();
    for &part in modifier_parts {
        match part {
            "alt" => modifiers |= Modifiers::ALT,
            "ctrl" => modifiers |= Modifiers::CONTROL,
            "meta" => modifiers |= Modifiers::META,
            "shift" => modifiers |= Modifiers::SHIFT,
            _ => {
                return Err(HotkeyError::ParseFailed {
                    combo: combo.to_owned(),
                });
            }
        }
    }

    let code = parse_key_code(key_str).ok_or_else(|| HotkeyError::ParseFailed {
        combo: combo.to_owned(),
    })?;

    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };

    Ok(HotKey::new(mods, code))
}

/// キーコード文字列を `Code` に変換する。
#[allow(clippy::too_many_lines)]
fn parse_key_code(key: &str) -> Option<Code> {
    match key {
        // 数字キー
        "0" => Some(Code::Digit0),
        "1" => Some(Code::Digit1),
        "2" => Some(Code::Digit2),
        "3" => Some(Code::Digit3),
        "4" => Some(Code::Digit4),
        "5" => Some(Code::Digit5),
        "6" => Some(Code::Digit6),
        "7" => Some(Code::Digit7),
        "8" => Some(Code::Digit8),
        "9" => Some(Code::Digit9),
        // アルファベットキー
        "a" => Some(Code::KeyA),
        "b" => Some(Code::KeyB),
        "c" => Some(Code::KeyC),
        "d" => Some(Code::KeyD),
        "e" => Some(Code::KeyE),
        "f" => Some(Code::KeyF),
        "g" => Some(Code::KeyG),
        "h" => Some(Code::KeyH),
        "i" => Some(Code::KeyI),
        "j" => Some(Code::KeyJ),
        "k" => Some(Code::KeyK),
        "l" => Some(Code::KeyL),
        "m" => Some(Code::KeyM),
        "n" => Some(Code::KeyN),
        "o" => Some(Code::KeyO),
        "p" => Some(Code::KeyP),
        "q" => Some(Code::KeyQ),
        "r" => Some(Code::KeyR),
        "s" => Some(Code::KeyS),
        "t" => Some(Code::KeyT),
        "u" => Some(Code::KeyU),
        "v" => Some(Code::KeyV),
        "w" => Some(Code::KeyW),
        "x" => Some(Code::KeyX),
        "y" => Some(Code::KeyY),
        "z" => Some(Code::KeyZ),
        // ファンクションキー
        "f1" => Some(Code::F1),
        "f2" => Some(Code::F2),
        "f3" => Some(Code::F3),
        "f4" => Some(Code::F4),
        "f5" => Some(Code::F5),
        "f6" => Some(Code::F6),
        "f7" => Some(Code::F7),
        "f8" => Some(Code::F8),
        "f9" => Some(Code::F9),
        "f10" => Some(Code::F10),
        "f11" => Some(Code::F11),
        "f12" => Some(Code::F12),
        // 特殊キー
        "space" => Some(Code::Space),
        "enter" => Some(Code::Enter),
        "escape" | "esc" => Some(Code::Escape),
        "tab" => Some(Code::Tab),
        "backspace" => Some(Code::Backspace),
        "delete" => Some(Code::Delete),
        "insert" => Some(Code::Insert),
        "home" => Some(Code::Home),
        "end" => Some(Code::End),
        "pageup" => Some(Code::PageUp),
        "pagedown" => Some(Code::PageDown),
        "arrowup" | "up" => Some(Code::ArrowUp),
        "arrowdown" | "down" => Some(Code::ArrowDown),
        "arrowleft" | "left" => Some(Code::ArrowLeft),
        "arrowright" | "right" => Some(Code::ArrowRight),
        _ => None,
    }
}
