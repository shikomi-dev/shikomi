//! `shikomi vault {subcommand}` サブコマンド usecase 群（Sub-F #44 Phase 2）。
//!
//! 各 usecase は以下の責務を負う:
//!
//! 1. ユーザ入力（master password 等）を `io::terminal::read_password` で取得
//! 2. `IpcVaultRepository` の対応 round-trip を呼出
//! 3. presenter 経由で MSG-S{xx} 系の成功文言を render
//!
//! Phase 2 スコープ:
//! - clap 派生型 + IPC 送信ループの最小経路 wire のみ
//! - stdin 拒否 / `/dev/tty` 強制 / `umask(0o077)` / core dump 抑制 は **Phase 5** へ分離
//! - 24 語の `--output {print,braille,audio}` 経路は **Phase 5** へ分離（Phase 2 は `Screen` のみ）
//! - `cache_relocked: false` 経路の MSG-S07/S20 連結表示は **Phase 4** へ分離
//!   （Phase 2 は `RekeyOutcome` / `RotateRecoveryOutcome` の構造化応答受領まで）
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細（F-F1〜F-F7）

pub mod change_password;
pub mod decrypt;
pub mod encrypt;
pub mod lock;
pub mod rekey;
pub mod rotate_recovery;
pub mod unlock;

use crate::error::CliError;
use crate::io::terminal;
use shikomi_core::SecretString;

/// `master password: ` プロンプトで非エコー入力 1 行を取得する共通 helper。
///
/// Phase 2 では既存 `io::terminal::read_password` をそのまま流用。`/dev/tty` 強制
/// 経路 / stdin パイプ拒否 (C-38) は Phase 5 で `input::password::prompt` に
/// 集約する（本 helper は Phase 5 で内部実装が差し替わるが API は不変）。
///
/// # Errors
/// stdin / TTY 失敗時に `CliError::Persistence` を返す。
pub(crate) fn read_master_password(prompt: &str) -> Result<SecretString, CliError> {
    terminal::read_password(prompt).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<tty>"),
            source: e,
        })
    })
}
