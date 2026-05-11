//! shikomi-gui — GUI フロントエンド層（Tauri v2 + SolidJS）。
//!
//! ## モジュール構成（将来拡張）
//!
//! - `commands`: Tauri Commands ハンドラ群（Sub-B 以降で IPC 委譲実装）
//! - `error`: `GUIError` enum（Sub-B 以降で SolidJS への統一エラー型実装）
//!
//! ## 起動経路
//!
//! - バイナリ `shikomi-gui`: `main.rs` から `run()` を直接呼び出す
//! - `shikomi gui` CLI: `shikomi-gui` バイナリを `std::process::Command` で起動する
//!   （将来的に `run()` を直接呼び出す経路に移行する可能性あり）
//!
//! 設計根拠: `docs/features/shikomi-gui/feature-spec.md` R1-GUI-01
//! `docs/architecture/tech-stack.md §2.6`

/// Tauri アプリケーションを起動する。
///
/// `shikomi-gui` バイナリの `main` および将来の CLI 直接呼び出し経路から使用する。
///
/// # Errors
///
/// Tauri ランタイムの初期化または起動に失敗した場合に `tauri::Error` を返す。
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
}
