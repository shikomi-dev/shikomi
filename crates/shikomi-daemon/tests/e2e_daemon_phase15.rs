//! daemon E2E (Phase 1.5) — test-design/e2e.md
//! TC-E2E-011 / 012 / 013 / 014 / 015 / 016（Issue #30 で初活性化）
//!
//! Phase 1.5（Issue #30）の核：PR #29 の runtime reject 経路を撤去し、`--ipc add` /
//! `--ipc edit` / `--ipc remove` が実 daemon 経由で透過動作する。本ファイルは
//! 「ユーザーが完全ブラックボックスで観測する振る舞い」を `std::process::Command`
//! 経由で検証する。
//!
//! - DB 直接確認 / 内部状態参照 / テスト用裏口は禁止（テスト戦略ガイド準拠）
//! - 状態確認は `shikomi --ipc list` のラウンドトリップで実施
//! - secret マーカー `SECRET_TEST_VALUE` の不在は全 stdout/stderr で横串アサート
//!
//! 対応 Issue: #30 / PR #32

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const SECRET_MARKER: &str = "SECRET_TEST_VALUE";

// -------------------------------------------------------------------
// 共通ヘルパ（e2e_daemon.rs と同型、内部限定で複製、cross-test 共有を避ける）
// -------------------------------------------------------------------

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

fn tight_tempdir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700");
    dir
}

struct DaemonGuard {
    child: Option<Child>,
    stderr_log: Arc<Mutex<String>>,
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

        let sock_path: PathBuf = xdg_runtime_dir.join("shikomi").join("daemon.sock");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut listening_seen = false;
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            if sock_path.exists() {
                if let Ok(log) = stderr_log.lock() {
                    if log.contains("listening on") {
                        listening_seen = true;
                        break;
                    }
                }
            }
        }
        assert!(
            sock_path.exists() && listening_seen,
            "daemon failed to listen within 5s. stderr:\n{}",
            stderr_log.lock().map(|s| s.clone()).unwrap_or_default()
        );
        Self {
            child: Some(child),
            stderr_log,
        }
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

/// `shikomi <args>` を実行し `(stdout, stderr, exit_code)` を返す。
fn run_shikomi(xdg: &Path, vault_dir: &Path, args: &[&str]) -> (String, String, Option<i32>) {
    let bin = assert_cmd::cargo::cargo_bin("shikomi");
    let output = Command::new(bin)
        .env("XDG_RUNTIME_DIR", xdg)
        .env("SHIKOMI_VAULT_DIR", vault_dir)
        .args(args)
        .output()
        .expect("spawn shikomi cli");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

/// `added: <uuid>` / `updated: <uuid>` / `removed: <uuid>` から uuid 文字列を抽出。
fn extract_uuid_after(prefix: &str, stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix(prefix) {
            let uuid = rest.trim();
            // 36 文字 UUID v7 形式の最低限チェック（hyphen 4 + hex 32）
            assert_eq!(uuid.len(), 36, "expected 36-char uuid, got {uuid:?}");
            return uuid.to_owned();
        }
    }
    panic!("prefix {prefix:?} not found in stdout:\n{stdout}");
}

// -------------------------------------------------------------------
// TC-E2E-016: PR #29 runtime reject 撤去回帰検証（Phase 1.5-α / REQ-DAEMON-027）
// -------------------------------------------------------------------
//
// PR #29 段階で `--ipc add` は `--ipc currently supports only the list subcommand`
// の reject エラーで exit 1 になっていた。Phase 1.5 でその経路を撤去し、daemon
// 経由で AddRecord が透過するようにした。本 TC はその撤去契約が**実機で観測可能**
// であることを検証する。CI grep TC-CI-028 と二重防衛。
#[test]
fn tc_e2e_016_pr29_runtime_reject_removed_for_ipc_add() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    let (stdout, stderr, code) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &[
            "--ipc",
            "add",
            "--kind",
            "text",
            "--label",
            "phase15-regression",
            "--value",
            "regression-value",
        ],
    );

    assert_eq!(
        code,
        Some(0),
        "exit code should be 0 (reject removed). stdout={stdout} stderr={stderr} daemon_stderr={}",
        guard.stderr()
    );
    assert!(
        stdout.contains("added: "),
        "stdout should contain 'added: <id>': {stdout}"
    );
    // 旧 reject メッセージの不在
    assert!(
        !stderr.contains("currently supports only"),
        "stderr should NOT contain old PR#29 reject message: {stderr}"
    );
    assert!(!stdout.contains(SECRET_MARKER));
    assert!(!stderr.contains(SECRET_MARKER));
}

// -------------------------------------------------------------------
// TC-E2E-011: --ipc add (text) → --ipc list で反映確認
// -------------------------------------------------------------------
#[test]
fn tc_e2e_011_ipc_add_text_then_list_shows_record() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let _guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    let (add_stdout, add_stderr, add_code) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &[
            "--ipc",
            "add",
            "--kind",
            "text",
            "--label",
            "tc011-label",
            "--value",
            "tc011-value",
        ],
    );
    assert_eq!(
        add_code,
        Some(0),
        "add should succeed: stdout={add_stdout} stderr={add_stderr}"
    );
    let new_id = extract_uuid_after("added: ", &add_stdout);

    // 反映確認：--ipc list で新 id が出る（ラウンドトリップ検証）
    let (list_stdout, list_stderr, list_code) =
        run_shikomi(xdg.path(), vault_dir.path(), &["--ipc", "list"]);
    assert_eq!(
        list_code,
        Some(0),
        "list should succeed: stdout={list_stdout} stderr={list_stderr}"
    );
    assert!(
        list_stdout.contains(&new_id),
        "list stdout should contain the new id {new_id}: {list_stdout}"
    );
    assert!(
        list_stdout.contains("tc011-label"),
        "list stdout should contain the new label: {list_stdout}"
    );
    // text 値はプレビュー出力されるはず（TC-E2E-011 期待）
    assert!(
        list_stdout.contains("tc011-value"),
        "text value should appear in list preview: {list_stdout}"
    );
    // 横串
    assert!(!add_stderr.contains(SECRET_MARKER));
    assert!(!list_stdout.contains(SECRET_MARKER));
    assert!(!list_stderr.contains(SECRET_MARKER));
}

// -------------------------------------------------------------------
// TC-E2E-012: --ipc add (secret --stdin) で SECRET 非露出
// -------------------------------------------------------------------
#[test]
fn tc_e2e_012_ipc_add_secret_stdin_never_echoes_marker() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    let bin = assert_cmd::cargo::cargo_bin("shikomi");
    let mut child = Command::new(bin)
        .env("XDG_RUNTIME_DIR", xdg.path())
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args([
            "--ipc",
            "add",
            "--kind",
            "secret",
            "--label",
            "tc012-secret",
            "--stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn shikomi cli");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{SECRET_MARKER}").expect("write stdin");
    }
    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "add secret should succeed: stdout={stdout} stderr={stderr} daemon_stderr={}",
        guard.stderr()
    );
    // 横串：CLI stdout/stderr + daemon stderr 全てに SECRET_MARKER 非含有
    assert!(
        !stdout.contains(SECRET_MARKER),
        "CLI stdout leaked secret: {stdout}"
    );
    assert!(
        !stderr.contains(SECRET_MARKER),
        "CLI stderr leaked secret: {stderr}"
    );
    let dlog = guard.stderr();
    assert!(
        !dlog.contains(SECRET_MARKER),
        "daemon stderr leaked secret: {dlog}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-013: --ipc edit --label NEW で list 反映
// -------------------------------------------------------------------
#[test]
fn tc_e2e_013_ipc_edit_label_then_list_shows_new_label() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let _guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    // 事前 add
    let (add_stdout, _, _) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &[
            "--ipc",
            "add",
            "--kind",
            "text",
            "--label",
            "tc013-old",
            "--value",
            "v",
        ],
    );
    let id = extract_uuid_after("added: ", &add_stdout);

    // edit
    let (edit_stdout, edit_stderr, edit_code) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &["--ipc", "edit", "--id", &id, "--label", "tc013-new"],
    );
    assert_eq!(
        edit_code,
        Some(0),
        "edit should succeed: stdout={edit_stdout} stderr={edit_stderr}"
    );
    assert!(
        edit_stdout.contains(&id),
        "edit stdout should echo id: {edit_stdout}"
    );

    // 反映確認
    let (list_stdout, _, _) = run_shikomi(xdg.path(), vault_dir.path(), &["--ipc", "list"]);
    assert!(
        list_stdout.contains("tc013-new"),
        "list should show the new label: {list_stdout}"
    );
    assert!(
        !list_stdout.contains("tc013-old"),
        "list should NOT contain the old label: {list_stdout}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-014: --ipc remove --yes で list から消える
// -------------------------------------------------------------------
#[test]
fn tc_e2e_014_ipc_remove_yes_then_list_shows_no_record() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let _guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    // 事前 add
    let (add_stdout, _, _) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &[
            "--ipc",
            "add",
            "--kind",
            "text",
            "--label",
            "tc014-doomed",
            "--value",
            "v",
        ],
    );
    let id = extract_uuid_after("added: ", &add_stdout);

    // 削除前 list で存在確認
    let (list_before, _, _) = run_shikomi(xdg.path(), vault_dir.path(), &["--ipc", "list"]);
    assert!(list_before.contains(&id));

    // remove --yes
    let (rm_stdout, rm_stderr, rm_code) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &["--ipc", "remove", "--id", &id, "--yes"],
    );
    assert_eq!(
        rm_code,
        Some(0),
        "remove should succeed: stdout={rm_stdout} stderr={rm_stderr}"
    );
    assert!(
        rm_stdout.contains(&id),
        "remove stdout should echo id: {rm_stdout}"
    );

    // 削除後 list から消えている
    let (list_after, _, _) = run_shikomi(xdg.path(), vault_dir.path(), &["--ipc", "list"]);
    assert!(
        !list_after.contains(&id),
        "list should not contain removed id: {list_after}"
    );
    assert!(
        !list_after.contains("tc014-doomed"),
        "list should not contain removed label: {list_after}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-015: --ipc edit --id <非存在> → exit 1 + RecordNotFound 経路
// -------------------------------------------------------------------
#[test]
fn tc_e2e_015_ipc_edit_nonexistent_id_returns_user_error() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    let (stdout, stderr, code) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &[
            "--ipc",
            "edit",
            // 全 0 の UUID v7 — vault に存在しない
            "--id",
            "00000000-0000-0000-0000-000000000000",
            "--label",
            "anything",
        ],
    );
    assert!(
        code.unwrap_or(0) != 0,
        "edit on non-existent id should fail: exit={code:?} stdout={stdout} stderr={stderr}"
    );
    // ユーザー観測可能なメッセージ：「見つからない」「not found」「record」のいずれかを期待
    let lower = stderr.to_lowercase();
    assert!(
        lower.contains("not found") || lower.contains("見つかりません") || lower.contains("record"),
        "stderr should mention not-found or record: {stderr}"
    );
    // 横串
    assert!(!stdout.contains(SECRET_MARKER));
    assert!(!stderr.contains(SECRET_MARKER));
    assert!(!guard.stderr().contains(SECRET_MARKER));
}
