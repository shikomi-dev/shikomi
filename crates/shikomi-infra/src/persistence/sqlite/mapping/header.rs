//! `VaultHeader` ↔ `SQLite` 行のマッピング。
//!
//! Sub-A 改訂: `WrappedVek` の内部構造分離型化に伴い、暗号化ヘッダの BLOB シリアライズ
//! 形式は **Sub-D で `bincode` 等の正式フォーマットに確定する**。
//! Sub-A 〜 Sub-C 期間中、暗号化ヘッダの永続化は型レベルで未実装とし、本マッピング層は
//! `UnsupportedYet { feature: "encrypted vault header (Sub-D)" }` を即返す。
//! `repository::save_inner` / `repository::load` も Encrypted モードを Fail Fast で
//! 弾いており、本層は二重防御として動作する。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use shikomi_core::{ProtectionMode, VaultHeader, VaultVersion};

use crate::persistence::error::{CorruptedReason, PersistenceError};

use super::{params::HeaderParams, Mapping};

/// Sub-D で実装予定の暗号化ヘッダ永続化の追跡 Issue 番号 (Epic #37 / Sub-D #42)。
const TRACKING_ISSUE_ENCRYPTED_HEADER: Option<u32> = Some(42);

impl Mapping {
    /// `VaultHeader` → `HeaderParams` に変換する。
    ///
    /// # Errors
    ///
    /// - `created_at` の RFC3339 フォーマット失敗: `PersistenceError::Corrupted`
    /// - 暗号化モードヘッダ: `PersistenceError::UnsupportedYet`
    ///   (Sub-D で `WrappedVek` の正式 BLOB シリアライザを実装するまで未対応)
    pub(crate) fn vault_header_to_params(
        header: &VaultHeader,
    ) -> Result<HeaderParams, PersistenceError> {
        let protection_mode = header.protection_mode().as_persisted_str();
        let vault_version = header.version().value();
        let created_at_rfc3339 =
            header
                .created_at()
                .format(&Rfc3339)
                .map_err(|e| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: None,
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!("failed to serialize created_at as RFC3339: {e}"),
                    },
                    source: None,
                })?;

        match header.protection_mode() {
            ProtectionMode::Plaintext => Ok(HeaderParams {
                protection_mode,
                vault_version,
                created_at_rfc3339,
                kdf_salt: None,
                wrapped_vek_by_pw: None,
                wrapped_vek_by_recovery: None,
            }),
            ProtectionMode::Encrypted => Err(PersistenceError::UnsupportedYet {
                feature: "encrypted vault header (Sub-D)",
                tracking_issue: TRACKING_ISSUE_ENCRYPTED_HEADER,
            }),
        }
    }

    /// `SQLite` 行 → `VaultHeader` に変換する。
    ///
    /// # Errors
    ///
    /// - 保護モード不明: `PersistenceError::Corrupted`
    /// - vault バージョン範囲外: `PersistenceError::Corrupted`
    /// - RFC3339 パース失敗: `PersistenceError::Corrupted`
    /// - 暗号化モード行: `PersistenceError::UnsupportedYet`
    ///   (Sub-D で `WrappedVek` の正式 BLOB デシリアライザを実装するまで未対応)
    pub(crate) fn row_to_vault_header(
        row: &rusqlite::Row<'_>,
    ) -> Result<VaultHeader, PersistenceError> {
        // Col 0: protection_mode
        let protection_mode_raw: String = row
            .get(0)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let protection_mode = ProtectionMode::try_from_persisted_str(&protection_mode_raw)
            .map_err(|e| PersistenceError::Corrupted {
                table: "vault_header",
                row_key: Some("1".to_string()),
                reason: CorruptedReason::UnknownProtectionMode {
                    raw: protection_mode_raw.clone(),
                },
                source: Some(e),
            })?;

        // Col 1: vault_version (INTEGER → i64 → u16)
        let vault_version_raw: i64 = row
            .get(1)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let vault_version_u16 =
            u16::try_from(vault_version_raw).map_err(|_| PersistenceError::Corrupted {
                table: "vault_header",
                row_key: Some("1".to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!("vault_version {vault_version_raw} out of u16 range"),
                },
                source: None,
            })?;
        let vault_version =
            VaultVersion::try_new(vault_version_u16).map_err(|e| PersistenceError::Corrupted {
                table: "vault_header",
                row_key: Some("1".to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!("unsupported vault version: {vault_version_u16}"),
                },
                source: Some(e),
            })?;

        // Col 2: created_at (RFC3339 TEXT)
        let created_at_raw: String = row
            .get(2)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let created_at = OffsetDateTime::parse(&created_at_raw, &Rfc3339).map_err(|_| {
            PersistenceError::Corrupted {
                table: "vault_header",
                row_key: Some("1".to_string()),
                reason: CorruptedReason::InvalidRfc3339 {
                    column: "created_at",
                    raw: created_at_raw.clone(),
                },
                source: None,
            }
        })?;

        match protection_mode {
            ProtectionMode::Plaintext => VaultHeader::new_plaintext(vault_version, created_at)
                .map_err(|e| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: Some("1".to_string()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: e.to_string(),
                    },
                    source: Some(e),
                }),
            ProtectionMode::Encrypted => Err(PersistenceError::UnsupportedYet {
                feature: "encrypted vault header (Sub-D)",
                tracking_issue: TRACKING_ISSUE_ENCRYPTED_HEADER,
            }),
        }
    }
}
