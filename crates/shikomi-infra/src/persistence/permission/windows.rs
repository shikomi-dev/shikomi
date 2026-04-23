//! Windows 固有のパーミッション実装（スタブ）。
//!
//! Windows ACL の完全実装は Win32 API (unsafe) を必要とするため、
//! 現フェーズでは標準ライブラリのアクセス可能な権限操作のみを実装する。
//! 完全な所有者専用 DACL 設定は将来のセキュリティ強化 Issue で対応する。

use std::path::Path;

use crate::persistence::error::PersistenceError;

/// ディレクトリを作成する（パーミッション設定は Windows では省略）。
///
/// # Errors
///
/// - ディレクトリ作成失敗: `PersistenceError::Io`
pub(super) fn ensure_dir(path: &Path) -> Result<(), PersistenceError> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| PersistenceError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    }
    Ok(())
}

/// ファイルのパーミッション設定（Windows では省略）。
///
/// Windows での完全な ACL 設定は Win32 API (unsafe) が必要なため、現フェーズでは no-op。
pub(super) fn ensure_file(_path: &Path) -> Result<(), PersistenceError> {
    // Windows: full ACL requires Win32 API (unsafe) — deferred to future security hardening issue
    Ok(())
}

/// ディレクトリの存在確認（Windows では基本チェックのみ）。
///
/// # Errors
///
/// - パスがディレクトリでない: `PersistenceError::Io`
pub(super) fn verify_dir(path: &Path) -> Result<(), PersistenceError> {
    if !path.is_dir() {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"),
        });
    }
    Ok(())
}

/// ファイルの存在確認（Windows では基本チェックのみ）。
///
/// # Errors
///
/// - パスがファイルでない: `PersistenceError::Io`
pub(super) fn verify_file(path: &Path) -> Result<(), PersistenceError> {
    if !path.is_file() {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not a file"),
        });
    }
    Ok(())
}
