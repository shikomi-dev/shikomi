//! OS 別 transport（UDS / Named Pipe）の入口。
//!
//! `ListenerEnum` は `cfg(unix)` / `cfg(windows)` で別バリアントを持つ。

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

// Unix variant のみが `PathBuf` を使う。Windows ビルドで unused 警告にならないよう cfg gate。
#[cfg(unix)]
use std::path::PathBuf;

// -------------------------------------------------------------------
// ListenerEnum
// -------------------------------------------------------------------

/// `SingleInstanceLock::take_listener` から `IpcServer` に渡される listener の OS 包み型。
pub enum ListenerEnum {
    /// Unix Domain Socket。
    #[cfg(unix)]
    Unix {
        /// `tokio::net::UnixListener`。
        listener: tokio::net::UnixListener,
        /// ソケットパス（observability ログ用）。
        socket_path: PathBuf,
    },
    /// Windows Named Pipe。
    #[cfg(windows)]
    Windows {
        /// 初期 pipe instance（`FILE_FLAG_FIRST_PIPE_INSTANCE` で作成済み）。
        server: tokio::net::windows::named_pipe::NamedPipeServer,
        /// pipe 名（observability ログ用、例: `\\.\pipe\shikomi-daemon-S-1-5-...`）。
        pipe_name: String,
    },
}
