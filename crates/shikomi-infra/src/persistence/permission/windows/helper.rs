#![allow(unsafe_code)]

use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, PSID};
use windows_sys::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, DACL_SECURITY_INFORMATION,
    EXPLICIT_ACCESS_W, NO_INHERITANCE, NO_MULTIPLE_TRUSTEE, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID,
    TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{GetSecurityDescriptorControl, ACL};

use super::guard::{LocalFreeAclGuard, SecurityDescriptorGuard};
use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// ヘルパ：SID → 文字列
// ---------------------------------------------------------------------------

/// SID を `ConvertSidToStringSidW` で文字列化する。失敗時は `"<unknown>"` を返す。
pub(super) fn sid_to_string(sid: PSID) -> String {
    use super::guard::SidStringGuard;
    use windows_sys::Win32::Security::ConvertSidToStringSidW;

    if sid.is_null() {
        return String::from("<null>");
    }
    let mut raw_ptr: *mut u16 = ptr::null_mut();
    // SAFETY: sid は有効な SID ポインタ。raw_ptr は出力先。
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut raw_ptr) };
    if ok == 0 || raw_ptr.is_null() {
        return String::from("<unknown>");
    }
    SidStringGuard { ptr: raw_ptr }.to_string_lossy()
}

// ---------------------------------------------------------------------------
// ヘルパ：Path → ワイド文字列
// ---------------------------------------------------------------------------

/// `&Path` を Win32 API に渡せるヌル終端ワイド文字列（`Vec<u16>`）に変換する。
pub(super) fn path_to_wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0u16))
        .collect()
}

// ---------------------------------------------------------------------------
// fetch_owner_sid_from_path
// ---------------------------------------------------------------------------

/// ファイル / ディレクトリの `OWNER_SECURITY_INFORMATION` から所有者 SID を取得する。
///
/// 返される `PSID` は `SecurityDescriptorGuard` 内部ポインタで、
/// ガードの寿命内でのみ参照可能。
///
/// # Errors
///
/// `GetNamedSecurityInfoW` 失敗時: `PersistenceError::Io`
pub(super) fn fetch_owner_sid_from_path(
    path: &Path,
) -> Result<(SecurityDescriptorGuard, PSID), PersistenceError> {
    let wide = path_to_wide(path);
    let mut psid_owner: PSID = ptr::null_mut();
    let mut psd: *mut core::ffi::c_void = ptr::null_mut();

    // SAFETY: wide はヌル終端ワイド文字列。psid_owner / psd は出力先ポインタ。
    // GetNamedSecurityInfoW は成功時 psd に LocalAlloc 領域を書く（呼出元が LocalFree）。
    let err = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut psid_owner,
            ptr::null_mut(), // psidGroup: 不要
            ptr::null_mut(), // ppDacl: 不要
            ptr::null_mut(), // ppSacl: 不要
            &mut psd,
        )
    };

    if err != ERROR_SUCCESS {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::from_raw_os_error(err as i32),
        });
    }

    Ok((SecurityDescriptorGuard { ptr: psd }, psid_owner))
}

// ---------------------------------------------------------------------------
// build_owner_only_dacl
// ---------------------------------------------------------------------------

/// 所有者 SID への 1 ACE のみを持つ DACL を `SetEntriesInAclW` で生成する。
///
/// - `grfAccessMode = SET_ACCESS`
/// - `grfInheritance = NO_INHERITANCE`（AceFlags = 0）
/// - `TrusteeForm = TRUSTEE_IS_SID`
///
/// # Errors
///
/// `SetEntriesInAclW` 失敗時: `PersistenceError::InvalidPermission`
pub(super) fn build_owner_only_dacl(
    owner_sid: PSID,
    access_mask: u32,
    path: &Path,
    expected_str: &'static str,
) -> Result<LocalFreeAclGuard, PersistenceError> {
    // TRUSTEE_IS_SID のとき ptstrName に SID ポインタを cast して渡す（Windows SDK 規約）。
    // SAFETY: owner_sid は有効な SID ポインタで TRUSTEE_W の寿命内で生存する。
    #[allow(clippy::cast_ptr_alignment)]
    let trustee = TRUSTEE_W {
        pMultipleTrustee: ptr::null_mut(),
        MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: TRUSTEE_IS_UNKNOWN,
        ptstrName: owner_sid.cast::<u16>(),
    };

    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: access_mask,
        grfAccessMode: SET_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: trustee,
    };

    let mut new_acl: *mut ACL = ptr::null_mut();

    // SAFETY: ea は初期化済みの EXPLICIT_ACCESS_W。new_acl は出力先。
    // SetEntriesInAclW は成功時 new_acl に LocalAlloc 領域を書く（呼出元が LocalFree）。
    let err = unsafe { SetEntriesInAclW(1, &ea, ptr::null_mut(), &mut new_acl) };

    if err != ERROR_SUCCESS || new_acl.is_null() {
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: expected_str,
            actual: format!("SetEntriesInAclW failed: os_error={err}"),
        });
    }

    Ok(LocalFreeAclGuard { ptr: new_acl })
}

// ---------------------------------------------------------------------------
// apply_protected_dacl
// ---------------------------------------------------------------------------

/// `SetNamedSecurityInfoW` で所有者専用 DACL をパスに適用する。
///
/// `PROTECTED_DACL_SECURITY_INFORMATION` を付与して親からの ACE 継承を破棄する。
/// `OWNER_SECURITY_INFORMATION` / `GROUP_SECURITY_INFORMATION` は `SecurityInfo` に含めず、
/// 所有者 SID は touch しない（UAC 環境での `BUILTIN\Administrators` 所有者に対応）。
///
/// # Errors
///
/// `SetNamedSecurityInfoW` 失敗時: `PersistenceError::Io`
pub(super) fn apply_protected_dacl(
    path: &Path,
    acl_guard: &LocalFreeAclGuard,
) -> Result<(), PersistenceError> {
    // SetNamedSecurityInfoW の pObjectName は PWSTR（可変）だが読み取りのみ。
    // path_to_wide で Vec<u16> を確保し as_mut_ptr() を渡す。
    let mut wide = path_to_wide(path);

    // SAFETY: wide はヌル終端ワイド文字列。acl_guard.as_ptr() は有効な ACL ポインタ。
    // SetNamedSecurityInfoW は WIN32_ERROR を直接返す。
    let err = unsafe {
        SetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(), // psidOwner: touch しない
            ptr::null_mut(), // psidGroup: touch しない
            acl_guard.as_ptr(),
            ptr::null_mut(), // pSacl: 不要
        )
    };

    if err != ERROR_SUCCESS {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::from_raw_os_error(err as i32),
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// fetch_dacl_and_owner
// ---------------------------------------------------------------------------

/// 検証用: `GetNamedSecurityInfoW` で DACL・所有者 SID・Control Flags を取得する。
///
/// 返される `PSID` / `*mut ACL` / `u16`（control flags）は
/// `SecurityDescriptorGuard` の寿命内でのみ参照可能。
///
/// # Errors
///
/// Win32 API 失敗時: `PersistenceError::Io`
pub(super) fn fetch_dacl_and_owner(
    path: &Path,
) -> Result<(SecurityDescriptorGuard, PSID, *mut ACL, u16), PersistenceError> {
    let wide = path_to_wide(path);
    let mut psid_owner: PSID = ptr::null_mut();
    let mut pdacl: *mut ACL = ptr::null_mut();
    let mut psd: *mut core::ffi::c_void = ptr::null_mut();

    // SAFETY: wide はヌル終端ワイド文字列。各変数は出力先ポインタ。
    let err = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut psid_owner,
            ptr::null_mut(),
            &mut pdacl,
            ptr::null_mut(),
            &mut psd,
        )
    };

    if err != ERROR_SUCCESS {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::from_raw_os_error(err as i32),
        });
    }

    let guard = SecurityDescriptorGuard { ptr: psd };

    // Control Flags を取得して SE_DACL_PROTECTED bit を確認する
    let mut control: u16 = 0;
    let mut revision: u32 = 0;

    // SAFETY: psd は有効な PSECURITY_DESCRIPTOR。control / revision は出力先。
    let ok = unsafe { GetSecurityDescriptorControl(psd, &mut control, &mut revision) };
    if ok == 0 {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }

    Ok((guard, psid_owner, pdacl, control))
}
