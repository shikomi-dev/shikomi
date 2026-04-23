//! アトミック書き込みユーティリティ。
//!
//! `.new` ファイルへの書き込み → fsync → rename によるアトミック更新を提供する。

use std::path::Path;

use rusqlite::OpenFlags;
use shikomi_core::Vault;

use crate::persistence::error::{AtomicWriteStage, PersistenceError};
use crate::persistence::paths::VaultPaths;
use crate::persistence::permission::PermissionGuard;

use super::mapping::Mapping;
use super::schema::SchemaSql;

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
    /// # Errors
    ///
    /// - ファイル作成失敗: `PersistenceError::AtomicWriteFailed { stage: PrepareNew }`
    /// - SQLite エラー: `PersistenceError::Sqlite`
    /// - パーミッション設定失敗: `PersistenceError::Io`
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

        // SQLite open 後にパーミッションを再設定
        PermissionGuard::ensure_file(new_path)?;

        // トランザクション: vault_header と全レコードを挿入
        {
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| PersistenceError::Sqlite { source: e })?;

            let header_params = Mapping::vault_header_to_params(vault.header());
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
                let p = Mapping::record_to_params(record);
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

        Ok(())
    }

    /// `.new` ファイルを fsync し、`vault.db` にアトミックにリネームする。
    ///
    /// # Errors
    ///
    /// - `.new` オープン失敗: `PersistenceError::AtomicWriteFailed { stage: FsyncTemp }`
    /// - fsync 失敗: `PersistenceError::AtomicWriteFailed { stage: FsyncTemp }`
    /// - ディレクトリ fsync 失敗（Unix のみ）: `PersistenceError::AtomicWriteFailed { stage: FsyncDir }`
    /// - リネーム失敗: `PersistenceError::AtomicWriteFailed { stage: Rename }`
    pub(crate) fn fsync_and_rename(paths: &VaultPaths) -> Result<(), PersistenceError> {
        let new_path = paths.vault_db_new();
        let final_path = paths.vault_db();

        // `.new` ファイルを fsync
        let file = std::fs::File::open(new_path).map_err(|e| PersistenceError::AtomicWriteFailed {
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
            let dir =
                std::fs::File::open(paths.dir()).map_err(|e| {
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

        // アトミックリネーム（POSIX では原子的）
        std::fs::rename(new_path, final_path).map_err(|e| {
            Self::cleanup_orphan_best_effort(new_path);
            PersistenceError::AtomicWriteFailed {
                stage: AtomicWriteStage::Rename,
                source: e,
            }
        })?;

        Ok(())
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

    /// `vault.db.new` を書き込むが fsync/rename は行わない（テスト専用）。
    ///
    /// atomic write の中断状態を決定的に再現するためのテストフック。
    /// `write_new` と同一ロジックで `.new` を書き込み、`fsync_and_rename` を呼ばずに返す。
    ///
    /// # Errors
    ///
    /// - `write_new` と同じ
    #[cfg(test)]
    pub(crate) fn write_new_only(paths: &VaultPaths, vault: &Vault) -> Result<(), PersistenceError> {
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
