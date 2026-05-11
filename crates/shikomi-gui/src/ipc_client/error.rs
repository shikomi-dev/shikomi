//! GUI 統一エラー型。
//!
//! `GUIError` は全 Tauri Commands の統一エラー型。`serde::Serialize` を実装し、
//! SolidJS 側で `{ "kind": "...", "message": "..." }` JSON として受け取れる。
//!
//! `kind` フィールドは Sub-C（UI 層）が switch して日本語メッセージを表示するための
//! 機械的判別子。`message` フィールドはデバッグ・ログ用の英語技術情報であり、
//! ユーザーに直接表示してはならない。
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
/// `Serialize` 実装で `{ "kind": "...", "message": "..." }` 形式に写像する。
/// SolidJS 側は `kind` で分岐し、`message` はログ・開発ツール専用。
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
// Serialize 実装: { "kind": "...", "message": "..." }
// ---------------------------------------------------------------------------

impl Serialize for GUIError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        let (kind, message): (&str, String) = match self {
            Self::DaemonNotRunning => ("daemon_not_running", "daemon is not running".to_owned()),
            Self::ConnectionFailed(msg) => ("connection_failed", msg.clone()),
            Self::ProtocolVersionMismatch { server, client } => (
                "protocol_version_mismatch",
                format!("server={server}, client={client}"),
            ),
            Self::Ipc(code) => ("ipc_error", code.to_string()),
            Self::Encode(msg) => ("encode_error", msg.clone()),
            Self::Decode(msg) => ("decode_error", msg.clone()),
            Self::UnexpectedResponse(msg) => ("unexpected_response", msg.clone()),
            Self::InvalidInput(msg) => ("invalid_input", msg.clone()),
            Self::NotConnected => ("not_connected", "not connected to daemon".to_owned()),
        };
        map.serialize_entry("kind", kind)?;
        map.serialize_entry("message", &message)?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::GUIError;
    use shikomi_core::ipc::IpcErrorCode;

    // TC-GUI-IPC-UT10
    #[test]
    fn ut10_daemon_not_running_kind() {
        let e = GUIError::DaemonNotRunning;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "daemon_not_running");
        assert!(!v["message"].as_str().unwrap_or("").is_empty());
    }

    // TC-GUI-IPC-UT11
    #[test]
    fn ut11_not_connected_kind() {
        let e = GUIError::NotConnected;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "not_connected");
        assert!(!v["message"].as_str().unwrap_or("").is_empty());
    }

    // TC-GUI-IPC-UT12
    #[test]
    fn ut12_protocol_version_mismatch_kind_and_message() {
        let e = GUIError::ProtocolVersionMismatch {
            server: "v1".to_owned(),
            client: "v2".to_owned(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "protocol_version_mismatch");
        let msg = v["message"].as_str().unwrap();
        assert!(
            msg.contains("v1"),
            "message should contain server version: {msg}"
        );
        assert!(
            msg.contains("v2"),
            "message should contain client version: {msg}"
        );
    }

    // TC-GUI-IPC-UT13
    #[test]
    fn ut13_ipc_vault_locked_kind_and_message() {
        let e = GUIError::Ipc(IpcErrorCode::VaultLocked);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "ipc_error");
        let msg = v["message"].as_str().unwrap();
        let expected = IpcErrorCode::VaultLocked.to_string();
        assert_eq!(
            msg, expected,
            "message must match IpcErrorCode::VaultLocked Display"
        );
    }

    // TC-GUI-IPC-UT14
    #[test]
    fn ut14_invalid_input_kind_and_message() {
        let e = GUIError::InvalidInput("test message".to_owned());
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "invalid_input");
        assert_eq!(v["message"].as_str().unwrap(), "test message");
    }
}
