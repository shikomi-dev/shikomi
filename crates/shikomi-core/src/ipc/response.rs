//! daemon → クライアントの IPC レスポンス。
//!
//! `#[non_exhaustive]` により後続 feature の variant 追加が非破壊変更として扱える。

use serde::{Deserialize, Serialize};

use crate::vault::id::RecordId;

use super::error_code::IpcErrorCode;
use super::secret_bytes::SerializableSecretBytes;
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

    // ---------------- Sub-E (#43) IPC V2 拡張 ----------------
    /// **V2**: `Unlock` 成功。VEK 自体は IPC に乗せず、daemon 内 `VekCache` のみ保持。
    Unlocked,
    /// **V2**: `Lock` 成功（VEK zeroize 完了）。
    Locked,
    /// **V2**: `ChangePassword` 成功（O(1)、VEK 不変）。
    PasswordChanged,
    /// **V2**: `RotateRecovery` 成功。新 24 語を**初回 1 度のみ**返却
    /// （`RecoveryDisclosure::disclose` 所有権消費の IPC 経路実装）。
    /// 受信側は読了後即 zeroize する責務（C-25 同型、Sub-A `RecoveryWords` 哲学継承）。
    RecoveryRotated {
        /// 新 BIP-39 24 語（順序保持、空白区切りでなく Vec で構造化）。
        /// daemon 側 zeroize 経路: F-E4 step 9 (a)〜(d) 全段防衛で型レベル強制。
        words: Vec<SerializableSecretBytes>,
    },
    /// **V2**: `Rekey` 成功。再暗号化レコード件数 + 新 24 語を返却。
    Rekeyed {
        /// 再暗号化されたレコード件数。
        records_count: usize,
        /// 新 BIP-39 24 語（rekey + recovery rotation 1 atomic で更新済、F-E5）。
        words: Vec<SerializableSecretBytes>,
    },
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
            Self::Unlocked => "unlocked",
            Self::Locked => "locked",
            Self::PasswordChanged => "password_changed",
            Self::RecoveryRotated { .. } => "recovery_rotated",
            Self::Rekeyed { .. } => "rekeyed",
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

    // -----------------------------------------------------------------
    // TC-E-U13: V2 IpcResponse variant_name 全網羅
    // -----------------------------------------------------------------
    //
    // 設計書凍結文字列:
    //   "unlocked" / "locked" / "password_changed" / "recovery_rotated" / "rekeyed"

    #[test]
    fn test_variant_name_v2_unlocked() {
        assert_eq!(IpcResponse::Unlocked.variant_name(), "unlocked");
    }

    #[test]
    fn test_variant_name_v2_locked() {
        assert_eq!(IpcResponse::Locked.variant_name(), "locked");
    }

    #[test]
    fn test_variant_name_v2_password_changed() {
        assert_eq!(
            IpcResponse::PasswordChanged.variant_name(),
            "password_changed"
        );
    }

    #[test]
    fn test_variant_name_v2_recovery_rotated() {
        let resp = IpcResponse::RecoveryRotated { words: vec![] };
        assert_eq!(resp.variant_name(), "recovery_rotated");
    }

    #[test]
    fn test_variant_name_v2_rekeyed() {
        let resp = IpcResponse::Rekeyed {
            records_count: 0,
            words: vec![],
        };
        assert_eq!(resp.variant_name(), "rekeyed");
    }
}
