//! daemon → クライアントの IPC レスポンス。
//!
//! `#[non_exhaustive]` により後続 feature の variant 追加が非破壊変更として扱える。

use serde::{Deserialize, Serialize};

use crate::vault::id::RecordId;

use super::error_code::IpcErrorCode;
use super::summary::RecordSummary;
use super::version::IpcProtocolVersion;

// -------------------------------------------------------------------
// IpcResponse
// -------------------------------------------------------------------

/// daemon → クライアントの IPC レスポンス。
///
/// 設計根拠: docs/features/daemon-ipc/detailed-design/protocol-types.md §`IpcResponse`
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcResponse {
    /// ハンドシェイク成功。
    Handshake {
        /// daemon 側のプロトコルバージョン。
        server_version: IpcProtocolVersion,
    },
    /// プロトコル不一致、両側のバージョンを返す。直後に接続切断。
    ProtocolVersionMismatch {
        /// daemon 側のプロトコルバージョン。
        server: IpcProtocolVersion,
        /// クライアント側のプロトコルバージョン（受信値）。
        client: IpcProtocolVersion,
    },
    /// `ListRecords` への応答（投影された機密非含有 summary 列）。
    Records(Vec<RecordSummary>),
    /// `AddRecord` 成功。
    Added {
        /// 追加された ID。
        id: RecordId,
    },
    /// `EditRecord` 成功。
    Edited {
        /// 更新された ID。
        id: RecordId,
    },
    /// `RemoveRecord` 成功。
    Removed {
        /// 削除された ID。
        id: RecordId,
    },
    /// 各種失敗（ハードコード固定文言の `IpcErrorCode` のみ）。
    Error(IpcErrorCode),
}

impl IpcResponse {
    /// variant 名（log 出力等で全体 Debug を避けるため）。
    #[must_use]
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Handshake { .. } => "handshake",
            Self::ProtocolVersionMismatch { .. } => "protocol_version_mismatch",
            Self::Records(_) => "records",
            Self::Added { .. } => "added",
            Self::Edited { .. } => "edited",
            Self::Removed { .. } => "removed",
            Self::Error(_) => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_name_handshake() {
        let resp = IpcResponse::Handshake {
            server_version: IpcProtocolVersion::V1,
        };
        assert_eq!(resp.variant_name(), "handshake");
    }

    #[test]
    fn test_variant_name_records() {
        let resp = IpcResponse::Records(vec![]);
        assert_eq!(resp.variant_name(), "records");
    }

    #[test]
    fn test_variant_name_protocol_version_mismatch() {
        let resp = IpcResponse::ProtocolVersionMismatch {
            server: IpcProtocolVersion::V1,
            client: IpcProtocolVersion::V1,
        };
        assert_eq!(resp.variant_name(), "protocol_version_mismatch");
    }

    #[test]
    fn test_variant_name_error() {
        let resp = IpcResponse::Error(IpcErrorCode::EncryptionUnsupported);
        assert_eq!(resp.variant_name(), "error");
    }
}
