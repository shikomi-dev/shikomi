//! IPC プロトコルバージョン。
//!
//! 破壊的変更時にバリアントを追加する（`V2` / `V3` …）。`#[non_exhaustive]` により
//! 後続 feature の追加が非破壊変更として扱える（`VaultVersion` の前例踏襲）。

use std::fmt;

use serde::{Deserialize, Serialize};

// -------------------------------------------------------------------
// IpcProtocolVersion
// -------------------------------------------------------------------

/// IPC プロトコルバージョン enum。
///
/// 設計根拠: docs/features/daemon-ipc/detailed-design/protocol-types.md §`IpcProtocolVersion`
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcProtocolVersion {
    /// 初期バージョン（`Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord`）。
    V1,
}

impl IpcProtocolVersion {
    /// 本ビルドが対応するプロトコルバージョンを返す。
    ///
    /// daemon / cli 双方が `Handshake` でこの値を交換し、不一致なら接続切断（Fail Fast）。
    #[must_use]
    pub const fn current() -> Self {
        Self::V1
    }
}

impl fmt::Display for IpcProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1 => f.write_str("v1"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_returns_v1() {
        assert_eq!(IpcProtocolVersion::current(), IpcProtocolVersion::V1);
    }

    #[test]
    fn test_display_v1_returns_v1_string() {
        assert_eq!(IpcProtocolVersion::V1.to_string(), "v1");
    }
}
