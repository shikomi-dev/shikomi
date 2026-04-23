//! vault ファイルロック RAII ハンドル。
//!
//! `fs4` クレートの `FileExt` trait を使い、OS レベルのアドバイザリロックを取得する。

use std::fs::OpenOptions;

use fs4::fs_std::FileExt;

use super::error::PersistenceError;
use super::paths::VaultPaths;

// -------------------------------------------------------------------
// VaultLock
// -------------------------------------------------------------------

/// vault のロックファイルを保持する RAII ハンドル。
///
/// `Drop` 時にロックを解放する。
pub(crate) struct VaultLock {
    _file: std::fs::File,
}

impl VaultLock {
    /// 排他ロックを取得する。
    ///
    /// # Errors
    ///
    /// - ロックファイルのオープン失敗: `PersistenceError::Io`
    /// - 排他ロック取得失敗（他プロセスが保持中）: `PersistenceError::Locked`
    pub(crate) fn acquire_exclusive(paths: &VaultPaths) -> Result<Self, PersistenceError> {
        let path = paths.vault_db_lock();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| PersistenceError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;

        file.try_lock_exclusive().map_err(|_| PersistenceError::Locked {
            path: path.to_path_buf(),
            holder_hint: None,
        })?;

        Ok(Self { _file: file })
    }

    /// 共有ロックを取得する。
    ///
    /// # Errors
    ///
    /// - ロックファイルのオープン失敗: `PersistenceError::Io`
    /// - 共有ロック取得失敗（排他ロック保持中）: `PersistenceError::Locked`
    pub(crate) fn acquire_shared(paths: &VaultPaths) -> Result<Self, PersistenceError> {
        let path = paths.vault_db_lock();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| PersistenceError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;

        file.try_lock_shared().map_err(|_| PersistenceError::Locked {
            path: path.to_path_buf(),
            holder_hint: None,
        })?;

        Ok(Self { _file: file })
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        // ロック解放失敗は無視する（プロセス終了時に OS が解放する）
        self._file.unlock().ok();
    }
}
