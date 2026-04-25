//! クライアント → daemon の IPC リクエスト。
//!
//! `#[non_exhaustive]` により後続 feature の variant 追加が非破壊変更として扱える。

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::vault::id::RecordId;
use crate::vault::record::{RecordKind, RecordLabel};

use super::secret_bytes::SerializableSecretBytes;
use super::version::IpcProtocolVersion;

// -------------------------------------------------------------------
// IpcRequest
// -------------------------------------------------------------------

/// クライアント → daemon の IPC リクエスト。
///
/// 設計根拠: docs/features/daemon-ipc/detailed-design/protocol-types.md §`IpcRequest`
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcRequest {
    /// 接続直後の必須 1 往復、プロトコル一致確認。
    Handshake {
        /// クライアント側のプロトコルバージョン。
        client_version: IpcProtocolVersion,
    },
    /// 全レコードの `RecordSummary` 列を要求。
    ListRecords,
    /// 新規レコード追加。
    AddRecord {
        /// レコード種別。
        kind: RecordKind,
        /// ラベル（検証済み）。
        label: RecordLabel,
        /// 値（IPC 専用 secret 経路）。
        value: SerializableSecretBytes,
        /// クライアント側で生成した UTC 時刻（RFC3339）。
        #[serde(with = "time::serde::rfc3339")]
        now: OffsetDateTime,
    },
    /// 既存レコードの部分更新。
    EditRecord {
        /// 対象 ID。
        id: RecordId,
        /// 新ラベル（任意）。
        label: Option<RecordLabel>,
        /// 新値（任意、IPC 専用 secret 経路）。
        value: Option<SerializableSecretBytes>,
        /// クライアント側で生成した UTC 時刻（RFC3339）。
        #[serde(with = "time::serde::rfc3339")]
        now: OffsetDateTime,
    },
    /// レコード削除。
    RemoveRecord {
        /// 対象 ID。
        id: RecordId,
    },
}

impl IpcRequest {
    /// variant 名（log 出力等で全体 Debug を避けるため）。
    #[must_use]
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Handshake { .. } => "handshake",
            Self::ListRecords => "list_records",
            Self::AddRecord { .. } => "add_record",
            Self::EditRecord { .. } => "edit_record",
            Self::RemoveRecord { .. } => "remove_record",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_name_handshake() {
        let req = IpcRequest::Handshake {
            client_version: IpcProtocolVersion::V1,
        };
        assert_eq!(req.variant_name(), "handshake");
    }

    #[test]
    fn test_variant_name_list_records() {
        assert_eq!(IpcRequest::ListRecords.variant_name(), "list_records");
    }

    #[test]
    fn test_variant_name_remove_record() {
        let id = RecordId::try_from_str("01234567-0123-7000-8000-0123456789ab").unwrap();
        let req = IpcRequest::RemoveRecord { id };
        assert_eq!(req.variant_name(), "remove_record");
    }
}
