//! `SqliteVaultRepository` — `VaultRepository` の SQLite 実装。

use std::path::PathBuf;
use std::time::Instant;

use rusqlite::{Connection, OpenFlags};
use shikomi_core::{ProtectionMode, Record, Vault};

use super::{
    audit::Audit,
    error::{CorruptedReason, PersistenceError},
    lock::VaultLock,
    paths::{self, VaultPaths},
    permission::PermissionGuard,
    sqlite::{atomic::AtomicWriter, mapping::Mapping, schema::SchemaSql},
    VaultRepository, TRACKING_ISSUE_ENCRYPTED_VAULT,
};

// -------------------------------------------------------------------
// SqliteVaultRepository
// -------------------------------------------------------------------

/// SQLite バックエンドの `VaultRepository` 実装。
pub struct SqliteVaultRepository {
    paths: VaultPaths,
}

impl SqliteVaultRepository {
    /// 環境変数 `SHIKOMI_VAULT_DIR` またはデフォルトディレクトリから `SqliteVaultRepository` を構築する。
    ///
    /// # Errors
    ///
    /// - vault ディレクトリの解決失敗: `PersistenceError::CannotResolveVaultDir`
    /// - ディレクトリ検証失敗: `PersistenceError::InvalidVaultDir`
    pub fn new() -> Result<Self, PersistenceError> {
        let dir = if let Ok(val) = std::env::var(paths::ENV_VAR_VAULT_DIR) {
            PathBuf::from(val)
        } else {
            dirs::data_dir()
                .ok_or(PersistenceError::CannotResolveVaultDir)?
                .join(paths::APP_SUBDIR_NAME)
        };
        let paths = VaultPaths::new(dir)?;
        Ok(Self { paths })
    }

    /// vault パス情報への参照を返す。
    #[must_use]
    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    /// `Instant` からミリ秒経過時間を計算する。オーバーフロー時は `u64::MAX`。
    fn elapsed_ms(start: Instant) -> u64 {
        start.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
    }
}

impl VaultRepository for SqliteVaultRepository {
    fn load(&self) -> Result<Vault, PersistenceError> {
        let start = Instant::now();
        Audit::entry_load(&self.paths);
        match self.load_inner() {
            Ok((vault, record_count)) => {
                let protection_mode = vault.protection_mode();
                Audit::exit_ok_load(record_count, protection_mode, Self::elapsed_ms(start));
                Ok(vault)
            }
            Err(e) => {
                Audit::exit_err(&e, Self::elapsed_ms(start));
                Err(e)
            }
        }
    }

    fn save(&self, vault: &Vault) -> Result<(), PersistenceError> {
        let start = Instant::now();
        let record_count = vault.records().len();
        Audit::entry_save(&self.paths, record_count);
        match self.save_inner(vault) {
            Ok(bytes_written) => {
                Audit::exit_ok_save(record_count, bytes_written, Self::elapsed_ms(start));
                Ok(())
            }
            Err(e) => {
                Audit::exit_err(&e, Self::elapsed_ms(start));
                Err(e)
            }
        }
    }

    fn exists(&self) -> Result<bool, PersistenceError> {
        let result = self
            .paths
            .vault_db()
            .try_exists()
            .map_err(|e| PersistenceError::Io {
                path: self.paths.vault_db().to_path_buf(),
                source: e,
            });
        match &result {
            Ok(v) => {
                tracing::debug!(
                    exists = v,
                    vault_db = %self.paths.vault_db().display(),
                    "exists: checked"
                );
            }
            Err(e) => {
                tracing::debug!(error = %e, "exists: error");
            }
        }
        result
    }
}

// -------------------------------------------------------------------
// 内部実装
// -------------------------------------------------------------------

impl SqliteVaultRepository {
    /// `load` の実装本体。audit ログなしで vault を読み込む。
    fn load_inner(&self) -> Result<(Vault, usize), PersistenceError> {
        // Step 2: ディレクトリのパーミッション確認
        PermissionGuard::verify_dir(self.paths.dir())?;

        // Step 3: 共有ロック取得
        let _lock = VaultLock::acquire_shared(&self.paths)?;

        // Step 4: 孤立 `.new` ファイルの検出
        AtomicWriter::detect_orphan(self.paths.vault_db_new())?;

        // Step 5: vault.db の存在確認
        let db_exists = self
            .paths
            .vault_db()
            .try_exists()
            .map_err(|e| PersistenceError::Io {
                path: self.paths.vault_db().to_path_buf(),
                source: e,
            })?;
        if !db_exists {
            return Err(PersistenceError::Io {
                path: self.paths.vault_db().to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "vault.db not found"),
            });
        }

        // Step 6: ファイルのパーミッション確認
        PermissionGuard::verify_file(self.paths.vault_db())?;

        // Step 7: SQLite 接続（読み取り専用）
        let conn = Connection::open_with_flags(
            self.paths.vault_db(),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // Step 8: application_id 確認
        let app_id: u32 = conn
            .query_row(SchemaSql::PRAGMA_APPLICATION_ID_GET, [], |row| row.get(0))
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        if app_id != SchemaSql::APPLICATION_ID {
            return Err(PersistenceError::SchemaMismatch {
                expected_application_id: SchemaSql::APPLICATION_ID,
                found_application_id: app_id,
                expected_version_min: SchemaSql::USER_VERSION_SUPPORTED_MIN,
                expected_version_max: SchemaSql::USER_VERSION_SUPPORTED_MAX,
                found_user_version: 0,
            });
        }

        // Step 9: user_version 確認
        let user_version: u32 = conn
            .query_row(SchemaSql::PRAGMA_USER_VERSION_GET, [], |row| row.get(0))
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        if user_version < SchemaSql::USER_VERSION_SUPPORTED_MIN
            || user_version > SchemaSql::USER_VERSION_SUPPORTED_MAX
        {
            return Err(PersistenceError::SchemaMismatch {
                expected_application_id: SchemaSql::APPLICATION_ID,
                found_application_id: app_id,
                expected_version_min: SchemaSql::USER_VERSION_SUPPORTED_MIN,
                expected_version_max: SchemaSql::USER_VERSION_SUPPORTED_MAX,
                found_user_version: user_version,
            });
        }

        // Step 10: vault_header を SELECT
        let header = Self::select_vault_header(&conn)?;

        // Step 12: 暗号化モードは未実装
        if header.protection_mode() == ProtectionMode::Encrypted {
            return Err(PersistenceError::UnsupportedYet {
                feature: "encrypted vault persistence",
                tracking_issue: TRACKING_ISSUE_ENCRYPTED_VAULT,
            });
        }

        // Step 13: Vault 集約を構築
        let mut vault = Vault::new(header);

        // Step 14-15: records を SELECT して追加
        let records = Self::select_records(&conn)?;
        let record_count = records.len();
        for record in records {
            let row_key = record.id().to_string();
            vault
                .add_record(record)
                .map_err(|e| PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(row_key),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: e.to_string(),
                    },
                    source: Some(e),
                })?;
        }

        Ok((vault, record_count))
    }

    /// vault_header テーブルから1行を読み込む。
    fn select_vault_header(
        conn: &Connection,
    ) -> Result<shikomi_core::VaultHeader, PersistenceError> {
        let mut stmt = conn
            .prepare(SchemaSql::SELECT_VAULT_HEADER)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let mut rows = stmt
            .query([])
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        let row = rows
            .next()
            .map_err(|e| PersistenceError::Sqlite { source: e })?
            .ok_or_else(|| PersistenceError::Corrupted {
                table: "vault_header",
                row_key: None,
                reason: CorruptedReason::MissingVaultHeader,
                source: None,
            })?;
        let header = Mapping::row_to_vault_header(row)?;

        // CHECK(id=1) 制約があるため複数行は存在しないはずだが防衛的確認
        if rows
            .next()
            .map_err(|e| PersistenceError::Sqlite { source: e })?
            .is_some()
        {
            return Err(PersistenceError::Corrupted {
                table: "vault_header",
                row_key: None,
                reason: CorruptedReason::InvalidRowCombination {
                    detail: "multiple vault_header rows found".to_string(),
                },
                source: None,
            });
        }

        Ok(header)
    }

    /// records テーブルから全行を読み込む。
    fn select_records(conn: &Connection) -> Result<Vec<Record>, PersistenceError> {
        let mut stmt = conn
            .prepare(SchemaSql::SELECT_RECORDS_ORDERED)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let mut rows = stmt
            .query([])
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        let mut records = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::Sqlite { source: e })?
        {
            records.push(Mapping::row_to_record(row)?);
        }
        Ok(records)
    }

    /// `save` の実装本体。audit ログなしで vault を書き込む。書き込みバイト数を返す。
    fn save_inner(&self, vault: &Vault) -> Result<u64, PersistenceError> {
        // Step 2: 暗号化モードは未実装（Fail Fast）
        if vault.protection_mode() == ProtectionMode::Encrypted {
            return Err(PersistenceError::UnsupportedYet {
                feature: "encrypted vault persistence",
                tracking_issue: TRACKING_ISSUE_ENCRYPTED_VAULT,
            });
        }

        // Step 3: ディレクトリを作成し、適切なパーミッションを設定
        PermissionGuard::ensure_dir(self.paths.dir())?;

        // Step 4: 排他ロック取得
        let _lock = VaultLock::acquire_exclusive(&self.paths)?;

        // Step 5: 孤立 `.new` ファイルの検出
        AtomicWriter::detect_orphan(self.paths.vault_db_new())?;

        // Step 6: `.new` ファイルに書き込む
        AtomicWriter::write_new(&self.paths, vault)?;

        // Step 7: fsync + リネーム
        AtomicWriter::fsync_and_rename(&self.paths)?;

        // 書き込みバイト数を取得（監査ログ用）
        let bytes_written = self
            .paths
            .vault_db()
            .metadata()
            .map_err(|e| PersistenceError::Io {
                path: self.paths.vault_db().to_path_buf(),
                source: e,
            })?
            .len();

        Ok(bytes_written)
    }
}
