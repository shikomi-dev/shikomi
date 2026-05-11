//! IPC エンドポイント解決（ソケットパスの単一真実源）。
//!
//! `IpcEndpoint::default_for_current_user()` は GUI の `lib.rs::setup()` および
//! CLI の `IpcVaultRepository::default_socket_path()` から呼び出される。
//! パス解決ロジックを複数箇所に重複させない（DRY）。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md REQ-IPC-13
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §1.5

use std::path::PathBuf;

use crate::persistence::PersistenceError;

// ---------------------------------------------------------------------------
// Windows SID 解決（unsafe を本モジュールに局所化）
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_sid {
    use crate::persistence::PersistenceError;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL,
    };
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// 自プロセスの User SID を文字列形式で返す。
    ///
    /// # Errors
    /// kernel API 失敗時 `PersistenceError::IpcIo`（reason は固定文言）。
    pub fn resolve_self_user_sid() -> Result<String, PersistenceError> {
        let mut token: HANDLE = 0;
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
        // PSID は windows-sys 0.52 で廃止。SID_AND_ATTRIBUTES::Sid の実型へ as キャストする。
        let sid = unsafe { (*token_user).User.Sid as *mut ::core::ffi::c_void };
        sid_to_string(sid)
    }

    fn sid_to_string(sid: *mut ::core::ffi::c_void) -> Result<String, PersistenceError> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt as _;

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

    // safety: ヌル終端 wide 文字列の長さを返す純関数。
    unsafe fn wide_strlen(p: *const u16) -> usize {
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        n
    }
}

// ---------------------------------------------------------------------------
// IpcEndpoint
// ---------------------------------------------------------------------------

/// IPC ソケットパスの解決を担う型。
///
/// `default_for_current_user()` が現ユーザーのデフォルト IPC ソケットパスを返す。
/// CLI の `IpcVaultRepository::default_socket_path()` と GUI の `lib.rs::setup()` が
/// 同メソッドを呼び出すことで、パス解決ロジックを一箇所に集約する（DRY、REQ-IPC-13）。
pub struct IpcEndpoint;

impl IpcEndpoint {
    /// 現ユーザーのデフォルト IPC ソケットパスを解決する。
    ///
    /// 解決優先順（Unix）：
    /// 1. `$XDG_RUNTIME_DIR/shikomi/daemon.sock`（設定済の場合）
    /// 2. macOS: `dirs::cache_dir()/shikomi/daemon.sock`
    ///    Linux / その他 Unix: `dirs::runtime_dir()/shikomi/daemon.sock`
    ///
    /// 解決優先順（Windows）：
    /// 1. `\\.\pipe\shikomi-daemon-{user-sid}`（SID 取得成功の場合）
    ///
    /// # Errors
    /// 解決元が利用不能な場合 `PersistenceError::CannotResolveVaultDir`。
    /// Windows で SID 取得失敗の場合 `PersistenceError::IpcIo`。
    pub fn default_for_current_user() -> Result<PathBuf, PersistenceError> {
        #[cfg(unix)]
        {
            unix_default_socket_path()
        }
        #[cfg(windows)]
        {
            let sid = windows_sid::resolve_self_user_sid()?;
            Ok(PathBuf::from(format!(r"\\.\pipe\shikomi-daemon-{sid}")))
        }
    }
}

// ---------------------------------------------------------------------------
// Unix 内部実装
// ---------------------------------------------------------------------------

/// Unix のデフォルトソケットパスを解決する。
///
/// - `$XDG_RUNTIME_DIR` 設定済み: `$XDG_RUNTIME_DIR/shikomi/daemon.sock`
/// - macOS フォールバック: `dirs::cache_dir()/shikomi/daemon.sock`
/// - Linux / その他: `dirs::runtime_dir()/shikomi/daemon.sock`
///
/// daemon が `resolve_socket_dir().join("daemon.sock")` で bind する経路と一致する。
#[cfg(unix)]
fn unix_default_socket_path() -> Result<PathBuf, PersistenceError> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir).join("shikomi").join("daemon.sock"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs::cache_dir()
            .map(|d| d.join("shikomi").join("daemon.sock"))
            .ok_or(PersistenceError::CannotResolveVaultDir)
    }

    #[cfg(not(target_os = "macos"))]
    {
        dirs::runtime_dir()
            .map(|d| d.join("shikomi").join("daemon.sock"))
            .ok_or(PersistenceError::CannotResolveVaultDir)
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Unix: XDG_RUNTIME_DIR が設定されている場合、`daemon.sock` を返す。
    #[test]
    #[cfg(unix)]
    #[serial_test::serial(env_xdg_home)]
    fn ipc_endpoint_with_xdg_runtime_dir_returns_daemon_sock() {
        let saved = std::env::var("XDG_RUNTIME_DIR").ok();
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");

        let path = IpcEndpoint::default_for_current_user().unwrap();
        assert!(
            path.ends_with("shikomi/daemon.sock"),
            "expected shikomi/daemon.sock suffix, got: {path:?}"
        );
        assert!(path.starts_with("/run/user/1000"));

        if let Some(v) = saved {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }
}
