//! Windows 用の OS API 呼出（GetNamedPipeClientProcessId / OpenProcessToken /
//! GetTokenInformation / ConvertSidToStringSidW / GetCurrentProcessToken）。
//!
//! `unsafe` ブロックを許可する数少ないファイルの 1 つ
//! （`basic-design/security.md §unsafe_code の扱い` daemon 側）。

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, FALSE, HANDLE, HLOCAL,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, PSID, TOKEN_QUERY, TOKEN_USER};
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::permission::peer_credential::{
    PeerCredentialSource, PeerIdentity, PeerVerificationError,
};

// -------------------------------------------------------------------
// NamedPipeServer への trait 実装
// -------------------------------------------------------------------

impl PeerCredentialSource for tokio::net::windows::named_pipe::NamedPipeServer {
    fn peer_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        use std::os::windows::io::AsRawHandle;
        let handle = self.as_raw_handle() as HANDLE;
        let pid = peer_pid(handle)?;
        let sid = sid_for_pid(pid)?;
        Ok(PeerIdentity::Sid(sid))
    }

    fn self_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        let sid = resolve_self_user_sid_inner()?;
        Ok(PeerIdentity::Sid(sid))
    }
}

// -------------------------------------------------------------------
// public API: 自プロセス User SID 取得
// -------------------------------------------------------------------

/// 自プロセスの User SID を文字列形式で返す。
///
/// `lifecycle::socket_path::resolve_pipe_name` から呼ばれる（unsafe を `lifecycle/` に置かない）。
///
/// # Errors
/// kernel API が失敗した場合 `std::io::Error`。
pub fn resolve_self_user_sid() -> Result<String, std::io::Error> {
    resolve_self_user_sid_inner().map_err(|e| match e {
        PeerVerificationError::Lookup(io) => io,
        other => std::io::Error::other(other.to_string()),
    })
}

// -------------------------------------------------------------------
// 内部実装
// -------------------------------------------------------------------

fn peer_pid(handle: HANDLE) -> Result<u32, PeerVerificationError> {
    let mut pid: u32 = 0;
    // safety: `GetNamedPipeClientProcessId` は read-only。
    let ok = unsafe { GetNamedPipeClientProcessId(handle, std::ptr::addr_of_mut!(pid)) };
    if ok == 0 {
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }
    Ok(pid)
}

fn sid_for_pid(pid: u32) -> Result<String, PeerVerificationError> {
    // safety: `OpenProcess` は HANDLE を返す。失敗時 0、`CloseHandle` で解放する。
    let process_handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if process_handle.is_null() {
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }

    let result = sid_from_process_handle(process_handle);

    // safety: 上で取得した HANDLE を閉じる。
    unsafe {
        CloseHandle(process_handle);
    }
    result
}

fn sid_from_process_handle(process_handle: HANDLE) -> Result<String, PeerVerificationError> {
    let mut token: HANDLE = std::ptr::null_mut();
    // safety: `OpenProcessToken` は失敗時 0、`CloseHandle` で解放する。
    let ok =
        unsafe { OpenProcessToken(process_handle, TOKEN_QUERY, std::ptr::addr_of_mut!(token)) };
    if ok == 0 {
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }

    let result = sid_from_token(token);

    // safety: 上で取得した HANDLE を閉じる。
    unsafe {
        CloseHandle(token);
    }
    result
}

fn sid_from_token(token: HANDLE) -> Result<String, PeerVerificationError> {
    // 必要バッファサイズを取得（最初の呼出は失敗で size を埋める）
    let mut size: u32 = 0;
    // safety: 第 4 引数 NULL の場合は size の問い合わせのみ。
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
        return Err(PeerVerificationError::Lookup(
            std::io::Error::from_raw_os_error(last_err as i32),
        ));
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
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }

    // safety: `buf` は `TOKEN_USER` レイアウト互換。SID ポインタは TOKEN_USER 内部 alloc に紐づく。
    let token_user: *const TOKEN_USER = buf.as_ptr().cast();
    let sid: PSID = unsafe { (*token_user).User.Sid };
    sid_to_string(sid)
}

fn sid_to_string(sid: PSID) -> Result<String, PeerVerificationError> {
    let mut wsid: *mut u16 = std::ptr::null_mut();
    // safety: 戻り値 0 は失敗。成功時は LocalFree で解放する。
    let ok = unsafe { ConvertSidToStringSidW(sid, std::ptr::addr_of_mut!(wsid)) };
    if ok == 0 {
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }
    if wsid.is_null() {
        return Err(PeerVerificationError::Lookup(std::io::Error::other(
            "ConvertSidToStringSidW returned null pointer",
        )));
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

fn resolve_self_user_sid_inner() -> Result<String, PeerVerificationError> {
    // safety: 自プロセスの pseudo handle を返す。CloseHandle 不要（pseudo handle）。
    let process_handle = unsafe { GetCurrentProcess() };
    sid_from_process_handle(process_handle)
}
