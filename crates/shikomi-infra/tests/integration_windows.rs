//! vault-persistence 結合テスト — TC-I24〜TC-I27 (Windows only)
//!
//! テスト設計書: docs/features/vault-persistence/test-design/integration.md
//! 対応 Issue: #14 (Windows ACL 強制)

#![cfg(windows)]
#![allow(unsafe_code)]

use std::path::Path;
use std::sync::Mutex;

use shikomi_core::{
    Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
    VaultVersion,
};
use shikomi_infra::persistence::{PersistenceError, SqliteVaultRepository, VaultRepository};
use tempfile::TempDir;
use time::OffsetDateTime;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// テストヘルパー
// ---------------------------------------------------------------------------

/// 環境変数 `SHIKOMI_VAULT_DIR` へのアクセスをプロセス全体で直列化するミューテックス。
static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// tempdir を使った `SqliteVaultRepository` を構築する。
fn make_repo(dir: &Path) -> SqliteVaultRepository {
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir);
    let repo = SqliteVaultRepository::new().unwrap();
    std::env::remove_var("SHIKOMI_VAULT_DIR");
    repo
}

/// 平文モードの `VaultHeader` を作る。
fn plaintext_header() -> VaultHeader {
    VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap()
}

/// 平文 `Record` を 1 件作る。
fn make_record(label: &str, value: &str) -> Record {
    let now = OffsetDateTime::now_utc();
    Record::new(
        RecordId::new(Uuid::now_v7()).unwrap(),
        RecordKind::Secret,
        RecordLabel::try_new(label.to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string(value.to_string())),
        now,
    )
}

/// N 件のレコードを持つ平文 vault を作る。
fn make_plaintext_vault(n: usize) -> Vault {
    let mut vault = Vault::new(plaintext_header());
    for i in 0..n {
        vault
            .add_record(make_record(&format!("label-{i}"), &format!("value-{i}")))
            .unwrap();
    }
    vault
}

// ---------------------------------------------------------------------------
// TC-I24: save 後の load が成功する（vault.db の 4 不変条件がすべて満たされる）
// ---------------------------------------------------------------------------

/// TC-I24 — `save` 後の `load` が成功する（vault.db の 4 不変条件がすべて満たされる）。
///
/// `load_inner` が `verify_file(vault.db)` を呼ぶため、
/// DACL の 4 不変条件（① SE_DACL_PROTECTED / ② AceCount=1 / ③ EqualSid / ④ AccessMask）
/// がすべて満たされていれば `Ok` を返す。AC-06 対応。
#[test]
fn tc_i24_save_vault_db_dacl_all_4invariants_ok_on_load() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    // save は ensure_file(vault.db) で 4 不変条件を満たす DACL を設定する
    repo.save(&vault).expect("save が失敗");

    // load は verify_file(vault.db) で DACL の 4 不変条件を確認する
    let result = repo.load();
    assert!(
        result.is_ok(),
        "load が失敗（vault.db DACL 不変条件違反）: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// TC-I25: save 後に vault.db DACL を破壊すると load が不変条件② 違反を返す
// ---------------------------------------------------------------------------

/// TC-I25 — `save` 後に vault.db DACL を破壊（BUILTIN\Users ACE を追加 / PROTECTED 維持）
///          すると、`load` が `InvalidPermission { actual.starts_with("ace_count: 2") }` を返す。
///
/// `load_inner` → `verify_file(vault.db)` → 不変条件② 違反検知。AC-06 対応。
#[test]
fn tc_i25_load_detects_broken_file_dacl_ace_count_2() {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, PSID};
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
        NO_MULTIPLE_TRUSTEE, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN,
        TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        AllocateAndInitializeSid, FreeSid, ACL, SID_IDENTIFIER_AUTHORITY,
    };
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapFree};
    // SECURITY_INFORMATION フラグのリテラル値（windows-sys 0.52 互換）
    const DACL_SEC_INFO: u32 = 0x0000_0004; // DACL_SECURITY_INFORMATION
    const PROTECTED_DACL_SEC_INFO: u32 = 0x8000_0000; // PROTECTED_DACL_SECURITY_INFORMATION

    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    repo.save(&make_plaintext_vault(1)).expect("save が失敗");

    // vault.db のパスを wide 文字列に変換する
    let vault_db = dir.path().join("vault.db");
    let wide: Vec<u16> = vault_db.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        // 1. 現在の DACL を取得する（ensure_file が設定した owner ACE 1 個）
        let mut p_dacl: *mut ACL = std::ptr::null_mut();
        let mut p_sd: *mut core::ffi::c_void = std::ptr::null_mut();
        let ret = GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SEC_INFO,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut p_dacl,
            std::ptr::null_mut(),
            &mut p_sd,
        );
        assert_eq!(ret, ERROR_SUCCESS, "GetNamedSecurityInfoW が失敗: {ret}");

        // 2. BUILTIN\Users SID を確保する（S-1-5-32-545）
        let authority = SID_IDENTIFIER_AUTHORITY {
            Value: [0, 0, 0, 0, 0, 5], // SECURITY_NT_AUTHORITY
        };
        let mut users_sid: PSID = std::ptr::null_mut();
        let ok = AllocateAndInitializeSid(&authority, 2, 32, 545, 0, 0, 0, 0, 0, 0, &mut users_sid);
        assert!(ok != 0, "AllocateAndInitializeSid が失敗");

        // 3. 既存 DACL に BUILTIN\Users ACE をマージした新 DACL を作成する
        //    → AceCount=2（owner ACE + BUILTIN\Users ACE）
        let ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: 0x0012_019F, // EXPECTED_FILE_MASK
            grfAccessMode: SET_ACCESS,
            grfInheritance: 0, // NO_INHERITANCE = 0
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: users_sid as *mut u16,
            },
        };
        let mut new_acl: *mut ACL = std::ptr::null_mut();
        let ret = SetEntriesInAclW(1, &ea, p_dacl, &mut new_acl);
        assert_eq!(ret, ERROR_SUCCESS, "SetEntriesInAclW が失敗: {ret}");

        // 4. PROTECTED フラグを維持して新 DACL を適用する — 不変条件② を破る
        //    SetNamedSecurityInfoW は pObjectName を PWSTR (*mut u16) で受け取る
        let mut wide_mut: Vec<u16> = vault_db.as_os_str().encode_wide().chain(Some(0)).collect();
        let ret = SetNamedSecurityInfoW(
            wide_mut.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SEC_INFO | PROTECTED_DACL_SEC_INFO,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            new_acl,
            std::ptr::null_mut(),
        );
        assert_eq!(ret, ERROR_SUCCESS, "SetNamedSecurityInfoW が失敗: {ret}");

        // LocalAlloc（= HeapAlloc(GetProcessHeap())）確保領域を解放する
        HeapFree(GetProcessHeap(), 0, new_acl as *const core::ffi::c_void);
        FreeSid(users_sid);
        HeapFree(GetProcessHeap(), 0, p_sd as *const core::ffi::c_void);
    }

    // load は verify_file を呼ぶ → AceCount=2 で不変条件② 違反
    let result = repo.load();
    match result {
        Err(PersistenceError::InvalidPermission { actual, .. }) => {
            assert!(
                actual.starts_with("ace_count: 2 (expected 1)"),
                "actual ラベルが ace_count 不変条件② に一致しない: {actual:?}"
            );
        }
        other => panic!("InvalidPermission を期待したが: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// TC-I26: save 後の vault ディレクトリ DACL に SE_DACL_PROTECTED が設定されている
// ---------------------------------------------------------------------------

/// TC-I26 — `save` 後の vault ディレクトリ DACL に `SE_DACL_PROTECTED` ビットが設定されている。
///
/// `save_inner` → `ensure_dir` が `PROTECTED_DACL_SECURITY_INFORMATION` で DACL を設定する
/// ことを Win32 API で直接確認する。AC-06 対応。
#[test]
fn tc_i26_save_dir_dacl_has_protected_bit() {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{GetSecurityDescriptorControl, ACL, SE_DACL_PROTECTED};
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapFree};
    const DACL_SEC_INFO: u32 = 0x0000_0004; // DACL_SECURITY_INFORMATION

    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    repo.save(&make_plaintext_vault(1)).expect("save が失敗");

    // ディレクトリパスを wide 文字列に変換する
    let wide: Vec<u16> = dir
        .path()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();

    let is_protected = unsafe {
        // 1. ディレクトリのセキュリティ記述子を取得する
        let mut p_dacl: *mut ACL = std::ptr::null_mut();
        let mut p_sd: *mut core::ffi::c_void = std::ptr::null_mut();
        let ret = GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SEC_INFO,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut p_dacl,
            std::ptr::null_mut(),
            &mut p_sd,
        );
        assert_eq!(ret, ERROR_SUCCESS, "GetNamedSecurityInfoW が失敗: {ret}");

        // 2. セキュリティ記述子コントロールから SE_DACL_PROTECTED を確認する
        let mut control: u16 = 0;
        let mut revision: u32 = 0;
        let ok = GetSecurityDescriptorControl(p_sd, &mut control, &mut revision);
        let protected = ok != 0 && (control & SE_DACL_PROTECTED) != 0;

        // LocalAlloc（= HeapAlloc(GetProcessHeap())）確保領域を解放する
        HeapFree(GetProcessHeap(), 0, p_sd as *const core::ffi::c_void);
        protected
    };

    assert!(
        is_protected,
        "vault dir の DACL に SE_DACL_PROTECTED ビットが設定されていない（AC-06 違反）"
    );
}

// ---------------------------------------------------------------------------
// TC-I27: save 後に vault dir の SE_DACL_PROTECTED を除去すると load が不変条件① 違反を返す
// ---------------------------------------------------------------------------

/// TC-I27 — `save` 後に vault dir の `SE_DACL_PROTECTED` を除去すると、
///          `load` が `InvalidPermission { actual == "inherited DACL (SE_DACL_PROTECTED not set)" }`
///          を返す。
///
/// `load_inner` → `verify_dir` → 不変条件① 違反検知。AC-06 対応。
#[test]
fn tc_i27_load_detects_dir_dacl_not_protected() {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::ACL;
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapFree};
    const DACL_SEC_INFO: u32 = 0x0000_0004; // DACL_SECURITY_INFORMATION

    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    repo.save(&make_plaintext_vault(1)).expect("save が失敗");

    // ディレクトリパスを wide 文字列に変換する
    let mut wide: Vec<u16> = dir
        .path()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();

    unsafe {
        // 1. ディレクトリの現在 DACL を取得する
        let mut p_dacl: *mut ACL = std::ptr::null_mut();
        let mut p_sd: *mut core::ffi::c_void = std::ptr::null_mut();
        let ret = GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SEC_INFO,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut p_dacl,
            std::ptr::null_mut(),
            &mut p_sd,
        );
        assert_eq!(ret, ERROR_SUCCESS, "GetNamedSecurityInfoW が失敗: {ret}");

        // 2. UNPROTECTED_DACL_SECURITY_INFORMATION (0x2000_0000) で PROTECTED ビットを除去する
        //    → SE_DACL_PROTECTED が消え、不変条件① が破れる
        //    SetNamedSecurityInfoW は pObjectName を PWSTR (*mut u16) で受け取る
        let ret = SetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SEC_INFO | 0x2000_0000u32, // DACL | UNPROTECTED_DACL_SECURITY_INFORMATION
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            p_dacl,
            std::ptr::null_mut(),
        );
        assert_eq!(ret, ERROR_SUCCESS, "SetNamedSecurityInfoW が失敗: {ret}");

        // LocalAlloc（= HeapAlloc(GetProcessHeap())）確保領域を解放する
        HeapFree(GetProcessHeap(), 0, p_sd as *const core::ffi::c_void);
    }

    // load は verify_dir を呼ぶ → SE_DACL_PROTECTED 未設定で不変条件① 違反
    let result = repo.load();
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
