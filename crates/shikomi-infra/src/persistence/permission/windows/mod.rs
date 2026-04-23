//! Windows 固有のパーミッション実装（owner-only DACL 強制 / 検証）。
//!
//! Microsoft 公式 `windows-sys` crate が Win32 Security API を `unsafe fn` で公開しているため、
//! owner-only DACL の `SetNamedSecurityInfoW` / `GetNamedSecurityInfoW` 等を呼ぶ各モジュールに限り
//! `unsafe_code` lint を許容する。他モジュールは `forbid` を保持。
//! 参照: https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setnamedsecurityinfow

mod guard;
mod helper;
mod verify;
mod ensure;
#[cfg(test)]
mod tests;

pub(super) use ensure::{ensure_dir, ensure_file, verify_dir, verify_file};

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
