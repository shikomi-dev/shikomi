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
///
/// `Unknown` は受信側が認識できない未知のバージョン文字列を吸収する `#[serde(other)]`
/// フォールバック。これにより、未知 version を受け取った daemon が
/// `ProtocolVersionMismatch` 応答を返してから切断できる（fail-secure + 観測可能な
/// diagnostics の両立）。`current()` がこの値を返すことはない。
///
/// バグ参照: BUG-DAEMON-IPC-001（旧実装は decode 失敗で応答なし切断していた）。
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcProtocolVersion {
    /// 初期バージョン（`Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord`）。
    V1,
    /// 受信側が認識できない未知のバージョン文字列を吸収するフォールバック。
    /// 直近のレスポンスで `ProtocolVersionMismatch.client = Unknown` として返り、
    /// 「daemon が知らないバージョンを送られた」ことをクライアントが診断できる。
    /// `current()` がこの値を返すことはない。
    #[serde(other)]
    Unknown,
}

impl IpcProtocolVersion {
    /// 本ビルドが対応するプロトコルバージョンを返す。
    ///
    /// daemon / cli 双方が `Handshake` でこの値を交換し、不一致なら接続切断（Fail Fast）。
    /// `Unknown` は決して返さない（fail-secure 契約）。
    #[must_use]
    pub const fn current() -> Self {
        Self::V1
    }
}

impl fmt::Display for IpcProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1 => f.write_str("v1"),
            Self::Unknown => f.write_str("unknown"),
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

    #[test]
    fn test_display_unknown_returns_unknown_string() {
        assert_eq!(IpcProtocolVersion::Unknown.to_string(), "unknown");
    }

    /// `current()` が `Unknown` を返さないこと（fail-secure 契約）。
    #[test]
    fn test_current_never_returns_unknown() {
        assert_ne!(IpcProtocolVersion::current(), IpcProtocolVersion::Unknown);
    }

    // serde の `#[serde(other)]` 経路（未知文字列 → `Unknown` 吸収）は、
    // `shikomi-core` が `rmp-serde` / `serde_json` に依存しない設計のため、
    // shikomi-daemon の IT (`tc_it_020`) と protocol round-trip IT で実バイナリ経路で検証する。
}
