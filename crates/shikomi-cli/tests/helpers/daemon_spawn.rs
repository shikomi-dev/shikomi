//! 実 `shikomi-daemon` 子プロセスのライフサイクル管理ヘルパー。
//!
//! ## 責務
//! - `TempDir` 内に `XDG_RUNTIME_DIR/shikomi/` を 0700 で作成し daemon を起動
//! - `Drop` で `kill()` → `wait()` の二段階（ゾンビ化防止: CI 並列実行時の pid リソース枯渇防止）
//! - C-40 env seam（`SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS` / `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL`）
//!   を `with_*` メソッド経由で注入
//!
//! ## セキュリティ契約（daemon-ipc/security.md §シングルインスタンス準拠）
//! 1. socket 親ディレクトリを `0700` で作成してから daemon を起動する
//! 2. daemon 起動後、socket 親の mode を `stat` で検証 — 不一致なら `anyhow::bail!`
//!
//! 設計根拠: `docs/features/cli-vault-commands/test-design/integration.md §10.2`
//! 対応 Issue: #77

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// 実 `shikomi-daemon` 子プロセスのガード。
///
/// - `_xdg_dir`: `XDG_RUNTIME_DIR` 用 `TempDir`（Drop で自動削除）
/// - `socket_path`: `daemon.sock` のフルパス
/// - `process`: 子プロセスハンドル
///
/// Drop 時に `kill()` → `wait()` の二段階でゾンビ化を防ぐ。
pub struct DaemonSpawn {
    _xdg_dir: TempDir,
    socket_path: PathBuf,
    process: Child,
    extra_env: Vec<(OsString, OsString)>,
}

impl DaemonSpawn {
    /// 指定 vault_dir で daemon を起動する。
    ///
    /// # セキュリティ契約
    /// 1. `{xdg_dir}/shikomi/` を 0700 で作成してから daemon を起動する
    /// 2. socket が現れた後、socket 親の mode を stat で検証する
    ///
    /// # Errors
    /// - daemon バイナリが見つからない / spawn 失敗
    /// - socket が 5 秒以内に現れない
    /// - socket 親 mode が 0700 でない
    pub fn new(vault_dir: &Path) -> anyhow::Result<Self> {
        let xdg_dir = TempDir::new()?;

        // socket 親ディレクトリを 0700 で事前作成（セキュリティ契約ステップ 1）
        let socket_parent = xdg_dir.path().join("shikomi");
        std::fs::create_dir_all(&socket_parent)?;
        std::fs::set_permissions(&socket_parent, std::fs::Permissions::from_mode(0o700))?;

        let socket_path = socket_parent.join("daemon.sock");

        // daemon バイナリ取得 + 起動
        let daemon_bin = assert_cmd::cargo::cargo_bin("shikomi-daemon");
        let process = Command::new(&daemon_bin)
            .env("XDG_RUNTIME_DIR", xdg_dir.path())
            .env("SHIKOMI_VAULT_DIR", vault_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let xdg_dir_path = xdg_dir.path().to_path_buf();
        let deadline = Instant::now() + Duration::from_secs(5);

        // socket ファイルが現れるまでポーリング
        while !socket_path.exists() {
            if Instant::now() > deadline {
                anyhow::bail!(
                    "daemon socket not created within 5s: {}",
                    socket_path.display()
                );
            }
            thread::sleep(Duration::from_millis(50));
        }

        // セキュリティ契約ステップ 2: socket 親 mode 検証
        let mode = std::fs::metadata(&socket_parent)?.permissions().mode() & 0o777;
        anyhow::ensure!(
            mode == 0o700,
            "socket parent dir mode {mode:#o} != 0700 (path: {})",
            socket_parent.display()
        );

        let _ = xdg_dir_path;
        Ok(Self {
            _xdg_dir: xdg_dir,
            socket_path,
            process,
            extra_env: Vec::new(),
        })
    }

    /// C-40 allowlist 経由で idle 短縮 threshold を注入する（`#[cfg(debug_assertions)]` 限定）。
    #[cfg(debug_assertions)]
    pub fn with_idle_threshold(mut self, secs: u64) -> Self {
        self.extra_env.push((
            OsString::from("SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS"),
            OsString::from(secs.to_string()),
        ));
        self
    }

    /// C-40 allowlist 経由で `cache_relocked=false` 故障注入を有効化する（`#[cfg(debug_assertions)]` 限定）。
    ///
    /// TC-F-I07c (`shikomi vault rekey` の relock 失敗経路) で使用する。
    #[cfg(debug_assertions)]
    pub fn with_force_relock_fail(mut self) -> Self {
        self.extra_env.push((
            OsString::from("SHIKOMI_DAEMON_FORCE_RELOCK_FAIL"),
            OsString::from("1"),
        ));
        self
    }

    /// daemon との通信に必要な env vars を返す。
    ///
    /// `assert_cmd::Command::envs(daemon.env_args())` で CLI テストコマンドに注入する。
    pub fn env_args(&self) -> Vec<(OsString, OsString)> {
        let mut envs = vec![(
            OsString::from("XDG_RUNTIME_DIR"),
            self._xdg_dir.path().as_os_str().to_owned(),
        )];
        envs.extend(self.extra_env.iter().cloned());
        envs
    }

    /// daemon.sock のフルパス。
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for DaemonSpawn {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait(); // ゾンビ化防止: kill 後に必ず wait する（CI 並列実行時の pid リソース枯渇防止）
    }
}
