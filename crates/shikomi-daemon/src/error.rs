//! daemon プロセス全体の総合エラー型と終了コード。

use std::process::ExitCode;

use thiserror::Error;

use crate::lifecycle::single_instance::SingleInstanceError;

// -------------------------------------------------------------------
// DaemonExit
// -------------------------------------------------------------------

/// daemon プロセスの終了コード列挙。
///
/// 設計根拠: docs/features/daemon-ipc/requirements.md §REQ-DAEMON-024
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DaemonExit {
    /// 正常終了（graceful shutdown 完了）
    Success = 0,
    /// システムエラー（vault load 失敗 / bind 失敗 / SDDL 設定失敗 等）
    SystemError = 1,
    /// シングルインスタンス先取り失敗（他 daemon 稼働中等）
    SingleInstanceUnavailable = 2,
    /// 暗号化 vault 検出（本 feature 未対応）
    EncryptionUnsupported = 3,
}

impl From<DaemonExit> for ExitCode {
    fn from(e: DaemonExit) -> Self {
        ExitCode::from(e as u8)
    }
}

// -------------------------------------------------------------------
// ServerError
// -------------------------------------------------------------------

/// IpcServer 実行時エラー。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerError {
    /// `accept` 呼出失敗。
    #[error("accept failed: {0}")]
    Accept(std::io::Error),
    /// 接続単位タスクの join 失敗。
    #[error("connection task join failed: {0}")]
    Join(tokio::task::JoinError),
}

// -------------------------------------------------------------------
// DaemonError
// -------------------------------------------------------------------

/// daemon 内部の総合エラー型（`run` から `match` で `DaemonExit` に写像）。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DaemonError {
    /// シングルインスタンス先取り失敗。
    #[error("single instance: {0}")]
    SingleInstance(SingleInstanceError),
    /// IpcServer 実行時エラー。
    #[error("server error: {0}")]
    Server(ServerError),
}
