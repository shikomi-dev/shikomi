//! `Record` ↔ `SQLite` 行のマッピング。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use shikomi_core::{
    Aad, CipherText, NonceBytes, Record, RecordId, RecordKind, RecordLabel, RecordPayload,
    RecordPayloadEncrypted, SecretString, VaultVersion,
};

use crate::persistence::error::{CorruptedReason, PersistenceError};

use super::{params::RecordParams, Mapping};

impl Mapping {
    /// `Record` → `RecordParams` に変換する。
    ///
    /// # Errors
    ///
    /// - `created_at` / `updated_at` の RFC3339 フォーマット失敗: `PersistenceError::Corrupted`
    pub(crate) fn record_to_params<'a>(
        record: &'a Record,
    ) -> Result<RecordParams<'a>, PersistenceError> {
        let id = record.id().to_string();
        let kind = match record.kind() {
            RecordKind::Text => "text",
            RecordKind::Secret => "secret",
        };
        let label = record.label().as_str();
        let created_at =
            record
                .created_at()
                .format(&Rfc3339)
                .map_err(|e| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id.clone()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!("failed to serialize created_at as RFC3339: {e}"),
                    },
                    source: None,
                })?;
        let updated_at =
            record
                .updated_at()
                .format(&Rfc3339)
                .map_err(|e| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id.clone()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!("failed to serialize updated_at as RFC3339: {e}"),
                    },
                    source: None,
                })?;

        match record.payload() {
            RecordPayload::Plaintext(secret) => Ok(RecordParams {
                id,
                kind,
                label,
                payload_variant: "plaintext",
                plaintext_value: Some(secret.expose_secret()),
                nonce: None,
                ciphertext: None,
                aad_bytes: None,
                created_at,
                updated_at,
            }),
            RecordPayload::Encrypted(enc) => Ok(RecordParams {
                id,
                kind,
                label,
                payload_variant: "encrypted",
                plaintext_value: None,
                nonce: Some(enc.nonce().as_array().as_ref()),
                ciphertext: Some(enc.ciphertext().as_bytes()),
                aad_bytes: Some(enc.aad().to_canonical_bytes()),
                created_at,
                updated_at,
            }),
        }
    }

    /// `SQLite` 行 → `Record` に変換する。
    ///
    /// # Errors
    ///
    /// - `RecordId` パース失敗: `PersistenceError::Corrupted`
    /// - 不明な kind/payload_variant: `PersistenceError::Corrupted`
    /// - NULL 違反: `PersistenceError::Corrupted`
    /// - RFC3339 パース失敗: `PersistenceError::Corrupted`
    /// - ドメイン型の構築失敗: `PersistenceError::Corrupted`
    pub(crate) fn row_to_record(row: &rusqlite::Row<'_>) -> Result<Record, PersistenceError> {
        // Col 0: id (TEXT)
        let id_str: String = row
            .get(0)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let record_id =
            RecordId::try_from_str(&id_str).map_err(|e| PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.clone()),
                reason: CorruptedReason::InvalidUuidString {
                    raw: id_str.clone(),
                },
                source: Some(e),
            })?;

        // Col 1: kind (TEXT)
        let kind_str: String = row
            .get(1)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let kind = match kind_str.as_str() {
            "text" => RecordKind::Text,
            "secret" => RecordKind::Secret,
            other => {
                return Err(PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!("unknown kind: {other:?}"),
                    },
                    source: None,
                });
            }
        };

        // Col 2: label (TEXT)
        let label_str: String = row
            .get(2)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let label = RecordLabel::try_new(label_str).map_err(|e| PersistenceError::Corrupted {
            table: "records",
            row_key: Some(id_str.clone()),
            reason: CorruptedReason::InvalidRowCombination {
                detail: format!("invalid label: {e}"),
            },
            source: Some(e),
        })?;

        // Col 3: payload_variant (TEXT)
        let payload_variant: String = row
            .get(3)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // Col 8: created_at (RFC3339 TEXT)
        let created_at_raw: String = row
            .get(8)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let created_at = OffsetDateTime::parse(&created_at_raw, &Rfc3339).map_err(|_| {
            PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.clone()),
                reason: CorruptedReason::InvalidRfc3339 {
                    column: "created_at",
                    raw: created_at_raw.clone(),
                },
                source: None,
            }
        })?;

        // Col 9: updated_at (RFC3339 TEXT)
        let updated_at_raw: String = row
            .get(9)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let updated_at = OffsetDateTime::parse(&updated_at_raw, &Rfc3339).map_err(|_| {
            PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.clone()),
                reason: CorruptedReason::InvalidRfc3339 {
                    column: "updated_at",
                    raw: updated_at_raw.clone(),
                },
                source: None,
            }
        })?;

        // ペイロード構築
        let payload = match payload_variant.as_str() {
            "plaintext" => {
                // Col 4: plaintext_value (TEXT)
                let plaintext: Option<String> = row
                    .get(4)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let value = plaintext.ok_or_else(|| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::NullViolation {
                        column: "plaintext_value",
                    },
                    source: None,
                })?;
                RecordPayload::Plaintext(SecretString::from_string(value))
            }
            "encrypted" => {
                // Col 5: nonce (BLOB, 12 bytes)
                let nonce_raw: Option<Vec<u8>> = row
                    .get(5)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let nonce_bytes = nonce_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::NullViolation { column: "nonce" },
                    source: None,
                })?;
                let nonce =
                    NonceBytes::try_new(&nonce_bytes).map_err(|e| PersistenceError::Corrupted {
                        table: "records",
                        row_key: Some(id_str.clone()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("invalid nonce: {e}"),
                        },
                        source: Some(e),
                    })?;

                // Col 6: ciphertext (BLOB)
                let ct_raw: Option<Vec<u8>> = row
                    .get(6)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let ct_bytes = ct_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::NullViolation {
                        column: "ciphertext",
                    },
                    source: None,
                })?;
                let ciphertext = CipherText::try_new(ct_bytes.into_boxed_slice()).map_err(|e| {
                    PersistenceError::Corrupted {
                        table: "records",
                        row_key: Some(id_str.clone()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("invalid ciphertext: {e}"),
                        },
                        source: Some(e),
                    }
                })?;

                // Col 7: aad (BLOB, 26 bytes)
                let aad_raw: Option<Vec<u8>> = row
                    .get(7)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let aad_bytes = aad_raw.ok_or_else(|| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::NullViolation { column: "aad" },
                    source: None,
                })?;
                if aad_bytes.len() != 26 {
                    return Err(PersistenceError::Corrupted {
                        table: "records",
                        row_key: Some(id_str.clone()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("aad must be 26 bytes, got {}", aad_bytes.len()),
                        },
                        source: None,
                    });
                }
                // Extract vault_version from AAD bytes [16..18]
                let vault_version_raw = u16::from_be_bytes([aad_bytes[16], aad_bytes[17]]);
                let vault_version = VaultVersion::try_new(vault_version_raw).map_err(|e| {
                    PersistenceError::Corrupted {
                        table: "records",
                        row_key: Some(id_str.clone()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("invalid vault version in aad: {e}"),
                        },
                        source: Some(e),
                    }
                })?;

                // Reconstruct Aad from record_id, vault_version, and created_at
                let aad = Aad::new(record_id.clone(), vault_version, created_at).map_err(|e| {
                    PersistenceError::Corrupted {
                        table: "records",
                        row_key: Some(id_str.clone()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("failed to reconstruct aad: {e}"),
                        },
                        source: Some(e),
                    }
                })?;

                let enc = RecordPayloadEncrypted::new(nonce, ciphertext, aad).map_err(|e| {
                    PersistenceError::Corrupted {
                        table: "records",
                        row_key: Some(id_str.clone()),
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: format!("failed to build encrypted payload: {e}"),
                        },
                        source: Some(e),
                    }
                })?;

                RecordPayload::Encrypted(enc)
            }
            other => {
                return Err(PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!(
                            "unknown payload_variant: {other:?}; expected 'plaintext' or 'encrypted'"
                        ),
                    },
                    source: None,
                });
            }
        };

        // Record::new sets both created_at = updated_at = now (truncated to µs)
        let record = Record::new(record_id, kind, label.clone(), payload, created_at);

        // If updated_at differs from created_at, apply it via with_updated_label
        let record = if updated_at != record.created_at() {
            record.with_updated_label(label, updated_at).map_err(|e| {
                PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!("failed to restore updated_at: {e}"),
                    },
                    source: Some(e),
                }
            })?
        } else {
            record
        };

        Ok(record)
    }
}
