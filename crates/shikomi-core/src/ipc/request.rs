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

    // ---------------- Sub-E (#43) IPC V2 拡張 ----------------
    /// **V2 only**: vault のロック解除。daemon 内 `VekCache` を `Unlocked` 遷移させる。
    /// `recovery: Some` の場合は recovery 24 語経路で unlock。
    ///
    /// daemon ハンドラは handshake で `client_version == V2` を確認済の場合のみ受理。
    /// V1 client が送信した場合は `IpcErrorCode::ProtocolDowngrade` 拒否（C-28）。
    Unlock {
        /// マスターパスワード（IPC 専用 secret 経路、Drop 時 zeroize）。
        master_password: SerializableSecretBytes,
        /// recovery 24 語（Some なら recovery 経路、None ならパスワード経路）。
        ///
        /// 24 個の文字列を保持（BIP-39 wordlist 検証は daemon 側 Sub-D 経由で実施）。
        recovery: Option<Vec<SerializableSecretBytes>>,
    },

    /// **V2 only**: vault を明示ロック。VEK を即 zeroize。
    Lock,

    /// **V2 only**: マスターパスワード変更（O(1)、VEK 不変、`wrapped_VEK_by_pw` のみ再 wrap）。
    ChangePassword {
        /// 旧マスターパスワード。
        old: SerializableSecretBytes,
        /// 新マスターパスワード。
        new: SerializableSecretBytes,
    },

    /// **V2 only**: recovery 24 語ローテーション（rekey と組み合わせ atomic 化、F-E5）。
    /// daemon は `VaultMigration::rekey_with_recovery_rotation` を呼出、新 24 語を応答に含める。
    RotateRecovery {
        /// マスターパスワード（再認証用）。
        master_password: SerializableSecretBytes,
    },

    /// **V2 only**: VEK 入替 + recovery rotation atomic 実行（F-E5）。
    /// `RotateRecovery` と内部実装は同一、外向き名称を使い分け（明示的 rekey 用途）。
    Rekey {
        /// マスターパスワード（再認証用）。
        master_password: SerializableSecretBytes,
    },

    // ---------------- Sub-F (#44) IPC V2 拡張 ----------------
    /// **V2 only (Sub-F)**: 平文 vault → 暗号化 vault 初回マイグレーション (F-F1)。
    /// daemon は Sub-D `VaultMigration::encrypt_vault` を呼出、新 24 語を `Encrypted`
    /// レスポンスで返却。
    Encrypt {
        /// マスターパスワード (Sub-A `MasterPassword::new` の強度ゲート対象)。
        master_password: SerializableSecretBytes,
        /// `--accept-limits` フラグ (REQ-S08 強度ゲート緩和の明示同意)。
        accept_limits: bool,
    },

    /// **V2 only (Sub-F)**: 暗号化 vault → 平文 vault 戻し (F-F2)。
    /// daemon は Sub-D `VaultMigration::decrypt_vault` を呼出。**`DecryptConfirmation`
    /// は CLI 側で `subtle::ConstantTimeEq` 比較 + paste 抑制 + 大文字検証を済ませた**
    /// 結果として **`confirmed: true`** で IPC に乗る (Sub-D Rev2 凍結契約、daemon 側で
    /// `DecryptConfirmation::confirm()` を構築する経路)。
    Decrypt {
        /// マスターパスワード。
        master_password: SerializableSecretBytes,
        /// 確認入力検証通過済みフラグ (CLI 側で `DECRYPT` 文字列 + paste 抑制 +
        /// 大文字一致を確認済の証跡。`false` の場合 daemon は受理拒否)。
        confirmed: bool,
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
            Self::Unlock { .. } => "unlock",
            Self::Lock => "lock",
            Self::ChangePassword { .. } => "change_password",
            Self::RotateRecovery { .. } => "rotate_recovery",
            Self::Rekey { .. } => "rekey",
            Self::Encrypt { .. } => "encrypt",
            Self::Decrypt { .. } => "decrypt",
        }
    }

    /// V2 専用 variant か（V1 サブセットなら false、V2 新規 7 件なら true）。
    /// handshake 許可リスト検証 (C-28) で `client_version` との組合せを判定する用途。
    /// Sub-F で `Encrypt` / `Decrypt` 2 件追加 (合計 7 件)。
    #[must_use]
    pub fn is_v2_only(&self) -> bool {
        matches!(
            self,
            Self::Unlock { .. }
                | Self::Lock
                | Self::ChangePassword { .. }
                | Self::RotateRecovery { .. }
                | Self::Rekey { .. }
                | Self::Encrypt { .. }
                | Self::Decrypt { .. }
        )
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

    // -----------------------------------------------------------------
    // TC-E-U13: V2 variant_name 全網羅 (Sub-E test-design §14.4)
    // -----------------------------------------------------------------
    //
    // 設計書凍結文字列:
    //   IpcRequest V2: "unlock" / "lock" / "change_password" /
    //                  "rotate_recovery" / "rekey"
    // `is_v2_only()` も同 5 variant 全てで true、V1 サブセット 5 variant では
    // false であることを担保する。

    use crate::ipc::secret_bytes::SerializableSecretBytes;
    use crate::secret::SecretBytes;

    fn empty_secret() -> SerializableSecretBytes {
        SerializableSecretBytes::new(SecretBytes::from_vec(b"x".to_vec()))
    }

    #[test]
    fn test_variant_name_v2_unlock() {
        let req = IpcRequest::Unlock {
            master_password: empty_secret(),
            recovery: None,
        };
        assert_eq!(req.variant_name(), "unlock");
        assert!(req.is_v2_only(), "Unlock must be V2-only");
    }

    #[test]
    fn test_variant_name_v2_lock() {
        let req = IpcRequest::Lock;
        assert_eq!(req.variant_name(), "lock");
        assert!(req.is_v2_only(), "Lock must be V2-only");
    }

    #[test]
    fn test_variant_name_v2_change_password() {
        let req = IpcRequest::ChangePassword {
            old: empty_secret(),
            new: empty_secret(),
        };
        assert_eq!(req.variant_name(), "change_password");
        assert!(req.is_v2_only(), "ChangePassword must be V2-only");
    }

    #[test]
    fn test_variant_name_v2_rotate_recovery() {
        let req = IpcRequest::RotateRecovery {
            master_password: empty_secret(),
        };
        assert_eq!(req.variant_name(), "rotate_recovery");
        assert!(req.is_v2_only(), "RotateRecovery must be V2-only");
    }

    #[test]
    fn test_variant_name_v2_rekey() {
        let req = IpcRequest::Rekey {
            master_password: empty_secret(),
        };
        assert_eq!(req.variant_name(), "rekey");
        assert!(req.is_v2_only(), "Rekey must be V2-only");
    }

    #[test]
    fn test_v1_variants_are_not_v2_only() {
        assert!(!IpcRequest::ListRecords.is_v2_only());
        assert!(!IpcRequest::Handshake {
            client_version: IpcProtocolVersion::V2,
        }
        .is_v2_only());
        let id = RecordId::try_from_str("01234567-0123-7000-8000-0123456789ab").unwrap();
        assert!(!IpcRequest::RemoveRecord { id }.is_v2_only());
    }
}
