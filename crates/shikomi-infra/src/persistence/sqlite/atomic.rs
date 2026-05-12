//! アトミック書き込みユーティリティ。
//!
//! Phase 8 リファクタ（Issue #73）: `AtomicWriter` ZST + 静的メソッド連鎖から
//! セッション型 `AtomicWriteSession { conn, new_path }` に移行。
//! `finalize(self, retry_policy)` の所有権消費でクローズ順序契約を型レベルで強制。
//!
//! - **`AtomicWriteSession`**: `.new` 書込から `conn.close()` + fsync + rename まで一連で完結。
//! - **`AtomicWriter`** (ZST): `detect_orphan` / `cleanup_new` の名前空間として残存。
//! - **`RetryPolicy`** trait: Win rename retry の振る舞いをテスト注入可能に抽象化。
//!
//! Bug-G-001 反映（2026-04-27）: Win CI ランナーで Defender / Search Indexer が
//! ハンドルを `drop` 後も `~250ms+` 保持し続けるため、retry を指数バックオフへ拡張
//! （`50ms × 2^(n-1)` ± `25ms` jitter × 最大 5 回、最悪 ~1675ms / 平均 ~1550ms）。
//!
//! Windows DACL 適用順序確定（Phase 8、`./classes.md` §3.3）: rename 前に `.new` に
//! `ensure_file` を適用し、`MoveFileExW` がソース SD を保持することで `vault.db` に引継。

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

use super::mapping::Mapping;
use super::schema::SchemaSql;

// -------------------------------------------------------------------
// 内部定数
// -------------------------------------------------------------------

/// `SQLite` サイドカーファイル名のサフィックス。
const SQLITE_SIDECAR_SUFFIXES: &[&str] = &["-journal", "-wal", "-shm"];

// -------------------------------------------------------------------
// RetryPolicy trait
// -------------------------------------------------------------------

/// rename retry の振る舞いを抽象化する trait（Phase 8、Issue #73）。
///
/// `cfg(windows)` 限定 rename retry で使用する。`NoSleepRetryPolicy` を差し込むことで
/// テスト時の実際の sleep を排除し CI を高速化する（`./classes.md` §3.4 参照）。
///
/// Unix では `finalize` 内で `should_retry` / `sleep_duration` は一切呼ばれない
/// （Unix の rename は即 fail fast、retry なし）。
///
/// `cfg(windows)` 限定ロジックで使用するため非 Windows ビルドでは dead_code になるが意図的。
#[allow(dead_code)]
pub(crate) trait RetryPolicy {
    /// 最大 retry 回数。超過したら `AtomicWriteFailed { stage: Rename }` で return。
    fn max_attempts(&self) -> u32;

    /// `attempt` 番目（1-indexed）の retry 前 sleep 量を返す。jitter を内部で生成してよい。
    fn sleep_duration(&self, attempt: u32) -> Duration;

    /// OS エラーコードが一過性エラーか否かを判定する。
    ///
    /// `cfg(windows)`:
    /// - `ERROR_ACCESS_DENIED (5)` / `ERROR_SHARING_VIOLATION (32)` /
    ///   `ERROR_LOCK_VIOLATION (33)` → `true`、それ以外 → `false`
    ///
    /// `cfg(not(windows))`: 常に `false`。
    fn should_retry(&self, raw_os_error: i32) -> bool;
}

// -------------------------------------------------------------------
// ExponentialBackoffRetryPolicy
// -------------------------------------------------------------------

/// 指数バックオフ retry の production default 実装。
///
/// `max_attempts = 5`、`base_ms = 50`、`jitter = ±25ms`（`OsRng` 一様乱数）。
///
/// | attempt | 中央値 | range       | 累積中央値 |
/// |---------|--------|-------------|----------|
/// | 1       | 50ms   | 25〜75ms   | 50ms     |
/// | 2       | 100ms  | 75〜125ms  | 150ms    |
/// | 3       | 200ms  | 175〜225ms | 350ms    |
/// | 4       | 400ms  | 375〜425ms | 750ms    |
/// | 5       | 800ms  | 775〜825ms | 1550ms   |
///
/// 最悪 ~1675ms / 平均 ~1550ms。SSoT:
/// `docs/features/vault-persistence/basic-design/security.md` §jitter。
pub(crate) struct ExponentialBackoffRetryPolicy;

impl Default for ExponentialBackoffRetryPolicy {
    fn default() -> Self {
        Self
    }
}

impl RetryPolicy for ExponentialBackoffRetryPolicy {
    fn max_attempts(&self) -> u32 {
        5
    }

    fn sleep_duration(&self, attempt: u32) -> Duration {
        #[cfg(windows)]
        {
            use rand_core::{OsRng, RngCore};

            const BASE_MS: u64 = 50;
            const JITTER_HALF_RANGE_MS: u64 = 25;
            // 0..=50 を一様抽選後 -25 シフトで [-25, +25]
            // HALF_RANGE_MS ≤ 127 の範囲で u8 へのキャストは安全
            const JITTER_RANGE: u8 = (JITTER_HALF_RANGE_MS * 2 + 1) as u8;

            let mut buf = [0u8; 1];
            OsRng.fill_bytes(&mut buf);
            let jitter_pos = u64::from(buf[0] % JITTER_RANGE);
            // attempt は 1..=max_attempts(=5) なので左シフト overflow なし
            let multiplier: u64 = 1u64 << (attempt.saturating_sub(1));
            let center_ms = BASE_MS.saturating_mul(multiplier);
            let delay_ms = center_ms + jitter_pos - JITTER_HALF_RANGE_MS;
            Duration::from_millis(delay_ms)
        }
        #[cfg(not(windows))]
        {
            let _ = attempt;
            Duration::ZERO
        }
    }

    fn should_retry(&self, raw_os_error: i32) -> bool {
        #[cfg(windows)]
        {
            matches!(raw_os_error, 5 | 32 | 33)
        }
        #[cfg(not(windows))]
        {
            let _ = raw_os_error;
            false
        }
    }
}

// -------------------------------------------------------------------
// NoSleepRetryPolicy（テスト専用）
// -------------------------------------------------------------------

/// テスト専用 `RetryPolicy`（sleep なし・CI 高速化）。
///
/// `sleep_duration` は常に `Duration::ZERO`。`should_retry` は
/// `ExponentialBackoffRetryPolicy` と同じ判定ロジックを使用。
/// TC-I29 / TC-I29-B の retry 発火テストで実際の sleep を排除する（`./classes.md` §3.4）。
#[cfg(test)]
pub(crate) struct NoSleepRetryPolicy {
    pub(crate) max_attempts: u32,
}

#[cfg(test)]
impl RetryPolicy for NoSleepRetryPolicy {
    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    fn sleep_duration(&self, _attempt: u32) -> Duration {
        Duration::ZERO
    }

    fn should_retry(&self, raw_os_error: i32) -> bool {
        #[cfg(windows)]
        {
            matches!(raw_os_error, 5 | 32 | 33)
        }
        #[cfg(not(windows))]
        {
            let _ = raw_os_error;
            false
        }
    }
}

// -------------------------------------------------------------------
// AtomicWriteSession
// -------------------------------------------------------------------

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
    pub(crate) fn new(paths: &VaultPaths, vault: &Vault) -> Result<Self, PersistenceError> {
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
        .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // Step 6.4: ensure_file — SQLite が open 時に mode を変えた場合の再強制。
        // Windows では owner-only DACL を rename 前に設定する（`MoveFileExW` がソース SD を
        // 保持するため rename 後の vault.db へ DACL が引き継がれる、`./classes.md` §3.3 参照）。
        PermissionGuard::ensure_file(&new_path)?;

        // Step 6.5: PRAGMA 設定
        conn.execute_batch(SchemaSql::PRAGMA_APPLICATION_ID_SET)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::PRAGMA_USER_VERSION_SET)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::PRAGMA_JOURNAL_MODE)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // Step 6.6: テーブル / インデックス作成
        conn.execute_batch(SchemaSql::CREATE_VAULT_HEADER)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::CREATE_RECORDS)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::CREATE_HOTKEY_INDEX)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // Steps 6.7-6.10: 単一トランザクション内で vault_header + 全レコードを INSERT → COMMIT
        {
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| PersistenceError::Sqlite { source: e })?;

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
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

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
                .map_err(|e| PersistenceError::Sqlite { source: e })?;
            }

            tx.commit()
                .map_err(|e| PersistenceError::Sqlite { source: e })?;
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
                PersistenceError::Sqlite { source: e }
            })?;

        // Step 7.2: journal_mode を DELETE に再強制
        // （schema.rs::PRAGMA_JOURNAL_MODE と冗長だが、将来 WAL 採用時に本 step を消し忘れて
        //   Win rename race を再導入する罠を構造的に塞ぐ Boy Scout / Fail Safe）
        conn.execute_batch("PRAGMA journal_mode = DELETE;")
            .map_err(|e| {
                AtomicWriter::cleanup_new(&path);
                PersistenceError::Sqlite { source: e }
            })?;

        // Step 7.3: 明示的 close（Drop の sqlite3_close_v2 遅延 semantics を回避）
        // 失敗時は PersistenceError::Sqlite を直接返す（型情報を失う変換禁止、
        // `../basic-design/error.md` §禁止事項と整合）。
        if let Err((_, e)) = conn.close() {
            AtomicWriter::cleanup_new(&path);
            return Err(PersistenceError::Sqlite { source: e });
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
    fn reverify_no_reparse_point(path: &Path) -> Result<(), PersistenceError> {
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

// -------------------------------------------------------------------
// AtomicWriter（ZST 名前空間）
// -------------------------------------------------------------------

/// アトミック書き込みの補助ユーティリティ（ZST 名前空間）。
///
/// Phase 8 リファクタで主要ロジックは `AtomicWriteSession` に移行。
/// `detect_orphan` / `cleanup_new` の名前空間として残存する。
pub(crate) struct AtomicWriter;

impl AtomicWriter {
    /// `vault.db.new` が存在する場合に孤立ファイルエラーを返す。
    ///
    /// # Errors
    ///
    /// - `.new` が存在する: `PersistenceError::OrphanNewFile`
    /// - 存在確認 IO エラー: `PersistenceError::Io`
    pub(crate) fn detect_orphan(new_path: &Path) -> Result<(), PersistenceError> {
        match new_path.try_exists() {
            Ok(true) => Err(PersistenceError::OrphanNewFile {
                path: new_path.to_path_buf(),
            }),
            Ok(false) => Ok(()),
            Err(e) => Err(PersistenceError::Io {
                path: new_path.to_path_buf(),
                source: e,
            }),
        }
    }

    /// `vault.db.new` を best-effort 削除する。
    ///
    /// 失敗時は `tracing::warn!` でログ出力し、呼出側のエラーを上書きしない（上位に伝播しない）。
    pub(crate) fn cleanup_new(new_path: &Path) {
        if let Err(e) = std::fs::remove_file(new_path) {
            tracing::warn!(
                path = %new_path.display(),
                error = %e,
                "failed to cleanup .new file (best-effort)"
            );
        }
    }

    /// `vault.db.new` に vault の内容を書き込むが fsync/rename は行わない（テスト専用）。
    ///
    /// `AtomicWriteSession::new` と同一ロジックで `.new` を書き込み、`finalize` を呼ばずに返す。
    /// atomic write の中断状態を決定的に再現するためのテストフック（AC-06 対応）。
    ///
    /// # Errors
    ///
    /// - `AtomicWriteSession::new` と同じ
    #[cfg(test)]
    pub(crate) fn write_new_only(
        paths: &VaultPaths,
        vault: &Vault,
    ) -> Result<(), PersistenceError> {
        let mut session = AtomicWriteSession::new(paths, vault)?;
        // conn を正常 drop して SQLite を閉じる（sqlite3_close_v2 経由）
        drop(session.conn.take());
        // new_path を None に — Drop が cleanup_new を呼ばないようにして .new を残す
        session.new_path = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 内部テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{
        Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
        VaultVersion,
    };
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn plaintext_vault(label: &str, value: &str) -> Vault {
        let header =
            VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap();
        let mut vault = Vault::new(header);
        let record = Record::new(
            RecordId::new(Uuid::now_v7()).unwrap(),
            RecordKind::Secret,
            RecordLabel::try_new(label.to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string(value.to_string())),
            OffsetDateTime::now_utc(),
        );
        vault.add_record(record).unwrap();
        vault
    }

    /// TC-I06 — `write_new_only` フックで .new のみ書き込み→load が `OrphanNewFile` を返す。
    ///
    /// AC-06 対応。`write_new_only` は `finalize` を呼ばないため .new が残り、
    /// vault.db の内容は初期 vault のままになる。
    #[test]
    fn tc_i06_write_new_only_hook_orphan() {
        let dir = TempDir::new().unwrap();
        let paths = VaultPaths::new_unchecked(dir.path().to_path_buf());

        // ディレクトリのパーミッションを設定
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        // 初期 vault を save（vault.db が存在する状態にする）
        let initial_vault = plaintext_vault("initial", "initial-value");
        let session = AtomicWriteSession::new(&paths, &initial_vault).unwrap();
        session.finalize(&ExponentialBackoffRetryPolicy).unwrap();

        // vault.db のバイト列を記録
        let db_bytes_before = std::fs::read(paths.vault_db()).unwrap();

        // write_new_only で別内容の .new のみ作成（rename しない）
        let new_vault = plaintext_vault("updated", "updated-value");
        AtomicWriter::write_new_only(&paths, &new_vault).unwrap();

        // .new ファイルが存在することを確認
        assert!(
            paths.vault_db_new().exists(),
            ".new ファイルが作成されていない"
        );

        // .new が残存している状態での load は OrphanNewFile になる
        let orphan_result = AtomicWriter::detect_orphan(paths.vault_db_new());
        assert!(
            matches!(orphan_result, Err(PersistenceError::OrphanNewFile { .. })),
            "OrphanNewFile を期待したが {orphan_result:?}"
        );

        // vault.db の内容が初期 vault のまま（.new の内容が反映されていない）
        let db_bytes_after = std::fs::read(paths.vault_db()).unwrap();
        assert_eq!(
            db_bytes_before, db_bytes_after,
            "vault.db の内容が変わっている（.new がリネームされてしまった）"
        );
    }

    // -------------------------------------------------------------------
    // Phase 8 (Issue #73) — AtomicWriteSession / RetryPolicy 追加テスト群
    // -------------------------------------------------------------------

    /// TC-U-Drop — `finalize` を呼ばずに drop された場合、`Drop` impl が
    /// `AtomicWriter::cleanup_new` を best-effort 呼出して `.new` を削除する。
    ///
    /// `new_path` が `Some` のまま drop される経路（panic / early-return 等）で
    /// `.new` の残存を防ぐ Drop safety 設計を確認する。
    /// 対応設計書: `./classes.md` §3.1 Drop safety
    #[test]
    fn tc_u_drop_without_finalize_removes_new_file() {
        let dir = TempDir::new().unwrap();
        let paths = VaultPaths::new_unchecked(dir.path().to_path_buf());
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        let vault = plaintext_vault("drop-guard", "test-value");
        let session = AtomicWriteSession::new(&paths, &vault).unwrap();

        assert!(
            paths.vault_db_new().exists(),
            "AtomicWriteSession::new 後に .new ファイルが存在しない"
        );

        // finalize を呼ばずに drop → Drop impl が cleanup_new を呼ぶ
        drop(session);

        assert!(
            !paths.vault_db_new().exists(),
            "drop 後も .new が残存している — Drop guard が AtomicWriter::cleanup_new を呼んでいない"
        );
    }

    /// TC-U-NoSleep-1 — `NoSleepRetryPolicy::sleep_duration` は常に `Duration::ZERO` を返す。
    ///
    /// CI 高速化のための sleep-free 設計が正しく実装されていることを確認。
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[test]
    fn tc_u_no_sleep_retry_policy_sleep_is_zero() {
        use std::time::Duration;
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        for attempt in 1..=5u32 {
            assert_eq!(
                policy.sleep_duration(attempt),
                Duration::ZERO,
                "attempt {attempt}: sleep_duration が Duration::ZERO でない"
            );
        }
    }

    /// TC-U-NoSleep-2 — `NoSleepRetryPolicy::max_attempts` は構築時に指定した値を返す。
    ///
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[test]
    fn tc_u_no_sleep_retry_policy_max_attempts() {
        let policy3 = NoSleepRetryPolicy { max_attempts: 3 };
        assert_eq!(policy3.max_attempts(), 3);
        let policy5 = NoSleepRetryPolicy { max_attempts: 5 };
        assert_eq!(policy5.max_attempts(), 5);
    }

    /// TC-U-NoSleep-3 (non-Windows) — 非 Windows では `should_retry` は常に `false`。
    ///
    /// Unix は rename が即 fail fast（retry なし）のため、エラーコードに関係なく
    /// retry しないことを確認する。
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[cfg(not(windows))]
    #[test]
    fn tc_u_no_sleep_retry_policy_should_not_retry_non_windows() {
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        // Unix では Windows エラーコード相当でも retry しない
        assert!(!policy.should_retry(5)); // ERROR_ACCESS_DENIED 相当
        assert!(!policy.should_retry(32)); // ERROR_SHARING_VIOLATION 相当
        assert!(!policy.should_retry(33)); // ERROR_LOCK_VIOLATION 相当
        assert!(!policy.should_retry(0));
        assert!(!policy.should_retry(2));
    }

    /// TC-U-NoSleep-3 (Windows) — Windows では `should_retry(5/32/33)` は `true`、
    /// それ以外は `false`。
    ///
    /// `ERROR_ACCESS_DENIED (5)` / `ERROR_SHARING_VIOLATION (32)` /
    /// `ERROR_LOCK_VIOLATION (33)` が retry 対象として正しく判定されることを確認する。
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[cfg(windows)]
    #[test]
    fn tc_u_no_sleep_retry_policy_should_retry_windows() {
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        // retry 対象（一過性 file lock エラー）
        assert!(policy.should_retry(5)); // ERROR_ACCESS_DENIED
        assert!(policy.should_retry(32)); // ERROR_SHARING_VIOLATION
        assert!(policy.should_retry(33)); // ERROR_LOCK_VIOLATION
                                          // retry 非対象
        assert!(!policy.should_retry(0));
        assert!(!policy.should_retry(2)); // ERROR_FILE_NOT_FOUND
        assert!(!policy.should_retry(6)); // ERROR_INVALID_HANDLE
    }

    /// TC-U-FinalizeFail (Unix) — `finalize` が FsyncTemp ステップで失敗した場合に
    /// `.new` ファイルが best-effort 削除される。
    ///
    /// `finalize` は冒頭で `new_path.take()` により Drop への cleanup 委譲を切り离し、
    /// 各失敗ステップで `AtomicWriter::cleanup_new` を明示呼出する設計を確認する。
    ///
    /// 注入方法: `.new` を chmod 0o400（書き込み不可）にして
    /// `FsyncTemp` の `.write(true).open()` を `PermissionDenied` で失敗させる。
    /// 親ディレクトリは 0o700 のまま維持するため `cleanup_new` の `remove_file` は成功する。
    ///
    /// 対応設計書: `./classes.md` §3.2 finalize § cleanup_new 明示呼出
    #[cfg(unix)]
    #[test]
    fn tc_u_finalize_failure_cleans_new_on_fsync_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let paths = VaultPaths::new_unchecked(dir.path().to_path_buf());
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        let vault = plaintext_vault("finalize-fail", "test-value");
        let session = AtomicWriteSession::new(&paths, &vault).unwrap();
        assert!(paths.vault_db_new().exists(), ".new が作成されていない");

        // .new を書き込み不可（0o400）に変更 → FsyncTemp が open(.write(true)) で失敗。
        // 親ディレクトリは 0o700 のまま → cleanup_new の remove_file は成功する。
        std::fs::set_permissions(paths.vault_db_new(), std::fs::Permissions::from_mode(0o400))
            .unwrap();

        let result = session.finalize(&ExponentialBackoffRetryPolicy);

        // FsyncTemp で AtomicWriteFailed が返ること
        assert!(
            matches!(
                result,
                Err(PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::FsyncTemp,
                    ..
                })
            ),
            "AtomicWriteFailed {{ stage: FsyncTemp }} を期待したが: {:?}",
            result.err()
        );

        // finalize 失敗後でも .new が cleanup されている
        assert!(
            !paths.vault_db_new().exists(),
            ".new が残存している — finalize 失敗時の明示的 cleanup_new が機能していない"
        );
    }

    /// TC-U-WinNoSleep (Windows) — `NoSleepRetryPolicy` を使うと retry が即座に全敗し、
    /// `AtomicWriteFailed { stage: Rename }` が返り `.new` が cleanup される。
    ///
    /// `NoSleepRetryPolicy` が sleep 0ms のため、`vault.db` を長時間 `FILE_SHARE_NONE`
    /// で保持している間に finalize を呼ぶと 5 回の retry が一瞬で全て失敗する。これにより:
    ///
    /// 1. retry ループが実際に発火することを確認
    /// 2. 全敗時に `.new` が cleanup されることを確認
    ///
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy` / `./classes.md` §3.2 finalize
    /// AC-19 (Issue #65 retry 補強) 対応。
    #[cfg(windows)]
    #[test]
    fn tc_u_windows_no_sleep_retry_exhausts_on_held_file() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        let dir = TempDir::new().unwrap();
        let paths = VaultPaths::new_unchecked(dir.path().to_path_buf());
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        // 初期 vault.db を作成
        let vault1 = plaintext_vault("v1", "initial");
        let session1 = AtomicWriteSession::new(&paths, &vault1).unwrap();
        session1
            .finalize(&ExponentialBackoffRetryPolicy)
            .expect("初期 vault 作成失敗");

        // 更新用 session を新規作成
        let vault2 = plaintext_vault("v2", "updated");
        let session2 = AtomicWriteSession::new(&paths, &vault2).unwrap();
        assert!(
            paths.vault_db_new().exists(),
            "session2 作成後 .new が存在しない"
        );

        // 補助スレッドが vault.db を FILE_SHARE_NONE で保持する。
        // NoSleepRetryPolicy は sleep 0ms のため 5 回の即時 retry が完了するより長く保持する。
        const FILE_SHARE_NONE: u32 = 0;
        const HOLD_MS: u64 = 500; // 5 回即時 retry (~数十μs) より十分長い
        let vault_db = paths.vault_db().to_path_buf();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let handle = thread::spawn(move || {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_NONE)
                .open(&vault_db)
                .expect("vault.db の排他オープン失敗");
            ready_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(HOLD_MS));
            drop(f);
        });
        ready_rx.recv().unwrap();

        // NoSleepRetryPolicy で finalize → 5 回即時 retry が全て失敗
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        let result = session2.finalize(&policy);
        handle.join().unwrap();

        // AtomicWriteFailed { stage: Rename } が返る（retry 全敗 + fail fast）
        assert!(
            matches!(
                result,
                Err(PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::Rename,
                    ..
                })
            ),
            "AtomicWriteFailed {{ stage: Rename }} を期待したが: {:?}",
            result.err()
        );

        // .new が cleanup されている（finalize 全敗時の明示的 cleanup_new）
        assert!(
            !paths.vault_db_new().exists(),
            ".new が残存している — retry 全敗時の cleanup_new が機能していない"
        );
    }

    // -------------------------------------------------------------------
    // TC-I29-D (unit) — Win retry 中 TOCTOU 再検証ユニットテスト群
    // -------------------------------------------------------------------
    //
    // `AtomicWriteSession::reverify_no_reparse_point` は `cfg(windows)` の
    // rename retry ループ内で各 attempt の sleep 直後に呼ばれ、retry 窓中の
    // symlink / NTFS reparse point 差替えを fail fast する。
    // 基本設計 `security.md §atomic write の二次防衛線 §Win retry 中 TOCTOU` 対応。
    //
    // 整合する受入基準: AC-19 (Issue #65 retry 補強の二次防衛線、TOCTOU 側)。

    /// TC-I29-D-1: 通常ファイル → Ok。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d1_reverify_returns_ok_for_regular_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("regular.bin");
        std::fs::write(&f, b"x").unwrap();
        AtomicWriteSession::reverify_no_reparse_point(&f)
            .expect("通常ファイルで reverify が誤判定 (Ok を期待)");
    }

    /// TC-I29-D-2: 未存在パス (vault.db 初回作成時の final_path) → Ok。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d2_reverify_returns_ok_for_missing_path() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("not_yet.bin");
        AtomicWriteSession::reverify_no_reparse_point(&f)
            .expect("未存在パスで reverify が誤判定 (Ok を期待 — 初回 save の final_path 経路)");
    }

    /// TC-I29-D-3: ディレクトリ junction (`mklink /J`) → SymlinkNotAllowed。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d3_reverify_detects_directory_junction() {
        use crate::persistence::error::VaultDirReason;

        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target_dir");
        std::fs::create_dir(&target).unwrap();
        let junction = dir.path().join("junction_dir");

        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .status()
            .expect("cmd /C mklink /J 実行不能");

        if !status.success() {
            eprintln!(
                "skipping tc_i29_d3: mklink /J が失敗した (権限不足の可能性、exit={:?})",
                status.code()
            );
            return;
        }

        let result = AtomicWriteSession::reverify_no_reparse_point(&junction);
        match result {
            Err(PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::SymlinkNotAllowed,
                ..
            }) => {}
            other => panic!(
                "junction で SymlinkNotAllowed を期待したが {:?}",
                other.err()
            ),
        }
    }

    /// TC-I29-D-4: ディレクトリ symlink → SymlinkNotAllowed。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d4_reverify_detects_directory_symlink() {
        use crate::persistence::error::VaultDirReason;
        use std::os::windows::fs::symlink_dir;

        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target_dir");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("symlink_dir");

        if symlink_dir(&target, &link).is_err() {
            eprintln!("skipping tc_i29_d4: dir symlink 作成権限が無い (Developer Mode 無効)");
            return;
        }

        let result = AtomicWriteSession::reverify_no_reparse_point(&link);
        match result {
            Err(PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::SymlinkNotAllowed,
                ..
            }) => {}
            other => panic!(
                "dir symlink で SymlinkNotAllowed を期待したが {:?}",
                other.err()
            ),
        }
    }
}
