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
    file: std::fs::File,
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

        file.try_lock_exclusive()
            .map_err(|_| PersistenceError::Locked {
                path: path.to_path_buf(),
                holder_hint: None,
            })?;

        Ok(Self { file })
    }

    /// 共有ロックを取得する。
    ///
    /// # Errors
    ///
    /// - ロックファイルのオープン失敗: `PersistenceError::Io`
    /// - 共有ロック取得失敗（排他ロック保持中）: `PersistenceError::Locked`
    // fs4 0.9 の try_lock_shared / unlock は Rust 1.89 で std に同名メソッドが安定化されたため
    // MSRV (1.80) との互換性 lint が誤検知する。実体は fs4 trait 実装（flock ベース）。
    #[allow(clippy::incompatible_msrv)]
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

        file.try_lock_shared()
            .map_err(|_| PersistenceError::Locked {
                path: path.to_path_buf(),
                holder_hint: None,
            })?;

        Ok(Self { file })
    }
}

impl Drop for VaultLock {
    // fs4 0.9 の unlock は Rust 1.89 で std に同名メソッドが安定化されたため lint を抑制
    #[allow(clippy::incompatible_msrv)]
    fn drop(&mut self) {
        // ロック解放失敗は無視する（プロセス終了時に OS が解放する）
        self.file.unlock().ok();
    }
}
