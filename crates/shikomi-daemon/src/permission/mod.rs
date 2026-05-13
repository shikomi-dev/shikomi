//! ピア検証 + Windows SID 取得（OS API 呼出を隔離するモジュール）。
//!
//! `unsafe` ブロックは本ディレクトリ配下の `unix.rs` / `windows.rs` のみで許可される
//! （`basic-design/security.md §unsafe_code の扱い`、CI grep TC-CI-019）。

pub mod peer_credential;

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub mod windows_acl;
