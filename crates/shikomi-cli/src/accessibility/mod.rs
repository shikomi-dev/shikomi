//! アクセシビリティ代替経路（MSG-S18、WCAG 2.1 AA、Sub-F #44 Phase 6/7 + 工程5）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 / §セキュリティ設計 §`--output print` / `--output braille` の一時ファイル / リダイレクト対策
//!
//! 構成 (Phase 7 完了 + 工程5 BLOCKER 解消後の実態):
//! - `umask`: `--output {print,braille}` 経路で出力直前に `umask(0o077)` を内部適用
//!   し、ユーザのリダイレクト先 (`> recovery.brf` 等) を所有者限定 0600 相当で生成。
//! - `braille_brf`: 24 語を BRF (Braille Ready Format) で stdout 書出。
//!   `Zeroizing<Vec<u8>>` で構築し drop で自動 zeroize (工程5 BLOCKER 3)。
//! - `audio_tts`: 24 語を OS TTS subprocess の stdin にパイプ。中間ファイルなし、
//!   `Stdio::piped()` のみ。macOS `say` / Linux `espeak` は本実装、Windows は
//!   PowerShell ScriptBlockLogging 経由の 24 語 Event Log 残留経路を遮断するため
//!   **fail fast (unsupported)** で塞ぎ Phase 8 helper bin (COM 直接呼出) で再設計
//!   (工程5 BLOCKER 4)。
//! - `print_pdf`: 24 語をハイコントラスト PDF 1.4 で stdout 書出 (printpdf 依存
//!   採用せず手書き、`Zeroizing<Vec<u8>>` で zeroize 維持、工程5 BLOCKER 3)。
//! - `screen_reader`: Linux pgrep + Windows PowerShell で本実装、macOS は
//!   Phase 8 先送り (`VOICEOVER_RUNNING` env hint のみ)。
//! - `output_target`: `SHIKOMI_ACCESSIBILITY` env / `screen_reader` 検出 / 明示
//!   `--output` フラグ判定で既定 `Screen` を `Braille` に自動切替。
//!
//! Phase 8 引継ぎ:
//! - macOS スクリーンリーダー検出を `NSWorkspace` accessibility API or
//!   `defaults read` subprocess で本実装。
//! - Windows audio TTS の `shikomi-windows-tts.exe` helper bin (COM 経由 SAPI) と
//!   `--to-file <path>` フラグ + `SetSecurityInfo` DACL 設定経路の正式実装。

pub mod audio_tts;
pub mod braille_brf;
pub mod output_target;
pub mod print_pdf;
pub mod screen_reader;
pub mod umask;
// `file_acl` は工程5 服部指摘 (BLOCKER 2) で削除。Windows ACL リダイレクト先制御
// は実装スタブのまま誰からも呼ばれない完全デッドコードだったため、`Boy Scout Rule
// + 中途半端は怠惰` の原則に従って撤去 (Phase 8 で `--to-file <path>` フラグ +
// `SetSecurityInfo` 経由 DACL 設定の本実装が確定するまで導入しない)。
