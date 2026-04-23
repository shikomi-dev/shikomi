//! `VaultHeader` ↔ `SQLite` 行のマッピング。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use shikomi_core::{KdfSalt, ProtectionMode, VaultHeader, VaultVersion, WrappedVek};

use crate::persistence::error::{CorruptedReason, PersistenceError};

use super::{params::HeaderParams, Mapping};

impl Mapping {
    /// `VaultHeader` → `HeaderParams` に変換する。
    ///
    /// # Errors
    ///
    /// - `created_at` の RFC3339 フォーマット失敗: `PersistenceError::Corrupted`
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
            ProtectionMode::Encrypted => {
                let kdf_salt = header.kdf_salt().map(|s| s.as_array().to_vec());
                let wrapped_vek_by_pw = header.wrapped_vek_by_pw().map(|w| w.as_bytes().to_vec());
                let wrapped_vek_by_recovery = header
                    .wrapped_vek_by_recovery()
                    .map(|w| w.as_bytes().to_vec());
                Ok(HeaderParams {
                    protection_mode,
                    vault_version,
                    created_at_rfc3339,
                    kdf_salt,
                    wrapped_vek_by_pw,
                    wrapped_vek_by_recovery,
                })
            }
        }
    }

    /// `SQLite` 行 → `VaultHeader` に変換する。
    ///
    /// # Errors
    ///
    /// - 保護モード不明: `PersistenceError::Corrupted`
    /// - vault バージョン範囲外: `PersistenceError::Corrupted`
    /// - RFC3339 パース失敗: `PersistenceError::Corrupted`
    /// - 暗号化フィールドが NULL: `PersistenceError::Corrupted`
    /// - ドメイン型の構築失敗: `PersistenceError::Corrupted`
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
            ProtectionMode::Encrypted => {
                // Col 3: kdf_salt (BLOB)
                let kdf_salt_raw: Option<Vec<u8>> = row
                    .get(3)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let kdf_salt_bytes = kdf_salt_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: Some("1".to_string()),
                    reason: CorruptedReason::NullViolation { column: "kdf_salt" },
                    source: None,
                })?;
                let kdf_salt =
                    KdfSalt::try_new(&kdf_salt_bytes).map_err(|e| PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: Some("1".to_string()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: e.to_string(),
                        },
                        source: Some(e),
                    })?;

                // Col 4: wrapped_vek_by_pw (BLOB)
                let pw_raw: Option<Vec<u8>> = row
                    .get(4)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let pw_bytes = pw_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: Some("1".to_string()),
                    reason: CorruptedReason::NullViolation {
                        column: "wrapped_vek_by_pw",
                    },
                    source: None,
                })?;
                let wrapped_vek_by_pw =
                    WrappedVek::try_new(pw_bytes.into_boxed_slice()).map_err(|e| {
                        PersistenceError::Corrupted {
                            table: "vault_header",
                            row_key: Some("1".to_string()),
                            reason: CorruptedReason::InvalidRowCombination {
                                detail: e.to_string(),
                            },
                            source: Some(e),
                        }
                    })?;

                // Col 5: wrapped_vek_by_recovery (BLOB)
                let rec_raw: Option<Vec<u8>> = row
                    .get(5)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let rec_bytes = rec_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: Some("1".to_string()),
                    reason: CorruptedReason::NullViolation {
                        column: "wrapped_vek_by_recovery",
                    },
                    source: None,
                })?;
                let wrapped_vek_by_recovery = WrappedVek::try_new(rec_bytes.into_boxed_slice())
                    .map_err(|e| PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: Some("1".to_string()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: e.to_string(),
                        },
                        source: Some(e),
                    })?;

                VaultHeader::new_encrypted(
                    vault_version,
                    created_at,
                    kdf_salt,
                    wrapped_vek_by_pw,
                    wrapped_vek_by_recovery,
                )
                .map_err(|e| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: Some("1".to_string()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: e.to_string(),
                    },
                    source: Some(e),
                })
            }
        }
    }
}
