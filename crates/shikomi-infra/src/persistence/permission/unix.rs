//! Unix 固有のパーミッション実装。
//!
//! ディレクトリは 0700、ファイルは 0600 を強制する。

use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

use crate::persistence::error::PersistenceError;

/// ディレクトリの期待パーミッション（所有者 rwx のみ）。
const DIR_MODE_UNIX: u32 = 0o700;

/// ファイルの期待パーミッション（所有者 rw のみ）。
const FILE_MODE_UNIX: u32 = 0o600;

/// パーミッションのマスク（下位 9 ビット）。
const MODE_MASK_UNIX: u32 = 0o777;

/// ディレクトリを作成し、0700 のパーミッションを設定する。
///
/// # Errors
///
/// - ディレクトリ作成失敗: `PersistenceError::Io`
/// - パーミッション設定失敗: `PersistenceError::Io`
pub(super) fn ensure_dir(path: &Path) -> Result<(), PersistenceError> {
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE_UNIX)
            .create(path)
            .map_err(|e| PersistenceError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
    }
    // 既存でも mode を強制設定
    fs::set_permissions(path, fs::Permissions::from_mode(DIR_MODE_UNIX)).map_err(|e| {
        PersistenceError::Io {
            path: path.to_path_buf(),
            source: e,
        }
    })
}

/// ファイルに 0600 のパーミッションを設定する。
///
/// # Errors
///
/// - パーミッション設定失敗: `PersistenceError::Io`
pub(super) fn ensure_file(path: &Path) -> Result<(), PersistenceError> {
    fs::set_permissions(path, fs::Permissions::from_mode(FILE_MODE_UNIX)).map_err(|e| {
        PersistenceError::Io {
            path: path.to_path_buf(),
            source: e,
        }
    })
}

/// ディレクトリのパーミッションが 0700 であることを確認する。
///
/// # Errors
///
/// - メタデータ取得失敗: `PersistenceError::Io`
/// - パーミッション不一致: `PersistenceError::InvalidPermission`
pub(super) fn verify_dir(path: &Path) -> Result<(), PersistenceError> {
    let meta = fs::metadata(path).map_err(|e| PersistenceError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mode = meta.mode() & MODE_MASK_UNIX;
    if mode != DIR_MODE_UNIX {
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: "0700",
            actual: format!("{mode:04o}"),
        });
    }
    Ok(())
}

/// ファイルのパーミッションが 0600 であることを確認する。
///
/// # Errors
///
/// - メタデータ取得失敗: `PersistenceError::Io`
/// - パーミッション不一致: `PersistenceError::InvalidPermission`
pub(super) fn verify_file(path: &Path) -> Result<(), PersistenceError> {
    let meta = fs::metadata(path).map_err(|e| PersistenceError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mode = meta.mode() & MODE_MASK_UNIX;
    if mode != FILE_MODE_UNIX {
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: "0600",
            actual: format!("{mode:04o}"),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::error::PersistenceError;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    // --- TC-U09: PermissionGuard::verify_dir — 0700 → Ok ---

    #[test]
    fn tc_u09_verify_dir_0700_ok() {
        let dir = TempDir::new().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(verify_dir(dir.path()).is_ok(), "0700 は Ok のはず");
    }

    // --- TC-U10: PermissionGuard::verify_dir — 0755 → InvalidPermission ---

    #[test]
    fn tc_u10_verify_dir_0755_invalid() {
        let dir = TempDir::new().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        let result = verify_dir(dir.path());
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidPermission {
                    expected: "0700",
                    ..
                })
            ),
            "InvalidPermission を期待したが {result:?}"
        );
    }

    // --- TC-U11: PermissionGuard::verify_file — 0600 → Ok ---

    #[test]
    fn tc_u11_verify_file_0600_ok() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.db");
        std::fs::write(&file_path, b"").unwrap();
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(verify_file(&file_path).is_ok(), "0600 は Ok のはず");
    }

    // --- TC-U12: PermissionGuard::verify_file — 0644 → InvalidPermission ---

    #[test]
    fn tc_u12_verify_file_0644_invalid() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.db");
        std::fs::write(&file_path, b"").unwrap();
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o644)).unwrap();

        let result = verify_file(&file_path);
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidPermission {
                    expected: "0600",
                    ..
                })
            ),
            "InvalidPermission を期待したが {result:?}"
        );
    }
}
