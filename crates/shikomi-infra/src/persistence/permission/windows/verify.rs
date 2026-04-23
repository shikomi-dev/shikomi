#![allow(unsafe_code)]

use std::mem;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::PSID;
use windows_sys::Win32::Security::{
    AclSizeInformation, EqualSid, GetAce, GetAclInformation, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
    ACL_SIZE_INFORMATION, SE_DACL_PROTECTED,
};

use super::helper::sid_to_string;
use super::ACE_TYPE_ACCESS_ALLOWED;
use crate::persistence::error::PersistenceError;

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
pub(super) fn verify_dacl_owner_only(
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

    // AceFlags == 0: 継承フラグが設定されていないことを確認する
    // CONTAINER_INHERIT_ACE / OBJECT_INHERIT_ACE 等の継承フラグは拒否する。
    if ace_hdr.AceFlags != 0 {
        let actual = format!(
            "ace_count: 1 (expected 1); ace[0]: ace_flags=0x{:02X} (expected 0x00, no inheritance flags)",
            ace_hdr.AceFlags
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
