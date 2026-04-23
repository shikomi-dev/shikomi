#![allow(unsafe_code)]

use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Security::ACL;
use windows_sys::Win32::System::Memory::LocalFree;

// ---------------------------------------------------------------------------
// RAII ガード
// ---------------------------------------------------------------------------

/// `GetNamedSecurityInfoW` が `LocalAlloc` で返した `PSECURITY_DESCRIPTOR` の RAII ラッパ。
///
/// `Drop` で `LocalFree(ptr)` を呼ぶ。早期 return / panic でも確実に解放する
/// （Microsoft Learn 明記のメモリ解放責務を型で強制）。
pub(super) struct SecurityDescriptorGuard {
    pub(super) ptr: *mut core::ffi::c_void,
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
pub(super) struct LocalFreeAclGuard {
    pub(super) ptr: *mut ACL,
}

impl LocalFreeAclGuard {
    /// 内部 ACL ポインタを返す。ガードより長生きさせてはならない。
    pub(super) fn as_ptr(&self) -> *mut ACL {
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
pub(super) struct SidStringGuard {
    pub(super) ptr: *mut u16,
}

impl SidStringGuard {
    /// SID ワイド文字列を Rust `String` に変換する（UTF-16 lossy）。
    pub(super) fn to_string_lossy(&self) -> String {
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
