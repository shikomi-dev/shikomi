//! `AtomicWriteSession` — SQLite 書込セッション型（Phase 8 新設、Issue #73）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::OpenFlags;
use shikomi_core::Vault;

// `Audit::retry_event` は cfg(windows) rename retry でのみ呼出される。
#[cfg(windows)]
use crate::persistence::audit::{Audit, RetryOutcome};
use crate::persistence::error::{AtomicWriteStage, PersistenceError};
use crate::persistence::paths::VaultPaths;
use crate::persistence::permission::PermissionGuard;

use crate::persistence::sqlite::mapping::Mapping;
use crate::persistence::sqlite::schema::SchemaSql;

use super::constants::SQLITE_SIDECAR_SUFFIXES;
use super::retry_policy::RetryPolicy;
use super::writer::AtomicWriter;

/// SQLite 書込中の `Connection` を保持するセッション型（Phase 8 新設、Issue #73）。
///
/// `new(paths, vault)` でセッション開始、`finalize(self, retry_policy)` の所有権消費で
/// クローズ順序契約（PRAGMA → `close()` → sidecar DACL → fsync → rename）を
/// 型レベルで強制する（`./classes.md` §3.1 / §3.2 参照）。
///
/// **`Drop`**: `finalize` 未呼出のまま drop された場合は `AtomicWriter::cleanup_new` を
/// best-effort 実行し panic しない（Fail Safe）。
/// `new_path` が `None` ならば cleanup 不要（`finalize` が所有権を取得済）。
pub(crate) struct AtomicWriteSession {
    conn: Option<rusqlite::Connection>,
    /// `finalize` 冒頭で `take()` し `None` にする。
    /// `Drop` は `Some` の場合のみ cleanup_new を呼ぶ。
    new_path: Option<PathBuf>,
    final_path: PathBuf,
    dir_path: PathBuf,
}

impl AtomicWriteSession {
    /// `.new` ファイル作成から SQLite COMMIT まで実行し、`conn` を保持したセッションを返す。
    ///
    /// `flows.md §save` step 6.1〜6.10 に対応（`./classes.md` §3.2 参照）。
    ///
    /// # Errors
    ///
    /// - ファイル作成失敗: `PersistenceError::AtomicWriteFailed { stage: PrepareNew }`
    /// - パーミッション設定失敗: `PersistenceError::InvalidPermission` / `PersistenceError::Io`
    /// - `SQLite` エラー（PRAGMA / DDL / TX / COMMIT）: `PersistenceError::Sqlite`
    /// `busy_timeout` は `from_directory_with_busy_timeout` 経由で構築した
    /// `SqliteVaultRepository` の場合のみ `Some`。`vault.db.new` は新規ファイルのため
    /// 他コネクションとの競合は起きないが、将来の WAL 移行等も見据えて全接続に適用する
    /// （Issue #146、服部平次指摘対応）。
    pub(crate) fn new(
        paths: &VaultPaths,
        vault: &Vault,
        busy_timeout: Option<Duration>,
    ) -> Result<Self, PersistenceError> {
        let new_path = paths.vault_db_new().to_path_buf();
        let final_path = paths.vault_db().to_path_buf();
        let dir_path = paths.dir().to_path_buf();

        // Step 6.1-6.2: 適切なパーミッションでファイルを事前作成し、file handle を drop
        Self::create_with_permissions(&new_path)?;

        // Step 6.3: SQLite 接続を開く
        let conn = rusqlite::Connection::open_with_flags(
            &new_path,
            OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(PersistenceError::from)?;

        // Step 6.4: ensure_file — SQLite が open 時に mode を変えた場合の再強制。
        // Windows では owner-only DACL を rename 前に設定する（`MoveFileExW` がソース SD を
        // 保持するため rename 後の vault.db へ DACL が引き継がれる、`./classes.md` §3.3 参照）。
        PermissionGuard::ensure_file(&new_path)?;

        // Issue #146: busy_timeout が設定されている場合に適用する。
        // `vault.db.new` は新規ファイルなので競合は起きないが、全接続統一ポリシー。
        if let Some(timeout) = busy_timeout {
            conn.busy_timeout(timeout).map_err(PersistenceError::from)?;
        }

        // Step 6.5: PRAGMA 設定
        conn.execute_batch(SchemaSql::PRAGMA_APPLICATION_ID_SET)
            .map_err(PersistenceError::from)?;
        conn.execute_batch(SchemaSql::PRAGMA_USER_VERSION_SET)
            .map_err(PersistenceError::from)?;
        conn.execute_batch(SchemaSql::PRAGMA_JOURNAL_MODE)
            .map_err(PersistenceError::from)?;

        // Step 6.6: テーブル / インデックス作成
        conn.execute_batch(SchemaSql::CREATE_VAULT_HEADER)
            .map_err(PersistenceError::from)?;
        conn.execute_batch(SchemaSql::CREATE_RECORDS)
            .map_err(PersistenceError::from)?;
        conn.execute_batch(SchemaSql::CREATE_HOTKEY_INDEX)
            .map_err(PersistenceError::from)?;

        // Steps 6.7-6.10: 単一トランザクション内で vault_header + 全レコードを INSERT → COMMIT
        {
            let tx = conn
                .unchecked_transaction()
                .map_err(PersistenceError::from)?;

            let header_params = Mapping::vault_header_to_params(vault.header())?;
            tx.execute(
                SchemaSql::INSERT_VAULT_HEADER,
                rusqlite::params![
                    header_params.protection_mode,
                    header_params.vault_version,
                    header_params.created_at_rfc3339,
                    header_params.kdf_salt,
                    header_params.wrapped_vek_by_pw,
                    header_params.wrapped_vek_by_recovery,
                ],
            )
            .map_err(PersistenceError::from)?;

            for record in vault.records() {
                let p = Mapping::record_to_params(record)?;
                tx.execute(
                    SchemaSql::INSERT_RECORD,
                    rusqlite::params![
                        p.id,
                        p.kind,
                        p.label,
                        p.payload_variant,
                        p.plaintext_value,
                        p.nonce,
                        p.ciphertext,
                        p.aad_bytes.map(|b| b.to_vec()),
                        p.created_at,
                        p.updated_at,
                        p.hotkey_combo,
                    ],
                )
                .map_err(PersistenceError::from)?;
            }

            tx.commit().map_err(PersistenceError::from)?;
        }

        Ok(AtomicWriteSession {
            conn: Some(conn),
            new_path: Some(new_path),
            final_path,
            dir_path,
        })
    }

    /// `AtomicWriteSession` を所有権消費し、クローズ順序固定で以下を順次実行する:
    ///
    /// 1. `PRAGMA wal_checkpoint(TRUNCATE)` — WAL サイドカー物理空化
    /// 2. `PRAGMA journal_mode = DELETE` — close 時サイドカー物理消去の契約再強制
    /// 3. `conn.close()` 明示呼出 — Drop 任せの遅延クローズを回避
    /// 4. サイドカー DACL 適用（best-effort）
    /// 5. FsyncTemp
    /// 6. FsyncDir（Unix のみ）
    /// 7. rename（Win: `retry_policy` に従う指数バックオフ retry + symlink 再検証）
    ///
    /// `flows.md §save` step 7.1〜7.7 に対応（`./classes.md` §3.2 参照）。
    ///
    /// # Errors
    ///
    /// - PRAGMA / close 失敗: `PersistenceError::Sqlite`
    /// - FsyncTemp 失敗: `PersistenceError::AtomicWriteFailed { stage: FsyncTemp }`
    /// - FsyncDir 失敗（Unix のみ）: `PersistenceError::AtomicWriteFailed { stage: FsyncDir }`
    /// - rename 失敗（Win: retry 全敗後）: `PersistenceError::AtomicWriteFailed { stage: Rename }`
    /// - retry 中 symlink / reparse point 検出（Win のみ）: `PersistenceError::InvalidVaultDir`
    pub(crate) fn finalize(
        mut self,
        retry_policy: &dyn RetryPolicy,
    ) -> Result<(), PersistenceError> {
        // new_path を take して None に — Drop がこの後 cleanup_new を呼ばないようにする。
        // 以降の失敗パスでは AtomicWriter::cleanup_new(&path) を明示呼出する。
        let path = self.new_path.take().expect("new_path already consumed");
        let conn = self.conn.take().expect("conn already consumed");
        let final_path = self.final_path.clone();
        let dir_path = self.dir_path.clone();

        // Step 7.1: WAL チェックポイント（DELETE モード採用時は no-op、副作用なし）
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|e| {
                AtomicWriter::cleanup_new(&path);
                PersistenceError::from(e)
            })?;

        // Step 7.2: journal_mode を DELETE に再強制
        // （schema.rs::PRAGMA_JOURNAL_MODE と冗長だが、将来 WAL 採用時に本 step を消し忘れて
        //   Win rename race を再導入する罠を構造的に塞ぐ Boy Scout / Fail Safe）
        conn.execute_batch("PRAGMA journal_mode = DELETE;")
            .map_err(|e| {
                AtomicWriter::cleanup_new(&path);
                PersistenceError::from(e)
            })?;

        // Step 7.3: 明示的 close（Drop の sqlite3_close_v2 遅延 semantics を回避）
        // Issue #146: PersistenceError::from で DatabaseBusy 検出を有効化。
        if let Err((_, e)) = conn.close() {
            AtomicWriter::cleanup_new(&path);
            return Err(PersistenceError::from(e));
        }

        // Step 7.4: サイドカー DACL 適用（best-effort）
        Self::apply_sidecar_permissions_if_present(&path);

        // Step 7.5: FsyncTemp
        // Windows では FlushFileBuffers が書込権限を要求するため read+write でオープン
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| {
                AtomicWriter::cleanup_new(&path);
                PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::FsyncTemp,
                    source: e,
                }
            })?;
        file.sync_all().map_err(|e| {
            AtomicWriter::cleanup_new(&path);
            PersistenceError::AtomicWriteFailed {
                stage: AtomicWriteStage::FsyncTemp,
                source: e,
            }
        })?;
        drop(file);

        // Step 7.6: FsyncDir（Unix のみ、POSIX 2008 rename メタデータ永続化）
        #[cfg(unix)]
        {
            let dir = std::fs::File::open(&dir_path).map_err(|e| {
                AtomicWriter::cleanup_new(&path);
                PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::FsyncDir,
                    source: e,
                }
            })?;
            dir.sync_all().map_err(|e| {
                AtomicWriter::cleanup_new(&path);
                PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::FsyncDir,
                    source: e,
                }
            })?;
        }
        // dir_path は cfg(unix) 以外で未使用だが、フィールドとして保持するため lint 抑制
        #[cfg(not(unix))]
        let _ = &dir_path;

        // Step 7.7: アトミックリネーム（Win: retry_policy に従う retry + symlink 再検証）
        Self::rename_atomic(&path, &final_path, retry_policy)
    }

    /// conn を close し new_path を `None` にしてセッションを終了する（テスト専用）。
    ///
    /// `finalize` を呼ばずに `.new` ファイルを意図的に残したい場合（AC-06 の中断状態再現）に使用。
    /// `conn` を正常 close することで SQLite ハンドルを解放し、`new_path = None` により
    /// `Drop` impl が `cleanup_new` を呼ばないことを保証する。
    #[cfg(test)]
    pub(crate) fn close_without_rename(mut self) {
        // conn を take() して drop — SQLite ハンドルを解放する
        drop(self.conn.take());
        // new_path を None に — Drop が cleanup_new を呼ばないようにして .new を残す
        self.new_path = None;
        // self が drop されるが new_path = None のため cleanup は発火しない
    }

    // ------------------------------------------------------------------
    // 内部ヘルパ
    // ------------------------------------------------------------------

    /// アトミックリネーム本体。初回成功時は即 return。
    /// 失敗時は Windows のみ retry に委譲、それ以外は即 fail fast。
    fn rename_atomic(
        new_path: &Path,
        final_path: &Path,
        retry_policy: &dyn RetryPolicy,
    ) -> Result<(), PersistenceError> {
        let initial_err = match std::fs::rename(new_path, final_path) {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };

        #[cfg(windows)]
        {
            let raw = initial_err.raw_os_error().unwrap_or(0);
            if retry_policy.should_retry(raw) {
                return Self::windows_rename_retry(new_path, final_path, initial_err, retry_policy);
            }
        }

        // cfg(not(windows)) ではここに到達。unused_variables 抑制（retry_policy は
        // cfg(windows) ブロックでのみ参照される）。
        let _ = retry_policy;

        // Unix 全般 / Windows 非一過性エラーは即 fail fast
        AtomicWriter::cleanup_new(new_path);
        Err(PersistenceError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            source: initial_err,
        })
    }

    /// `cfg(windows)` 限定 rename retry ループ（Issue #65、Bug-G-001 反映済）。
    ///
    /// 各試行前に `retry_policy.sleep_duration(attempt)` だけ sleep し、
    /// rename 直前に symlink / reparse point を再検証して TOCTOU 差替えを fail fast。
    /// 各試行発火・成功・全敗を `Audit::retry_event` で監査ログに記録。
    #[cfg(windows)]
    fn windows_rename_retry(
        new_path: &Path,
        final_path: &Path,
        initial_err: std::io::Error,
        retry_policy: &dyn RetryPolicy,
    ) -> Result<(), PersistenceError> {
        let start = std::time::Instant::now();
        let mut last_err = initial_err;
        let max = retry_policy.max_attempts();

        for attempt in 1..=max {
            let last_raw_os = last_err.raw_os_error().unwrap_or(0);
            let elapsed_ms_pending = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            Audit::retry_event(
                "rename",
                attempt,
                last_raw_os,
                elapsed_ms_pending,
                RetryOutcome::Pending,
            );

            std::thread::sleep(retry_policy.sleep_duration(attempt));

            // TOCTOU 再検証 — retry 窓中の symlink / junction 差替えを fail fast
            Self::reverify_no_reparse_point(new_path)?;
            Self::reverify_no_reparse_point(final_path)?;

            match std::fs::rename(new_path, final_path) {
                Ok(()) => {
                    let elapsed_ms_ok =
                        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    Audit::retry_event(
                        "rename",
                        attempt,
                        last_raw_os,
                        elapsed_ms_ok,
                        RetryOutcome::Succeeded,
                    );
                    return Ok(());
                }
                Err(e) => {
                    let raw = e.raw_os_error().unwrap_or(0);
                    if !retry_policy.should_retry(raw) {
                        AtomicWriter::cleanup_new(new_path);
                        return Err(PersistenceError::AtomicWriteFailed {
                            stage: AtomicWriteStage::Rename,
                            source: e,
                        });
                    }
                    last_err = e;
                }
            }
        }

        let elapsed_ms_exhausted = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        let final_raw_os = last_err.raw_os_error().unwrap_or(0);
        Audit::retry_event(
            "rename",
            max,
            final_raw_os,
            elapsed_ms_exhausted,
            RetryOutcome::Exhausted,
        );
        AtomicWriter::cleanup_new(new_path);
        Err(PersistenceError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            source: last_err,
        })
    }

    /// retry 直前の symlink / NTFS reparse point 再検証（Win TOCTOU 対策）。
    ///
    /// `FILE_ATTRIBUTE_REPARSE_POINT (0x400)` または `is_symlink()` を検出したら fail fast。
    /// 対象パスが未存在（vault.db の初回作成時等）は Ok を返す。
    /// `../basic-design/security.md` §atomic write の二次防衛線 §Win retry 中 TOCTOU 参照。
    #[cfg(windows)]
    pub(crate) fn reverify_no_reparse_point(path: &Path) -> Result<(), PersistenceError> {
        use crate::persistence::error::VaultDirReason;
        use std::os::windows::fs::MetadataExt;

        // https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

        match std::fs::symlink_metadata(path) {
            Ok(meta) => {
                let is_reparse = (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
                if meta.file_type().is_symlink() || is_reparse {
                    Err(PersistenceError::InvalidVaultDir {
                        path: path.to_path_buf(),
                        reason: VaultDirReason::SymlinkNotAllowed,
                    })
                } else {
                    Ok(())
                }
            }
            // 初回 save 時に vault.db (final_path) が未存在のケースは正常
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(PersistenceError::Io {
                path: path.to_path_buf(),
                source: e,
            }),
        }
    }

    /// `SQLite` サイドカーが残存していれば owner-only パーミッションを強制適用する
    /// （best-effort、失敗は warn のみ）。
    fn apply_sidecar_permissions_if_present(new_path: &Path) {
        let Some(parent) = new_path.parent() else {
            return;
        };
        let Some(file_name) = new_path.file_name() else {
            return;
        };
        for suffix in SQLITE_SIDECAR_SUFFIXES {
            let mut sidecar_name = file_name.to_os_string();
            sidecar_name.push(suffix);
            let sidecar_path = parent.join(&sidecar_name);
            match sidecar_path.try_exists() {
                Ok(true) => {
                    if let Err(e) = PermissionGuard::ensure_file(&sidecar_path) {
                        tracing::warn!(
                            path = %sidecar_path.display(),
                            error = %e,
                            "failed to apply DACL to SQLite sidecar (best-effort)"
                        );
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        path = %sidecar_path.display(),
                        error = %e,
                        "failed to check SQLite sidecar existence (best-effort)"
                    );
                }
            }
        }
    }

    /// 適切なパーミッションでファイルを作成し、file handle を drop する（step 6.1-6.2）。
    fn create_with_permissions(path: &Path) -> Result<(), PersistenceError> {
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(path)
                    .map_err(|e| PersistenceError::AtomicWriteFailed {
                        stage: AtomicWriteStage::PrepareNew,
                        source: e,
                    })?;
            } else {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(path)
                    .map_err(|e| PersistenceError::AtomicWriteFailed {
                        stage: AtomicWriteStage::PrepareNew,
                        source: e,
                    })?;
            }
        }
        Ok(())
    }
}

impl Drop for AtomicWriteSession {
    /// `finalize` 未呼出のまま drop された場合は `.new` を best-effort 削除（Fail Safe）。
    ///
    /// `new_path` が `None`（`finalize` が所有権を取得済）の場合は何もしない。
    ///
    /// **順序**: `conn` を先に close してからファイル削除する。
    /// Windows はオープン中のファイルハンドルを持つファイルの削除を
    /// `ERROR_ACCESS_DENIED (5)` で拒否するため、`cleanup_new` の前に
    /// `conn.take()` で `rusqlite::Connection` を drop しなければならない。
    fn drop(&mut self) {
        // Windows: conn が Some のまま remove_file すると ERROR_ACCESS_DENIED (5)。
        // take() で Connection を drop し、ファイルハンドルを解放してから削除する。
        drop(self.conn.take());
        if let Some(ref path) = self.new_path {
            AtomicWriter::cleanup_new(path);
        }
    }
}
