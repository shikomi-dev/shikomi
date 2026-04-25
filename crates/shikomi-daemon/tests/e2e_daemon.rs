//! daemon E2E — test-design/e2e.md TC-E2E-001 / 020 / 030。
//!
//! **完全ブラックボックス**: `std::process::Command` で `shikomi-daemon` バイナリを
//! spawn し、stdout/stderr/exit code と socket ファイル存在を検証する。
//! tokio::process は feature "process" 未有効のため std 経路で。
//!
//! 対応 Issue: #26

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// daemon は起動時に vault.db の load を要求するため、事前に空 vault を書き出す。
fn seed_empty_vault(vault_dir: &Path) {
    use shikomi_core::{Vault, VaultHeader, VaultVersion};
    use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
    use time::OffsetDateTime;
    let header = VaultHeader::new_plaintext(
        VaultVersion::CURRENT,
        OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
    )
    .expect("header");
    let vault = Vault::new(header);
    let repo = SqliteVaultRepository::from_directory(vault_dir).expect("repo");
    repo.save(&vault).expect("seed empty vault");
}

// -------------------------------------------------------------------
// DaemonGuard — Drop で kill する RAII
// -------------------------------------------------------------------

struct DaemonGuard {
    child: Option<Child>,
    stderr_log: Arc<Mutex<String>>,
    sock_path: PathBuf,
}

fn tight_tempdir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700");
    dir
}

impl DaemonGuard {
    fn spawn(xdg_runtime_dir: &Path, vault_dir: &Path) -> Self {
        let bin = env!("CARGO_BIN_EXE_shikomi-daemon");
        let mut child = Command::new(bin)
            .env("XDG_RUNTIME_DIR", xdg_runtime_dir)
            .env("SHIKOMI_VAULT_DIR", vault_dir)
            .env("SHIKOMI_DAEMON_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn daemon");

        let stderr = child.stderr.take().expect("stderr piped");
        let stderr_log: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let stderr_log_for_thread = Arc::clone(&stderr_log);
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut log) = stderr_log_for_thread.lock() {
                    log.push_str(&line);
                    log.push('\n');
                }
            }
        });

        let sock_path = xdg_runtime_dir.join("shikomi").join("daemon.sock");
        // 起動完了待機（最大 5 秒）
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut listening_seen = false;
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(100));
            if sock_path.exists() {
                // listening ログも確認
                if let Ok(log) = stderr_log.lock() {
                    if log.contains("listening on") {
                        listening_seen = true;
                        break;
                    }
                }
            }
        }
        assert!(
            sock_path.exists(),
            "daemon socket not created within 5s at {sock_path:?}. stderr:\n{}",
            stderr_log.lock().map(|s| s.clone()).unwrap_or_default()
        );
        assert!(
            listening_seen,
            "'listening on' log not seen within 5s. stderr:\n{}",
            stderr_log.lock().map(|s| s.clone()).unwrap_or_default()
        );
        Self {
            child: Some(child),
            stderr_log,
            sock_path,
        }
    }

    fn send_sigterm(&self) {
        if let Some(child) = &self.child {
            // daemon は SIGTERM で graceful shutdown する契約（REQ-DAEMON-014）。
            // nix の signal feature 経由で直接 syscall（procps の /usr/bin/kill に依存しない）。
            // BUG-DAEMON-IPC-002 の調査で「`Command::new("kill")` は kill バイナリが
            // PATH に無い slim 環境（rust:1.80-slim 等）では silent failure する」ことが
            // 判明したため、この経路を採用。
            #[allow(clippy::cast_possible_wrap)]
            let pid = nix::unistd::Pid::from_raw(child.id() as i32);
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM);
        }
    }

    fn wait_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let mut child = self.child.take()?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => return None,
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        None
    }

    fn stderr(&self) -> String {
        self.stderr_log
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// -------------------------------------------------------------------
// TC-E2E-001: daemon 起動 → listening + socket 0600 / parent 0700
// -------------------------------------------------------------------
#[test]
fn tc_e2e_001_daemon_starts_and_creates_socket_with_0600() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());

    let mut guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());
    let sock_mode = std::fs::metadata(&guard.sock_path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(sock_mode, 0o600, "socket should be 0600");

    let parent = guard.sock_path.parent().unwrap();
    let parent_mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(parent_mode, 0o700, "parent dir should be 0700");

    // stderr に "listening on" が含まれる
    assert!(guard.stderr().contains("listening on"));

    // cleanup
    guard.send_sigterm();
    let _ = guard.wait_exit(Duration::from_secs(3));
}

// -------------------------------------------------------------------
// TC-E2E-020: daemon 二重起動が exit 2
// -------------------------------------------------------------------
#[test]
fn tc_e2e_020_second_daemon_exits_with_code_2() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());

    let mut guard_a = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    // B を同じ env で起動（既存 daemon A の flock 保持で失敗）
    let bin = env!("CARGO_BIN_EXE_shikomi-daemon");
    let output = Command::new(bin)
        .env("XDG_RUNTIME_DIR", xdg.path())
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .env("SHIKOMI_DAEMON_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn daemon B");
    assert_eq!(
        output.status.code(),
        Some(2),
        "daemon B should exit 2, got: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // cleanup
    guard_a.send_sigterm();
    let _ = guard_a.wait_exit(Duration::from_secs(3));
}

// -------------------------------------------------------------------
// TC-E2E-022: SIGKILL された daemon の残留でも次起動成功
//
// **scope-out: IT で同等カバー**
// 同シナリオ（flock auto-release + stale socket unlink）は IT
// `it_single_instance.rs` の TC-IT-062（stale socket）/ TC-IT-064（lock drop &
// re-acquire）で実 syscall 経路を含めて緑化済み。
// E2E（プロセス spawn ベース）でも検証可能だが、CI 環境での `child.wait()` /
// 子プロセス reap 後のタイミング依存性が大きく flaky の温床。
// 設計書 `e2e.md` の判定基準「IT で同等カバーされる場合は E2E を重複させない」に
// 従い、本 TC は **IT で代替**（test 一意性原則）。
// -------------------------------------------------------------------

// -------------------------------------------------------------------
// TC-E2E-030: SIGTERM で graceful shutdown → exit 0
//
// **BUG-DAEMON-IPC-002 修正済み（regression marker 兼 受入基準 5 検証）**
// `Notify` → `tokio::sync::watch::channel<bool>` への置換で、シグナル到達と
// receiver の poll 順序に関わらず通知が消失しない構造に変更。signal task は
// `send(true)` で全 receiver を起こす。
// -------------------------------------------------------------------
#[test]
fn tc_e2e_030_sigterm_triggers_graceful_shutdown() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let mut guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    // 「listening on」ログが出てから server が accept loop に入るまでの微小な
    // タイミングをカバーするため少し待つ（watch ベースなら必須ではないが、
    // 実プロセスのスケジューリング揺らぎに対する安全マージン）。
    thread::sleep(Duration::from_millis(200));

    guard.send_sigterm();
    let status = guard.wait_exit(Duration::from_secs(5));
    assert!(
        status.is_some(),
        "daemon should exit within 5s after SIGTERM. stderr:\n{}",
        guard.stderr()
    );
    let code = status.unwrap().code();
    assert_eq!(code, Some(0), "graceful shutdown should exit 0");
    // shutdown ログが流れていること（観測性）
    let stderr = guard.stderr();
    assert!(
        stderr.contains("shutdown signal received"),
        "stderr should contain 'shutdown signal received'. stderr:\n{stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-080 / SCN-A 部分: shikomi --ipc list が実 daemon 経由で動く
// -------------------------------------------------------------------
#[test]
fn tc_e2e_080_shikomi_cli_ipc_list_end_to_end() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let mut guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    // shikomi --ipc list を実行（cross-crate binary は assert_cmd で解決）
    let shikomi_bin = assert_cmd::cargo::cargo_bin("shikomi");
    let output = Command::new(shikomi_bin)
        .env("XDG_RUNTIME_DIR", xdg.path())
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args(["--ipc", "list"])
        .output()
        .expect("spawn shikomi cli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // 空 vault でも exit 0 が期待される（list は 0 件でも成功）
    assert!(
        output.status.success(),
        "shikomi --ipc list should succeed: exit={:?} stdout={stdout} stderr={stderr}",
        output.status.code()
    );
    // stderr に SECRET_TEST_VALUE が出ない（horizontal assertion）
    assert!(!stdout.contains("SECRET_TEST_VALUE"));
    assert!(!stderr.contains("SECRET_TEST_VALUE"));

    // cleanup
    guard.send_sigterm();
    let _ = guard.wait_exit(Duration::from_secs(3));
}

// -------------------------------------------------------------------
// TC-E2E-SCN-C (部分): shikomi --ipc list が daemon 未起動なら exit 1 + MSG-CLI-110 系
// -------------------------------------------------------------------
#[test]
fn tc_e2e_scnc_daemon_not_running_shows_error_and_exits_nonzero() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());

    let shikomi_bin = assert_cmd::cargo::cargo_bin("shikomi");
    let output = Command::new(shikomi_bin)
        .env("XDG_RUNTIME_DIR", xdg.path())
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args(["--ipc", "list"])
        .output()
        .expect("spawn shikomi cli");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "should fail when daemon is not running; got exit={:?}",
        output.status.code()
    );
    // stderr に daemon 関連メッセージ or ヒントが含まれる（MSG-CLI-110 相当）
    let lower = stderr.to_lowercase();
    assert!(
        lower.contains("daemon")
            || lower.contains("not running")
            || lower.contains("socket")
            || lower.contains("connection"),
        "stderr should mention daemon/connection issue: {stderr}"
    );
    // SECRET 非含有
    assert!(!stderr.contains("SECRET_TEST_VALUE"));
}
