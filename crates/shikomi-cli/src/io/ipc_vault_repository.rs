//! `IpcVaultRepository` — daemon 経由で `--ipc list` を実装する細い IPC クライアント。
//!
//! ## スコープ（縮減）
//!
//! 当初は `VaultRepository` trait を full impl して `--ipc add/edit/remove` も透過化する
//! 案 C（投影 Vault）を採用していたが、レビューで以下の致命的問題が判明したため
//! **`--ipc list` 専用に縮減**する（外部レビュー指摘に対応）:
//!
//! - `load()` が daemon 由来 summary を `RecordPayload::Plaintext(SecretString::from_string(""))`
//!   で偽装してドメイン集約に乗せる必要があった（嘘の値の注入）
//! - `save()` が CLI 側生成 `RecordId` をそのまま提示する一方、daemon が独自 ID を再生成して
//!   保存するため**ユーザに表示される ID が daemon vault に存在しない**（嘘の ID 出荷）
//! - `exists()` が常に `Ok(true)` を返し trait 契約への嘘
//!
//! 本 PR では `VaultRepository` trait 実装を**完全に削除**し、`list` のみを支える narrow
//! API（`list_summaries() -> Vec<RecordSummary>`）に絞った。`add/edit/remove` の IPC 透過化は
//! 案 B（trait 分割）への移行 PR で正式実装する。
//!
//! 設計根拠: docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md §縮減後 API

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use shikomi_core::ipc::{IpcErrorCode, IpcRequest, IpcResponse, RecordSummary};
use shikomi_infra::persistence::PersistenceError;
use tokio::runtime::Runtime;

use super::ipc_client::IpcClient;

// -------------------------------------------------------------------
// IpcVaultRepository
// -------------------------------------------------------------------

/// daemon との接続を保持し、`IpcRequest::ListRecords` を発行して `RecordSummary` 列を取得する。
///
/// `VaultRepository` trait は実装しない（縮減後のスコープでは不要）。
pub struct IpcVaultRepository {
    runtime: Runtime,
    client: Mutex<IpcClient>,
    socket_path: PathBuf,
}

impl IpcVaultRepository {
    /// daemon に接続して `IpcVaultRepository` を構築する（同期 wrapper）。
    ///
    /// # Errors
    /// 接続失敗（daemon 未起動）/ ハンドシェイク失敗時に `PersistenceError`。
    pub fn connect(socket_path: &Path) -> Result<Self, PersistenceError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| PersistenceError::IpcIo {
                reason: e.to_string(),
            })?;
        let client = runtime.block_on(IpcClient::connect(socket_path))?;
        Ok(Self {
            runtime,
            client: Mutex::new(client),
            socket_path: socket_path.to_path_buf(),
        })
    }

    /// OS デフォルトのソケットパスを解決する。
    ///
    /// - **Linux**: `$XDG_RUNTIME_DIR/shikomi/daemon.sock`、未設定時は `dirs::runtime_dir()`
    /// - **macOS**: `dirs::cache_dir()/shikomi/daemon.sock`
    /// - **Windows**: `\\.\pipe\shikomi-daemon-{user-sid}`
    ///
    /// # Errors
    /// 解決元が利用不能な場合 `PersistenceError::CannotResolveVaultDir`。
    pub fn default_socket_path() -> Result<PathBuf, PersistenceError> {
        #[cfg(unix)]
        {
            unix_default_socket_path()
        }
        #[cfg(windows)]
        {
            let sid = crate::io::windows_sid::resolve_self_user_sid()?;
            Ok(PathBuf::from(format!(r"\\.\pipe\shikomi-daemon-{sid}")))
        }
    }

    /// 接続先ソケットパスへの参照を返す（ログ・エラー表示用）。
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// daemon にレコード summary 列を要求する（`--ipc list` の主経路）。
    ///
    /// `IpcRequest::ListRecords` を 1 往復し、`Records` variant を抽出して返す。
    /// daemon 側の暗号化検出 / その他エラーは `PersistenceError` に写像する。
    ///
    /// # Errors
    /// IPC 失敗 / 暗号化 vault 検出 / 不正応答時に `PersistenceError`。
    pub fn list_summaries(&self) -> Result<Vec<RecordSummary>, PersistenceError> {
        match self.round_trip(&IpcRequest::ListRecords)? {
            IpcResponse::Records(s) => Ok(s),
            IpcResponse::Error(code) => Err(map_ipc_error_code(&code)),
            other => Err(PersistenceError::IpcDecode {
                reason: format!("unexpected response variant: {}", other.variant_name()),
            }),
        }
    }

    fn round_trip(&self, request: &IpcRequest) -> Result<IpcResponse, PersistenceError> {
        let mut client = self.client.lock().map_err(|_| PersistenceError::IpcIo {
            reason: "ipc client lock poisoned".to_owned(),
        })?;
        self.runtime.block_on(client.round_trip(request))
    }
}

#[cfg(unix)]
fn unix_default_socket_path() -> Result<PathBuf, PersistenceError> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir).join("shikomi").join("daemon.sock"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        return dirs::cache_dir()
            .map(|d| d.join("shikomi").join("daemon.sock"))
            .ok_or(PersistenceError::CannotResolveVaultDir);
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::runtime_dir()
            .map(|d| d.join("shikomi").join("daemon.sock"))
            .ok_or(PersistenceError::CannotResolveVaultDir)
    }
}

// -------------------------------------------------------------------
// 補助関数
// -------------------------------------------------------------------

fn map_ipc_error_code(code: &IpcErrorCode) -> PersistenceError {
    match code {
        IpcErrorCode::EncryptionUnsupported => PersistenceError::UnsupportedYet {
            feature: "encrypted vault persistence",
            tracking_issue: None,
        },
        IpcErrorCode::NotFound { .. } => PersistenceError::IpcDecode {
            reason: "record not found".to_owned(),
        },
        IpcErrorCode::InvalidLabel { .. } => PersistenceError::IpcDecode {
            reason: "invalid label".to_owned(),
        },
        IpcErrorCode::Persistence { .. } => PersistenceError::IpcIo {
            reason: "persistence error".to_owned(),
        },
        IpcErrorCode::Domain { .. } => PersistenceError::IpcIo {
            reason: "domain error".to_owned(),
        },
        IpcErrorCode::Internal { .. } => PersistenceError::IpcIo {
            reason: "internal error".to_owned(),
        },
        // `#[non_exhaustive]` 属性により future variant が追加される可能性に備える。
        // 本 wildcard は **cross-crate `non_exhaustive` の必須回避**であり、観測可能な reason
        // 文言を返すためコード追跡を阻害しない。
        _ => PersistenceError::IpcIo {
            reason: "unknown error code".to_owned(),
        },
    }
}
