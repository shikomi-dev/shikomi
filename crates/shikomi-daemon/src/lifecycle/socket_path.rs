//! ソケットパス解決（OS 別、`process-model.md` §4.2 準拠）。
//!
//! - **Linux**: `$XDG_RUNTIME_DIR/shikomi/`、未設定時は `/run/user/$UID/shikomi`
//! - **macOS**: `~/Library/Caches/shikomi/`
//! - **Windows**: `\\.\pipe\shikomi-daemon-{user-sid}`
//!
//! `unsafe` を本ファイルに置かない（Windows SID 取得は `permission::windows::resolve_self_user_sid`
//! 経由で安全な `String` を得る、`basic-design/security.md §unsafe_code の扱い`）。

use std::path::PathBuf;

use thiserror::Error;

// -------------------------------------------------------------------
// SocketPathError
// -------------------------------------------------------------------

/// ソケットパス解決の失敗理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SocketPathError {
    /// 解決元（XDG_RUNTIME_DIR / dirs::*）が利用不能。
    #[error("cannot resolve socket directory")]
    CannotResolve,
    /// Windows: 自プロセスの User SID 取得失敗。
    #[error("failed to resolve self user SID: {0}")]
    SidLookup(std::io::Error),
}

// -------------------------------------------------------------------
// Unix: socket dir 解決
// -------------------------------------------------------------------

/// Unix のソケット親ディレクトリを解決する。
///
/// - Linux: `$XDG_RUNTIME_DIR/shikomi`、未設定時は `dirs::runtime_dir()/shikomi`
/// - macOS: `dirs::cache_dir()/shikomi` （Apple 規約に従い XDG は適用しない）
///
/// # Errors
/// 解決元が利用不能な場合 `SocketPathError::CannotResolve`。
#[cfg(unix)]
pub fn resolve_socket_dir() -> Result<PathBuf, SocketPathError> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir).join("shikomi"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs::cache_dir()
            .map(|d| d.join("shikomi"))
            .ok_or(SocketPathError::CannotResolve)
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::runtime_dir()
            .map(|d| d.join("shikomi"))
            .ok_or(SocketPathError::CannotResolve)
    }
}

// -------------------------------------------------------------------
// Windows: pipe name 解決
// -------------------------------------------------------------------

/// Windows の Named Pipe 名を解決する（`\\.\pipe\shikomi-daemon-{user-sid}`）。
///
/// SID 取得は `permission::windows::resolve_self_user_sid` 経由（unsafe を本ファイルに置かない）。
///
/// # Errors
/// SID 取得失敗時 `SocketPathError::SidLookup`。
#[cfg(windows)]
pub fn resolve_pipe_name() -> Result<String, SocketPathError> {
    let sid =
        crate::permission::windows::resolve_self_user_sid().map_err(SocketPathError::SidLookup)?;
    Ok(format!(r"\\.\pipe\shikomi-daemon-{sid}"))
}

/// Windows: `resolve_socket_dir` のスタブ（API 揃え用、未使用）。
///
/// `Result` は既に `#[must_use]` 型のため、関数本体側 `#[must_use]` は冗長
/// （clippy::double_must_use）。
#[cfg(windows)]
pub fn resolve_socket_dir() -> Result<PathBuf, SocketPathError> {
    // Windows では Named Pipe を使うため socket dir 概念がない（実装の都合で `temp_dir` を返す
    // が、この値は `SingleInstanceLock` が `cfg(windows)` で参照しない）。
    Ok(std::env::temp_dir())
}
