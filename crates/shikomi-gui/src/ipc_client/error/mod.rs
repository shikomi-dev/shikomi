//! GUI 統一エラー型。
//!
//! `GUIError` は全 Tauri Commands の統一エラー型。`serde::Serialize` を実装し、
//! SolidJS 側で JSON として受け取れる。
//!
//! ## JSON フィールド仕様
//!
//! | フィールド | 全 variant | 説明 |
//! |---|---|---|
//! | `kind` | 常に存在 | Sub-C が `switch` する最上位判別子 |
//! | `message` | 常に存在 | デバッグ・ログ用英語技術情報。**ユーザーに直接表示禁止** |
//! | `ipc_code` | `kind == "ipc_error"` のみ | daemon エラー種別の安定識別子（§2.3）|
//! | `wait_secs` | `ipc_code == "backoff_active"` のみ | 次回試行可能までの待機秒数 |
//! | `crypto_reason` | `ipc_code == "crypto"` のみ | 暗号エラー詳細識別子（kebab-case 固定文言: `wrong-password` / `weak-password` / `nonce-limit-exceeded` 等）。Sub-C はこの値で UI 分岐する |
//! | `hotkey_conflict_entry` | `ipc_code == "hotkey_conflict"` のみ | 競合している既存エントリ名の文字列。Sub-C は競合エントリ名を UI 表示する（R1-GUI-08）。`message` パースへの依存禁止 |
//!
//! | `invalid_input_code` | `kind == "invalid_input"` のみ | バリデーション失敗種別の安定識別子。Sub-C はこの値で UI 分岐する（`message` パース禁止）|
//!
//! Sub-C は `kind` → `ipc_code` / `invalid_input_code` の順で分岐し、`message` は開発ツール専用。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md §2.2
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §2

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use shikomi_core::ipc::IpcErrorCode;
use thiserror::Error;

// ---------------------------------------------------------------------------
// GUIError
// ---------------------------------------------------------------------------

/// Tauri Commands の統一エラー型。
///
/// `Serialize` 実装でモジュール doc の JSON フィールド仕様に従い写像する。
/// `Ipc` variant のみ `ipc_code` を持ち、さらに `BackoffActive` は `wait_secs`、
/// `Crypto` は `crypto_reason`、`HotkeyConflict` は `hotkey_conflict_entry` を追加フィールドとして持つ。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GUIError {
    /// UDS / Named Pipe が存在しない（daemon 未起動）。
    #[error("daemon is not running")]
    DaemonNotRunning,

    /// 接続後の IO エラー（切断含む）。
    ///
    /// `message` には `io::Error::kind().to_string()` のみを使用し、
    /// OS 内部情報（ソケットパス・FD 番号等）を含めない（OWASP A04）。
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Handshake バージョン不一致。
    #[error("protocol version mismatch (server={server}, client={client})")]
    ProtocolVersionMismatch {
        /// daemon 側のバージョン文字列。
        server: String,
        /// クライアント側のバージョン文字列。
        client: String,
    },

    /// daemon から返却された `IpcErrorCode` の透過伝搬。
    #[error("ipc error: {0}")]
    Ipc(IpcErrorCode),

    /// `MessagePack` シリアライズ失敗。
    #[error("encode error: {0}")]
    Encode(String),

    /// `MessagePack` デシリアライズ失敗。
    #[error("decode error: {0}")]
    Decode(String),

    /// 予期しない `IpcResponse` variant。
    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),

    /// Rust 側 input validation 失敗（R1-GUI-19）。
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// `AppState` が `None`（daemon 未接続）。
    #[error("not connected to daemon")]
    NotConnected,
}

// ---------------------------------------------------------------------------
// Serialize 実装
// ---------------------------------------------------------------------------

impl Serialize for GUIError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // Ipc variant: kind + ipc_code + message。
            // variant ごとの追加フィールド（§2.3）:
            //   BackoffActive → wait_secs
            //   Crypto        → crypto_reason
            //   HotkeyConflict→ hotkey_conflict_entry
            // Sub-C は ipc_code で daemon エラー種別を switch する（detailed-design.md §2.3）。
            Self::Ipc(code) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("kind", "ipc_error")?;
                map.serialize_entry("ipc_code", ipc_code_key(code))?;
                if let IpcErrorCode::BackoffActive { wait_secs } = code {
                    map.serialize_entry("wait_secs", wait_secs)?;
                }
                if let IpcErrorCode::Crypto { reason } = code {
                    map.serialize_entry("crypto_reason", reason)?;
                }
                if let IpcErrorCode::HotkeyConflict { reason } = code {
                    map.serialize_entry("hotkey_conflict_entry", reason)?;
                }
                map.serialize_entry("message", &code.to_string())?;
                map.end()
            }
            // InvalidInput variant: kind + invalid_input_code + message の 3 フィールド。
            // Sub-C は invalid_input_code で分岐し、message パースに依存しない（§2.2）。
            Self::InvalidInput(msg) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("kind", "invalid_input")?;
                map.serialize_entry("invalid_input_code", invalid_input_code_key(msg))?;
                map.serialize_entry("message", msg)?;
                map.end()
            }
            // 他の variant: kind + message の 2 フィールド。
            other => {
                let mut map = serializer.serialize_map(Some(2))?;
                let (kind, message): (&str, String) = match other {
                    Self::DaemonNotRunning => {
                        ("daemon_not_running", "daemon is not running".to_owned())
                    }
                    Self::ConnectionFailed(msg) => ("connection_failed", msg.clone()),
                    Self::ProtocolVersionMismatch { server, client } => (
                        "protocol_version_mismatch",
                        format!("server={server}, client={client}"),
                    ),
                    Self::Encode(msg) => ("encode_error", msg.clone()),
                    Self::Decode(msg) => ("decode_error", msg.clone()),
                    Self::UnexpectedResponse(msg) => ("unexpected_response", msg.clone()),
                    Self::NotConnected => ("not_connected", "not connected to daemon".to_owned()),
                    Self::Ipc(_) | Self::InvalidInput(_) => {
                        unreachable!("handled in dedicated arms")
                    }
                };
                map.serialize_entry("kind", kind)?;
                map.serialize_entry("message", &message)?;
                map.end()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ipc_code_key: IpcErrorCode → snake_case 安定識別子
// ---------------------------------------------------------------------------

/// `InvalidInput` の message 文字列を Sub-C が `switch` する安定識別子へ写像する（§2.2）。
///
/// 各エントリポイントの `InvalidInput` 生成箇所（entries.rs, vault.rs, hotkey.rs）の
/// 固定文言を網羅する。新たな `InvalidInput` を追加した場合はここに必ず追記すること。
pub(super) fn invalid_input_code_key(msg: &str) -> &'static str {
    match msg {
        "label must not be empty" => "label_empty",
        "value must not be empty" => "value_empty",
        "invalid record id format" => "id_invalid",
        "master password must not be empty" => "password_empty",
        "decrypt confirmation required" => "confirmation_required",
        "invalid hotkey format" => "hotkey_invalid",
        // entries.rs: RecordLabel::try_new のエラーを凍結文言で包んだもの（§2.2）
        "invalid label format" => "label_invalid",
        _ => "unknown",
    }
}

/// `IpcErrorCode` variant を Sub-C が `switch` する安定 snake_case 識別子へ写像する（§2.3）。
///
/// `IpcErrorCode` は `#[non_exhaustive]` のため将来バリアント追加時の
/// フォールバック `"unknown"` を保持する。Sub-C 側は `"unknown"` を汎用エラーとして処理せよ。
pub(super) fn ipc_code_key(code: &IpcErrorCode) -> &'static str {
    match code {
        IpcErrorCode::EncryptionUnsupported => "encryption_unsupported",
        IpcErrorCode::NotFound { .. } => "not_found",
        IpcErrorCode::InvalidLabel { .. } => "invalid_label",
        IpcErrorCode::Persistence { .. } => "persistence",
        IpcErrorCode::Domain { .. } => "domain",
        IpcErrorCode::Internal { .. } => "internal",
        IpcErrorCode::VaultLocked => "vault_locked",
        IpcErrorCode::BackoffActive { .. } => "backoff_active",
        IpcErrorCode::RecoveryRequired => "recovery_required",
        IpcErrorCode::ProtocolDowngrade => "protocol_downgrade",
        IpcErrorCode::Crypto { .. } => "crypto",
        IpcErrorCode::HotkeyConflict { .. } => "hotkey_conflict",
        IpcErrorCode::HotkeyParseError { .. } => "hotkey_parse_error",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests;
