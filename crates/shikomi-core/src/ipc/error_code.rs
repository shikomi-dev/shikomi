//! IPC エラーコード。
//!
//! `reason` フィールドはハードコード固定文言のみ（絶対パス・lock holder PID・
//! ピア UID 漏洩を構造的に遮断、`basic-design/error.md §IpcErrorCode バリアント詳細`）。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::vault::id::RecordId;

// -------------------------------------------------------------------
// IpcErrorCode
// -------------------------------------------------------------------

/// IPC エラーコード enum。
///
/// `IpcResponse::Error(IpcErrorCode)` で運搬され、CLI 側 `presenter::error::render_error` で
/// `MSG-CLI-101〜109` に写像される。`reason` は英語短文の固定文言のみ。
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    /// 暗号化 vault 検出（本 Issue 未対応）。
    EncryptionUnsupported,
    /// 対象 id が vault に存在しない。
    NotFound {
        /// 対象 ID（秘密情報なし、UUIDv7）。
        id: RecordId,
    },
    /// ラベル検証失敗（防御的再検証）。
    InvalidLabel {
        /// 固定文言（例: "invalid label"）。
        reason: String,
    },
    /// 永続化レイヤーのエラー。
    Persistence {
        /// 固定文言（例: "persistence error" / "vault corrupted"）。
        reason: String,
    },
    /// ドメイン整合性エラー。
    Domain {
        /// 固定文言（例: "domain error" / "duplicate record id"）。
        reason: String,
    },
    /// 想定外バグ（防御的経路）。
    Internal {
        /// 固定文言（例: "unexpected error"）。
        reason: String,
    },
}

impl fmt::Display for IpcErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncryptionUnsupported => f.write_str("encryption unsupported"),
            Self::NotFound { id } => write!(f, "not found: {id}"),
            Self::InvalidLabel { reason } => write!(f, "invalid label: {reason}"),
            Self::Persistence { reason } => write!(f, "persistence error: {reason}"),
            Self::Domain { reason } => write!(f, "domain error: {reason}"),
            Self::Internal { reason } => write!(f, "internal error: {reason}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_encryption_unsupported_does_not_contain_path() {
        let code = IpcErrorCode::EncryptionUnsupported;
        let s = code.to_string();
        assert!(!s.contains('/'));
        assert!(!s.contains('\\'));
    }

    #[test]
    fn test_display_persistence_uses_fixed_reason_only() {
        let code = IpcErrorCode::Persistence {
            reason: "persistence error".to_owned(),
        };
        assert_eq!(code.to_string(), "persistence error: persistence error");
    }
}
