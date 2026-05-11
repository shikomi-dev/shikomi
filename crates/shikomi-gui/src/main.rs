//! shikomi-gui バイナリエントリポイント。
//!
//! `shikomi_gui::run()` を呼ぶ 3 行ラッパ。
//! `#[cfg(not(debug_assertions))]` で Windows の `SUBSYSTEM:WINDOWS` を設定し
//! リリースビルドでコンソールウィンドウを非表示にする。

// Windows リリースビルドではコンソールウィンドウを非表示にする（R1-GUI-01）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = shikomi_gui::run() {
        eprintln!("shikomi-gui: {e}");
        std::process::exit(1);
    }
}
