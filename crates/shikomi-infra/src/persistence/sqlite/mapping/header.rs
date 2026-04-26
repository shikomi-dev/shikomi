//! `VaultHeader` ↔ `SQLite` 行のマッピング。
//!
//! Sub-D (#42) 改訂: 暗号化モード分岐を解禁。
//! `wrapped_vek_by_pw` 列は composite container BLOB として詰められ
//! (`vault_migration::storage` で構築)、`nonce_counter` / `kdf_params` /
//! `header_aead_envelope` を内包する。本層は BLOB を **不透明な `Vec<u8>` として**
//! SQLite に保存・復元するだけで、暗号化アルゴリズムには無知 (REQ-P11 改訂、
//! 設計書 §責務境界: SqliteVaultRepository は暗号化に「無知」のまま据え置き)。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use shikomi_core::{
    AuthTag, KdfSalt, NonceBytes, ProtectionMode, VaultHeader, VaultVersion, WrappedVek,
};

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

        match header {
            VaultHeader::Plaintext(_) => Ok(HeaderParams {
                protection_mode,
                vault_version,
                created_at_rfc3339,
                kdf_salt: None,
                wrapped_vek_by_pw: None,
                wrapped_vek_by_recovery: None,
            }),
            VaultHeader::Encrypted(_) => {
                let kdf_salt = header.kdf_salt().map(|s| s.as_array().to_vec()).ok_or(
                    PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: None,
                        reason: CorruptedReason::NullViolation { column: "kdf_salt" },
                        source: None,
                    },
                )?;
                let wrapped_vek_by_pw = header
                    .wrapped_vek_by_pw()
                    .map(serialize_wrapped_vek)
                    .ok_or(PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: None,
                        reason: CorruptedReason::NullViolation {
                            column: "wrapped_vek_by_pw",
                        },
                        source: None,
                    })?;
                let wrapped_vek_by_recovery = header
                    .wrapped_vek_by_recovery()
                    .map(serialize_wrapped_vek)
                    .ok_or(PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: None,
                        reason: CorruptedReason::NullViolation {
                            column: "wrapped_vek_by_recovery",
                        },
                        source: None,
                    })?;
                Ok(HeaderParams {
                    protection_mode,
                    vault_version,
                    created_at_rfc3339,
                    kdf_salt: Some(kdf_salt),
                    wrapped_vek_by_pw: Some(wrapped_vek_by_pw),
                    wrapped_vek_by_recovery: Some(wrapped_vek_by_recovery),
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
    /// - 暗号化モードで NULL 必須カラム欠落: `PersistenceError::Corrupted`
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
                // Col 3: kdf_salt (BLOB 16B)
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
                            detail: format!("invalid kdf_salt: {e}"),
                        },
                        source: Some(e),
                    })?;

                // Col 4: wrapped_vek_by_pw (BLOB, composite container >= 32B)
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
                let wrapped_vek_by_pw = deserialize_wrapped_vek(&pw_bytes).map_err(|e| {
                    PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: Some("1".to_string()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("invalid wrapped_vek_by_pw: {e}"),
                        },
                        source: Some(e),
                    }
                })?;

                // Col 5: wrapped_vek_by_recovery (BLOB)
                let recovery_raw: Option<Vec<u8>> = row
                    .get(5)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let recovery_bytes = recovery_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "vault_header",
                    row_key: Some("1".to_string()),
                    reason: CorruptedReason::NullViolation {
                        column: "wrapped_vek_by_recovery",
                    },
                    source: None,
                })?;
                let wrapped_vek_by_recovery =
                    deserialize_wrapped_vek(&recovery_bytes).map_err(|e| {
                        PersistenceError::Corrupted {
                            table: "vault_header",
                            row_key: Some("1".to_string()),
                            reason: CorruptedReason::InvalidRowCombination {
                                detail: format!("invalid wrapped_vek_by_recovery: {e}"),
                            },
                            source: Some(e),
                        }
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

/// `WrappedVek` を `nonce(12) ‖ tag(16) ‖ ciphertext(N)` 形式に直列化する
/// (vault_header 列保存用、N は composite container 含み可変)。
fn serialize_wrapped_vek(w: &WrappedVek) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + 16 + w.ciphertext().len());
    out.extend_from_slice(w.nonce().as_array());
    out.extend_from_slice(w.tag().as_array());
    out.extend_from_slice(w.ciphertext());
    out
}

/// `nonce(12) ‖ tag(16) ‖ ciphertext(N)` BLOB を `WrappedVek` に復元する。
fn deserialize_wrapped_vek(bytes: &[u8]) -> Result<WrappedVek, shikomi_core::DomainError> {
    if bytes.len() < 12 + 16 + 32 {
        // 構造的最小サイズ: nonce 12 + tag 16 + ciphertext (>=32 = WrappedVek 最小)
        return Err(shikomi_core::DomainError::InvalidVaultHeader(
            shikomi_core::InvalidVaultHeaderReason::WrappedVekTooShort,
        ));
    }
    let mut nonce_arr = [0u8; 12];
    nonce_arr.copy_from_slice(&bytes[0..12]);
    let mut tag_arr = [0u8; 16];
    tag_arr.copy_from_slice(&bytes[12..28]);
    let ciphertext = bytes[28..].to_vec();
    WrappedVek::new(
        ciphertext,
        NonceBytes::from_random(nonce_arr),
        AuthTag::from_array(tag_arr),
    )
}

// 不使用の型のみエクスポート - `DomainError` 構築に必要な場合に備えて
#[allow(unused_imports)]
use shikomi_core::InvalidVaultHeaderReason as _;

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{KdfSalt, NonceBytes};

    fn dummy_wrapped_vek() -> WrappedVek {
        WrappedVek::new(
            vec![0xAAu8; 48], // composite container 模倣 (>= 32B)
            NonceBytes::from_random([0xBBu8; 12]),
            AuthTag::from_array([0xCCu8; 16]),
        )
        .unwrap()
    }

    #[test]
    fn serialize_deserialize_wrapped_vek_round_trip() {
        let original = dummy_wrapped_vek();
        let bytes = serialize_wrapped_vek(&original);
        let restored = deserialize_wrapped_vek(&bytes).unwrap();
        assert_eq!(restored.ciphertext(), original.ciphertext());
        assert_eq!(restored.nonce().as_array(), original.nonce().as_array());
        assert_eq!(restored.tag().as_array(), original.tag().as_array());
    }

    #[test]
    fn deserialize_wrapped_vek_too_short_returns_err() {
        let err = deserialize_wrapped_vek(&[0u8; 30]).unwrap_err();
        assert!(matches!(
            err,
            shikomi_core::DomainError::InvalidVaultHeader(_)
        ));
    }

    #[test]
    fn vault_header_to_params_for_encrypted_returns_some_blobs() {
        let kdf_salt = KdfSalt::from_array([0u8; 16]);
        let wrapped = dummy_wrapped_vek();
        let header = VaultHeader::new_encrypted(
            VaultVersion::CURRENT,
            OffsetDateTime::UNIX_EPOCH,
            kdf_salt,
            wrapped.clone(),
            wrapped.clone(),
        )
        .unwrap();
        let params = Mapping::vault_header_to_params(&header).unwrap();
        assert_eq!(params.protection_mode, "encrypted");
        assert!(params.kdf_salt.is_some());
        assert!(params.wrapped_vek_by_pw.is_some());
        assert!(params.wrapped_vek_by_recovery.is_some());
    }

    #[test]
    fn vault_header_to_params_for_plaintext_returns_none_blobs() {
        let header =
            VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap();
        let params = Mapping::vault_header_to_params(&header).unwrap();
        assert_eq!(params.protection_mode, "plaintext");
        assert!(params.kdf_salt.is_none());
        assert!(params.wrapped_vek_by_pw.is_none());
        assert!(params.wrapped_vek_by_recovery.is_none());
    }
}
