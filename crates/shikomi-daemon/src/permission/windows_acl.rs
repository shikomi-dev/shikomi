//! Windows Named Pipe 用 owner-only DACL の構築（`SecurityAttributes`）。
//!
//! 設計根拠: docs/features/daemon-ipc/basic-design/security.md §「Named Pipe SDDL
//! owner-only ACE」。デフォルト DACL は **Everyone に GENERIC_READ/GENERIC_WRITE を付与**するため、
//! `peer_credential::verify` の単層に縮退し Defense in Depth が崩れる。kernel ACL で
//! **OS 層から所有者以外を遮断**することで、pipe を `CreateFile` で開くこと自体を OS 拒否させる。
//!
//! ## SDDL フォーマット
//!
//! `D:P(A;;GA;;;<SELF_USER_SID>)`
//! - `D:` DACL セクション
//! - `P` SDDL_PROTECTED — 親オブジェクトからの ACE 継承を遮断
//! - `(A;;GA;;;SID)` Allow ACE — `GA` (`GENERIC_ALL`) を SID に付与
//!
//! バグ参照: 服部平次レビュー §犯行手口（PR #29）

#![allow(unsafe_code)]

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
#[cfg(windows)]
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
#[cfg(windows)]
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

#[cfg(windows)]
use crate::permission::windows::resolve_self_user_sid;

// -------------------------------------------------------------------
// OwnerOnlySecurityAttributes
// -------------------------------------------------------------------

/// 所有者（自プロセス User SID）にのみ全アクセスを許す DACL を内包する `SECURITY_ATTRIBUTES`。
///
/// `tokio::net::windows::named_pipe::ServerOptions::create_with_security_attributes_raw` に
/// 渡すための raw ポインタを `as_ptr()` で取得する。`Drop` で `LocalFree` するため、
/// `SECURITY_ATTRIBUTES` の有効期間は本構造体の lifetime と等しい。kernel は `create()` 呼出時に
/// SD のコピーを行うため、`create()` 後は本構造体を drop してよい。
#[cfg(windows)]
pub struct OwnerOnlySecurityAttributes {
    /// LocalAlloc された SECURITY_DESCRIPTOR。Drop で `LocalFree`。
    descriptor: *mut std::ffi::c_void,
    /// `SECURITY_ATTRIBUTES` 本体（`descriptor` を参照する）。
    attributes: SECURITY_ATTRIBUTES,
}

#[cfg(windows)]
impl OwnerOnlySecurityAttributes {
    /// 自プロセス User SID を取得し、SDDL `D:P(A;;GA;;;<SID>)` から SECURITY_DESCRIPTOR を構築する。
    ///
    /// # Errors
    /// - SID 解決失敗
    /// - `ConvertStringSecurityDescriptorToSecurityDescriptorW` 失敗
    pub fn new() -> Result<Self, std::io::Error> {
        let sid = resolve_self_user_sid()?;
        let sddl_str = format!("D:P(A;;GA;;;{sid})");
        // null 終端の wide 文字列に変換
        let wide: Vec<u16> = OsStr::new(&sddl_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut descriptor: *mut std::ffi::c_void = std::ptr::null_mut();
        // safety: ConvertStringSecurityDescriptorToSecurityDescriptorW は wide ヌル終端を入力に取り、
        // 成功時 LocalAlloc された descriptor を返す。失敗時 0、`LocalFree` で解放する。
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                std::ptr::addr_of_mut!(descriptor),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };

        Ok(Self {
            descriptor,
            attributes,
        })
    }

    /// `ServerOptions::create_with_security_attributes_raw` に渡す raw ポインタ。
    ///
    /// 本構造体を drop すると `descriptor` も解放されるため、`create()` 完了までは
    /// 本構造体を生かしておくこと（`create()` 後 kernel は SD のコピーを保持するため drop OK）。
    #[must_use]
    pub fn as_ptr(&self) -> *mut std::ffi::c_void {
        std::ptr::addr_of!(self.attributes) as *mut std::ffi::c_void
    }
}

#[cfg(windows)]
impl Drop for OwnerOnlySecurityAttributes {
    fn drop(&mut self) {
        if !self.descriptor.is_null() {
            // safety: descriptor は LocalAlloc されたポインタ。double-free は起こらない
            // （`Drop` は 1 度だけ呼ばれる契約）。
            unsafe {
                LocalFree(self.descriptor as HLOCAL);
            }
            self.descriptor = std::ptr::null_mut();
        }
    }
}

// -------------------------------------------------------------------
// SAFE wrapper: ServerOptions::create_with_security_attributes_raw
// -------------------------------------------------------------------

/// owner-only DACL を適用した最初の Named Pipe インスタンスを生成する。
///
/// 設計意図: `unsafe` ブロックを `permission/` 配下に閉じ、
/// 呼び出し側（`lifecycle::single_instance` / `ipc::server`）から unsafe を排除する
/// （CI grep TC-CI-019 の単一真実源化）。
///
/// # Errors
/// - SID 解決失敗 / SDDL 変換失敗 → `std::io::Error`
/// - `create_with_security_attributes_raw` 失敗 → `std::io::Error`（既存 `ERROR_ACCESS_DENIED`
///   / `ERROR_PIPE_BUSY` 検出は呼び出し側で行う）
#[cfg(windows)]
pub fn create_first_pipe_instance_owner_only(
    pipe_name: &str,
    max_instances: usize,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let sa = OwnerOnlySecurityAttributes::new()?;
    // safety: `sa` は本関数 scope 中の lifetime を持ち、`create_with_security_attributes_raw`
    // 呼出時点で `sa.as_ptr()` は有効。kernel は呼出時に SECURITY_DESCRIPTOR をコピーするため、
    // 関数 return 後の `sa` drop で kernel object に影響しない。
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .max_instances(max_instances)
            .create_with_security_attributes_raw(pipe_name, sa.as_ptr())
    }
}

/// owner-only DACL を適用した次のインスタンスを生成する（accept ループから呼ばれる）。
///
/// `first_pipe_instance(true)` は付与しない（最初のインスタンスは既に作成済み）。
///
/// # Errors
/// 上記と同じ。
#[cfg(windows)]
pub fn create_next_pipe_instance_owner_only(
    pipe_name: &str,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let sa = OwnerOnlySecurityAttributes::new()?;
    // safety: 同上。
    unsafe { ServerOptions::new().create_with_security_attributes_raw(pipe_name, sa.as_ptr()) }
}

// Windows 以外では空の placeholder を提供しない（呼び出し側が `#[cfg(windows)]` で囲む）。
