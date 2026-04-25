//! Unix 用の OS API 呼出（SO_PEERCRED / LOCAL_PEERCRED / geteuid）。
//!
//! `unsafe` ブロックを許可する数少ないファイルの 1 つ
//! （`basic-design/security.md §unsafe_code の扱い` daemon 側）。

#![allow(unsafe_code)]

use std::os::unix::io::AsRawFd;

use nix::libc;

use crate::permission::peer_credential::{
    PeerCredentialSource, PeerIdentity, PeerVerificationError,
};

// -------------------------------------------------------------------
// UnixStream への trait 実装
// -------------------------------------------------------------------

impl PeerCredentialSource for tokio::net::UnixStream {
    fn peer_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        let uid = peer_uid(self.as_raw_fd())?;
        Ok(PeerIdentity::Uid(uid))
    }

    fn self_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        // safety: `geteuid` は副作用なしの read-only syscall。
        let uid = unsafe { libc::geteuid() };
        Ok(PeerIdentity::Uid(uid))
    }
}

// -------------------------------------------------------------------
// 内部実装
// -------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn peer_uid(fd: std::os::unix::io::RawFd) -> Result<u32, PeerVerificationError> {
    // SO_PEERCRED で `struct ucred` を取得（read-only）
    #[repr(C)]
    struct Ucred {
        pid: i32,
        uid: u32,
        gid: u32,
    }

    let mut cred: Ucred = Ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len: libc::socklen_t =
        u32::try_from(std::mem::size_of::<Ucred>()).unwrap_or(libc::socklen_t::MAX);
    // safety: `getsockopt` は read-only syscall。`cred` / `len` の生存期間は本関数内。
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            std::ptr::addr_of_mut!(cred).cast(),
            std::ptr::addr_of_mut!(len),
        )
    };
    if ret != 0 {
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }
    Ok(cred.uid)
}

#[cfg(target_os = "macos")]
fn peer_uid(fd: std::os::unix::io::RawFd) -> Result<u32, PeerVerificationError> {
    // LOCAL_PEERCRED で `struct xucred` を取得
    #[repr(C)]
    struct Xucred {
        cr_version: u32,
        cr_uid: u32,
        cr_ngroups: i16,
        cr_groups: [u32; 16],
    }

    let mut cred: Xucred = Xucred {
        cr_version: 0,
        cr_uid: 0,
        cr_ngroups: 0,
        cr_groups: [0; 16],
    };
    let mut len: libc::socklen_t =
        u32::try_from(std::mem::size_of::<Xucred>()).unwrap_or(libc::socklen_t::MAX);

    // SOL_LOCAL = 0, LOCAL_PEERCRED = 1（macOS 標準値）
    const SOL_LOCAL: libc::c_int = 0;
    const LOCAL_PEERCRED: libc::c_int = 1;

    // safety: `getsockopt` は read-only syscall。
    let ret = unsafe {
        libc::getsockopt(
            fd,
            SOL_LOCAL,
            LOCAL_PEERCRED,
            std::ptr::addr_of_mut!(cred).cast(),
            std::ptr::addr_of_mut!(len),
        )
    };
    if ret != 0 {
        return Err(PeerVerificationError::Lookup(
            std::io::Error::last_os_error(),
        ));
    }
    Ok(cred.cr_uid)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn peer_uid(_fd: std::os::unix::io::RawFd) -> Result<u32, PeerVerificationError> {
    Err(PeerVerificationError::Lookup(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "peer credential lookup not implemented for this OS",
    )))
}
