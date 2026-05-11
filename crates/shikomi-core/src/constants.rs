//! shikomi-core グローバル定数。

/// クリップボード自動クリアまでの秒数（R1-HK-05）。
///
/// secret エントリのホットキー押下後、この秒数が経過すると `ClearTimer` が
/// クリップボードを空文字で上書きする。MVP 固定値（設定変更なし）。
pub const CLEAR_TIMEOUT_SECS: u64 = 30;
