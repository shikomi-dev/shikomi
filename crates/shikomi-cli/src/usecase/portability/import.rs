//! `import_records` UseCase — JSON ファイルから vault へレコードを import する。
//!
//! 設計根拠: docs/features/data-portability/cli/detailed-design/usecase.md
//! §`usecase/portability/import.rs` の設計詳細
//!
//! # 設計判断: IPC 経路廃止・SQLite 一本化
//! Import も export と同様に常に `SqliteVaultRepository` を使用する。
//! IPC per-record `add_record()` は途中クラッシュで vault が半書き込み状態になり
//! `R1-DP-09` の atomicity 要件に非適合。SQLite `repo.save()` が atomic write を保証する。
//!
//! # セキュリティ考慮
//! - `serde_json::from_reader` によるストリーミングパース（OOM 防止、threat-model.md §7.5）
//! - `repo.save()` による atomic write（R1-DP-09）
//! - `import_record_to_domain` で `Redacted` ペイロードに到達した場合の `unreachable!`（告知的プログラミング）

use std::collections::HashSet;

use shikomi_core::portability::ExportRecordPayload;
use shikomi_core::portability::{ImportPayload, ImportValidator};
use shikomi_core::{
    Hotkey, Record, RecordId, RecordLabel, RecordPayload, SecretString, VaultHeader, VaultVersion,
};
use shikomi_infra::persistence::{PersistenceError, VaultRepository};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::cli::{ImportArgs, OnConflictArg};
use crate::error::CliError;

use super::error::DataPortabilityError;

// -------------------------------------------------------------------
// ImportSummary
// -------------------------------------------------------------------

/// `import_records` の正常系戻り値。
#[derive(Debug)]
pub struct ImportSummary {
    /// 新規追加したレコード件数。
    pub added: usize,
    /// 衝突により skip したレコード件数（`--on-conflict skip` 時のみ非ゼロ）。
    pub skipped: usize,
    /// 既存レコードを上書きしたレコード件数（`--on-conflict overwrite` 時のみ非ゼロ）。
    pub overwritten: usize,
}

// -------------------------------------------------------------------
// import_records
// -------------------------------------------------------------------

/// JSON ファイルから vault へレコードを import する UseCase 関数（単一 SQLite 経路）。
///
/// 設計根拠: docs/features/data-portability/cli/detailed-design/usecase.md §`import_records` 関数
///
/// # 処理順序
/// 1. import ファイルを開く
/// 2. `serde_json::from_reader` でストリーミングパース
/// 3. vault の準備（存在すれば `load`、なければ `Vault::new`）
/// 4. 既存 ID セット構築
/// 5. `ImportValidator::validate`
/// 6. `--on-conflict error` で衝突検出 → エラー
/// 7. 各レコードを走査して add / skip / overwrite
/// 8. `repo.save` で atomic write
/// 9. `ImportSummary` 返却
///
/// # Errors
/// - `CliError::ExportImportVaultLocked` — vault がロック済み
/// - `CliError::ImportDeserializationFailed` — JSON パース失敗
/// - `CliError::ImportConflict` — `--on-conflict error` で衝突検出
/// - `CliError::ImportValidationFailed` — `ImportValidator::validate` 失敗
/// - `CliError::Persistence` — I/O / SQLite エラー
pub fn import_records(
    repo: &dyn VaultRepository,
    args: &ImportArgs,
    now: OffsetDateTime,
) -> Result<ImportSummary, CliError> {
    // Step 1: ファイルを開く
    let file = std::fs::File::open(&args.input).map_err(DataPortabilityError::IoError)?;

    // Step 2: ストリーミングパース（OOM 防止: read_to_string は使用しない、threat-model.md §7.5）
    let payload: ImportPayload = serde_json::from_reader(file).map_err(|e| {
        CliError::from(DataPortabilityError::DeserializationFailed {
            reason: e.to_string(),
        })
    })?;

    // Step 3: vault の準備
    let mut vault = if repo.exists()? {
        match repo.load() {
            Ok(v) => v,
            // Issue #146: SQLITE_BUSY（busy_timeout 2000ms 超過後も未解消）→ VaultBusy
            Err(PersistenceError::DatabaseBusy) => {
                return Err(DataPortabilityError::VaultBusy.into());
            }
            Err(e) => {
                let cli_err = CliError::from(e);
                return Err(match cli_err {
                    CliError::EncryptionUnsupported => DataPortabilityError::VaultLocked.into(),
                    other => other,
                });
            }
        }
    } else {
        shikomi_core::Vault::new(
            VaultHeader::new_plaintext(VaultVersion::CURRENT, now).map_err(CliError::Domain)?,
        )
    };

    // Step 4: 既存 ID セット構築
    let existing_ids: HashSet<String> =
        vault.records().iter().map(|r| r.id().to_string()).collect();

    // Step 5: ImportValidator::validate
    let report = ImportValidator::validate(&payload, &existing_ids)
        .map_err(|err| CliError::from(DataPortabilityError::ValidationFailed(err)))?;

    // Step 6: --on-conflict error で衝突検出
    if args.on_conflict == OnConflictArg::Error && !report.conflicting_ids.is_empty() {
        return Err(DataPortabilityError::ConflictError {
            ids: report.conflicting_ids,
        }
        .into());
    }

    // Step 7: 各レコードを走査
    let mut added: usize = 0;
    let mut skipped: usize = 0;
    let mut overwritten: usize = 0;

    for record in &payload.records {
        let is_conflicting = report.conflicting_ids.contains(&record.id);

        if is_conflicting && args.on_conflict == OnConflictArg::Skip {
            skipped += 1;
            continue;
        }

        let domain_record = import_record_to_domain(record)?;

        if is_conflicting && args.on_conflict == OnConflictArg::Overwrite {
            vault
                .remove_record(domain_record.id())
                .map_err(CliError::Domain)?;
            vault.add_record(domain_record).map_err(CliError::Domain)?;
            overwritten += 1;
        } else {
            vault.add_record(domain_record).map_err(CliError::Domain)?;
            added += 1;
        }
    }

    // Step 8: atomic write（SqliteVaultRepository::save が tempfile + rename で保証、R1-DP-09 適合）
    // Issue #146: DatabaseBusy は VaultBusy に変換。その他の PersistenceError は従来通り伝播。
    repo.save(&vault).map_err(|e| -> CliError {
        match e {
            PersistenceError::DatabaseBusy => DataPortabilityError::VaultBusy.into(),
            other => other.into(),
        }
    })?;

    // Step 9: ImportSummary 返却
    Ok(ImportSummary {
        added,
        skipped,
        overwritten,
    })
}

// -------------------------------------------------------------------
// import_record_to_domain（private helper）
// -------------------------------------------------------------------

/// `ImportRecord` を domain の `Record` に変換する。
///
/// # 処理順序
/// 1. UUID 文字列 → `uuid::Uuid` へパース
/// 2. `RecordId::new`
/// 3. `RecordLabel::try_new`
/// 4. ペイロード変換（`Redacted` は `unreachable!`）
/// 5. `created_at` RFC 3339 パース
/// 6. `updated_at` RFC 3339 パース
/// 7. `hotkey` パース
/// 8. `Record::rehydrate`
///
/// # `unreachable!` の根拠
/// `ImportValidator::validate` が `RedactedPayload` を `Err` として返すため、
/// 本関数が呼ばれた時点でリダクトペイロードは除外済みの契約が成立している。
/// `unreachable!` は告知的プログラミングによる二重安全網。
///
/// # Errors
/// フィールドの解析失敗時に `CliError::ImportDeserializationFailed` を返す。
fn import_record_to_domain(
    r: &shikomi_core::portability::ImportRecord,
) -> Result<Record, CliError> {
    // Step 1: UUID パース
    let uuid = uuid::Uuid::parse_str(&r.id).map_err(|e| CliError::ImportDeserializationFailed {
        reason: format!("invalid record id '{}': {e}", r.id),
    })?;

    // Step 2: RecordId
    let id = RecordId::new(uuid).map_err(|e| CliError::ImportDeserializationFailed {
        reason: e.to_string(),
    })?;

    // Step 3: RecordLabel
    let label = RecordLabel::try_new(r.label.clone()).map_err(|e| {
        CliError::ImportDeserializationFailed {
            reason: format!("invalid label: {e}"),
        }
    })?;

    // Step 4: ペイロード変換
    let payload = match &r.payload {
        ExportRecordPayload::Plaintext { value } => {
            RecordPayload::Plaintext(SecretString::from_string(value.clone()))
        }
        ExportRecordPayload::Redacted => {
            unreachable!(
                "ImportValidator rejects Redacted payload; \
                 import_record_to_domain must not be called for redacted records"
            )
        }
    };

    // Step 5: created_at パース
    let created_at = OffsetDateTime::parse(&r.created_at, &Rfc3339).map_err(|e| {
        CliError::ImportDeserializationFailed {
            reason: format!("invalid created_at '{}': {e}", r.created_at),
        }
    })?;

    // Step 6: updated_at パース
    let updated_at = OffsetDateTime::parse(&r.updated_at, &Rfc3339).map_err(|e| {
        CliError::ImportDeserializationFailed {
            reason: format!("invalid updated_at '{}': {e}", r.updated_at),
        }
    })?;

    // Step 7: hotkey パース
    let hotkey = r
        .hotkey
        .as_deref()
        .map(Hotkey::parse)
        .transpose()
        .map_err(|e| CliError::ImportDeserializationFailed {
            reason: format!("invalid hotkey: {e}"),
        })?;

    // Step 8: Record::rehydrate（updated_at < created_at で DomainError::VaultConsistencyError）
    let record = Record::rehydrate(id, r.kind, label, payload, created_at, updated_at, hotkey)
        .map_err(|e| CliError::ImportDeserializationFailed {
            reason: e.to_string(),
        })?;

    Ok(record)
}
