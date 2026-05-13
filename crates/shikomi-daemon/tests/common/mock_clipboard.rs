//! テスト用 `MockClipboardWriter`。
//!
//! `ClipboardWriter` trait のテスト用実装。write/clear 操作の履歴を記録する。
//! `#[cfg(test)]` ガードを持たず `tests/` 配下に物理分離して配置（本番コードへの混入禁止）。
//!
//! 設計根拠: `docs/features/daemon-hotkey-clipboard/daemon/test-design.md §3`

use shikomi_daemon::hotkey::clipboard::{ClipboardError, ClipboardWriter};

/// テスト用の記録型クリップボード。write/clear 操作の履歴を `Vec<String>` に蓄積する。
///
/// - `write(v)` は `v` を push
/// - `clear()` は空文字列を push し `cleared_count` をインクリメント
#[derive(Default)]
pub struct MockClipboardWriter {
    /// 書き込み履歴（write した値。clear は空文字列として記録）。
    pub history: Vec<String>,
    /// `clear()` が呼ばれた回数。タイマー発火の確認に使用。
    pub cleared_count: usize,
}

impl MockClipboardWriter {
    /// 空の `MockClipboardWriter` を構築する。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 最後に書き込まれた値を返す（write または clear いずれか最後）。
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.history.last().map(String::as_str)
    }

    /// 書き込み回数（clear を除く）を返す。
    #[must_use]
    pub fn write_count(&self) -> usize {
        self.history.len() - self.cleared_count
    }
}

impl ClipboardWriter for MockClipboardWriter {
    fn write(&mut self, value: &str) -> Result<(), ClipboardError> {
        self.history.push(value.to_owned());
        Ok(())
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        self.history.push(String::new());
        self.cleared_count += 1;
        Ok(())
    }
}
