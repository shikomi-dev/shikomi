//! `export_records` UseCase — vault レコードを JSON ファイルへ export する。
//!
//! 設計根拠: docs/features/data-portability/cli/detailed-design/usecase.md
//! §`usecase/portability/export.rs` の設計詳細
//!
//! # セキュリティ考慮
//! - export ファイルのパーミッション: Unix では `tempfile::Builder::permissions(0o600)`。
//! - atomic write: `tempfile` + `persist` で書き込みの完全性を保証（R1-DP-09）。
//! - Encrypted ペイロード: `vault.protection_mode() == Encrypted` を即時検出して Fail Fast。

use std::ffi::OsStr;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use shikomi_core::portability::{ExportPayload, ExportRecord};
use shikomi_core::{ProtectionMode};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;

use crate::cli::ExportArgs;
use crate::error::CliError;

use super::error::DataPortabilityError;

// -------------------------------------------------------------------
// ExportSummary
// -------------------------------------------------------------------

/// `export_records` の正常系戻り値。
pub struct ExportSummary {
    /// export したレコード件数（vault 不存在時は 0）。
    pub record_count: usize,
    /// 実際に書き込んだファイルパス。
    pub output_path: PathBuf,
}

// -------------------------------------------------------------------
// export_records
// -------------------------------------------------------------------

/// vault レコードを JSON ファイルへ export する UseCase 関数。
///
/// 設計根拠: docs/features/data-portability/cli/detailed-design/usecase.md §`export_records` 関数
///
/// # 処理順序
/// 1. 出力ファイル既存確認（`--force` 未指定時はエラー）
/// 2. vault 不存在確認（0 件 export として正常終了）
/// 3. vault 読み込み（`EncryptionUnsupported` → `ExportImportVaultLocked`）
/// 4. `ProtectionMode::Encrypted` 確認
/// 5. 全レコード取得
/// 6. `ExportRecord::try_from` 変換
/// 7. vault_name 構築
/// 8. `ExportPayload` 構築
/// 9. JSON シリアライズ
/// 10. 親ディレクトリ取得
/// 11. tempfile 構築（Unix: 0o600 パーミッション）
/// 12. JSON bytes 書き込み
/// 13. `persist` で atomic rename
/// 14. `ExportSummary` 返却
///
/// # Errors
/// - `CliError::ExportOutputFileExists` — 出力ファイルが既に存在し `--force` 未指定
/// - `CliError::ExportImportVaultLocked` — vault がロック済み
/// - `CliError::Persistence` — I/O / SQLite エラー
pub fn export_records(
    repo: &dyn VaultRepository,
    args: &ExportArgs,
    vault_dir: &Path,
    now: OffsetDateTime,
) -> Result<ExportSummary, CliError> {
    // Step 1: 出力ファイル既存確認
    if args.output.exists() && !args.force {
        return Err(DataPortabilityError::OutputFileExists {
            path: args.output.clone(),
        }
        .into());
    }

    // Steps 2–6: vault 読み込みとレコード変換
    let records_to_export: Vec<ExportRecord> = if !repo.exists()? {
        // vault 不存在は 0 件 export（エラーではない）
        vec![]
    } else {
        // Step 3: vault 読み込み（EncryptionUnsupported → VaultLocked に変換）
        let vault = match repo.load() {
            Ok(v) => v,
            Err(e) => {
                let cli_err = CliError::from(e);
                return Err(match cli_err {
                    CliError::EncryptionUnsupported => DataPortabilityError::VaultLocked.into(),
                    other => other,
                });
            }
        };

        // Step 4: protection_mode 確認
        if vault.protection_mode() == ProtectionMode::Encrypted {
            return Err(DataPortabilityError::VaultLocked.into());
        }

        // Steps 5–6: 全レコードを ExportRecord に変換
        vault
            .records()
            .iter()
            .map(|record| {
                ExportRecord::try_from((record, args.export_secrets)).map_err(|e| {
                    use shikomi_core::portability::ExportError;
                    match e {
                        ExportError::VaultLocked => {
                            CliError::from(DataPortabilityError::VaultLocked)
                        }
                    }
                })
            })
            .collect::<Result<Vec<_>, CliError>>()?
    };

    // Step 7: vault_name 構築
    let vault_name = vault_dir
        .file_name()
        .unwrap_or_else(|| OsStr::new("vault"))
        .to_string_lossy()
        .into_owned();

    // Step 8: ExportPayload 構築
    let record_count = records_to_export.len();
    let payload = ExportPayload::new(records_to_export, vault_name, now);

    // Step 9: JSON シリアライズ（Serialize の不変条件によりパニックは実用上起こらない）
    let json =
        serde_json::to_string_pretty(&payload).expect("ExportPayload is always serializable");

    // Step 10: 親ディレクトリ取得
    let parent = args.output.parent().unwrap_or_else(|| Path::new("."));

    // Steps 11–13: tempfile 構築 → 書き込み → atomic persist
    write_atomic(&json, parent, &args.output)?;

    // Step 14: ExportSummary 返却
    Ok(ExportSummary {
        record_count,
        output_path: args.output.clone(),
    })
}

/// `json` を `output` パスへ atomic に書き込む。
///
/// Unix では `tempfile::Builder::permissions(0o600)` を設定してからファイルを作成し、
/// `persist` で rename する（R1-DP-09 適合）。非 Unix ではパーミッション設定なし。
fn write_atomic(json: &str, parent: &Path, output: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    let mut tmp_file = {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt as _;
        tempfile::Builder::new()
            .permissions(Permissions::from_mode(0o600))
            .tempfile_in(parent)
            .map_err(DataPortabilityError::IoError)?
    };

    #[cfg(not(unix))]
    let mut tmp_file = tempfile::Builder::new()
        .tempfile_in(parent)
        .map_err(DataPortabilityError::IoError)?;

    tmp_file
        .write_all(json.as_bytes())
        .map_err(DataPortabilityError::IoError)?;

    // `PersistError::error` フィールドが `io::Error`
    tmp_file
        .persist(output)
        .map_err(|e| DataPortabilityError::IoError(e.error))?;

    Ok(())
}
