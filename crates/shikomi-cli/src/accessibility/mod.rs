//! アクセシビリティ代替経路（MSG-S18、WCAG 2.1 AA、Sub-F #44 Phase 6）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 / §セキュリティ設計 §`--output print` / `--output braille` の一時ファイル / リダイレクト対策
//!
//! 構成:
//! - `umask`: `--output {print,braille}` 経路で出力直前に `umask(0o077)` を内部適用
//!   し、ユーザのリダイレクト先 (`> recovery.brf`) を所有者限定 0600 相当で生成。
//! - `braille_brf`: 24 語を BRF (Braille Ready Format) で stdout に書出。
//! - `audio_tts`: 24 語を OS TTS (macOS `say` / Windows SAPI / Linux `espeak`)
//!   subprocess の stdin にパイプ。中間ファイルなし、`Stdio::piped()` のみ。
//! - `output_target`: `SHIKOMI_ACCESSIBILITY` env / 明示 `--output` フラグ判定で
//!   既定 `Screen` を `Braille` に自動切替（OS スクリーンリーダー検出は Phase 7）。
//!
//! Phase 6 スコープ外:
//! - PDF (`--output print`) 本実装は **Phase 7** で `printpdf` 依存追加と同時に
//!   入れる。Phase 6 では既存 fallback notice を維持する。
//! - OS スクリーンリーダー自動検出 (macOS `defaults read` / Windows Narrator プロセス
//!   検出 / Linux Orca DBus) は **Phase 7** に分離。

pub mod audio_tts;
pub mod braille_brf;
pub mod file_acl;
pub mod output_target;
pub mod print_pdf;
pub mod screen_reader;
pub mod umask;
