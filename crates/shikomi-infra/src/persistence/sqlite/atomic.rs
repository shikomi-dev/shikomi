//! アトミック書き込みユーティリティ。
//!
//! `.new` ファイルへの書き込み → fsync → rename によるアトミック更新を提供する。
//!
//! Issue #65 補強: `write_new` の終端で WAL チェックポイント+`journal_mode=DELETE`+
//! `Connection::close()` を明示実施し、サイドカー（`-journal` / `-wal` / `-shm`）の
//! 物理消去を契約として固定する。残存サイドカーには owner-only DACL を best-effort で
//! 適用する。`fsync_and_rename` の rename 段は Windows のみ一過性エラー
//! （`ERROR_ACCESS_DENIED` / `ERROR_SHARING_VIOLATION` / `ERROR_LOCK_VIOLATION`）に対し
//! 50ms ± jitter × 最大 5 回 retry を行い、retry 直前に symlink / reparse point を
//! 再検証して TOCTOU 差替えを fail fast する。詳細は `docs/features/vault-persistence/`
//! の各設計書を参照。

use std::path::Path;

use rusqlite::OpenFlags;
use shikomi_core::Vault;

// `Audit::retry_event` は cfg(windows) rename retry でのみ呼出される。
// 非 Windows ビルドの unused import 警告を避けるため import 自体を cfg gate する。
#[cfg(windows)]
use crate::persistence::audit::Audit;
use crate::persistence::error::{AtomicWriteStage, PersistenceError};
use crate::persistence::paths::VaultPaths;
use crate::persistence::permission::PermissionGuard;

use super::mapping::Mapping;
use super::schema::SchemaSql;

// -------------------------------------------------------------------
// 内部定数（Issue #65）
// -------------------------------------------------------------------

/// SQLite サイドカーファイル名のサフィックス（rusqlite が `<db>` の隣接に作成する）。
///
/// `PRAGMA wal_checkpoint(TRUNCATE)` + `PRAGMA journal_mode = DELETE` + `Connection::close()`
/// により原則消去される（`docs/features/vault-persistence/basic-design/security.md`
/// §atomic write の二次防衛線 §サイドカーの DACL 適用）。本定数は残存時の DACL 強制で参照する。
const SQLITE_SIDECAR_SUFFIXES: &[&str] = &["-journal", "-wal", "-shm"];

/// `cfg(windows)` 限定 rename retry の上限回数。
///
/// 5 回 × (50ms ± 25ms jitter) = 最悪 ~375ms / 平均 ~250ms。SSoT 上限値は
/// `docs/features/vault-persistence/basic-design/security.md` §jitter（同期参照先 4 ファイル列挙）。
#[cfg(windows)]
const RENAME_MAX_RETRIES: u32 = 5;

/// retry 間隔の中央値（ミリ秒）。
#[cfg(windows)]
const RENAME_BASE_DELAY_MS: u64 = 50;

/// retry 間隔の jitter 半幅（±N ミリ秒、timing oracle 防止）。
#[cfg(windows)]
const RENAME_JITTER_HALF_RANGE_MS: u64 = 25;

/// jitter 抽選範囲（`OsRng` 1 byte の `% N` 値、`0..=2*HALF_RANGE` を網羅）。
#[cfg(windows)]
const RENAME_JITTER_RANGE: u8 = 51;

/// 監査ログに記録する rename stage 名（`Audit::retry_event` の `stage` 引数）。
#[cfg(windows)]
const RENAME_STAGE_LABEL: &str = "rename";

// -------------------------------------------------------------------
// AtomicWriter
// -------------------------------------------------------------------

/// アトミック書き込み操作を提供するゼロサイズ型。
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

    /// `vault.db.new` に vault の内容を書き込む。
    ///
    /// Issue #65 補強: tx.commit 後に `PRAGMA wal_checkpoint(TRUNCATE)` +
    /// `PRAGMA journal_mode = DELETE` + `Connection::close()` を明示実施し、
    /// rusqlite Drop の `sqlite3_close_v2` 遅延 semantics を回避する。
    /// close 失敗時は `.new` を best-effort 削除して元の Sqlite エラーを伝播する。
    /// 最後に残存サイドカー（`-journal` / `-wal` / `-shm`）に owner-only DACL を強制適用する。
    ///
    /// # Errors
    ///
    /// - ファイル作成失敗: `PersistenceError::AtomicWriteFailed { stage: PrepareNew }`
    /// - SQLite エラー（PRAGMA / DDL / トランザクション / close）: `PersistenceError::Sqlite`
    pub(crate) fn write_new(paths: &VaultPaths, vault: &Vault) -> Result<(), PersistenceError> {
        let new_path = paths.vault_db_new();

        // 適切なパーミッションでファイルを事前作成
        Self::create_with_permissions(new_path)?;

        // SQLite 接続を開く
        let conn = rusqlite::Connection::open_with_flags(
            new_path,
            OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // PRAGMA 設定とテーブル作成
        conn.execute_batch(SchemaSql::PRAGMA_APPLICATION_ID_SET)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::PRAGMA_USER_VERSION_SET)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::PRAGMA_JOURNAL_MODE)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::CREATE_VAULT_HEADER)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;
        conn.execute_batch(SchemaSql::CREATE_RECORDS)
            .map_err(|e| PersistenceError::Sqlite { source: e })?;

        // トランザクション: vault_header と全レコードを挿入
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
                    ],
                )
                .map_err(|e| PersistenceError::Sqlite { source: e })?;
            }

            tx.commit()
                .map_err(|e| PersistenceError::Sqlite { source: e })?;
        }

        // Issue #65: WAL サイドカーをチェックポイント+truncate で物理空にする
        // （DELETE モード採用時は no-op、副作用なし。SQLite "Write-Ahead Logging" §`PRAGMA wal_checkpoint`）。
        Self::sqlite_pragma(&conn, new_path, "PRAGMA wal_checkpoint(TRUNCATE);")?;

        // Issue #65: 残存サイドカーを削除モードに切替し、close 時の物理消去を契約として固定。
        // schema.rs の初期 PRAGMA_JOURNAL_MODE と冗長だが、将来 WAL モードへ切り替える設計判断が
        // 入った時に Win rename race を再導入する罠を構造的に塞ぐ Boy Scout / Fail Safe。
        Self::sqlite_pragma(&conn, new_path, "PRAGMA journal_mode = DELETE;")?;

        // Issue #65: rusqlite Drop の sqlite3_close_v2 遅延 semantics
        // （pending stmt cache があると close 遅延）を回避するため明示クローズ。
        // 失敗時は .new を best-effort 削除し元の Sqlite エラーを伝播
        // （`docs/features/vault-persistence/basic-design/security.md`
        //  §atomic write の二次防衛線 §`Connection::close()` 失敗時の `.new` クリーンアップ）。
        if let Err((_, e)) = conn.close() {
            Self::cleanup_orphan_best_effort(new_path);
            return Err(PersistenceError::Sqlite { source: e });
        }

        // Issue #65: 残存サイドカーに owner-only DACL を強制適用（多層防御）。
        // 通常は journal_mode=DELETE で消去済だが、SQLite の特殊ケース（journal_mode=TRUNCATE
        // 採用時のヘッダ残存等）を取り溢さないよう存在チェックの上で best-effort 適用する。
        Self::apply_sidecar_permissions_if_present(new_path);

        Ok(())
    }

    /// `Connection::execute_batch` の薄いラッパ（write_new の close-flow 用）。
    ///
    /// 失敗時は `.new` を best-effort 削除して `PersistenceError::Sqlite` を返す。
    /// commit 後の close-flow（wal_checkpoint / journal_mode）で失敗した場合、
    /// `.new` は中途半端な状態になるため Fail Secure で破棄する（次回 save の OrphanNewFile を回避）。
    fn sqlite_pragma(
        conn: &rusqlite::Connection,
        new_path: &Path,
        sql: &'static str,
    ) -> Result<(), PersistenceError> {
        conn.execute_batch(sql).map_err(|e| {
            Self::cleanup_orphan_best_effort(new_path);
            PersistenceError::Sqlite { source: e }
        })
    }

    /// SQLite サイドカー（`-journal` / `-wal` / `-shm`）が残存していれば owner-only
    /// パーミッションを強制適用する（best-effort、失敗は warn のみ）。
    ///
    /// 通常は `PRAGMA journal_mode = DELETE` + `Connection::close()` で消去されるが、
    /// 多層防御として存在チェックの上で適用する
    /// （`docs/features/vault-persistence/basic-design/security.md`
    ///  §atomic write の二次防衛線 §サイドカーの DACL 適用）。
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

    /// `.new` ファイルを fsync し、`vault.db` にアトミックにリネームする。
    ///
    /// rename 段は POSIX では `rename(2)`、Windows では `MoveFileExW` 経由で原子的。
    /// Windows のみ Issue #65 由来の一過性エラー（`ERROR_ACCESS_DENIED` /
    /// `ERROR_SHARING_VIOLATION` / `ERROR_LOCK_VIOLATION`）に対し
    /// 50ms ± jitter × 最大 5 回 retry を補強し、retry 直前に symlink / reparse point の
    /// 再検証で TOCTOU 差替えを fail fast する
    /// （`docs/features/vault-persistence/detailed-design/flows.md` §`save` step 7）。
    ///
    /// # Errors
    ///
    /// - `.new` オープン失敗: `PersistenceError::AtomicWriteFailed { stage: FsyncTemp }`
    /// - fsync 失敗: `PersistenceError::AtomicWriteFailed { stage: FsyncTemp }`
    /// - ディレクトリ fsync 失敗（Unix のみ）: `PersistenceError::AtomicWriteFailed { stage: FsyncDir }`
    /// - リネーム失敗（Win では retry 5 回全敗後）: `PersistenceError::AtomicWriteFailed { stage: Rename }`
    /// - retry 中 symlink / reparse point 検出（Win のみ）: `PersistenceError::InvalidVaultDir { reason: SymlinkNotAllowed }`
    /// - リネーム後の DACL 設定失敗: `PersistenceError::Io` / `PersistenceError::InvalidPermission`
    pub(crate) fn fsync_and_rename(paths: &VaultPaths) -> Result<(), PersistenceError> {
        let new_path = paths.vault_db_new();
        let final_path = paths.vault_db();

        // `.new` ファイルを fsync
        // Windows では FlushFileBuffers（sync_all の実装）に書き込みアクセスが必要なため、
        // read(true).write(true) でオープンする。Unix では read-only でも sync_all は成功する。
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(new_path)
            .map_err(|e| PersistenceError::AtomicWriteFailed {
                stage: AtomicWriteStage::FsyncTemp,
                source: e,
            })?;
        file.sync_all().map_err(|e| {
            Self::cleanup_orphan_best_effort(new_path);
            PersistenceError::AtomicWriteFailed {
                stage: AtomicWriteStage::FsyncTemp,
                source: e,
            }
        })?;
        drop(file);

        // 親ディレクトリの fsync（Unix: POSIX 耐久性保証のため）
        #[cfg(unix)]
        {
            let dir = std::fs::File::open(paths.dir()).map_err(|e| {
                Self::cleanup_orphan_best_effort(new_path);
                PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::FsyncDir,
                    source: e,
                }
            })?;
            dir.sync_all().map_err(|e| {
                Self::cleanup_orphan_best_effort(new_path);
                PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::FsyncDir,
                    source: e,
                }
            })?;
        }

        // アトミックリネーム（POSIX では原子的、Windows は MoveFileExW 経由）
        // ensure_file は DELETE を含まない DACL を設定するため、rename 前ではなく rename 後に適用する。
        Self::rename_atomic(new_path, final_path)?;

        // リネーム後に最終ファイルへ owner-only DACL を適用する
        PermissionGuard::ensure_file(final_path)?;

        Ok(())
    }

    /// `.new` → `vault.db` のアトミックリネーム本体。
    ///
    /// 第 1 試行が成功すれば即 return。失敗時、Windows のみ一過性エラーを判別して
    /// `windows_rename_retry` に委譲。それ以外（POSIX 全般 / Windows 非一過性）は
    /// `.new` を best-effort 削除して `AtomicWriteFailed { stage: Rename }` を return。
    fn rename_atomic(new_path: &Path, final_path: &Path) -> Result<(), PersistenceError> {
        let initial_err = match std::fs::rename(new_path, final_path) {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };

        #[cfg(windows)]
        {
            if Self::is_windows_transient_rename_error(&initial_err) {
                return Self::windows_rename_retry(new_path, final_path, initial_err);
            }
        }

        Self::cleanup_orphan_best_effort(new_path);
        Err(PersistenceError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            source: initial_err,
        })
    }

    /// `.new` ファイルのクリーンアップを試みる（ベストエフォート）。
    fn cleanup_orphan_best_effort(new_path: &Path) {
        if let Err(e) = std::fs::remove_file(new_path) {
            tracing::warn!(
                path = %new_path.display(),
                error = %e,
                "failed to cleanup .new file (best-effort)"
            );
        }
    }

    /// Windows の一過性 rename エラー（Win Indexer / Defender / SQLite Drop 残響等）を判定する。
    ///
    /// retry 対象:
    /// - `ERROR_ACCESS_DENIED (5)` — file handle 解放遅延
    /// - `ERROR_SHARING_VIOLATION (32)` — 共有モード不一致
    /// - `ERROR_LOCK_VIOLATION (33)` — ファイルロック競合
    ///
    /// それ以外（`ERROR_DISK_FULL (112)` / `ERROR_PATH_NOT_FOUND (3)` 等）は即 fail fast。
    #[cfg(windows)]
    fn is_windows_transient_rename_error(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(5 | 32 | 33))
    }

    /// `cfg(windows)` 限定の rename retry ループ（Issue #65）。
    ///
    /// 各試行の前に jitter sleep（`50ms ± 25ms` 一様乱数、timing oracle 防止）を挟み、
    /// rename 直前に `.new` / `vault.db` 双方の symlink / NTFS reparse point を再検証する
    /// （`docs/features/vault-persistence/basic-design/security.md`
    ///  §atomic write の二次防衛線 §Win retry 中 TOCTOU）。
    /// 各試行の発火・成功・全敗を `Audit::retry_event` で監査ログに記録し、
    /// daemon 側 subscriber が DoS 兆候として上位通報する経路を有効化する。
    ///
    /// 上限は jitter 込み最悪 ~375ms（`50+25` × 5）/ 平均 ~250ms（同 §jitter）。
    #[cfg(windows)]
    fn windows_rename_retry(
        new_path: &Path,
        final_path: &Path,
        initial_err: std::io::Error,
    ) -> Result<(), PersistenceError> {
        let start = std::time::Instant::now();
        let mut last_err = initial_err;

        for attempt in 1..=RENAME_MAX_RETRIES {
            let last_raw_os = last_err.raw_os_error().unwrap_or(0);
            let elapsed_ms_pending = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            Audit::retry_event(
                RENAME_STAGE_LABEL,
                attempt,
                last_raw_os,
                elapsed_ms_pending,
                "pending",
            );

            std::thread::sleep(Self::jittered_retry_delay());

            // TOCTOU 再検証 — retry 窓中の symlink / junction 差替えを fail fast する
            Self::reverify_no_reparse_point(new_path)?;
            Self::reverify_no_reparse_point(final_path)?;

            match std::fs::rename(new_path, final_path) {
                Ok(()) => {
                    let elapsed_ms_ok =
                        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    Audit::retry_event(
                        RENAME_STAGE_LABEL,
                        attempt,
                        last_raw_os,
                        elapsed_ms_ok,
                        "succeeded",
                    );
                    return Ok(());
                }
                Err(e) => {
                    if !Self::is_windows_transient_rename_error(&e) {
                        Self::cleanup_orphan_best_effort(new_path);
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
            RENAME_STAGE_LABEL,
            RENAME_MAX_RETRIES,
            final_raw_os,
            elapsed_ms_exhausted,
            "exhausted",
        );
        Self::cleanup_orphan_best_effort(new_path);
        Err(PersistenceError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            source: last_err,
        })
    }

    /// retry 間隔の jitter 込み Duration を `OsRng` 由来の 1 byte で生成する。
    ///
    /// `50ms ± 25ms` 一様乱数（`[25, 75]` ms）。`OsRng.fill_bytes` は対象 OS では
    /// 事実上失敗しない（`crypto::rng` と同じ CSPRNG 経路）。`% 51` の僅かな mod bias は
    /// timing jitter 用途では無視可能（KISS）。
    #[cfg(windows)]
    fn jittered_retry_delay() -> std::time::Duration {
        use rand_core::{OsRng, RngCore};

        let mut buf = [0u8; 1];
        OsRng.fill_bytes(&mut buf);
        let jitter_pos = u64::from(buf[0] % RENAME_JITTER_RANGE); // 0..=50
        let delay_ms = RENAME_BASE_DELAY_MS + jitter_pos - RENAME_JITTER_HALF_RANGE_MS;
        std::time::Duration::from_millis(delay_ms)
    }

    /// retry 直前の symlink / NTFS reparse point 再検証（Win TOCTOU 対策）。
    ///
    /// `fs::symlink_metadata` で `is_symlink()` または
    /// `FILE_ATTRIBUTE_REPARSE_POINT (0x400)` ビットを検出したら fail fast。
    /// 対象パスが未存在（vault.db の初回作成時等）の場合は Ok を返す。
    /// 詳細は `docs/features/vault-persistence/basic-design/security.md`
    /// §atomic write の二次防衛線 §Win retry 中 TOCTOU を参照。
    #[cfg(windows)]
    fn reverify_no_reparse_point(path: &Path) -> Result<(), PersistenceError> {
        use crate::persistence::error::VaultDirReason;
        use std::os::windows::fs::MetadataExt;

        // FILE_ATTRIBUTE_REPARSE_POINT — Microsoft Learn "File Attribute Constants"
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

    /// `vault.db.new` を書き込むが fsync/rename は行わない（テスト専用）。
    ///
    /// atomic write の中断状態を決定的に再現するためのテストフック。
    /// `write_new` と同一ロジックで `.new` を書き込み、`fsync_and_rename` を呼ばずに返す。
    ///
    /// # Errors
    ///
    /// - `write_new` と同じ
    #[cfg(test)]
    pub(crate) fn write_new_only(
        paths: &VaultPaths,
        vault: &Vault,
    ) -> Result<(), PersistenceError> {
        Self::write_new(paths, vault)
    }

    /// 適切なパーミッションでファイルを作成する。
    ///
    /// # Errors
    ///
    /// - ファイル作成失敗: `PersistenceError::AtomicWriteFailed { stage: PrepareNew }`
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

    /// TC-I06 — write_new_only フックで .new のみ書き込み→load が OrphanNewFile を返す。
    ///
    /// AC-06 対応。write_new_only は fsync_and_rename を呼ばないため .new が残り、
    /// vault.db の内容は初期 vault のままになる。
    #[test]
    fn tc_i06_write_new_only_hook_orphan() {
        let dir = TempDir::new().unwrap();
        let paths = VaultPaths::new_unchecked(dir.path().to_path_buf());

        // ディレクトリのパーミッションを設定
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        // 初期 vault を save（vault.db が存在する状態にする）
        let initial_vault = plaintext_vault("initial", "initial-value");
        AtomicWriter::write_new(&paths, &initial_vault).unwrap();
        AtomicWriter::fsync_and_rename(&paths).unwrap();

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
        // （ここでは detect_orphan を直接呼んでテスト）
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
}
