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
//!
//! Sub-C は `kind` → `ipc_code` の順で分岐し、`message` は開発ツール専用。
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
/// `Ipc` variant のみ `ipc_code`（+ `BackoffActive` は `wait_secs`）を追加フィールドとして持つ。
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
            // Ipc variant: kind + ipc_code + message（BackoffActive は wait_secs 追加）。
            // Sub-C は ipc_code で daemon エラー種別を switch する（detailed-design.md §2.3）。
            Self::Ipc(code) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("kind", "ipc_error")?;
                map.serialize_entry("ipc_code", ipc_code_key(code))?;
                if let IpcErrorCode::BackoffActive { wait_secs } = code {
                    map.serialize_entry("wait_secs", wait_secs)?;
                }
                map.serialize_entry("message", &code.to_string())?;
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
                    Self::InvalidInput(msg) => ("invalid_input", msg.clone()),
                    Self::NotConnected => ("not_connected", "not connected to daemon".to_owned()),
                    Self::Ipc(_) => unreachable!("handled in Ipc arm"),
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

/// `IpcErrorCode` variant を Sub-C が `switch` する安定 snake_case 識別子へ写像する（§2.3）。
///
/// `IpcErrorCode` は `#[non_exhaustive]` のため将来バリアント追加時の
/// フォールバック `"unknown"` を保持する。Sub-C 側は `"unknown"` を汎用エラーとして処理せよ。
fn ipc_code_key(code: &IpcErrorCode) -> &'static str {
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

    // TC-GUI-IPC-UT13 — GUIError::Ipc(VaultLocked): kind/ipc_code/message 全フィールド検証
    //
    // ペガサス指摘 Option A（§2.3 ipc_code 追加）対応: Sub-C は ipc_code で分岐する。
    // ipc_code フィールドの存在と値を検証する（旧実装では kind のみ検証で ipc_code 欠落）。
    #[test]
    fn ut13_ipc_vault_locked_kind_ipc_code_and_message() {
        let e = GUIError::Ipc(IpcErrorCode::VaultLocked);
        let v = serde_json::to_value(&e).unwrap();
        // kind: "ipc_error"（全 IpcErrorCode 共通）
        assert_eq!(v["kind"], "ipc_error", "kind must be ipc_error: {v}");
        // ipc_code: "vault_locked"（Sub-C が UI 分岐に使う安定識別子）
        assert_eq!(
            v["ipc_code"], "vault_locked",
            "ipc_code must be vault_locked: {v}"
        );
        // message: VaultLocked の Display 文字列（デバッグ専用）
        let msg = v["message"].as_str().unwrap();
        let expected = IpcErrorCode::VaultLocked.to_string();
        assert_eq!(
            msg, expected,
            "message must match IpcErrorCode::VaultLocked Display"
        );
        // ipc_code == "vault_locked" のとき wait_secs は存在しない
        assert!(
            v.get("wait_secs").is_none() || v["wait_secs"].is_null(),
            "wait_secs must not be present for VaultLocked: {v}"
        );
    }

    // TC-GUI-IPC-UT13b — GUIError::Ipc(BackoffActive): wait_secs フィールド検証
    //
    // BackoffActive のみ wait_secs フィールドが追加される（§2.3 特例）。
    // Sub-C が待機カウントダウンを UI 表示するために必要。
    #[test]
    fn ut13b_ipc_backoff_active_has_wait_secs() {
        let e = GUIError::Ipc(IpcErrorCode::BackoffActive { wait_secs: 42 });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "ipc_error", "kind must be ipc_error: {v}");
        assert_eq!(
            v["ipc_code"], "backoff_active",
            "ipc_code must be backoff_active: {v}"
        );
        // wait_secs フィールドが数値として存在する
        assert_eq!(
            v["wait_secs"], 42,
            "wait_secs must be 42 for BackoffActive{{ wait_secs: 42 }}: {v}"
        );
        // message にも wait_secs が含まれる（Display 準拠）
        let msg = v["message"].as_str().unwrap();
        assert!(
            msg.contains("42"),
            "message must contain wait_secs value '42': {msg}"
        );
    }

    // TC-GUI-IPC-UT13c — GUIError::Ipc(HotkeyConflict): ipc_code 検証
    //
    // HotkeyConflict は IT08/IT12 で Rust Result として検証済みだが、
    // JSON ipc_code フィールドは UT で補完する（§9 カバレッジ基準）。
    #[test]
    fn ut13c_ipc_hotkey_conflict_ipc_code() {
        let e = GUIError::Ipc(IpcErrorCode::HotkeyConflict {
            reason: "slot occupied".to_owned(),
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "ipc_error");
        assert_eq!(v["ipc_code"], "hotkey_conflict");
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
