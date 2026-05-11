//! クリップボード書き込みの抽象インターフェース（R1-HK-04）。
//!
//! `ClipboardWriter` trait によりテスト時に `MockClipboardWriter` に差し替え可能。
//! ヘッドレス環境では `SHIKOMI_DISABLE_CLIPBOARD=1` で `NullClipboardWriter` を使用する。

// -------------------------------------------------------------------
// ClipboardError
// -------------------------------------------------------------------

/// `ClipboardWriter` が返すエラー型。
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// クリップボードが使用不可（ヘッドレス環境等）。
    #[error("clipboard unavailable: {reason}")]
    Unavailable { reason: String },

    /// 書き込み失敗。
    #[error("clipboard write failed: {reason}")]
    WriteFailed { reason: String },
}

// -------------------------------------------------------------------
// ClipboardWriter trait
// -------------------------------------------------------------------

/// OS クリップボードへの書き込みインターフェース。
///
/// `Send + 'static` 境界: `Arc<Mutex<dyn ClipboardWriter>>` で `HotkeyEventLoop` と
/// `ClearTimer` に共有するために必要。
pub trait ClipboardWriter: Send + 'static {
    /// クリップボードに UTF-8 テキストを書き込む。
    ///
    /// # Errors
    /// クリップボードが使用不可 / 書き込み失敗。
    fn write(&mut self, value: &str) -> Result<(), ClipboardError>;

    /// クリップボードを空文字列で上書きしてクリアする。
    ///
    /// # Errors
    /// クリップボードが使用不可 / 書き込み失敗。
    fn clear(&mut self) -> Result<(), ClipboardError>;
}

// -------------------------------------------------------------------
// ArboardClipboardWriter
// -------------------------------------------------------------------

/// `arboard::Clipboard` を使用する本番クリップボード実装。
///
/// `arboard::Clipboard` は `Send` であるため、`Arc<Mutex<>>` で安全に共有できる。
pub struct ArboardClipboardWriter {
    inner: arboard::Clipboard,
}

impl ArboardClipboardWriter {
    /// `arboard::Clipboard` を初期化する。
    ///
    /// 失敗時（ヘッドレス環境 / クリップボード未対応）は `ClipboardError::Unavailable`。
    ///
    /// # Errors
    /// `arboard::Clipboard::new()` が失敗した場合。
    pub fn new() -> Result<Self, ClipboardError> {
        arboard::Clipboard::new()
            .map(|inner| Self { inner })
            .map_err(|e| ClipboardError::Unavailable {
                reason: e.to_string(),
            })
    }

    /// daemon 起動時に使用するクリップボード共有オブジェクトを初期化する。
    ///
    /// `SHIKOMI_DISABLE_CLIPBOARD=1` 環境変数が設定されている場合、または
    /// `arboard::Clipboard::new()` が失敗した場合は `NullClipboardWriter` を返す。
    /// 呼び出し元は返り値を `HotkeyEventLoop::new` に渡す。
    #[must_use]
    pub fn init_shared() -> std::sync::Arc<tokio::sync::Mutex<dyn ClipboardWriter + Send>> {
        if std::env::var("SHIKOMI_DISABLE_CLIPBOARD").as_deref() == Ok("1") {
            tracing::info!("SHIKOMI_DISABLE_CLIPBOARD=1: clipboard disabled");
            return std::sync::Arc::new(tokio::sync::Mutex::new(NullClipboardWriter));
        }

        match Self::new() {
            Ok(writer) => {
                tracing::debug!("clipboard: initialized arboard clipboard");
                std::sync::Arc::new(tokio::sync::Mutex::new(writer))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "clipboard: arboard init failed, falling back to NullClipboardWriter"
                );
                std::sync::Arc::new(tokio::sync::Mutex::new(NullClipboardWriter))
            }
        }
    }
}

impl ClipboardWriter for ArboardClipboardWriter {
    fn write(&mut self, value: &str) -> Result<(), ClipboardError> {
        self.inner
            .set_text(value)
            .map_err(|e| ClipboardError::WriteFailed {
                reason: e.to_string(),
            })
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        // arboard 3.x では set_text("") でクリアする。
        self.inner
            .set_text("")
            .map_err(|e| ClipboardError::WriteFailed {
                reason: e.to_string(),
            })
    }
}

// -------------------------------------------------------------------
// NullClipboardWriter
// -------------------------------------------------------------------

/// ヘッドレス環境向けの noop クリップボード実装。
///
/// `arboard::Clipboard::new()` 失敗時のフォールバック。
/// 全操作が noop となり、`tracing::warn!` でログのみ出力する。
pub struct NullClipboardWriter;

impl ClipboardWriter for NullClipboardWriter {
    fn write(&mut self, _value: &str) -> Result<(), ClipboardError> {
        tracing::warn!("NullClipboardWriter: clipboard is unavailable, write suppressed");
        Ok(())
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        tracing::debug!("NullClipboardWriter: clipboard is unavailable, clear suppressed");
        Ok(())
    }
}

// -------------------------------------------------------------------
// MockClipboardWriter (テスト用)
// -------------------------------------------------------------------

/// テスト用の記録型クリップボード。操作履歴を `Vec<String>` に蓄積する。
#[cfg(test)]
#[derive(Default)]
pub struct MockClipboardWriter {
    pub history: Vec<String>,
}

#[cfg(test)]
impl MockClipboardWriter {
    /// 空の `MockClipboardWriter` を構築する。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl ClipboardWriter for MockClipboardWriter {
    fn write(&mut self, value: &str) -> Result<(), ClipboardError> {
        self.history.push(value.to_owned());
        Ok(())
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        self.history.push(String::new());
        Ok(())
    }
}
