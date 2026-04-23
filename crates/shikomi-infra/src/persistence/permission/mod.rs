//! ファイル・ディレクトリのパーミッション管理。
//!
//! OS 別の実装を `cfg_if` で切り替える。

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use super::error::PersistenceError;

// -------------------------------------------------------------------
// PermissionGuard
// -------------------------------------------------------------------

/// パーミッション操作を提供するゼロサイズ型。
pub(crate) struct PermissionGuard;

impl PermissionGuard {
    /// ディレクトリが存在しなければ作成し、適切なパーミッションを設定する。
    ///
    /// # Errors
    ///
    /// - ディレクトリ作成失敗: `PersistenceError::Io`
    /// - パーミッション設定失敗: `PersistenceError::Io`
    pub(crate) fn ensure_dir(path: &std::path::Path) -> Result<(), PersistenceError> {
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                unix::ensure_dir(path)
            } else if #[cfg(windows)] {
                windows::ensure_dir(path)
            } else {
                // 未サポートプラットフォーム: ベストエフォートで作成のみ
                if !path.exists() {
                    std::fs::create_dir_all(path).map_err(|e| PersistenceError::Io {
                        path: path.to_path_buf(),
                        source: e,
                    })?;
                }
                Ok(())
            }
        }
    }

    /// ファイルに適切なパーミッションを設定する。
    ///
    /// # Errors
    ///
    /// - パーミッション設定失敗: `PersistenceError::Io`
    pub(crate) fn ensure_file(path: &std::path::Path) -> Result<(), PersistenceError> {
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                unix::ensure_file(path)
            } else if #[cfg(windows)] {
                windows::ensure_file(path)
            } else {
                let _ = path;
                Ok(())
            }
        }
    }

    /// ディレクトリのパーミッションが期待値と一致することを確認する。
    ///
    /// # Errors
    ///
    /// - パーミッション不一致: `PersistenceError::InvalidPermission`
    /// - メタデータ取得失敗: `PersistenceError::Io`
    pub(crate) fn verify_dir(path: &std::path::Path) -> Result<(), PersistenceError> {
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                unix::verify_dir(path)
            } else if #[cfg(windows)] {
                windows::verify_dir(path)
            } else {
                let _ = path;
                Ok(())
            }
        }
    }

    /// ファイルのパーミッションが期待値と一致することを確認する。
    ///
    /// # Errors
    ///
    /// - パーミッション不一致: `PersistenceError::InvalidPermission`
    /// - メタデータ取得失敗: `PersistenceError::Io`
    pub(crate) fn verify_file(path: &std::path::Path) -> Result<(), PersistenceError> {
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                unix::verify_file(path)
            } else if #[cfg(windows)] {
                windows::verify_file(path)
            } else {
                let _ = path;
                Ok(())
            }
        }
    }
}
