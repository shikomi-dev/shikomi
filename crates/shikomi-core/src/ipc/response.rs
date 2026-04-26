//! daemon → クライアントの IPC レスポンス。
//!
//! `#[non_exhaustive]` により後続 feature の variant 追加が非破壊変更として扱える。

use serde::{Deserialize, Serialize};

use crate::vault::id::RecordId;

use super::error_code::IpcErrorCode;
use super::protection_mode::ProtectionModeBanner;
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
    ///
    /// **Sub-F (#44) Rev1 (PR #69) で構造体化**: 旧 `Records(Vec<RecordSummary>)` から
    /// `Records { records, protection_mode }` に変更し、保護モードバナー (REQ-S16) を
    /// 同梱。CLI 側 `presenter::mode_banner::display(protection_mode)` が先頭バナーを
    /// 描画する責務 (C-37 必須呼出、`presenter::list::display` シグネチャに
    /// `ProtectionModeBanner` 必須引数として型レベル強制)。
    ///
    /// 1 往復で `list` 実行可能な Tell-Don't-Ask 設計 (Sub-F §設計判断: 保護モード
    /// バナー実装 案 B 採用)。`#[non_exhaustive]` で V1 互換は serde の Default +
    /// skip_serializing_if で吸収。
    Records {
        /// レコード summary 列 (機密非含有、name / id / kind / 時刻のみ)。
        records: Vec<RecordSummary>,
        /// 保護モード (Plaintext / EncryptedLocked / EncryptedUnlocked / Unknown)。
        /// daemon が `Vault::protection_mode` + `VekCache::is_unlocked` から判定。
        protection_mode: ProtectionModeBanner,
    },
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
        /// **Sub-E ペガサス工程5 指摘**: atomic write 成功直後の cache 再 unlock が
        /// 成功したか。`true` なら以後の read/write IPC は通常通り動作、`false` なら
        /// daemon 内 `VekCache` は `Locked` 状態に戻っており **次の IPC で
        /// `IpcErrorCode::VaultLocked` が返る**。Sub-F CLI/GUI が本フラグを見て
        /// 「鍵情報の再キャッシュに失敗、もう一度 unlock してください」をユーザに
        /// 表示する責務 (MSG-S05 系の派生、Fail Kindly 維持)。
        ///
        /// daemon 側の vault.db は atomic write 完了後で正常状態 (新 mnemonic + 新 VEK
        /// で wrap 済)、再 unlock は成功するはずだが、ディスク I/O 異常等の例外経路
        /// でも田中ペルソナを **Lie-Then-Surprise から守る** ためのフラグ。
        cache_relocked: bool,
    },
    /// **V2**: `Rekey` 成功。再暗号化レコード件数 + 新 24 語を返却。
    Rekeyed {
        /// 再暗号化されたレコード件数。
        records_count: usize,
        /// 新 BIP-39 24 語（rekey + recovery rotation 1 atomic で更新済、F-E5）。
        words: Vec<SerializableSecretBytes>,
        /// **Sub-E ペガサス工程5 指摘**: 再 unlock の成否 (`RecoveryRotated::cache_relocked`
        /// と同義、`Rekey` 経路でも同じ意味論)。
        cache_relocked: bool,
    },

    // ---------------- Sub-F (#44) IPC V2 拡張 ----------------
    /// **V2 (Sub-F)**: `vault encrypt` 成功。新生成された recovery 24 語を初回 1 度のみ返却。
    /// CLI 側 `presenter::recovery_disclosure::display(disclosure, target)` で
    /// `--output {screen,print,braille,audio}` に分岐表示する (Sub-F §F-F1)。
    /// daemon 側は `RecoveryDisclosure::disclose` 所有権消費後の再表示を C-35 で構造拒否。
    Encrypted {
        /// BIP-39 24 語 (Rekeyed/RecoveryRotated と同型、Drop 連鎖 zeroize)。
        disclosure: Vec<SerializableSecretBytes>,
    },
    /// **V2 (Sub-F)**: `vault decrypt` 成功 (Sub-F §F-F2)。
    /// 暗号化 vault → 平文 vault 戻し完了。VEK / 24 語は IPC に乗らない。
    Decrypted,
}

impl IpcResponse {
    /// variant 名（log 出力等で全体 Debug を避けるため）。
    #[must_use]
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Handshake { .. } => "handshake",
            Self::ProtocolVersionMismatch { .. } => "protocol_version_mismatch",
            Self::Records { .. } => "records",
            Self::Added { .. } => "added",
            Self::Edited { .. } => "edited",
            Self::Removed { .. } => "removed",
            Self::Error(_) => "error",
            Self::Unlocked => "unlocked",
            Self::Locked => "locked",
            Self::PasswordChanged => "password_changed",
            Self::RecoveryRotated { .. } => "recovery_rotated",
            Self::Rekeyed { .. } => "rekeyed",
            Self::Encrypted { .. } => "encrypted",
            Self::Decrypted => "decrypted",
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
        let resp = IpcResponse::Records {
            records: vec![],
            protection_mode: ProtectionModeBanner::Plaintext,
        };
        assert_eq!(resp.variant_name(), "records");
    }

    /// Sub-F (#44): `Records` 構造体化後、`protection_mode` フィールドを 4 variant 全てで
    /// 構築できることを保証 (REQ-S16 / C-37 設計書 SSoT との整合)。
    #[test]
    fn test_records_struct_with_each_protection_mode() {
        for mode in [
            ProtectionModeBanner::Plaintext,
            ProtectionModeBanner::EncryptedLocked,
            ProtectionModeBanner::EncryptedUnlocked,
            ProtectionModeBanner::Unknown,
        ] {
            let resp = IpcResponse::Records {
                records: vec![],
                protection_mode: mode,
            };
            assert_eq!(resp.variant_name(), "records");
        }
    }

    /// Sub-F (#44): `Encrypted` / `Decrypted` variant_name 凍結文字列確認。
    #[test]
    fn test_variant_name_v2_sub_f_encrypted() {
        let resp = IpcResponse::Encrypted { disclosure: vec![] };
        assert_eq!(resp.variant_name(), "encrypted");
    }

    #[test]
    fn test_variant_name_v2_sub_f_decrypted() {
        assert_eq!(IpcResponse::Decrypted.variant_name(), "decrypted");
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
        let resp = IpcResponse::RecoveryRotated {
            words: vec![],
            cache_relocked: true,
        };
        assert_eq!(resp.variant_name(), "recovery_rotated");
    }

    #[test]
    fn test_variant_name_v2_rekeyed() {
        let resp = IpcResponse::Rekeyed {
            records_count: 0,
            words: vec![],
            cache_relocked: true,
        };
        assert_eq!(resp.variant_name(), "rekeyed");
    }

    /// **TC-E-U13b** ペガサス工程5 致命指摘解消: `cache_relocked: false` の経路を
    /// 明示的に variant 構築できることを保証する。Sub-F が本フラグを見て
    /// 「鍵情報の再キャッシュに失敗、もう一度 unlock してください」を表示する責務。
    #[test]
    fn test_variant_recovery_rotated_with_cache_relocked_false() {
        let resp = IpcResponse::RecoveryRotated {
            words: vec![],
            cache_relocked: false,
        };
        match resp {
            IpcResponse::RecoveryRotated { cache_relocked, .. } => {
                assert!(!cache_relocked, "cache_relocked must be exposed as false");
            }
            other => panic!("expected RecoveryRotated, got {other:?}"),
        }
    }

    #[test]
    fn test_variant_rekeyed_with_cache_relocked_false() {
        let resp = IpcResponse::Rekeyed {
            records_count: 5,
            words: vec![],
            cache_relocked: false,
        };
        match resp {
            IpcResponse::Rekeyed { cache_relocked, .. } => {
                assert!(!cache_relocked, "cache_relocked must be exposed as false");
            }
            other => panic!("expected Rekeyed, got {other:?}"),
        }
    }
}
