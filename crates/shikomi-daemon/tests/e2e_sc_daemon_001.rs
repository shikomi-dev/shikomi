//! 受入テスト E2E — SC-DAEMON-001: daemon 初回起動（vault.db 不在）
//!
//! 設計書: docs/acceptance-tests/scenarios/SC-DAEMON-001-first-launch.md
//! AC-001〜AC-004 の自動化可能な部分を完全ブラックボックスで検証する。
//!
//! **ブラックボックス方針**: `std::process::Command` で `shikomi-daemon` バイナリを
//! spawn し、stdout/stderr/exit code とファイルシステム観測のみで判定する。
//! DB 直接確認・内部状態参照・テスト用裏口・内部関数呼び出しは一切行わない。
//!
//! AC-002（GUI `VaultStatusBanner` 表示）は GUI プロセスを要し自動化困難なため
//! 手動テストとして SC-DAEMON-001-first-launch.md に委ねる（本ファイルではスキップ）。
//!
//! Issue: #80 / REQ-DAEMON-028
//! 対応受入基準: AC-001, AC-003, AC-004

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// DaemonGuard — Drop で kill する RAII
// ---------------------------------------------------------------------------

struct DaemonGuard {
    child: Option<Child>,
    stderr_log: Arc<Mutex<String>>,
    #[allow(dead_code)]
    sock_path: PathBuf,
}

fn tight_tempdir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700");
    dir
}

impl DaemonGuard {
    /// vault.db を事前に作成せず daemon を起動する（SC-DAEMON-001 用）。
    fn spawn_without_vault(xdg_runtime_dir: &Path, vault_dir: &Path) -> Self {
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
        // 起動完了待機（最大 8 秒: vault.db 生成 + ソケット作成の時間を考慮）
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut started = false;
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(100));
            if sock_path.exists() {
                if let Ok(log) = stderr_log.lock() {
                    if log.contains("listening on") {
                        started = true;
                        break;
                    }
                }
            }
        }
        assert!(
            started,
            "daemon が 8 秒以内に起動しなかった。vault.db 生成に失敗した可能性がある。\nstderr:\n{}",
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

// ---------------------------------------------------------------------------
// AC-001: daemon が自動起動して vault.db を生成する
//
// Given: SHIKOMI_VAULT_DIR に vault.db が存在しない
// When: shikomi-daemon を起動する
// Then:
//   - daemon が exit 0 以外で異常終了しないこと（起動成功）
//   - vault.db ファイルが SHIKOMI_VAULT_DIR に生成されていること
//   - `shikomi --ipc list` で空リスト（エラーなし）が返ること
// ---------------------------------------------------------------------------

#[test]
fn sc_daemon_001_ac_001_daemon_creates_vault_db_on_first_launch() {
    // SC-DAEMON-001 AC-001 / REQ-DAEMON-028 / Issue #80
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    // 事前条件: vault.db は存在しない
    let vault_db_path = vault_dir.path().join("vault.db");
    assert!(
        !vault_db_path.exists(),
        "事前条件: vault.db が存在しないこと"
    );

    // daemon を vault.db なしで起動
    let mut guard = DaemonGuard::spawn_without_vault(xdg.path(), vault_dir.path());

    // vault.db が生成されていること
    assert!(
        vault_db_path.exists(),
        "AC-001: vault.db が SHIKOMI_VAULT_DIR に生成されるべき"
    );

    // `shikomi list` で空リストが返ること（エラーなし）
    // Phase 2 (Issue #126): --ipc フラグは廃止。IPC が既定経路。
    let shikomi_bin = assert_cmd::cargo::cargo_bin("shikomi");
    let output = Command::new(&shikomi_bin)
        .env("XDG_RUNTIME_DIR", xdg.path())
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args(["list"])
        .output()
        .expect("shikomi list を実行");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr_cli = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "AC-001: `shikomi list` は exit 0 で成功するべき。\
         exit={:?} stdout={stdout} stderr={stderr_cli}",
        output.status.code()
    );

    // cleanup
    guard.send_sigterm();
    let status = guard.wait_exit(Duration::from_secs(5));
    assert!(
        status.map(|s| s.success()).unwrap_or(false),
        "AC-001: daemon は exit 0 で正常終了するべき"
    );
}

// ---------------------------------------------------------------------------
// AC-003: ペルソナ B 向け補助ログが出力される
//
// Given: vault.db が存在しない状態で daemon を起動する
// When: daemon の起動ログ（stderr, SHIKOMI_DAEMON_LOG=info）を確認する
// Then:
//   - "vault not found; created new plaintext vault at " を含む INFO ログ
//   - "hint: to enable encryption" を含む INFO ログ
//   - ログに秘密情報が含まれない
// ---------------------------------------------------------------------------

#[test]
fn sc_daemon_001_ac_003_init_log_on_first_launch_contains_hint_no_secrets() {
    // SC-DAEMON-001 AC-003 / REQ-DAEMON-028 / Issue #80
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();

    let mut guard = DaemonGuard::spawn_without_vault(xdg.path(), vault_dir.path());
    // 少し待ってログが流れるのを待機
    thread::sleep(Duration::from_millis(300));
    let stderr = guard.stderr();

    // vault not found ログ
    assert!(
        stderr.contains("vault not found; created new plaintext vault at"),
        "AC-003: 'vault not found; created new plaintext vault at' ログが出力されるべき。\nstderr:\n{stderr}"
    );
    // hint ログ
    assert!(
        stderr.contains("hint: to enable encryption"),
        "AC-003: 'hint: to enable encryption' ログが出力されるべき。\nstderr:\n{stderr}"
    );
    // 秘密情報が含まれないこと（横串アサート）
    assert!(
        !stderr.contains("SECRET_TEST_VALUE"),
        "AC-003: ログに SECRET_TEST_VALUE が含まれるべきでない"
    );
    // パスワード / secret / vault 内容等の機密文字列パターン（保守的チェック）
    let lower = stderr.to_lowercase();
    assert!(
        !lower.contains("password") || lower.contains("hint"),
        "AC-003: ログに 'password' 等の機密文字列が含まれるべきでない（hint 文脈除く）"
    );

    guard.send_sigterm();
    let _ = guard.wait_exit(Duration::from_secs(5));
}

// ---------------------------------------------------------------------------
// AC-004: 2 回目以降の起動では vault.db が再生成されない
//
// Given: AC-001 で vault.db が生成された状態
// When: daemon を停止して再起動する
// Then:
//   - "vault not found; created new plaintext vault" ログが出力されない
//   - 起動前後で vault.db の mtime が変化しない（上書きなし）
// ---------------------------------------------------------------------------

#[test]
fn sc_daemon_001_ac_004_second_launch_does_not_recreate_vault_db() {
    // SC-DAEMON-001 AC-004 / REQ-DAEMON-028 / Issue #80
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    let vault_db_path = vault_dir.path().join("vault.db");

    // 1 回目起動: vault.db を生成させる
    {
        let mut guard = DaemonGuard::spawn_without_vault(xdg.path(), vault_dir.path());
        assert!(
            vault_db_path.exists(),
            "AC-004 前提: 1 回目起動で vault.db が生成されるべき"
        );
        guard.send_sigterm();
        let status = guard.wait_exit(Duration::from_secs(5));
        assert!(
            status.is_some(),
            "AC-004 前提: 1 回目の daemon が終了するべき"
        );
    }

    // vault.db の mtime を記録
    let mtime_before: SystemTime = vault_db_path
        .metadata()
        .expect("vault.db metadata")
        .modified()
        .expect("mtime");

    // 少し待って mtime の精度差を吸収（Linux の mtime 分解能は 1ns 〜 1s）
    thread::sleep(Duration::from_millis(1100));

    // 2 回目起動
    let mut guard2 = DaemonGuard::spawn_without_vault(xdg.path(), vault_dir.path());
    thread::sleep(Duration::from_millis(300));
    let stderr2 = guard2.stderr();

    // "vault not found" ログが出ないこと（既存 vault を再利用）
    assert!(
        !stderr2.contains("vault not found; created new plaintext vault"),
        "AC-004: 2 回目起動では 'vault not found; created new plaintext vault' ログが出力されるべきでない。\nstderr:\n{stderr2}"
    );

    // vault.db の mtime が変化していないこと（上書きなし）
    let mtime_after: SystemTime = vault_db_path
        .metadata()
        .expect("vault.db metadata")
        .modified()
        .expect("mtime");
    assert_eq!(
        mtime_before, mtime_after,
        "AC-004: 2 回目起動では vault.db の mtime が変化するべきでない（上書きなし）"
    );

    guard2.send_sigterm();
    let _ = guard2.wait_exit(Duration::from_secs(5));
}
