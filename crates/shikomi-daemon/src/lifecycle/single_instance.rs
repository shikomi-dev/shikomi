//! シングルインスタンス先取り（OS 別、`process-model.md` §4.1 ルール 2）。
//!
//! - **Unix**: `flock(LOCK_EX|LOCK_NB)` 獲得 → `unlink` → `bind` の 3 段階厳守
//! - **Windows**: Named Pipe を `FILE_FLAG_FIRST_PIPE_INSTANCE` 付きで作成

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::ipc::transport::ListenerEnum;

// -------------------------------------------------------------------
// SingleInstanceError
// -------------------------------------------------------------------

/// シングルインスタンス先取り失敗の理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SingleInstanceError {
    /// 既に他 daemon が稼働中（Unix: flock EWOULDBLOCK / Windows: パイプ既存）。
    #[error("another daemon is already running")]
    AlreadyRunning {
        /// Unix の lock path、または Windows の pipe name。
        location: String,
    },
    /// ソケット親ディレクトリの権限が `0700` でない（Unix のみ）。
    #[error("socket directory permission is invalid: expected 0o700, got {got:o}")]
    InvalidDirectoryPermission {
        /// 検査対象パス。
        path: PathBuf,
        /// 実際の permission bits。
        got: u32,
    },
    /// flock 操作の OS エラー（Unix のみ）。
    #[error("flock failed: {0}")]
    Lock(std::io::Error),
    /// stale ソケットの unlink 失敗（Unix のみ）。
    #[error("unlink failed: {0}")]
    UnlinkFailed(std::io::Error),
    /// `bind` 失敗（Unix） / Named Pipe 作成失敗（Windows）。
    #[error("listener bind failed: {0}")]
    Bind(std::io::Error),
    /// I/O 一般エラー（ディレクトリ作成・stat 等）。
    #[error("io error: {0}")]
    Io(std::io::Error),
}

// -------------------------------------------------------------------
// SingleInstanceLock
// -------------------------------------------------------------------

/// シングルインスタンス先取りの RAII ガード。
///
/// `acquire` で取得した内部リソース（Unix: flock + ソケット / Windows: Named Pipe）は
/// `Drop` で解放される。`take_listener` で listener を `IpcServer` に移譲した後も、
/// flock 保持・ソケット unlink 責務は本ガードに残る。
pub struct SingleInstanceLock {
    inner: Inner,
}

#[allow(dead_code)]
enum Inner {
    #[cfg(unix)]
    Unix {
        // flock を保持する fd（Drop 時に File が drop されると flock も解放される）
        lock_file: std::fs::File,
        socket_path: PathBuf,
        listener: Option<tokio::net::UnixListener>,
    },
    #[cfg(windows)]
    Windows {
        pipe_name: String,
        pipe_server: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
    },
}

impl SingleInstanceLock {
    /// OS 検出して acquire を呼ぶ統一エントリ。
    ///
    /// # Errors
    /// `SingleInstanceError` 各バリアント（OS 依存）。
    pub fn acquire(socket_dir: &Path) -> Result<Self, SingleInstanceError> {
        #[cfg(unix)]
        {
            Self::acquire_unix(socket_dir)
        }
        #[cfg(windows)]
        {
            // socket_dir は Windows では使わない（pipe 名解決は呼出側）
            let _ = socket_dir;
            let pipe_name =
                crate::lifecycle::socket_path::resolve_pipe_name().map_err(|e| match e {
                    crate::lifecycle::socket_path::SocketPathError::SidLookup(io) => {
                        SingleInstanceError::Io(io)
                    }
                    other => SingleInstanceError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        other.to_string(),
                    )),
                })?;
            Self::acquire_windows(&pipe_name)
        }
    }

    /// listener を取り出して `IpcServer` に移譲する（一度だけ消費可能）。
    pub fn take_listener(&mut self) -> Option<ListenerEnum> {
        match &mut self.inner {
            #[cfg(unix)]
            Inner::Unix {
                listener,
                socket_path,
                ..
            } => listener.take().map(|l| ListenerEnum::Unix {
                listener: l,
                socket_path: socket_path.clone(),
            }),
            #[cfg(windows)]
            Inner::Windows {
                pipe_server,
                pipe_name,
            } => pipe_server.take().map(|p| ListenerEnum::Windows {
                server: p,
                pipe_name: pipe_name.clone(),
            }),
        }
    }
}

// -------------------------------------------------------------------
// Unix 実装
// -------------------------------------------------------------------

#[cfg(unix)]
impl SingleInstanceLock {
    /// Unix のシングルインスタンス先取り（3 段階厳守）。
    ///
    /// 1. 親ディレクトリ確保（`0700` で `mkdir -p` + 明示的 `chmod 0o700` + 検証）
    /// 2. lock ファイル open + `flock(LOCK_EX|LOCK_NB)` 獲得
    /// 3. 既存 socket を `unlink`（ENOENT は無視）
    /// 4. `UnixListener::bind` でソケット作成 + permission `0o600`
    pub fn acquire_unix(socket_dir: &Path) -> Result<Self, SingleInstanceError> {
        use std::fs::{self, OpenOptions};
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        // 1. 親ディレクトリ確保
        fs::create_dir_all(socket_dir).map_err(SingleInstanceError::Io)?;
        fs::set_permissions(socket_dir, fs::Permissions::from_mode(0o700))
            .map_err(SingleInstanceError::Io)?;

        let metadata = fs::metadata(socket_dir).map_err(SingleInstanceError::Io)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode != 0o700 {
            return Err(SingleInstanceError::InvalidDirectoryPermission {
                path: socket_dir.to_path_buf(),
                got: mode,
            });
        }

        // 2. lock ファイル open + flock
        let lock_path = socket_dir.join("daemon.lock");
        // `truncate(false)` は明示。advisory flock 用ファイルなので中身は使わない（0 byte 維持）。
        // clippy::suspicious_open_options は create(true) 時 truncate の意図を要求するため明示で黙らせる。
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .map_err(SingleInstanceError::Io)?;

        flock_exclusive_nonblock(&lock_file).map_err(|err| {
            if err.kind() == std::io::ErrorKind::WouldBlock {
                SingleInstanceError::AlreadyRunning {
                    location: lock_path.display().to_string(),
                }
            } else {
                SingleInstanceError::Lock(err)
            }
        })?;

        // 3. stale socket unlink（ロック獲得後にのみ実行、race-safe）
        let socket_path = socket_dir.join("daemon.sock");
        match fs::remove_file(&socket_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(SingleInstanceError::UnlinkFailed(err)),
        }

        // 4. bind
        let listener =
            tokio::net::UnixListener::bind(&socket_path).map_err(SingleInstanceError::Bind)?;
        // umask 影響を避けるため明示的に 0o600
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            .map_err(SingleInstanceError::Io)?;

        Ok(Self {
            inner: Inner::Unix {
                lock_file,
                socket_path,
                listener: Some(listener),
            },
        })
    }
}

#[cfg(unix)]
fn flock_exclusive_nonblock(file: &std::fs::File) -> std::io::Result<()> {
    // `fs4::fs_std::FileExt::try_lock_exclusive` は内部で `flock(LOCK_EX | LOCK_NB)`（Unix）
    // を呼ぶ薄ラッパ。File を借用するため SingleInstanceLock 内に File を保持し続ければ
    // ロックも維持される（File の Drop で自動解放）。
    //
    // fs4 0.9 の本メソッドは `io::Result<()>` を返し、`WouldBlock` は他プロセス保持中。
    use fs4::fs_std::FileExt;
    #[allow(clippy::incompatible_msrv)]
    file.try_lock_exclusive()
}

// -------------------------------------------------------------------
// Windows 実装
// -------------------------------------------------------------------

#[cfg(windows)]
impl SingleInstanceLock {
    /// Windows のシングルインスタンス先取り（Named Pipe `FIRST_PIPE_INSTANCE`）。
    pub fn acquire_windows(pipe_name: &str) -> Result<Self, SingleInstanceError> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .max_instances(255)
            .create(pipe_name)
            .map_err(|err| {
                if err.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32)
                    || err.raw_os_error()
                        == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                {
                    SingleInstanceError::AlreadyRunning {
                        location: pipe_name.to_owned(),
                    }
                } else {
                    SingleInstanceError::Bind(err)
                }
            })?;

        Ok(Self {
            inner: Inner::Windows {
                pipe_name: pipe_name.to_owned(),
                pipe_server: Some(server),
            },
        })
    }
}

// -------------------------------------------------------------------
// Drop（リソース解放）
// -------------------------------------------------------------------

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        match &mut self.inner {
            #[cfg(unix)]
            Inner::Unix {
                socket_path,
                listener,
                ..
            } => {
                drop(listener.take());
                // ベストエフォート（次回起動時に stale socket でも flock で race-safe）
                let _ = std::fs::remove_file(socket_path);
                // lock_file が drop されると flock は kernel が自動 release
            }
            #[cfg(windows)]
            Inner::Windows { pipe_server, .. } => {
                drop(pipe_server.take());
                // Named Pipe は kernel が自動 release
            }
        }
    }
}
