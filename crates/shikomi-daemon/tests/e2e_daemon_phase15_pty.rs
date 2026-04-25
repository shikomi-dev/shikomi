//! daemon E2E (Phase 1.5 / Issue #33) — pty 経由 fail-secure 観測
//!
//! TC-E2E-017: `--ipc edit --stdin` を疑似 TTY 上で実行したとき、ユーザーが打った
//! 文字（marker-typed-by-user）が **pty master 側の出力にエコーされない** ことを
//! 実観測する。これは PR #32 の方針 B（`composition-root.md §run_edit IPC 経路の
//! 方針 B`）が実機で fail-secure に倒れていることの**ユーザー観測可能な振る舞い**
//! 検証であり、shoulder-surfing 攻撃に対する構造的保護を担保する。
//!
//! 経路（決定論）:
//! 1. CLI: `existing_kind = None`（IPC 経路では事前 load 省略、`lib.rs::run_edit`）
//! 2. CLI: `decide_kind_for_input(None, Ipc) → Secret`（**方針 B の核**、TC-UT-132 で型検証）
//! 3. CLI: `read_value_from_stdin(Secret)` → `is_stdin_tty() == true && Secret` →
//!    `read_password("value: ")`（rpassword、termios `set_echo(false)`）
//! 4. PTY: 入力バイトはカーネルでエコーされず、master 側の read には現れない
//!
//! 配置: `crates/shikomi-daemon/tests/e2e_daemon_phase15_pty.rs`（**新規ファイル**、Unix 限定）。
//! Windows は別経路（ConsoleScreenBuffer）で観測手段が異なるため `#![cfg(unix)]` で skip。
//! プラットフォーム非依存の fail-secure 構造保証は TC-UT-132（`decide_kind_for_input`）
//! が担うため、本 E2E が Windows で skip しても多層防御は崩れない。
//!
//! 対応 Issue: #33 / 設計書 `docs/features/daemon-ipc/test-design/e2e.md §TC-E2E-017`

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use expectrl::process::unix::WaitStatus;
use expectrl::{Eof, Expect, Session};
use tempfile::TempDir;

const SECRET_MARKER: &str = "SECRET_TEST_VALUE";
const TYPED_MARKER: &str = "marker-typed-by-user";

// -------------------------------------------------------------------
// 共通ヘルパ（e2e_daemon_phase15.rs と同型、cross-test 共有を避けて内部限定で複製）
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

/// `shikomi <args>` を**通常の非 pty プロセスで**実行し `(stdout, stderr, exit_code)` を返す。
/// pre-add / 反映確認用ヘルパ（pty が不要なフェーズ）。
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

/// `added: <uuid>` から uuid 文字列を抽出。
fn extract_uuid_after(prefix: &str, stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix(prefix) {
            let uuid = rest.trim();
            assert_eq!(uuid.len(), 36, "expected 36-char uuid, got {uuid:?}");
            return uuid.to_owned();
        }
    }
    panic!("prefix {prefix:?} not found in stdout:\n{stdout}");
}

// -------------------------------------------------------------------
// TC-E2E-017: `--ipc edit --stdin` の TTY 入力が pty master にエコーされない
// docs/features/daemon-ipc/test-design/e2e.md §TC-E2E-017 / Issue #33
// -------------------------------------------------------------------
//
// 実観測の方法（pty master 側からの read）:
//   1. `expectrl::Session::spawn` で **完全な pty allocation** 配下で CLI を起動
//   2. CLI が `read_password("value: ")` のプロンプトを書き出す → master の read で
//      "value: " が見える（プロンプト出力経路）
//   3. テスト側が `send_line(TYPED_MARKER)` で stdin に入力を送る
//      （bytes が pty slave 側 stdin に到達。termios の echo flag が**off**なら
//      kernel は echo を slave 側 stdout（= master の read 側）に書き戻さない）
//   4. プロセス終了まで読み続け、master の read 出力を全て蓄積
//   5. 蓄積バッファに `marker-typed-by-user` が出現しないことを assert
//      （= ユーザーが画面で打った文字を見ない = shoulder-surfing 不可能）
//
// **本 TC が捕捉する回帰**: もし将来誰かが
//  - `decide_kind_for_input` を `(None, Ipc) → Text` に書き換える、または
//  - `read_value_from_stdin` の `if matches!(kind, Secret) && is_stdin_tty()` 分岐を逆転、
// すれば、本 TC は pty master に `marker-typed-by-user` が echo として出現することを
// 検出して fail する。型レベル（TC-UT-132）+ 実観測（本 TC）の二重防御。
#[test]
fn tc_e2e_017_ipc_edit_stdin_does_not_echo_typed_marker_on_pty() {
    let xdg = tight_tempdir();
    let vault_dir = tight_tempdir();
    seed_empty_vault(vault_dir.path());
    let guard = DaemonGuard::spawn(xdg.path(), vault_dir.path());

    // ---- 1. 事前 add（Text レコード、IPC 経由でも直結でも結果同じ）-----------
    // 本 TC は edit 経路の fail-secure 検証が主眼。pre-add は通常 stdin で OK。
    let (add_stdout, add_stderr, add_code) = run_shikomi(
        xdg.path(),
        vault_dir.path(),
        &[
            "--ipc",
            "add",
            "--kind",
            "text",
            "--label",
            "tc017-existing-text",
            "--value",
            "original-text",
        ],
    );
    assert_eq!(
        add_code,
        Some(0),
        "pre-add should succeed: stdout={add_stdout} stderr={add_stderr}"
    );
    let id = extract_uuid_after("added: ", &add_stdout);

    // ---- 2. pty 経由で edit --stdin を実行 ---------------------------------
    // expectrl::Session::spawn は std::process::Command を受ける。env を含めて構築。
    let bin = assert_cmd::cargo::cargo_bin("shikomi");
    let mut cmd = Command::new(bin);
    cmd.env("XDG_RUNTIME_DIR", xdg.path())
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args(["--ipc", "edit", "--id", &id, "--stdin"]);

    let mut session = Session::spawn(cmd).expect("spawn pty session");
    // 個別 expect は 5 秒、全体タイムアウトは expectrl の expect 側に委譲（CI 余裕）
    session.set_expect_timeout(Some(Duration::from_secs(5)));

    // master 出力（= ユーザーの「画面に映る内容」）を蓄積するバッファ
    let mut master_output: Vec<u8> = Vec::new();

    // ---- 3. プロンプト出現まで読む（"value:" が出れば read_password 経路）------
    // expect("value:") は内部で read を繰り返して "value:" 部分文字列を探す。
    // 見つかれば Captures::before() + matches() に蓄積されたバイトが返る。
    //
    // 現実装の経路:
    //   - MSG-CLI-051 IPC opt-in 警告（stderr）が先に出る可能性あり
    //   - その後 `read_password` が "value: " を出す
    // どちらの順でも "value:" が含まれた段階で expect が成功する。
    let prompt_caps = session
        .expect("value:")
        .expect("CLI should write 'value:' prompt within timeout");
    master_output.extend_from_slice(prompt_caps.before());
    for m in prompt_caps.matches() {
        master_output.extend_from_slice(m);
    }

    // ---- 4. ユーザーの「タイプ」を模擬: stdin に marker + 改行を送る ------------
    // pty の termios で echo がオフなら、ここで送ったバイトは master 側の read には
    // 現れない（kernel が echo back しない）。echo がオンなら、master の read で
    // 直後にエコー文字が出てくる。
    session
        .send_line(TYPED_MARKER)
        .expect("send_line stdin marker");

    // ---- 5. EOF（プロセス終了）まで master からの read を全て蓄積 -------------
    let eof_caps = session
        .expect(Eof)
        .expect("CLI should exit (Eof) within timeout");
    master_output.extend_from_slice(eof_caps.before());

    // ---- 6. プロセス終了ステータス確認（exit 0 のはず）-----------------------
    let status = session.get_process_mut().wait().expect("wait child status");
    let exit_code: i32 = match status {
        WaitStatus::Exited(_, code) => code,
        other => panic!("CLI exited abnormally: {other:?}"),
    };
    assert_eq!(
        exit_code,
        0,
        "CLI should exit 0 after successful edit. master_output={:?} daemon_stderr={}",
        String::from_utf8_lossy(&master_output),
        guard.stderr()
    );

    // ---- 7. 核アサート: pty master 出力に TYPED_MARKER が出現しない ----------
    let master_text = String::from_utf8_lossy(&master_output).into_owned();
    assert!(
        !master_text.contains(TYPED_MARKER),
        "FAIL-SECURE BREACH: pty master output leaked typed input marker.\n\
         expected: '{TYPED_MARKER}' should NOT appear (echo OFF)\n\
         master_output={master_text:?}\n\
         daemon_stderr={}",
        guard.stderr()
    );

    // ---- 8. 横串（secret マーカー不在）-------------------------------------
    // TYPED_MARKER とは別に SECRET_TEST_VALUE 由来のテストマーカーも通常通り検査
    // （本 TC では入力していないので原則出ないが、横串契約として保持）
    assert!(!master_text.contains(SECRET_MARKER));
    assert!(!guard.stderr().contains(SECRET_MARKER));

    // ---- 9. ラウンドトリップ検証: --ipc list で値が反映されている ------------
    // pty 外で list 実行（同 pty 内で実行すると list 出力経由で TYPED_MARKER が
    // master output に出てしまう罠を回避、ペガサス Boy Scout note 対応）。
    drop(session); // pty session を畳んでから別プロセスで list
    let (list_stdout, _list_stderr, list_code) =
        run_shikomi(xdg.path(), vault_dir.path(), &["--ipc", "list"]);
    assert_eq!(
        list_code,
        Some(0),
        "list should succeed: stdout={list_stdout}"
    );
    assert!(
        list_stdout.contains(&id),
        "list should still contain the edited id {id}: {list_stdout}"
    );

    // ---- 10. label は不変（value のみ edit）---------------------------------
    // 本 TC の主眼は「pty 上で TYPED_MARKER が画面に出ない」こと。値の中身まで
    // 含めた完全反映検証は TC-E2E-018（非 TTY パイプ edit）で担う。本 TC では
    // edit 経路が成功し（exit 0）id が引き続き list 上に存在することで間接担保。
    assert!(
        list_stdout.contains("tc017-existing-text"),
        "label should remain unchanged after value-only edit: {list_stdout}"
    );

    // 後始末は DaemonGuard の Drop に委譲（明示 drop で順序を読みやすく）。
    drop(guard);
}
