use std::path::Path;

use super::helper::{
    apply_protected_dacl, build_owner_only_dacl, fetch_dacl_and_owner, fetch_owner_sid_from_path,
};
use super::verify::verify_dacl_owner_only;
use super::{EXPECTED_DIR_MASK, EXPECTED_DIR_STR, EXPECTED_FILE_MASK, EXPECTED_FILE_STR};
use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// 公開 API（`permission/mod.rs` の cfg_if! ディスパッチ先）
// ---------------------------------------------------------------------------

/// ディレクトリを作成し、所有者専用 DACL（`SE_DACL_PROTECTED`、ACE 1 個）を適用する。
///
/// 既存ディレクトリの場合も DACL を上書きする。
/// 所有者 SID はファイルシステムの `OWNER_SECURITY_INFORMATION` から取得し touch しない
/// （UAC 昇格環境で `BUILTIN\Administrators` が所有者になるケースに対応）。
///
/// # Errors
///
/// - ディレクトリ作成失敗: `PersistenceError::Io`
/// - Win32 ACL API 失敗: `PersistenceError::Io` / `PersistenceError::InvalidPermission`
pub(in crate::persistence::permission) fn ensure_dir(path: &Path) -> Result<(), PersistenceError> {
    std::fs::create_dir_all(path).map_err(|e| PersistenceError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    // ポインタ寿命: sd_guard が drop するまで owner_sid は有効。
    let (_sd_guard, owner_sid) = fetch_owner_sid_from_path(path)?;
    let acl_guard = build_owner_only_dacl(owner_sid, EXPECTED_DIR_MASK, path, EXPECTED_DIR_STR)?;
    // acl_guard が drop するまで内部 ACL ポインタは有効。
    apply_protected_dacl(path, &acl_guard)?;
    // _sd_guard / acl_guard がここで drop し LocalFree が走る。
    Ok(())
}

/// ファイルに所有者専用 DACL（`SE_DACL_PROTECTED`、ACE 1 個）を適用する。
///
/// 所有者は touch しない（`OWNER_SECURITY_INFORMATION` を `SecurityInfo` に含めない）。
///
/// # Errors
///
/// - Win32 ACL API 失敗: `PersistenceError::Io` / `PersistenceError::InvalidPermission`
pub(in crate::persistence::permission) fn ensure_file(path: &Path) -> Result<(), PersistenceError> {
    let (_sd_guard, owner_sid) = fetch_owner_sid_from_path(path)?;
    let acl_guard = build_owner_only_dacl(owner_sid, EXPECTED_FILE_MASK, path, EXPECTED_FILE_STR)?;
    apply_protected_dacl(path, &acl_guard)?;
    Ok(())
}

/// ディレクトリの DACL が 4 不変条件をすべて満たすか検証する。
///
/// # Errors
///
/// - Win32 API 失敗: `PersistenceError::Io`
/// - 不変条件違反: `PersistenceError::InvalidPermission`
pub(in crate::persistence::permission) fn verify_dir(path: &Path) -> Result<(), PersistenceError> {
    let (_sd_guard, owner_sid, dacl, control) = fetch_dacl_and_owner(path)?;
    verify_dacl_owner_only(
        path,
        dacl,
        control,
        owner_sid,
        EXPECTED_DIR_MASK,
        EXPECTED_DIR_STR,
    )
}

/// ファイルの DACL が 4 不変条件をすべて満たすか検証する。
///
/// # Errors
///
/// - Win32 API 失敗: `PersistenceError::Io`
/// - 不変条件違反: `PersistenceError::InvalidPermission`
pub(in crate::persistence::permission) fn verify_file(path: &Path) -> Result<(), PersistenceError> {
    let (_sd_guard, owner_sid, dacl, control) = fetch_dacl_and_owner(path)?;
    verify_dacl_owner_only(
        path,
        dacl,
        control,
        owner_sid,
        EXPECTED_FILE_MASK,
        EXPECTED_FILE_STR,
    )
}
