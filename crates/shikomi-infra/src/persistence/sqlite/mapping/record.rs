//! `Record` ↔ `SQLite` 行のマッピング。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use shikomi_core::{
    Aad, CipherText, Hotkey, NonceBytes, Record, RecordId, RecordKind, RecordLabel, RecordPayload,
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
    pub(crate) fn record_to_params(record: &Record) -> Result<RecordParams<'_>, PersistenceError> {
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

        let hotkey_combo = record.hotkey().map(|h| h.as_str().to_owned());

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
                hotkey_combo,
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
                hotkey_combo,
            }),
        }
    }

    /// `SQLite` 行 → `Record` に変換する。
    ///
    /// # Errors
    ///
    /// - `RecordId` パース失敗: `PersistenceError::Corrupted`
    /// - 不明な `kind/payload_variant`: `PersistenceError::Corrupted`
    /// - NULL 違反: `PersistenceError::Corrupted`
    /// - RFC3339 パース失敗: `PersistenceError::Corrupted`
    /// - ドメイン型の構築失敗: `PersistenceError::Corrupted`
    pub(crate) fn row_to_record(row: &rusqlite::Row<'_>) -> Result<Record, PersistenceError> {
        let (record_id, id_str, kind, label, payload_variant, created_at, updated_at) =
            Self::row_to_common_fields(row)?;

        let payload = Self::build_payload(row, &id_str, &payload_variant, &record_id, created_at)?;

        // Col 10 (V2 only): hotkey_combo (TEXT, NULL OK)
        let hotkey_combo_str: Option<String> = row
            .get(10)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let hotkey = hotkey_combo_str
            .map(|s| {
                Hotkey::parse(&s).map_err(|e| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.clone()),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: format!("invalid hotkey_combo: {e}"),
                    },
                    source: None,
                })
            })
            .transpose()?;

        let record = Record::rehydrate(
            record_id, kind, label, payload, created_at, updated_at, hotkey,
        )
        .map_err(|e| PersistenceError::Corrupted {
            table: "records",
            row_key: Some(id_str.clone()),
            reason: CorruptedReason::InvalidRowCombination {
                detail: format!("failed to rehydrate record: {e}"),
            },
            source: Some(e),
        })?;

        Ok(record)
    }

    /// Col 0〜3, 8〜9 の共通フィールドを読み込む（`row_to_record` / `row_to_record_v1` 共用）。
    #[allow(clippy::type_complexity)]
    fn row_to_common_fields(
        row: &rusqlite::Row<'_>,
    ) -> Result<
        (
            RecordId,
            String,
            RecordKind,
            RecordLabel,
            String,
            OffsetDateTime,
            OffsetDateTime,
        ),
        PersistenceError,
    > {
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

        Ok((
            record_id,
            id_str,
            kind,
            label,
            payload_variant,
            created_at,
            updated_at,
        ))
    }

    /// `payload_variant` に応じてペイロードを構築する（V2 スキーマ用）。
    fn build_payload(
        row: &rusqlite::Row<'_>,
        id_str: &str,
        payload_variant: &str,
        record_id: &RecordId,
        created_at: OffsetDateTime,
    ) -> Result<RecordPayload, PersistenceError> {
        match payload_variant {
            "plaintext" => {
                // Col 4: plaintext_value (TEXT)
                let plaintext: Option<String> = row
                    .get(4)
                    .map_err(|e| PersistenceError::Sqlite { source: e })?;
                let value = plaintext.ok_or_else(|| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(id_str.to_string()),
                    reason: CorruptedReason::NullViolation {
                        column: "plaintext_value",
                    },
                    source: None,
                })?;
                Ok(RecordPayload::Plaintext(SecretString::from_string(value)))
            }
            "encrypted" => Self::build_encrypted_payload(row, id_str, record_id, created_at),
            other => Err(PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!(
                        "unknown payload_variant: {other:?}; expected 'plaintext' or 'encrypted'"
                    ),
                },
                source: None,
            }),
        }
    }

    /// 暗号化ペイロード（Col 5〜7）を構築する。
    fn build_encrypted_payload(
        row: &rusqlite::Row<'_>,
        id_str: &str,
        record_id: &RecordId,
        created_at: OffsetDateTime,
    ) -> Result<RecordPayload, PersistenceError> {
        // Col 5: nonce (BLOB, 12 bytes)
        let nonce_raw: Option<Vec<u8>> = row
            .get(5)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let nonce_bytes = nonce_raw.ok_or_else(|| PersistenceError::Corrupted {
            table: "records",
            row_key: Some(id_str.to_string()),
            reason: CorruptedReason::NullViolation { column: "nonce" },
            source: None,
        })?;
        let nonce = NonceBytes::try_new(&nonce_bytes).map_err(|e| PersistenceError::Corrupted {
            table: "records",
            row_key: Some(id_str.to_string()),
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
            row_key: Some(id_str.to_string()),
            reason: CorruptedReason::NullViolation {
                column: "ciphertext",
            },
            source: None,
        })?;
        let ciphertext = CipherText::try_new(ct_bytes.into_boxed_slice()).map_err(|e| {
            PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.to_string()),
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
            row_key: Some(id_str.to_string()),
            reason: CorruptedReason::NullViolation { column: "aad" },
            source: None,
        })?;
        if aad_bytes.len() != 26 {
            return Err(PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!("aad must be 26 bytes, got {}", aad_bytes.len()),
                },
                source: None,
            });
        }
        // Extract vault_version from AAD bytes [16..18]
        let vault_version_raw = u16::from_be_bytes([aad_bytes[16], aad_bytes[17]]);
        let vault_version =
            VaultVersion::try_new(vault_version_raw).map_err(|e| PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!("invalid vault version in aad: {e}"),
                },
                source: Some(e),
            })?;

        // Reconstruct Aad from record_id, vault_version, and created_at
        let aad = Aad::new(record_id.clone(), vault_version, created_at).map_err(|e| {
            PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!("failed to reconstruct aad: {e}"),
                },
                source: Some(e),
            }
        })?;

        let enc = RecordPayloadEncrypted::new(nonce, ciphertext, aad).map_err(|e| {
            PersistenceError::Corrupted {
                table: "records",
                row_key: Some(id_str.to_string()),
                reason: CorruptedReason::InvalidRowCombination {
                    detail: format!("failed to build encrypted payload: {e}"),
                },
                source: Some(e),
            }
        })?;

        Ok(RecordPayload::Encrypted(enc))
    }

    /// `SQLite` 行（V1 スキーマ、`hotkey_combo` カラムなし）→ `Record` に変換する。
    ///
    /// V1 DB の下位互換ロードで使用する。全レコードの `hotkey` は `None` になる。
    ///
    /// # Errors
    ///
    /// `row_to_record` と同じエラーを返すが、Col 10 (`hotkey_combo`) は読まない。
    pub(crate) fn row_to_record_v1(row: &rusqlite::Row<'_>) -> Result<Record, PersistenceError> {
        let (record_id, id_str, kind, label, payload_variant, created_at, updated_at) =
            Self::row_to_common_fields(row)?;

        let payload = Self::build_payload(row, &id_str, &payload_variant, &record_id, created_at)?;

        // V1 スキーマには hotkey_combo なし → None
        let record = Record::rehydrate(
            record_id, kind, label, payload, created_at, updated_at, None,
        )
        .map_err(|e| PersistenceError::Corrupted {
            table: "records",
            row_key: Some(id_str.clone()),
            reason: CorruptedReason::InvalidRowCombination {
                detail: format!("failed to rehydrate record: {e}"),
            },
            source: Some(e),
        })?;

        Ok(record)
    }
}
