//! `HotkeyBackend` trait と OS バックエンドの実行時ディスパッチ（`BackendEnum`）。

use std::sync::Arc;

use futures_util::stream::BoxStream;

pub(crate) mod global_hotkey;

// -------------------------------------------------------------------
// HotkeyEvent
// -------------------------------------------------------------------

/// ホットキー発火イベント。`HotkeyBackend::event_stream` が yield する型。
#[derive(Debug, Clone)]
pub struct HotkeyEvent {
    /// 発火したホットキーの正規化コンボ文字列（例: `"alt+ctrl+1"`）。
    pub combo: String,
}

// -------------------------------------------------------------------
// HotkeyError
// -------------------------------------------------------------------

/// `HotkeyBackend` が返すエラー型。
#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    /// バックエンド未対応（ヘッドレス環境等）。
    #[error("hotkey backend unavailable: {reason}")]
    Unavailable { reason: String },

    /// コンボ文字列の解析失敗（OS バックエンドへのマッピング不可）。
    #[error("hotkey combo parse failed: {combo}")]
    ParseFailed { combo: String },

    /// OS 登録失敗（他アプリと競合等）。
    #[error("hotkey register failed for {combo}: {reason}")]
    RegisterFailed { combo: String, reason: String },

    /// OS 解除失敗。
    #[error("hotkey unregister failed for {combo}: {reason}")]
    UnregisterFailed { combo: String, reason: String },
}

// -------------------------------------------------------------------
// HotkeyBackend trait
// -------------------------------------------------------------------

/// OS ホットキー登録・解除・イベント受信の抽象インターフェース。
///
/// `Send + Sync + 'static`: `Arc<BackendEnum>` で `HotkeyManager` と
/// `HotkeyEventLoop` に共有するために必要。
pub trait HotkeyBackend: Send + Sync + 'static {
    /// 指定コンボを OS に登録する。
    ///
    /// # Errors
    /// OS 登録失敗 / コンボ解析失敗。
    fn register(&self, combo: &str) -> Result<(), HotkeyError>;

    /// 指定コンボの OS 登録を解除する。
    ///
    /// # Errors
    /// OS 解除失敗。
    fn unregister(&self, combo: &str) -> Result<(), HotkeyError>;

    /// 登録済みホットキーのイベントストリームを返す。
    ///
    /// `HotkeyEventLoop::run` が一度だけ呼び出す。
    fn event_stream(&self) -> BoxStream<'static, HotkeyEvent>;
}

// -------------------------------------------------------------------
// BackendEnum（静的ディスパッチ）
// -------------------------------------------------------------------

/// OS バックエンドの静的ディスパッチ enum。
///
/// `dyn HotkeyBackend` の代わりに enum で実装することで、ホットキーイベントループの
/// hot path でのヒープアロケーションを避ける。
pub enum BackendEnum {
    /// `global-hotkey` crate を使用するクロスプラットフォームバックエンド。
    GlobalHotkey(global_hotkey::GlobalHotkeyBackend),
    /// バックエンド未対応環境向け noop 実装（tracing::warn のみ）。
    Null(NullBackend),
}

impl BackendEnum {
    /// OS / セッションを検出して適切なバックエンドを構築する。
    ///
    /// `global-hotkey` バックエンドの初期化に失敗した場合は `NullBackend` にフォールバック。
    pub fn detect() -> Arc<Self> {
        match global_hotkey::GlobalHotkeyBackend::new() {
            Ok(backend) => {
                tracing::info!("HotkeyBackend: using global-hotkey backend");
                Arc::new(Self::GlobalHotkey(backend))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "HotkeyBackend: global-hotkey init failed, using null backend (hotkeys disabled)"
                );
                Arc::new(Self::Null(NullBackend))
            }
        }
    }
}

impl HotkeyBackend for BackendEnum {
    fn register(&self, combo: &str) -> Result<(), HotkeyError> {
        match self {
            Self::GlobalHotkey(b) => b.register(combo),
            Self::Null(b) => b.register(combo),
        }
    }

    fn unregister(&self, combo: &str) -> Result<(), HotkeyError> {
        match self {
            Self::GlobalHotkey(b) => b.unregister(combo),
            Self::Null(b) => b.unregister(combo),
        }
    }

    fn event_stream(&self) -> BoxStream<'static, HotkeyEvent> {
        match self {
            Self::GlobalHotkey(b) => b.event_stream(),
            Self::Null(b) => b.event_stream(),
        }
    }
}

// -------------------------------------------------------------------
// NullBackend
// -------------------------------------------------------------------

/// バックエンド未対応環境向けの noop 実装。
///
/// 全操作が noop（`tracing::warn!` のみ）。イベントストリームは空（never yields）。
pub struct NullBackend;

impl HotkeyBackend for NullBackend {
    fn register(&self, combo: &str) -> Result<(), HotkeyError> {
        tracing::warn!(
            combo,
            "NullBackend: hotkey register suppressed (backend unavailable)"
        );
        Ok(())
    }

    fn unregister(&self, combo: &str) -> Result<(), HotkeyError> {
        tracing::debug!(combo, "NullBackend: hotkey unregister suppressed");
        Ok(())
    }

    fn event_stream(&self) -> BoxStream<'static, HotkeyEvent> {
        // 空のストリーム: 永遠に pending（never yields）
        Box::pin(futures_util::stream::pending())
    }
}
