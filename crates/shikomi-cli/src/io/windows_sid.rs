//! Windows: 自プロセス User SID 取得（CLI 側、`unsafe` を本ファイルに局所化）。
//!
//! `basic-design/security.md §unsafe_code の扱い` CLI 側 1 領域。daemon 側
//! `permission::windows::resolve_self_user_sid` と**同等機能を独立実装**する
//! （crate 境界を尊重、cli ⇄ daemon 直接依存を作らない）。

#![cfg(windows)]
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use shikomi_infra::persistence::PersistenceError;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, PSID, TOKEN_QUERY, TOKEN_USER};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// 自プロセスの User SID を文字列形式で返す。
///
/// `default_socket_path` から呼ばれ、Named Pipe 名（`\\.\pipe\shikomi-daemon-{sid}`）を
/// 組み立てる。
///
/// # Errors
/// kernel API 失敗時 `PersistenceError::IpcIo`（reason は固定文言で path 等を含めない）。
pub fn resolve_self_user_sid() -> Result<String, PersistenceError> {
    let mut token: HANDLE = std::ptr::null_mut();
    // safety: `GetCurrentProcess` は pseudo handle、`OpenProcessToken` は read-only。
    let ok = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            std::ptr::addr_of_mut!(token),
        )
    };
    if ok == 0 {
        return Err(PersistenceError::IpcIo {
            reason: "open process token failed".to_owned(),
        });
    }

    let result = sid_from_token(token);

    // safety: 上で取得した HANDLE を閉じる。
    unsafe {
        CloseHandle(token);
    }
    result
}

fn sid_from_token(token: HANDLE) -> Result<String, PersistenceError> {
    let mut size: u32 = 0;
    // safety: 第 4 引数 NULL でサイズ問い合わせのみ。
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            std::ptr::null_mut(),
            0,
            std::ptr::addr_of_mut!(size),
        );
    }
    let last_err = unsafe { GetLastError() };
    if size == 0 || last_err != ERROR_INSUFFICIENT_BUFFER {
        return Err(PersistenceError::IpcIo {
            reason: "token information size lookup failed".to_owned(),
        });
    }

    let mut buf: Vec<u8> = vec![0; size as usize];
    // safety: `buf` は `size` バイト確保済み。`GetTokenInformation` が write する。
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr().cast(),
            size,
            std::ptr::addr_of_mut!(size),
        )
    };
    if ok == 0 {
        return Err(PersistenceError::IpcIo {
            reason: "token information lookup failed".to_owned(),
        });
    }

    // safety: `buf` は `TOKEN_USER` レイアウト互換。
    let token_user: *const TOKEN_USER = buf.as_ptr().cast();
    let sid: PSID = unsafe { (*token_user).User.Sid };
    sid_to_string(sid)
}

fn sid_to_string(sid: PSID) -> Result<String, PersistenceError> {
    let mut wsid: *mut u16 = std::ptr::null_mut();
    // safety: 戻り値 0 は失敗。成功時は LocalFree で解放する。
    let ok = unsafe { ConvertSidToStringSidW(sid, std::ptr::addr_of_mut!(wsid)) };
    if ok == 0 || wsid.is_null() {
        return Err(PersistenceError::IpcIo {
            reason: "sid string conversion failed".to_owned(),
        });
    }

    // safety: `wsid` は LocalAlloc されたヌル終端 wide 文字列。
    let len = unsafe { wide_strlen(wsid) };
    let slice = unsafe { std::slice::from_raw_parts(wsid, len) };
    let s = OsString::from_wide(slice).to_string_lossy().into_owned();

    // safety: `LocalFree` は LocalAlloc で確保された pointer を解放する。
    unsafe {
        LocalFree(wsid as HLOCAL);
    }
    Ok(s)
}

unsafe fn wide_strlen(p: *const u16) -> usize {
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    n
}
