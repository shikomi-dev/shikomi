//! `SqliteVaultRepository` — `VaultRepository` の SQLite 実装。

use std::path::PathBuf;
use std::time::Instant;

use rusqlite::{Connection, OpenFlags};
use shikomi_core::{ProtectionMode, Record, Vault};

use super::{
    TRACKING_ISSUE_ENCRYPTED_VAULT, VaultRepository,
    audit,
    error::{CorruptedReason, PersistenceError},
    lock::VaultLock,
    paths::VaultPaths,
    permission::PermissionGuard,
    sqlite::{
        atomic::AtomicWriter,
        mapping::Mapping,
        schema::SchemaSql,
    },
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
        let dir = if let Ok(val) = std::env::var("SHIKOMI_VAULT_DIR") {
            PathBuf::from(val)
        } else {
            dirs::data_dir()
                .ok_or(PersistenceError::CannotResolveVaultDir)?
                .join("shikomi")
        };
        let paths = VaultPaths::new(dir)?;
        Ok(Self { paths })
    }

    /// 指定ディレクトリ（検証なし）で `SqliteVaultRepository` を構築する。
    ///
    /// テストや内部用途向け。検証をスキップするため、不正なパスを渡さないこと。
    #[doc(hidden)]
    #[must_use]
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            paths: VaultPaths::new_unchecked(dir),
        }
    }

    /// vault パス情報への参照を返す。
    #[must_use]
    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }
}

impl VaultRepository for SqliteVaultRepository {
    fn load(&self) -> Result<Vault, PersistenceError> {
        let start = Instant::now();
        audit::entry_load(&self.paths);

        // Step 2: ディレクトリのパーミッション確認
        if let Err(e) = PermissionGuard::verify_dir(self.paths.dir()) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 3: 共有ロック取得
        let _lock = match VaultLock::acquire_shared(&self.paths) {
            Ok(l) => l,
            Err(e) => {
                audit::exit_err(&e, elapsed_ms(start));
                return Err(e);
            }
        };

        // Step 4: 孤立 `.new` ファイルの検出
        if let Err(e) = AtomicWriter::detect_orphan(self.paths.vault_db_new()) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 5: vault.db の存在確認
        let db_exists = self
            .paths
            .vault_db()
            .try_exists()
            .map_err(|e| PersistenceError::Io {
                path: self.paths.vault_db().to_path_buf(),
                source: e,
            });
        let db_exists = match db_exists {
            Ok(v) => v,
            Err(e) => {
                audit::exit_err(&e, elapsed_ms(start));
                return Err(e);
            }
        };
        if !db_exists {
            let e = PersistenceError::Io {
                path: self.paths.vault_db().to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "vault.db not found"),
            };
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 6: ファイルのパーミッション確認
        if let Err(e) = PermissionGuard::verify_file(self.paths.vault_db()) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 7: SQLite 接続（読み取り専用）
        let conn = match Connection::open_with_flags(
            self.paths.vault_db(),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                let pe = PersistenceError::Sqlite { source: e };
                audit::exit_err(&pe, elapsed_ms(start));
                return Err(pe);
            }
        };

        // Step 8: application_id 確認
        let app_id: u32 = match conn
            .query_row(SchemaSql::PRAGMA_APPLICATION_ID_GET, [], |row| row.get(0))
            .map_err(|e| PersistenceError::Sqlite { source: e })
        {
            Ok(v) => v,
            Err(e) => {
                audit::exit_err(&e, elapsed_ms(start));
                return Err(e);
            }
        };
        if app_id != SchemaSql::APPLICATION_ID {
            let e = PersistenceError::SchemaMismatch {
                expected_application_id: SchemaSql::APPLICATION_ID,
                found_application_id: app_id,
                expected_version_min: SchemaSql::USER_VERSION_SUPPORTED_MIN,
                expected_version_max: SchemaSql::USER_VERSION_SUPPORTED_MAX,
                found_user_version: 0,
            };
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 9: user_version 確認
        let user_version: u32 = match conn
            .query_row(SchemaSql::PRAGMA_USER_VERSION_GET, [], |row| row.get(0))
            .map_err(|e| PersistenceError::Sqlite { source: e })
        {
            Ok(v) => v,
            Err(e) => {
                audit::exit_err(&e, elapsed_ms(start));
                return Err(e);
            }
        };
        if user_version < SchemaSql::USER_VERSION_SUPPORTED_MIN
            || user_version > SchemaSql::USER_VERSION_SUPPORTED_MAX
        {
            let e = PersistenceError::SchemaMismatch {
                expected_application_id: SchemaSql::APPLICATION_ID,
                found_application_id: app_id,
                expected_version_min: SchemaSql::USER_VERSION_SUPPORTED_MIN,
                expected_version_max: SchemaSql::USER_VERSION_SUPPORTED_MAX,
                found_user_version: user_version,
            };
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 10: vault_header を SELECT
        let header = {
            let mut stmt = match conn
                .prepare(SchemaSql::SELECT_VAULT_HEADER)
                .map_err(|e| PersistenceError::Sqlite { source: e })
            {
                Ok(s) => s,
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            };
            let mut rows = match stmt
                .query([])
                .map_err(|e| PersistenceError::Sqlite { source: e })
            {
                Ok(r) => r,
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            };
            let first_row = match rows
                .next()
                .map_err(|e| PersistenceError::Sqlite { source: e })
            {
                Ok(r) => r,
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            };
            let row = match first_row {
                None => {
                    let e = PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: None,
                        reason: CorruptedReason::MissingVaultHeader,
                        source: None,
                    };
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
                Some(r) => r,
            };
            let h = match Mapping::row_to_vault_header(row) {
                Ok(h) => h,
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            };
            // CHECK(id=1) 制約があるため複数行は存在しないはずだが防衛的確認
            match rows
                .next()
                .map_err(|e| PersistenceError::Sqlite { source: e })
            {
                Ok(Some(_)) => {
                    let e = PersistenceError::Corrupted {
                        table: "vault_header",
                        row_key: None,
                        reason: CorruptedReason::InvalidRowCombination {
                            detail: "multiple vault_header rows found".to_string(),
                        },
                        source: None,
                    };
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
                Ok(None) => {}
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            }
            h
        };

        // Step 12: 暗号化モードは未実装
        if header.protection_mode() == ProtectionMode::Encrypted {
            let e = PersistenceError::UnsupportedYet {
                feature: "encrypted vault persistence",
                tracking_issue: TRACKING_ISSUE_ENCRYPTED_VAULT,
            };
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 13: Vault 集約を構築
        let mut vault = Vault::new(header);

        // Step 14-15: records を SELECT して追加
        let records: Vec<Record> = {
            let mut stmt = match conn
                .prepare(SchemaSql::SELECT_RECORDS_ORDERED)
                .map_err(|e| PersistenceError::Sqlite { source: e })
            {
                Ok(s) => s,
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            };
            let mut rows = match stmt
                .query([])
                .map_err(|e| PersistenceError::Sqlite { source: e })
            {
                Ok(r) => r,
                Err(e) => {
                    audit::exit_err(&e, elapsed_ms(start));
                    return Err(e);
                }
            };
            let mut v: Vec<Record> = Vec::new();
            loop {
                let next = rows
                    .next()
                    .map_err(|e| PersistenceError::Sqlite { source: e });
                match next {
                    Ok(Some(row)) => match Mapping::row_to_record(row) {
                        Ok(r) => v.push(r),
                        Err(e) => {
                            audit::exit_err(&e, elapsed_ms(start));
                            return Err(e);
                        }
                    },
                    Ok(None) => break,
                    Err(e) => {
                        audit::exit_err(&e, elapsed_ms(start));
                        return Err(e);
                    }
                }
            }
            v
        };

        let record_count = records.len();
        for record in records {
            let row_key = record.id().to_string();
            if let Err(e) = vault.add_record(record) {
                let pe = PersistenceError::Corrupted {
                    table: "records",
                    row_key: Some(row_key),
                    reason: CorruptedReason::InvalidRowCombination {
                        detail: e.to_string(),
                    },
                    source: Some(e),
                };
                audit::exit_err(&pe, elapsed_ms(start));
                return Err(pe);
            }
        }

        let protection_mode = vault.protection_mode();
        audit::exit_ok_load(record_count, protection_mode, elapsed_ms(start));
        Ok(vault)
    }

    fn save(&self, vault: &Vault) -> Result<(), PersistenceError> {
        let start = Instant::now();
        audit::entry_save(&self.paths, vault.records().len());

        // Step 2: 暗号化モードは未実装（Fail Fast）
        if vault.protection_mode() == ProtectionMode::Encrypted {
            let e = PersistenceError::UnsupportedYet {
                feature: "encrypted vault persistence",
                tracking_issue: TRACKING_ISSUE_ENCRYPTED_VAULT,
            };
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 3: ディレクトリを作成し、適切なパーミッションを設定
        if let Err(e) = PermissionGuard::ensure_dir(self.paths.dir()) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 4: 排他ロック取得
        let _lock = match VaultLock::acquire_exclusive(&self.paths) {
            Ok(l) => l,
            Err(e) => {
                audit::exit_err(&e, elapsed_ms(start));
                return Err(e);
            }
        };

        // Step 5: 孤立 `.new` ファイルの検出
        if let Err(e) = AtomicWriter::detect_orphan(self.paths.vault_db_new()) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 6: `.new` ファイルに書き込む
        if let Err(e) = AtomicWriter::write_new(&self.paths, vault) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // Step 7: fsync + リネーム
        if let Err(e) = AtomicWriter::fsync_and_rename(&self.paths) {
            audit::exit_err(&e, elapsed_ms(start));
            return Err(e);
        }

        // 書き込みバイト数を取得（監査ログ用）
        let bytes_written = self
            .paths
            .vault_db()
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        audit::exit_ok_save(vault.records().len(), bytes_written, elapsed_ms(start));
        Ok(())
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
// ヘルパー
// -------------------------------------------------------------------

/// `Instant` からミリ秒経過時間を取得する。オーバーフロー時は `u64::MAX`。
fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}
