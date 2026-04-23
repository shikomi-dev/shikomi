#![allow(unsafe_code)]

use std::mem;
use std::path::Path;
use std::ptr;

use crate::persistence::error::PersistenceError;
use tempfile::TempDir;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, PSID};
use windows_sys::Win32::Security::Authorization::{
    SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SET_ACCESS,
    SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    AddAccessAllowedAceEx, AllocateAndInitializeSid, FreeSid, GetLengthSid, InitializeAcl, ACL,
    SID_IDENTIFIER_AUTHORITY,
};
use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapFree};

// SECURITY_INFORMATION フラグ: helper.rs と同じリテラル定義（windows-sys 0.52 互換）
const DACL_SECURITY_INFORMATION: u32 = 0x0000_0004;
const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;

use super::helper::{fetch_owner_sid_from_path, path_to_wide};
use super::{
    ensure_dir, ensure_file, verify_dir, verify_file, EXPECTED_DIR_MASK, EXPECTED_FILE_MASK,
};

// -----------------------------------------------------------------------
// テストヘルパー
// -----------------------------------------------------------------------

/// テスト用 `EXPLICIT_ACCESS_W` を生成する。
///
/// # Safety
/// `sid` は返値を使用する `SetEntriesInAclW` 呼び出しの完了まで有効でなければならない。
unsafe fn make_ea(sid: PSID, mask: u32) -> EXPLICIT_ACCESS_W {
    EXPLICIT_ACCESS_W {
        grfAccessPermissions: mask,
        grfAccessMode: SET_ACCESS,
        grfInheritance: 0, // NO_INHERITANCE = 0: ACE フラグなし
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid as *mut u16,
        },
    }
}

/// パスに任意の `EXPLICIT_ACCESS_W` 配列と PROTECTED フラグで DACL を適用するテスト用ヘルパー。
///
/// # Safety
/// `aces` 内の SID ポインタは `SetNamedSecurityInfoW` 完了まで有効でなければならない。
unsafe fn apply_dacl_for_test(path: &Path, aces: &[EXPLICIT_ACCESS_W], protected: bool) {
    let mut new_acl: *mut ACL = ptr::null_mut();
    let ret = SetEntriesInAclW(
        aces.len() as u32,
        aces.as_ptr(),
        ptr::null_mut(), // 既存 ACL なし — ゼロから構築
        &mut new_acl,
    );
    assert_eq!(ret, ERROR_SUCCESS, "SetEntriesInAclW が失敗: error={ret}");

    let mut wide = path_to_wide(path);
    let security_info = DACL_SECURITY_INFORMATION
        | if protected {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            0
        };
    let ret = SetNamedSecurityInfoW(
        wide.as_mut_ptr(),
        SE_FILE_OBJECT,
        security_info,
        ptr::null_mut(), // owner は変更しない
        ptr::null_mut(), // group は変更しない
        new_acl,
        ptr::null_mut(), // sacl は変更しない
    );
    assert_eq!(
        ret, ERROR_SUCCESS,
        "SetNamedSecurityInfoW が失敗: error={ret}"
    );
    // SetEntriesInAclW が LocalAlloc（= HeapAlloc(GetProcessHeap())）で確保した ACL を解放する
    HeapFree(GetProcessHeap(), 0, new_acl as *const core::ffi::c_void);
}

/// BUILTIN\Users (S-1-5-32-545) の SID を `AllocateAndInitializeSid` で確保する。
///
/// 呼び出し側は使用後に `FreeSid` で解放すること。
///
/// # Safety
/// Win32 API を直接呼ぶ。
unsafe fn alloc_builtin_users_sid() -> PSID {
    let authority = SID_IDENTIFIER_AUTHORITY {
        Value: [0, 0, 0, 0, 0, 5], // SECURITY_NT_AUTHORITY
    };
    let mut sid: PSID = ptr::null_mut();
    let ok = AllocateAndInitializeSid(
        &authority, 2,   // SubAuthorityCount
        32,  // SECURITY_BUILTIN_DOMAIN_RID
        545, // DOMAIN_ALIAS_RID_USERS
        0, 0, 0, 0, 0, 0, &mut sid,
    );
    assert!(ok != 0, "AllocateAndInitializeSid が失敗");
    sid
}

// -----------------------------------------------------------------------
// TC-U17: ensure_dir が owner-only DACL を作成する [positive]
// -----------------------------------------------------------------------

/// TC-U17 — `ensure_dir` が成功し、所有者専用 DACL が設定される。
///
/// AC-06 対応（Windows ACL 強制）。
#[test]
fn tc_u17_ensure_dir_creates_owner_only_dacl() {
    let tmp = TempDir::new().unwrap();
    let result = ensure_dir(tmp.path());
    assert!(
        result.is_ok(),
        "ensure_dir が失敗: {:?}",
        result.unwrap_err()
    );
}

// -----------------------------------------------------------------------
// TC-U18: ensure_dir 後の verify_dir が Ok を返す [positive]
// -----------------------------------------------------------------------

/// TC-U18 — `ensure_dir` 直後の `verify_dir` が `Ok(())` を返す。
///
/// AC-06 対応。
#[test]
fn tc_u18_verify_dir_after_ensure_ok() {
    let tmp = TempDir::new().unwrap();
    ensure_dir(tmp.path()).expect("ensure_dir が失敗");
    let result = verify_dir(tmp.path());
    assert!(
        result.is_ok(),
        "verify_dir が失敗: {:?}",
        result.unwrap_err()
    );
}

// -----------------------------------------------------------------------
// TC-U19: ensure_dir なしの verify_dir が不変条件① 違反を返す [negative]
// -----------------------------------------------------------------------

/// TC-U19 — `ensure_dir` を呼ばない（デフォルト継承 DACL）で `verify_dir` が
///          `actual == "inherited DACL (SE_DACL_PROTECTED not set)"` の
///          `InvalidPermission` を返す。
///
/// 不変条件①（`SE_DACL_PROTECTED`）の単独ネガティブテスト。
#[test]
fn tc_u19_verify_dir_without_ensure_inherited_dacl() {
    let tmp = TempDir::new().unwrap();
    // ensure_dir を呼ばない → デフォルト DACL は継承 ACE を含み SE_DACL_PROTECTED が未設定
    let result = verify_dir(tmp.path());
    match result {
        Err(PersistenceError::InvalidPermission { actual, .. }) => {
            assert_eq!(
                actual, "inherited DACL (SE_DACL_PROTECTED not set)",
                "actual ラベルが不変条件① に一致しない: {actual:?}"
            );
        }
        other => panic!("InvalidPermission を期待したが: {:?}", other),
    }
}

// -----------------------------------------------------------------------
// TC-U20: ensure_file が owner-only DACL を作成する [positive]
// -----------------------------------------------------------------------

/// TC-U20 — `ensure_file` が成功し、所有者専用 DACL が設定される。
///
/// AC-06 対応。
#[test]
fn tc_u20_ensure_file_creates_owner_only_dacl() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("vault.db");
    std::fs::write(&file, b"").unwrap();
    let result = ensure_file(&file);
    assert!(
        result.is_ok(),
        "ensure_file が失敗: {:?}",
        result.unwrap_err()
    );
}

// -----------------------------------------------------------------------
// TC-U21: ensure_file 後の verify_file が Ok を返す [positive]
// -----------------------------------------------------------------------

/// TC-U21 — `ensure_file` 直後の `verify_file` が `Ok(())` を返す。
///
/// AC-06 対応。
#[test]
fn tc_u21_verify_file_after_ensure_ok() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("vault.db");
    std::fs::write(&file, b"").unwrap();
    ensure_file(&file).expect("ensure_file が失敗");
    let result = verify_file(&file);
    assert!(
        result.is_ok(),
        "verify_file が失敗: {:?}",
        result.unwrap_err()
    );
}

// -----------------------------------------------------------------------
// TC-U22: AceCount=2 (PROTECTED 維持) → 不変条件② 違反 [negative]
// -----------------------------------------------------------------------

/// TC-U22 — `ensure_file` 後に owner ACE + BUILTIN\Users ACE (PROTECTED 維持)
///          で DACL を上書きすると、`verify_file` が
///          `actual.starts_with("ace_count: 2 (expected 1)")` の
///          `InvalidPermission` を返す。
///
/// 不変条件②（AceCount==1）の単独ネガティブテスト。不変条件① PROTECTED は通過させる。
#[test]
fn tc_u22_verify_file_ace_count_2_invariant_violation() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("vault.db");
    std::fs::write(&file, b"").unwrap();
    ensure_file(&file).expect("ensure_file が失敗");

    // 所有者 SID を取得する（_sd_guard は owner_sid の生存期間中保持）
    let (_sd_guard, owner_sid) =
        fetch_owner_sid_from_path(&file).expect("fetch_owner_sid_from_path が失敗");

    // BUILTIN\Users SID を確保する
    let users_sid = unsafe { alloc_builtin_users_sid() };

    // 2 ACE (owner + BUILTIN\Users) PROTECTED で適用する — 不変条件② を破る
    // ① PROTECTED: pass（維持）/ ② AceCount=2: 違反
    let aces = unsafe {
        [
            make_ea(owner_sid, EXPECTED_FILE_MASK),
            make_ea(users_sid, EXPECTED_FILE_MASK),
        ]
    };
    unsafe { apply_dacl_for_test(&file, &aces, true) };

    // SetEntriesInAclW / SetNamedSecurityInfoW 完了後に SID を解放する
    unsafe { FreeSid(users_sid) };

    let result = verify_file(&file);
    match result {
        Err(PersistenceError::InvalidPermission { actual, .. }) => {
            assert!(
                actual.starts_with("ace_count: 2 (expected 1)"),
                "actual ラベルが不変条件② に一致しない: {actual:?}"
            );
        }
        other => panic!("InvalidPermission を期待したが: {:?}", other),
    }
}

// -----------------------------------------------------------------------
// TC-U23: UAC 昇格ランナー（BUILTIN\Administrators 所有者）でも Ok [positive]
// -----------------------------------------------------------------------

/// TC-U23 — UAC 昇格環境（`BUILTIN\Administrators` が所有者になる場合）でも
///          `ensure_dir` → `verify_dir` が `Ok(())` を返す。
///
/// `ensure_dir` は `OWNER_SECURITY_INFORMATION` から所有者 SID を取得するため
/// 昇格有無によらず正しい SID で ACE を構築する（AC-06）。
#[test]
fn tc_u23_ensure_verify_dir_under_uac_elevation() {
    // UAC 昇格環境ではディレクトリの所有者が BUILTIN\Administrators になる場合がある。
    // ensure_dir が所有者 SID を正しく取得し、verify_dir が Ok を返すことを確認する。
    let tmp = TempDir::new().unwrap();
    ensure_dir(tmp.path()).expect("ensure_dir が失敗");
    let result = verify_dir(tmp.path());
    assert!(
        result.is_ok(),
        "UAC 昇格環境での verify_dir が失敗: {:?}",
        result.unwrap_err()
    );
}

// -----------------------------------------------------------------------
// TC-U24: WRITE_DAC 過剰ビット（PROTECTED, ACE=1, owner SID）→ 不変条件④ 違反 [negative]
// -----------------------------------------------------------------------

/// TC-U24 — `ensure_file` 後に owner SID + `EXPECTED_FILE_MASK | WRITE_DAC` で
///          ACE=1 PROTECTED DACL を上書きすると、`verify_file` が
///          `actual.starts_with("ace_mask=")` の `InvalidPermission` を返す。
///
/// 不変条件④（AccessMask 完全一致）の単独ネガティブテスト。
/// ① PROTECTED: pass / ② AceCount=1: pass / ③ EqualSid owner: pass / ④ Mask: 違反。
#[test]
fn tc_u24_verify_file_extra_write_dac_bit_invariant_violation() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("vault.db");
    std::fs::write(&file, b"").unwrap();
    ensure_file(&file).expect("ensure_file が失敗");

    // 所有者 SID を取得する（_sd_guard は owner_sid の生存期間中保持）
    let (_sd_guard, owner_sid) =
        fetch_owner_sid_from_path(&file).expect("fetch_owner_sid_from_path が失敗");

    // WRITE_DAC (0x0004_0000) を付加した過剰 AccessMask — 不変条件④ を破る
    // 結果: 0x0012_019F | 0x0004_0000 = 0x0016_019F
    let excess_mask = EXPECTED_FILE_MASK | 0x0004_0000u32;

    // 1 ACE (owner, excess_mask) PROTECTED で適用する
    // ① PROTECTED: pass / ② AceCount=1: pass / ③ EqualSid(owner, owner): pass
    // ④ Mask(0x0016_019F) ≠ EXPECTED_FILE_MASK(0x0012_019F): 違反
    let aces = unsafe { [make_ea(owner_sid, excess_mask)] };
    unsafe { apply_dacl_for_test(&file, &aces, true) };

    let result = verify_file(&file);
    match result {
        Err(PersistenceError::InvalidPermission { actual, .. }) => {
            assert!(
                actual.starts_with("ace_mask="),
                "actual ラベルが不変条件④ に一致しない: {actual:?}"
            );
        }
        other => panic!("InvalidPermission を期待したが: {:?}", other),
    }
}

// -----------------------------------------------------------------------
// TC-U25: BUILTIN\Users をトラスティに設定 → 不変条件③ EqualSid 単独違反 [negative]
// -----------------------------------------------------------------------

/// TC-U25 — `ensure_file` 後に BUILTIN\Users (S-1-5-32-545) を ACE トラスティとし
///          ACE=1 PROTECTED `EXPECTED_FILE_MASK` の DACL を上書きすると、
///          `verify_file` が `actual.starts_with("trustee_mismatch:")` の
///          `InvalidPermission` を返す。
///
/// 不変条件③（EqualSid）の単独ネガティブテスト（本 PR の核心テストケース）。
/// ① PROTECTED: pass / ② AceCount=1: pass
/// ③ EqualSid(BUILTIN\Users, owner): 違反（④ には到達しない）。
#[test]
fn tc_u25_verify_file_trustee_mismatch_invariant_violation() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("vault.db");
    std::fs::write(&file, b"").unwrap();
    ensure_file(&file).expect("ensure_file が失敗");

    // BUILTIN\Users SID を確保する（所有者 SID と異なるため EqualSid が偽になる）
    let users_sid = unsafe { alloc_builtin_users_sid() };

    // 1 ACE (BUILTIN\Users, EXPECTED_FILE_MASK) PROTECTED で適用する
    // ① PROTECTED: pass / ② AceCount=1: pass
    // ③ EqualSid(ace=Users, owner=<current user>): 偽 → InvalidPermission
    let aces = unsafe { [make_ea(users_sid, EXPECTED_FILE_MASK)] };
    unsafe { apply_dacl_for_test(&file, &aces, true) };

    // SetNamedSecurityInfoW 完了後に SID を解放する
    unsafe { FreeSid(users_sid) };

    let result = verify_file(&file);
    match result {
        Err(PersistenceError::InvalidPermission { actual, .. }) => {
            assert!(
                actual.starts_with("trustee_mismatch:"),
                "actual ラベルが不変条件③ に一致しない: {actual:?}"
            );
        }
        other => panic!("InvalidPermission を期待したが: {:?}", other),
    }
}

// -----------------------------------------------------------------------
// テストヘルパー（TC-U26 用）
// -----------------------------------------------------------------------

/// ACE に指定した `AceFlags` を設定した DACL を `AddAccessAllowedAceEx` 経由で適用する。
///
/// `apply_dacl_for_test` は `grfInheritance = NO_INHERITANCE` のみ扱うため、
/// `AceFlags != 0`（継承フラグ付き ACE）のテストにはこちらを使う。
///
/// # Safety
/// `sid` は `SetNamedSecurityInfoW` 完了まで有効でなければならない。
unsafe fn apply_dacl_with_ace_flags_for_test(
    path: &Path,
    sid: PSID,
    mask: u32,
    ace_flags: u8,
    protected: bool,
) {
    // ACL サイズ計算:
    //   ACL ヘッダ(8) + ACE ヘッダ(4) + AccessMask(4) + SID(GetLengthSid)
    //   → 4 バイト境界に揃える + 余裕 8 バイト
    let sid_len = GetLengthSid(sid) as usize;
    let ace_size = (8 + sid_len + 3) & !3; // 4-byte aligned
    let acl_size = mem::size_of::<ACL>() + ace_size + 8;

    // ACL 構造体は 4 バイトアライメントが必要なため u32 ベクタを使用する
    // （u8 ベクタからのキャストは clippy::cast_ptr_alignment を引き起こす）
    let acl_size_words = (acl_size + 3) / 4;
    let mut acl_buf = vec![0u32; acl_size_words];
    let acl_ptr = acl_buf.as_mut_ptr() as *mut ACL;

    // ACL を初期化する
    let ok = InitializeAcl(acl_ptr, acl_size as u32, 2 /* ACL_REVISION */);
    assert!(ok != 0, "InitializeAcl が失敗");

    // 指定した AceFlags で ACCESS_ALLOWED ACE を追加する
    let ok = AddAccessAllowedAceEx(
        acl_ptr,
        2, /* ACL_REVISION */
        ace_flags as u32,
        mask,
        sid,
    );
    assert!(ok != 0, "AddAccessAllowedAceEx が失敗");

    let mut wide = path_to_wide(path);
    let security_info = DACL_SECURITY_INFORMATION
        | if protected {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            0
        };
    let ret = SetNamedSecurityInfoW(
        wide.as_mut_ptr(),
        SE_FILE_OBJECT,
        security_info,
        ptr::null_mut(),
        ptr::null_mut(),
        acl_ptr,
        ptr::null_mut(),
    );
    assert_eq!(ret, ERROR_SUCCESS, "SetNamedSecurityInfoW が失敗: {ret}");
    // acl_buf は Vec 管理。SetNamedSecurityInfoW が内部コピーするため LocalFree 不要
}

// -----------------------------------------------------------------------
// TC-U26: AceFlags != 0（継承フラグ付き ACE）→ AceFlags==0 違反 [negative]
// -----------------------------------------------------------------------

/// TC-U26 — `ensure_dir` 後に owner SID + `EXPECTED_DIR_MASK` で ACE=1 PROTECTED DACL を設定するが
///          `AceFlags = CONTAINER_INHERIT_ACE (0x02)` を付加すると、`verify_dir` が
///          `actual.starts_with("ace_count: 1 (expected 1); ace[0]: ace_flags=")` の
///          `InvalidPermission` を返す。
///
/// AceFlags==0 検証の単独ネガティブテスト。
/// ① PROTECTED: pass / ② AceCount=1: pass / ② AceType=ACCESS_ALLOWED: pass
/// ② AceFlags=0x02 ≠ 0x00: 違反（③ EqualSid / ④ AccessMask には到達しない）。
///
/// ## 注意: ファイルではなくディレクトリで検証する理由
/// Windows は `SetNamedSecurityInfoW` でファイルACEに設定した `CONTAINER_INHERIT_ACE` を
/// 自動的に除去する（ファイルはコンテナでないため）。ディレクトリに対しては保持される。
#[test]
fn tc_u26_verify_dir_ace_flags_nonzero_invariant_violation() {
    let tmp = TempDir::new().unwrap();
    // ディレクトリに ensure_dir を適用する（AceFlags=0 の PROTECTED DACL が設定される）
    ensure_dir(tmp.path()).expect("ensure_dir が失敗");

    // 所有者 SID を取得する（_sd_guard は owner_sid の生存期間中保持）
    let (_sd_guard, owner_sid) =
        fetch_owner_sid_from_path(tmp.path()).expect("fetch_owner_sid_from_path が失敗");

    // CONTAINER_INHERIT_ACE (0x02) を AceFlags に設定した ACE=1 PROTECTED DACL を適用する
    // ディレクトリでは CONTAINER_INHERIT_ACE は保持される（ファイルとは異なり除去されない）
    // owner SID + EXPECTED_DIR_MASK で ①③④ の他の条件はすべて通過させる
    // ② AceFlags = 0x02 ≠ 0x00 でのみ違反させる
    unsafe {
        apply_dacl_with_ace_flags_for_test(
            tmp.path(),
            owner_sid,
            EXPECTED_DIR_MASK,
            0x02, // CONTAINER_INHERIT_ACE
            true, // protected
        );
    }

    let result = verify_dir(tmp.path());
    match result {
        Err(PersistenceError::InvalidPermission { actual, .. }) => {
            assert!(
                actual.starts_with("ace_count: 1 (expected 1); ace[0]: ace_flags="),
                "actual ラベルが AceFlags 違反に一致しない: {actual:?}"
            );
        }
        other => panic!("InvalidPermission を期待したが: {:?}", other),
    }
}
