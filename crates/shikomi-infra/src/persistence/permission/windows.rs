//! Windows 固有のパーミッション実装（owner-only DACL 強制 / 検証）。
//!
//! Microsoft 公式 `windows-sys` crate が Win32 Security API を `unsafe fn` で公開しているため、
//! owner-only DACL の `SetNamedSecurityInfoW` / `GetNamedSecurityInfoW` / `SetEntriesInAclW` 等を
//! 呼ぶ本ファイルに限り `unsafe_code` lint を許容する。他モジュールは `forbid` を保持。
//! 参照: https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setnamedsecurityinfow
#![allow(unsafe_code)]

use std::mem;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HLOCAL, PSID};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
    ConvertSidToStringSidW, EqualSid, GetAce, GetAclInformation, GetSecurityDescriptorControl,
    SE_DACL_PROTECTED,
};
use windows_sys::Win32::Security::Authorization::{
    DACL_SECURITY_INFORMATION, EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, NO_INHERITANCE,
    NO_MULTIPLE_TRUSTEE, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID,
    TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::System::Memory::LocalFree;

use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// 定数
// ---------------------------------------------------------------------------

/// `ACCESS_ALLOWED_ACE_TYPE` — AceHeader.AceType の期待値。
///
/// Windows SDK: `#define ACCESS_ALLOWED_ACE_TYPE 0x0`
const ACE_TYPE_ACCESS_ALLOWED: u8 = 0x00;

/// ファイルの期待 AccessMask（`FILE_GENERIC_READ | FILE_GENERIC_WRITE`）。
///
/// - FILE_GENERIC_READ  = SYNCHRONIZE(0x100000) | READ_CONTROL(0x20000)
///                      | FILE_READ_DATA(0x1) | FILE_READ_ATTRIBUTES(0x80) | FILE_READ_EA(0x8)
///                      = 0x0012_0089
/// - FILE_GENERIC_WRITE = SYNCHRONIZE(0x100000) | READ_CONTROL(0x20000)
///                      | FILE_WRITE_DATA(0x2) | FILE_WRITE_ATTRIBUTES(0x100)
///                      | FILE_WRITE_EA(0x10) | FILE_APPEND_DATA(0x4)
///                      = 0x0012_0116
const EXPECTED_FILE_MASK: u32 = 0x0012_0089 | 0x0012_0116; // = 0x0012_019F

/// ディレクトリの期待 AccessMask（`FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_TRAVERSE`）。
///
/// FILE_TRAVERSE = FILE_EXECUTE = 0x20
const EXPECTED_DIR_MASK: u32 = EXPECTED_FILE_MASK | 0x0000_0020; // = 0x0012_01BF

/// `InvalidPermission.expected` フィールドの値 — ファイル用（`&'static str`）。
const EXPECTED_FILE_STR: &str = "owner-only DACL (FILE_GENERIC_READ|FILE_GENERIC_WRITE)";

/// `InvalidPermission.expected` フィールドの値 — ディレクトリ用（`&'static str`）。
const EXPECTED_DIR_STR: &str =
    "owner-only DACL (FILE_GENERIC_READ|FILE_GENERIC_WRITE|FILE_TRAVERSE)";

// ---------------------------------------------------------------------------
// RAII ガード
// ---------------------------------------------------------------------------

/// `GetNamedSecurityInfoW` が `LocalAlloc` で返した `PSECURITY_DESCRIPTOR` の RAII ラッパ。
///
/// `Drop` で `LocalFree(ptr)` を呼ぶ。早期 return / panic でも確実に解放する
/// （Microsoft Learn 明記のメモリ解放責務を型で強制）。
struct SecurityDescriptorGuard {
    ptr: *mut core::ffi::c_void,
}

impl Drop for SecurityDescriptorGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: ptr は GetNamedSecurityInfoW が LocalAlloc で確保した領域。
            // LocalFree による解放が Microsoft Learn に規定されている。
            // Drop 内では panic しない（二重 panic 防止）。
            let result = unsafe { LocalFree(self.ptr as HLOCAL) };
            if result != 0 {
                tracing::warn!("LocalFree(SecurityDescriptorGuard) failed");
            }
        }
    }
}

/// `SetEntriesInAclW` が `LocalAlloc` で確保した新 ACL の RAII ラッパ。
///
/// `Drop` で `LocalFree(ptr)` を呼ぶ。
struct LocalFreeAclGuard {
    ptr: *mut ACL,
}

impl LocalFreeAclGuard {
    /// 内部 ACL ポインタを返す。ガードより長生きさせてはならない。
    fn as_ptr(&self) -> *mut ACL {
        self.ptr
    }
}

impl Drop for LocalFreeAclGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: ptr は SetEntriesInAclW が LocalAlloc で確保した領域。
            let result = unsafe { LocalFree(self.ptr as HLOCAL) };
            if result != 0 {
                tracing::warn!("LocalFree(LocalFreeAclGuard) failed");
            }
        }
    }
}

/// `ConvertSidToStringSidW` が `LocalAlloc` で確保した SID 文字列の RAII ラッパ。
///
/// `Drop` で `LocalFree(ptr)` を呼ぶ。診断文字列生成に使う。
struct SidStringGuard {
    ptr: *mut u16,
}

impl SidStringGuard {
    /// SID ワイド文字列を Rust `String` に変換する（UTF-16 lossy）。
    fn to_string_lossy(&self) -> String {
        if self.ptr.is_null() {
            return String::from("<null>");
        }
        // SAFETY: ptr は ConvertSidToStringSidW が返したヌル終端ワイド文字列。
        unsafe {
            let mut len = 0usize;
            while *self.ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(self.ptr, len))
        }
    }
}

impl Drop for SidStringGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: ptr は ConvertSidToStringSidW が LocalAlloc で確保した領域。
            let result = unsafe { LocalFree(self.ptr as HLOCAL) };
            if result != 0 {
                tracing::warn!("LocalFree(SidStringGuard) failed");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ヘルパ：SID → 文字列
// ---------------------------------------------------------------------------

/// SID を `ConvertSidToStringSidW` で文字列化する。失敗時は `"<unknown>"` を返す。
fn sid_to_string(sid: PSID) -> String {
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
fn path_to_wide(path: &Path) -> Vec<u16> {
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
fn fetch_owner_sid_from_path(
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
fn build_owner_only_dacl(
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
fn apply_protected_dacl(path: &Path, acl_guard: &LocalFreeAclGuard) -> Result<(), PersistenceError> {
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
fn fetch_dacl_and_owner(
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

// ---------------------------------------------------------------------------
// verify_dacl_owner_only — 4 不変条件の検証
// ---------------------------------------------------------------------------

/// DACL の 4 不変条件をすべて検証する。
///
/// 検証順序（Fail Fast):
/// 1. `SE_DACL_PROTECTED` が立つ（継承 ACE なし）
/// 2. `AceCount == 1` かつ `ACCESS_ALLOWED_ACE_TYPE`
/// 3. ACE トラスティ SID が所有者 SID と `EqualSid` で一致
/// 4. `AccessMask` が `expected_mask` と完全一致
///
/// # Errors
///
/// - 不変条件違反: `PersistenceError::InvalidPermission`
/// - Win32 API 失敗: `PersistenceError::Io`
fn verify_dacl_owner_only(
    path: &Path,
    dacl: *mut ACL,
    control: u16,
    owner_sid: PSID,
    expected_mask: u32,
    expected_str: &'static str,
) -> Result<(), PersistenceError> {
    // ① SE_DACL_PROTECTED: 継承 ACE が破棄済みか確認する
    if (control & SE_DACL_PROTECTED) == 0 {
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: expected_str,
            actual: String::from("inherited DACL (SE_DACL_PROTECTED not set)"),
        });
    }

    // ACL サイズ情報を取得する（AceCount の取得）
    let mut acl_info: ACL_SIZE_INFORMATION = unsafe { mem::zeroed() };
    // SAFETY: dacl は有効な ACL ポインタ。acl_info は zeroed 初期化済みの出力先。
    let ok = unsafe {
        GetAclInformation(
            dacl,
            std::ptr::addr_of_mut!(acl_info).cast::<core::ffi::c_void>(),
            mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }

    // ② AceCount == 1
    let ace_count = acl_info.AceCount;
    if ace_count != 1 {
        let actual = enumerate_aces(dacl, ace_count);
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: expected_str,
            actual,
        });
    }

    // 唯一の ACE を取得する
    let mut pace: *mut core::ffi::c_void = ptr::null_mut();
    // SAFETY: dacl は有効な ACL。ace_count == 1 なのでインデックス 0 は存在する。
    let ok = unsafe { GetAce(dacl, 0, &mut pace) };
    if ok == 0 || pace.is_null() {
        return Err(PersistenceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }

    // ACE_HEADER を読み取り AceType を確認する（ACCESS_ALLOWED_ACE_TYPE のみ受理）
    // SAFETY: pace は GetAce が返した有効な ACE ポインタ（最低 ACE_HEADER サイズ）。
    let ace_hdr = unsafe { &*(pace.cast::<ACE_HEADER>()) };
    if ace_hdr.AceType != ACE_TYPE_ACCESS_ALLOWED {
        let actual = format!(
            "ace_count: 1 (expected 1); ace[0]: ace_type={} (expected ACCESS_ALLOWED=0)",
            ace_hdr.AceType
        );
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: expected_str,
            actual,
        });
    }

    // ACCESS_ALLOWED_ACE としてキャストして Mask / SID を読む
    // SAFETY: AceType == ACCESS_ALLOWED_ACE_TYPE を確認済み。
    // ACCESS_ALLOWED_ACE の先頭フィールドは ACE_HEADER であり、キャストは安全。
    let ace = unsafe { &*(pace.cast::<ACCESS_ALLOWED_ACE>()) };

    // ③ EqualSid: ACE トラスティ SID が所有者 SID と一致するか確認する
    // SID は ACCESS_ALLOWED_ACE::SidStart フィールドから始まる可変長データ。
    let ace_sid: PSID = std::ptr::addr_of!(ace.SidStart).cast::<core::ffi::c_void>() as PSID;
    // SAFETY: ace_sid は ACE 内の SID ポインタ。owner_sid は SD 内の SID ポインタ。
    // ともに SecurityDescriptorGuard / LocalFreeAclGuard の寿命内で有効。
    let equal = unsafe { EqualSid(ace_sid, owner_sid) };
    if equal == 0 {
        let owner_str = sid_to_string(owner_sid);
        let ace_str = sid_to_string(ace_sid);
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: expected_str,
            actual: format!("trustee_mismatch: owner={owner_str}, ace={ace_str}"),
        });
    }

    // ④ AccessMask: 完全一致（ビット包含ではなく完全一致で過剰ビットを拒否する）
    // WRITE_DAC 等の過剰ビットがあると攻撃者が DACL を後から書き換えられるため。
    if ace.Mask != expected_mask {
        return Err(PersistenceError::InvalidPermission {
            path: path.to_path_buf(),
            expected: expected_str,
            actual: format!(
                "ace_mask=0x{:08X}, expected=0x{:08X}",
                ace.Mask, expected_mask
            ),
        });
    }

    Ok(())
}

/// 不変条件② 違反時の診断: ACL 内の全 ACE を文字列に列挙する。
fn enumerate_aces(dacl: *mut ACL, count: u32) -> String {
    let mut parts = vec![format!("ace_count: {count} (expected 1)")];
    for i in 0..count {
        let mut pace: *mut core::ffi::c_void = ptr::null_mut();
        // SAFETY: dacl は有効な ACL ポインタ。
        let ok = unsafe { GetAce(dacl, i, &mut pace) };
        if ok == 0 || pace.is_null() {
            parts.push(format!("ace[{i}]: <GetAce failed>"));
            continue;
        }
        // SAFETY: pace は有効な ACE ポインタ（最低 ACE_HEADER サイズ）。
        let hdr = unsafe { &*(pace.cast::<ACE_HEADER>()) };
        if hdr.AceType == ACE_TYPE_ACCESS_ALLOWED {
            // SAFETY: AceType == ACCESS_ALLOWED_ACE_TYPE を確認済み。
            let ace = unsafe { &*(pace.cast::<ACCESS_ALLOWED_ACE>()) };
            let sid: PSID = std::ptr::addr_of!(ace.SidStart).cast::<core::ffi::c_void>() as PSID;
            let sid_str = sid_to_string(sid);
            parts.push(format!(
                "ace[{i}]: trustee_sid={sid_str}, ace_type={}, access_mask=0x{:08X}",
                hdr.AceType, ace.Mask
            ));
        } else {
            parts.push(format!(
                "ace[{i}]: trustee_sid=<non-allow>, ace_type={}, access_mask=<unknown>",
                hdr.AceType
            ));
        }
    }
    parts.join("; ")
}

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
pub(super) fn ensure_dir(path: &Path) -> Result<(), PersistenceError> {
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
pub(super) fn ensure_file(path: &Path) -> Result<(), PersistenceError> {
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
pub(super) fn verify_dir(path: &Path) -> Result<(), PersistenceError> {
    let (_sd_guard, owner_sid, dacl, control) = fetch_dacl_and_owner(path)?;
    verify_dacl_owner_only(path, dacl, control, owner_sid, EXPECTED_DIR_MASK, EXPECTED_DIR_STR)
}

/// ファイルの DACL が 4 不変条件をすべて満たすか検証する。
///
/// # Errors
///
/// - Win32 API 失敗: `PersistenceError::Io`
/// - 不変条件違反: `PersistenceError::InvalidPermission`
pub(super) fn verify_file(path: &Path) -> Result<(), PersistenceError> {
    let (_sd_guard, owner_sid, dacl, control) = fetch_dacl_and_owner(path)?;
    verify_dacl_owner_only(path, dacl, control, owner_sid, EXPECTED_FILE_MASK, EXPECTED_FILE_STR)
}
