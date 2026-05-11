//! 結合テスト共通ヘルパー（Sub-F 専用）。
//!
//! - `daemon_spawn`: 実 `shikomi-daemon` 子プロセスを管理する `DaemonSpawn`（Unix 限定）
//!
//! 設計根拠: `docs/features/cli-vault-commands/test-design/integration.md §10.2`
//! 対応 Issue: #77

#![allow(dead_code)]

#[cfg(unix)]
pub mod daemon_spawn;

#[cfg(unix)]
pub use daemon_spawn::DaemonSpawn;
