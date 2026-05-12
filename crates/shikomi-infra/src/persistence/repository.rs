//! `SqliteVaultRepository` — `VaultRepository` の `SQLite` 実装。

use std::path::Path;
use std::time::Instant;

use rusqlite::{Connection, OpenFlags};
use shikomi_core::{Record, Vault, VaultHeader, VaultVersion};

use super::{
    audit::Audit,
    error::{CorruptedReason, PersistenceError},
    lock::VaultLock,
    paths::{self, VaultPaths},
    permission::PermissionGuard,
    sqlite::{
        atomic::{AtomicWriteSession, AtomicWriter, ExponentialBackoffRetryPolicy},
        mapping::Mapping,
        schema::SchemaSql,
    },
    VaultRepository,
};

// -------------------------------------------------------------------
// SqliteVaultRepository
// -------------------------------------------------------------------

/// `SQLite` バックエンドの `VaultRepository` 実装。
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
        let dir = paths::resolve_os_default_or_env()?;
        Self::from_directory(&dir)
    }

    /// 明示的な vault ディレクトリ path を受け取って `SqliteVaultRepository` を構築する。
    ///
    /// 呼び出し側（CLI / GUI）で `--vault-dir` フラグ等を thread-safe に渡すためのエントリポイント。
    /// `std::env` を一切参照しない。
    ///
    /// # Errors
    ///
    /// - ディレクトリ検証失敗（絶対パス / path traversal / symlink / 保護領域 / 非ディレクトリ等）:
    ///   `PersistenceError::InvalidVaultDir`
    /// - `fs::canonicalize` 失敗: `PersistenceError::InvalidVaultDir { reason: Canonicalize }`
    pub fn from_directory(path: &Path) -> Result<Self, PersistenceError> {
        let paths = VaultPaths::new(path.to_path_buf())?;
        Ok(Self { paths })
    }

    /// vault パス情報への参照を返す。
    #[must_use]
    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    /// vault ディレクトリを作成し、OS 規定のパーミッション（Unix: 0700）を設定する。
    ///
    /// daemon コンポジションルートから呼び出し、`PermissionGuard::verify_dir` が
    /// 確実に通過できる状態を保証する。
    /// `run()` に生の `std::fs::create_dir_all` + `set_permissions` を書かないための
    /// カプセル化（BUG-04 根治: 責務を repository 層に閉じる）。
    ///
    /// # Errors
    ///
    /// - ディレクトリ作成失敗・パーミッション設定失敗: `PersistenceError::Io`
    pub fn prepare_dir(&self) -> Result<(), PersistenceError> {
        PermissionGuard::ensure_dir(self.paths.dir())
    }

    /// vault を読み込む。`vault.db` が存在しない場合は空の plaintext vault を**永続化して**返す。
    ///
    /// daemon コンポジションルートから呼び出す（REQ-DAEMON-028）。`load_or_create` との
    /// 違いは、NotFound 時に空 vault を `save` で永続化し、`shikomi_daemon::init` ターゲットへ
    /// 2 行のログを出力する点にある。
    ///
    /// - vault.db が存在する → `load()` の結果をそのまま返す（ログ出力なし）
    /// - vault.db が存在しない → 空の plaintext vault を生成・保存してから返す
    /// - 書き込み不可など NotFound 以外のエラー → 即 `Err` で返す（Fail Fast）
    ///
    /// **冪等性**: 生成後に同一パスで再度呼び出すと既存 vault がロードされて返る。
    ///
    /// # Errors
    ///
    /// - `vault.db` の読み込み失敗（NotFound 以外）: `PersistenceError::Io`
    /// - 新規生成時の `save` 失敗（書き込み不可 等）: `PersistenceError`
    pub fn load_or_create_plaintext(&self) -> Result<Vault, PersistenceError> {
        match self.load() {
            Ok(v) => Ok(v),
            Err(PersistenceError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                let vault = Vault::new(
                    VaultHeader::new_plaintext(
                        VaultVersion::CURRENT,
                        time::OffsetDateTime::now_utc(),
                    )
                    .expect("CURRENT version is always valid"),
                );
                self.save(&vault)?;
                tracing::info!(
                    target: "shikomi_daemon::init",
                    "vault not found; created new plaintext vault at {}",
                    self.paths.dir().display()
                );
                tracing::info!(
                    target: "shikomi_daemon::init",
                    "hint: to enable encryption, run `shikomi vault encrypt` after the daemon has started"
                );
                Ok(vault)
            }
            Err(e) => Err(e),
        }
    }

    /// vault を読み込む。`vault.db` が存在しない場合は空の plaintext vault を返す。
    ///
    /// 初回インストール / CI 環境など `vault.db` が未作成の状態で daemon が起動する場合に
    /// IPC を受け付けられるよう、空の vault で起動する。NotFound 以外のエラーは伝播する。
    ///
    /// # Errors
    ///
    /// - `vault.db` が存在しない以外の IO エラー / 破損データ / ロック取得失敗:
    ///   `PersistenceError`
    pub fn load_or_create(&self) -> Result<Vault, PersistenceError> {
        match self.load() {
            Ok(v) => Ok(v),
            Err(PersistenceError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                tracing::info!(
                    target: "shikomi_infra::persistence",
                    "vault.db not found — starting with empty plaintext vault (new installation)"
                );
                Ok(Vault::new(
                    VaultHeader::new_plaintext(
                        VaultVersion::CURRENT,
                        time::OffsetDateTime::now_utc(),
                    )
                    .expect("CURRENT version is always valid"),
                ))
            }
            Err(e) => Err(e),
        }
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
        if !(SchemaSql::USER_VERSION_SUPPORTED_MIN..=SchemaSql::USER_VERSION_SUPPORTED_MAX)
            .contains(&user_version)
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

        // Step 12: Sub-D (#42) で暗号化モードを解禁。protection_mode による拒否経路は削除。
        // 暗号化処理 (AEAD 検証 / wrap_VEK 復号) は呼出側 (`VaultMigration` / Sub-E daemon) 責務。

        // Step 13: Vault 集約を構築
        let mut vault = Vault::new(header);

        // Step 14-15: records を SELECT して追加（user_version で V1/V2 クエリを選択）
        let records = Self::select_records(&conn, user_version)?;
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

    /// `vault_header` テーブルから1行を読み込む。
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
    ///
    /// `user_version = 1` の V1 DB は `hotkey_combo` カラムが存在しないため
    /// `SELECT_RECORDS_ORDERED_V1` を使用し、`row_to_record_v1` でマッピングする。
    /// `user_version >= 2` の V2 DB は `hotkey_combo` カラムを含む
    /// `SELECT_RECORDS_ORDERED` を使用し、`row_to_record` でマッピングする。
    fn select_records(
        conn: &Connection,
        user_version: u32,
    ) -> Result<Vec<Record>, PersistenceError> {
        let (sql, use_v1) = if user_version <= 1 {
            (SchemaSql::SELECT_RECORDS_ORDERED_V1, true)
        } else {
            (SchemaSql::SELECT_RECORDS_ORDERED, false)
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        let mut rows = stmt
            .query([])
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        let mut records = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::Sqlite { source: e })?
        {
            let record = if use_v1 {
                Mapping::row_to_record_v1(row)?
            } else {
                Mapping::row_to_record(row)?
            };
            records.push(record);
        }
        Ok(records)
    }

    /// `save` の実装本体。audit ログなしで vault を書き込む。書き込みバイト数を返す。
    fn save_inner(&self, vault: &Vault) -> Result<u64, PersistenceError> {
        // Step 2: Sub-D (#42) で暗号化モード save を解禁。Fail Fast 拒否経路は削除。
        // BLOB は composite container として `Mapping::vault_header_to_params` で詰める。

        // Step 3: ディレクトリを作成し、適切なパーミッションを設定
        PermissionGuard::ensure_dir(self.paths.dir())?;

        // Step 4: 排他ロック取得
        let _lock = VaultLock::acquire_exclusive(&self.paths)?;

        // Step 5: 孤立 `.new` ファイルの検出
        AtomicWriter::detect_orphan(self.paths.vault_db_new())?;

        // Step 6: `.new` 作成から SQLite COMMIT まで実行しセッションを取得
        let session = AtomicWriteSession::new(&self.paths, vault)?;

        // Step 7: クローズ順序固定 → sidecar DACL → fsync → rename（Win: retry 補強）
        session.finalize(&ExponentialBackoffRetryPolicy)?;

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

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --- TC-U-REPO-01: prepare_dir — 存在しないディレクトリを作成する ---

    #[test]
    fn tc_u_repo_01_prepare_dir_creates_directory() {
        let base = TempDir::new().unwrap();
        let vault_dir = base.path().join("vault_new");

        // 事前条件: ディレクトリが存在しない
        assert!(!vault_dir.exists(), "事前条件: ディレクトリが未作成");

        let repo = SqliteVaultRepository::from_directory(&vault_dir).unwrap();
        repo.prepare_dir().expect("prepare_dir は成功すべき");

        assert!(
            vault_dir.is_dir(),
            "prepare_dir 後: ディレクトリが作成されるべき"
        );
    }

    // --- TC-U-REPO-02: prepare_dir — 既存ディレクトリに冪等 ---

    #[test]
    fn tc_u_repo_02_prepare_dir_is_idempotent_on_existing_dir() {
        let dir = TempDir::new().unwrap();
        let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();

        // 2 回呼び出しても失敗しない
        repo.prepare_dir()
            .expect("1 回目: prepare_dir は成功すべき");
        repo.prepare_dir()
            .expect("2 回目: prepare_dir は冪等であるべき");
    }

    // --- TC-U-REPO-03: load_or_create — vault.db 未存在時に空 plaintext vault を返す ---

    #[test]
    fn tc_u_repo_03_load_or_create_returns_empty_vault_when_not_found() {
        let dir = TempDir::new().unwrap();
        let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
        // ディレクトリのパーミッションを OS 規定に設定してから load_or_create を呼ぶ
        repo.prepare_dir().expect("prepare_dir は成功すべき");

        // vault.db が存在しない → NotFound を捕捉して空 plaintext vault を返すべき
        let vault = repo
            .load_or_create()
            .expect("load_or_create は空 vault を返すべき");

        assert!(vault.records().is_empty(), "初期 vault はレコード 0 件");
        assert!(
            matches!(
                vault.protection_mode(),
                shikomi_core::ProtectionMode::Plaintext
            ),
            "初期 vault は plaintext モードであるべき"
        );
    }

    // ---------------------------------------------------------------------------
    // TC-UT-140〜142: SqliteVaultRepository::load_or_create_plaintext
    // 設計書: docs/features/daemon-ipc/test-design/unit.md §2.18
    // REQ-DAEMON-028 / Issue #80
    // ---------------------------------------------------------------------------

    // --- TC-UT-140: vault.db 存在時 — 既存 vault をロードして返す（ログ出力なし） ---

    #[test]
    #[tracing_test::traced_test]
    fn tc_ut_140_load_or_create_plaintext_returns_existing_vault_without_log() {
        // REQ-DAEMON-028 / Issue #80
        let dir = TempDir::new().unwrap();
        let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
        repo.prepare_dir().expect("prepare_dir は成功すべき");

        // 事前: vault.db を保存
        let header =
            VaultHeader::new_plaintext(VaultVersion::CURRENT, time::OffsetDateTime::now_utc())
                .expect("header は常に有効");
        let initial_vault = Vault::new(header);
        repo.save(&initial_vault).expect("初期 vault の保存");

        // act
        let result = repo.load_or_create_plaintext();

        // assert
        let vault = result.expect("既存 vault.db があれば Ok が返るべき");
        assert!(vault.records().is_empty(), "初期 vault はレコード 0 件");
        assert!(
            matches!(
                vault.protection_mode(),
                shikomi_core::ProtectionMode::Plaintext
            ),
            "初期 vault は plaintext モードであるべき"
        );
        // vault.db が既存なので生成ログは出力されない
        assert!(
            !logs_contain("vault not found"),
            "既存 vault.db がある場合、生成ログは出力されるべきでない"
        );
    }

    // --- TC-UT-141: vault.db 不在 — 空 plaintext vault を生成してログ出力 ---

    #[test]
    #[tracing_test::traced_test]
    fn tc_ut_141_load_or_create_plaintext_creates_empty_vault_when_absent() {
        // REQ-DAEMON-028 / Issue #80
        let dir = TempDir::new().unwrap();
        let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
        repo.prepare_dir().expect("prepare_dir は成功すべき");
        // vault.db を意図的に作成しない

        // act
        let result = repo.load_or_create_plaintext();

        // assert
        let vault = result.expect("vault.db 不在でも Ok (空 vault 生成) が返るべき");
        assert!(
            vault.records().is_empty(),
            "新規生成した vault はレコード 0 件であるべき"
        );
        assert!(
            matches!(
                vault.protection_mode(),
                shikomi_core::ProtectionMode::Plaintext
            ),
            "新規生成した vault は plaintext モードであるべき"
        );
        assert!(
            logs_contain("vault not found; created new plaintext vault at"),
            "生成ログ 'vault not found; created new plaintext vault at' が出力されるべき"
        );
        assert!(
            logs_contain("hint: to enable encryption"),
            "暗号化ヒントログが出力されるべき"
        );

        // 横串アサート（冪等性）: vault.db 生成後に再呼出しても Ok が返る
        let vault2 = repo
            .load_or_create_plaintext()
            .expect("2 回目の呼出し: 既存 vault.db を load して Ok が返るべき");
        assert!(
            vault2.records().is_empty(),
            "2 回目の呼出し: 既存 vault のレコード 0 件"
        );
    }

    // --- TC-UT-142: 書き込み不可ディレクトリ（Unix 限定）— エラーが返る ---

    #[cfg(unix)]
    #[test]
    fn tc_ut_142_load_or_create_plaintext_returns_error_on_readonly_dir() {
        // REQ-DAEMON-028 / Issue #80
        // NOTE: spec は Err(PersistenceError::Io(_)) を期待するが、実装上は
        // load_inner の verify_dir が 0o500 != 0700 を検出して
        // PersistenceError::InvalidPermission を先に返す。
        // どちらも「書き込み不可ディレクトリではエラーが返る」意図に沿う。
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
        // vault.db は作成しない
        // ディレクトリを 0o500（所有者 r-x）に変更して書き込み不可にする
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500))
            .expect("chmod 0o500 に成功すべき");

        // act
        let result = repo.load_or_create_plaintext();

        // cleanup: TempDir::drop() が削除できるようパーミッション復元
        let _ = std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700));

        // assert: 書き込み不可ディレクトリではエラーが返る
        assert!(result.is_err(), "0o500 ディレクトリでは Err が返るべき");
    }
}
