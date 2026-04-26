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

    // ---------------- Sub-E (#43) IPC V2 拡張 ----------------
    /// **V2**: vault が `Locked` 状態のまま read/write IPC を受信。MSG-S09 (c) キャッシュ揮発経路。
    /// daemon 内で `VaultUnlockState::Locked` の場合に各 V2 ハンドラ入口で型レベル拒否（C-22）。
    VaultLocked,
    /// **V2**: 連続 unlock 失敗 5 回後の指数バックオフ中。MSG-S09 (a) パスワード違いカテゴリ +
    /// 待機時間の併記。`wait_secs` は数値表示許容（Sub-D Rev5 の nonce 数値非表示とは別経路、
    /// `vek-cache-and-ipc.md §UnlockBackoff Fail-Secure 契約` 参照）。
    BackoffActive {
        /// 次回試行可能までの待機秒数（攻撃面なし、ユーザ表示用）。
        wait_secs: u32,
    },
    /// **V2**: パスワード経路 unlock が `MasterPassword::new` 失敗等で進めない時、
    /// recovery 経路 (`vault unlock --recovery`) への誘導を要求する。
    /// MSG-S09 (a) パスワード違いカテゴリ「リカバリ用 24 語でのアンロックも可能」案内
    /// （Sub-D Rev5 ペガサス指摘 + `MigrationError::RecoveryRequired` 透過契約）。
    RecoveryRequired,
    /// **V2**: V1 クライアントが V2 専用 variant を送信した（C-28 handshake 許可リスト違反）、
    /// または handshake 完了前に variant を送信した（C-29 handshake 必須）。
    /// MSG-S15 経路。直後に接続切断。
    ProtocolDowngrade,
    /// **V2**: 暗号エラー透過（`reason` に kebab-case 固定文言、例: "wrong-password" /
    /// "aead-tag-mismatch" / "nonce-limit-exceeded" / "weak-password" / "kdf-failed" /
    /// "invalid-mnemonic"）。
    /// 内部詳細秘匿のため `MigrationError → IpcError` マッピング表 (`vek-cache-and-ipc.md`)
    /// で 1:1 集約、CLI 側は `reason` で `MSG-S08`〜`MSG-S12` に振り分け。
    Crypto {
        /// 固定文言（kebab-case、許容セットは設計書 SSoT）。
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
            Self::VaultLocked => f.write_str("vault is locked, unlock required"),
            Self::BackoffActive { wait_secs } => {
                write!(f, "unlock blocked by backoff for {wait_secs}s")
            }
            Self::RecoveryRequired => f.write_str("recovery path required"),
            Self::ProtocolDowngrade => f.write_str("V1 client cannot use V2-only request"),
            Self::Crypto { reason } => write!(f, "crypto error: {reason}"),
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

    // -----------------------------------------------------------------
    // TC-E-U13 / TC-E-U11: V2 IpcErrorCode の Display 文字列固定確認
    // -----------------------------------------------------------------
    //
    // 設計書 §14.4 TC-E-U11: `RecoveryRequired` Display は「recovery path required」を含む。
    // §14.4 TC-E-U13: V2 4 新 variant Display 固定確認。

    #[test]
    fn test_display_vault_locked() {
        let s = IpcErrorCode::VaultLocked.to_string();
        assert!(
            s.contains("vault is locked"),
            "VaultLocked Display must contain 'vault is locked', got: {s}"
        );
    }

    #[test]
    fn test_display_backoff_active_includes_wait_secs() {
        let s = IpcErrorCode::BackoffActive { wait_secs: 30 }.to_string();
        assert!(
            s.contains("30s"),
            "BackoffActive Display must include wait_secs '30s', got: {s}"
        );
    }

    #[test]
    fn test_display_recovery_required() {
        let s = IpcErrorCode::RecoveryRequired.to_string();
        assert!(
            s.contains("recovery path required"),
            "RecoveryRequired Display must contain 'recovery path required', got: {s}"
        );
    }

    #[test]
    fn test_display_protocol_downgrade() {
        let s = IpcErrorCode::ProtocolDowngrade.to_string();
        assert!(
            s.contains("V1 client") || s.contains("V2-only"),
            "ProtocolDowngrade Display should mention V1/V2 boundary, got: {s}"
        );
    }

    #[test]
    fn test_display_crypto_with_kebab_reason() {
        let code = IpcErrorCode::Crypto {
            reason: "wrong-password".to_owned(),
        };
        let s = code.to_string();
        assert!(
            s.contains("wrong-password"),
            "Crypto Display must contain kebab-case reason, got: {s}"
        );
    }
}
