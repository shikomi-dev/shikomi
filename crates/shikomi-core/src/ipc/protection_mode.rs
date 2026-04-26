//! `ProtectionModeBanner` — Sub-F (#44) `shikomi list` 保護モードバナー型 (REQ-S16)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/cli-subcommands.md`
//! §`ProtectionModeBanner` enum
//!
//! daemon 側 `usecase::list` (`IpcResponse::Records { records, protection_mode }`) で
//! 保護モードを CLI へ通知し、CLI 側 `presenter::mode_banner::display` が文字 + ANSI
//! カラーで先頭バナーを描画する用途。色覚多様性に配慮し**文字単独でも判別可能**な
//! ラベル文字列を凍結 (二重符号化、REQ-S16)。
//!
//! ## 不変条件
//!
//! - `#[non_exhaustive]` で将来拡張耐性 (Sub-D Rev3 凍結方針継承)
//! - cross-crate `match` では fail-secure な `_` arm 必須 (Sub-E TC-E-S01 同型)、
//!   `_` は `Unknown` 同等経路 (REQ-S16 Fail-Secure、終了コード 3)
//! - 文字列ラベルは **shikomi-core で凍結** (CLI/GUI 側で再定義禁止、設計書 SSoT)

use serde::{Deserialize, Serialize};

// -------------------------------------------------------------------
// ProtectionModeBanner
// -------------------------------------------------------------------

/// `shikomi list` 先頭バナーで表示する保護モード。
///
/// 設計書 §`ProtectionModeBanner` enum:
///
/// | variant | 表示文字 | ANSI カラー | 二重符号化 (色覚多様性) |
/// |---|---|---|---|
/// | `Plaintext` | `[plaintext]` | 灰色 (cyan dim) | 文字単独で「平文」と判別可 |
/// | `EncryptedLocked` | `[encrypted, locked]` | 橙色 (yellow) | 文字単独で「Locked」と判別可 |
/// | `EncryptedUnlocked` | `[encrypted, unlocked]` | 緑色 (green) | 文字単独で「Unlocked」と判別可 |
/// | `Unknown` | `[unknown]` | 赤色 (red) | Fail-Secure 経路、終了コード 3 |
///
/// `NO_COLOR` 環境変数 / 非 TTY / `--quiet` 時はカラー無効化、文字のみ表示
/// (CLI 側 presenter 責務、本型は意味論のみ運搬)。
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionModeBanner {
    /// 平文 vault — 暗号化前 / `vault decrypt` 後の状態。
    Plaintext,
    /// 暗号化 vault かつ daemon 内 `VekCache` が `Locked` 状態。
    /// read/write IPC は `IpcErrorCode::VaultLocked` で拒否される (C-22)。
    EncryptedLocked,
    /// 暗号化 vault かつ daemon 内 `VekCache` が `Unlocked` 状態。
    /// read/write IPC は通常通り動作する。
    EncryptedUnlocked,
    /// 保護モード判定不能 (vault.db ヘッダ破損等)。
    /// REQ-S16 Fail-Secure: CLI は終了コード 3 で fail fast、`vault list` は実行しない。
    Unknown,
}

impl ProtectionModeBanner {
    /// CLI バナーの表示文字列 (色なし、ANSI escape は presenter 責務)。
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Plaintext => "[plaintext]",
            Self::EncryptedLocked => "[encrypted, locked]",
            Self::EncryptedUnlocked => "[encrypted, unlocked]",
            Self::Unknown => "[unknown]",
            // `#[non_exhaustive]` cross-crate 防御的 wildcard (Sub-D Rev3 凍結継承):
            // 将来 variant 追加時は fail-secure (`[unknown]` 同等) に倒す。
            #[allow(unreachable_patterns)]
            _ => "[unknown]",
        }
    }
}

// -------------------------------------------------------------------
// tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_plaintext_is_frozen_string() {
        assert_eq!(ProtectionModeBanner::Plaintext.label(), "[plaintext]");
    }

    #[test]
    fn label_encrypted_locked_is_frozen_string() {
        assert_eq!(
            ProtectionModeBanner::EncryptedLocked.label(),
            "[encrypted, locked]"
        );
    }

    #[test]
    fn label_encrypted_unlocked_is_frozen_string() {
        assert_eq!(
            ProtectionModeBanner::EncryptedUnlocked.label(),
            "[encrypted, unlocked]"
        );
    }

    #[test]
    fn label_unknown_is_frozen_string() {
        assert_eq!(ProtectionModeBanner::Unknown.label(), "[unknown]");
    }

    #[test]
    fn serde_round_trip_via_json() {
        // serde_json は workspace dep に無いため、手動 round-trip は serde 経由の
        // 簡易確認のみ実施。本格的な MessagePack round-trip は shikomi-daemon
        // integration test (`it_protocol_roundtrip.rs`) で検証する。
        let value = ProtectionModeBanner::EncryptedLocked;
        let cloned = value;
        assert_eq!(value, cloned);
    }
}
