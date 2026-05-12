//! 受入テスト E2E — SC-DDM-002: daemon OS 自動起動（Sub-B）
//!
//! 対応受入基準: AC-DDM-07〜10
//!   docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md
//! Vモデル: 受入テスト（最上位・完全ブラックボックス）
//! 対応 TC: SC-DDM-002-TC-001〜004
//! 対応 Issue: #127
//!
//! **ブラックボックス方針**:
//!   `assert_cmd::Command::cargo_bin("shikomi")` でサブプロセスを起動し、
//!   stdout / stderr / exit code とファイルシステム観測のみで判定する。
//!   DB 直接確認・内部状態参照・テスト用裏口・内部関数呼び出しは一切行わない。
//!
//! **副作用隔離**:
//!   `HOME` 環境変数を `tempfile::TempDir` にオーバーライドし、
//!   実システムの設定ディレクトリ（`~/Library/LaunchAgents/` 等）への副作用を排除する。
//!   環境変数操作は `#[serial_test::serial]` で直列実行し、競合を防ぐ。
//!
//! **CI 実行条件**:
//!   Linux CI: XDG Autostart バックエンド（D-Bus 未設定のため systemd フォールバック）
//!   macOS CI: launchd バックエンド（ファイル書き込み部分のみ検証、launchctl は ignore）
//!   Windows CI: 本テストファイルは #[cfg(unix)] でスコープ外

#![cfg(unix)]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// ヘルパー
// ---------------------------------------------------------------------------

/// テスト用の HOME ディレクトリを作成し `(TempDir, PathBuf)` を返す。
/// TempDir が Drop するまで HOME オーバーライドが有効。
fn make_home_dir() -> TempDir {
    TempDir::new().expect("tempdir")
}

/// このテスト環境でインストールされる自動起動ファイルのパスを推定する。
///
/// Linux (D-Bus 未設定 → XDG Autostart): `~/.config/autostart/shikomi-daemon.desktop`
/// macOS: `~/Library/LaunchAgents/dev.shikomi.daemon.plist`
/// その他: None（パス確認スキップ）
fn expected_autostart_file(home: &std::path::Path) -> Option<PathBuf> {
    if cfg!(target_os = "linux") {
        Some(home.join(".config/autostart/shikomi-daemon.desktop"))
    } else if cfg!(target_os = "macos") {
        Some(home.join("Library/LaunchAgents/dev.shikomi.daemon.plist"))
    } else {
        None
    }
}

/// `shikomi daemon <args>` を `HOME=home_dir` 環境で実行して結果を返す。
fn run_daemon_cmd(home_dir: &std::path::Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("shikomi")
        .expect("shikomi binary")
        .env("HOME", home_dir)
        .arg("daemon")
        .args(args)
        .assert()
}

// ---------------------------------------------------------------------------
// SC-DDM-002-TC-001: AC-DDM-07 — install 成功 + 自動起動ファイル配置確認
// ---------------------------------------------------------------------------

/// AC-DDM-07: `shikomi daemon install` が成功し、
///   stdout に "shikomi-daemon autostart enabled" + OS 固有 hint が出力され、
///   OS 固有の自動起動ファイルが配置されること。
///
/// 設計根拠: docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md §AC-DDM-07
/// REQ-DDM-010, REQ-DDM-013〜016
#[test]
#[serial]
fn sc_ddm_002_tc001_ac07_daemon_install_creates_autostart_file() {
    let home = make_home_dir();

    run_daemon_cmd(home.path(), &["install"])
        // exit 0
        .success()
        // stdout に成功メッセージ
        .stdout(predicate::str::contains("shikomi-daemon autostart enabled"))
        // stdout に OS 固有 hint
        .stdout(predicate::str::contains("hint:"))
        // stderr に "error:" なし（tracing INFO は許容）
        .stderr(predicate::str::contains("error:").not());

    // OS 固有の自動起動ファイルが配置されていること
    if let Some(autostart_path) = expected_autostart_file(home.path()) {
        assert!(
            autostart_path.exists(),
            "AC-DDM-07: 自動起動ファイルが配置されるべき: {}",
            autostart_path.display()
        );
        // プレースホルダが残っていないこと
        let content = std::fs::read_to_string(&autostart_path).expect("autostart file readable");
        assert!(
            !content.contains("{daemon_path}"),
            "AC-DDM-07: テンプレートプレースホルダ {{daemon_path}} が残っているべきでない\n内容:\n{content}"
        );
        assert!(
            !content.contains("{log_dir}"),
            "AC-DDM-07: テンプレートプレースホルダ {{log_dir}} が残っているべきでない\n内容:\n{content}"
        );
        // 秘密情報非含有（横串アサート）
        let lower = content.to_lowercase();
        assert!(
            !lower.contains("password") && !lower.contains("secret") && !lower.contains("token"),
            "AC-DDM-07: 自動起動ファイルに秘密情報が含まれるべきでない\n内容:\n{content}"
        );
    }
}

// ---------------------------------------------------------------------------
// SC-DDM-002-TC-002: AC-DDM-08 — uninstall 成功 + ファイル削除確認
// ---------------------------------------------------------------------------

/// AC-DDM-08: `shikomi daemon uninstall` が成功し、
///   stdout に "shikomi-daemon autostart disabled" が出力され、
///   自動起動ファイルが削除されること。
///
/// 設計根拠: docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md §AC-DDM-08
/// REQ-DDM-011, REQ-DDM-013〜016
#[test]
#[serial]
fn sc_ddm_002_tc002_ac08_daemon_uninstall_removes_autostart_file() {
    let home = make_home_dir();

    // 前提: install 済み状態にする
    run_daemon_cmd(home.path(), &["install"]).success();

    // autostart ファイルが存在することを確認してから uninstall
    if let Some(autostart_path) = expected_autostart_file(home.path()) {
        assert!(
            autostart_path.exists(),
            "AC-DDM-08 前提: install 後にファイルが存在するべき"
        );
    }

    run_daemon_cmd(home.path(), &["uninstall"])
        // exit 0
        .success()
        // stdout に成功メッセージ
        .stdout(predicate::str::contains(
            "shikomi-daemon autostart disabled",
        ))
        // stderr に "error:" なし
        .stderr(predicate::str::contains("error:").not());

    // 自動起動ファイルが削除されていること
    if let Some(autostart_path) = expected_autostart_file(home.path()) {
        assert!(
            !autostart_path.exists(),
            "AC-DDM-08: uninstall 後に自動起動ファイルが存在するべきでない: {}",
            autostart_path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// SC-DDM-002-TC-003: AC-DDM-09 — status が常に exit 0 + 2 行出力
// ---------------------------------------------------------------------------

/// AC-DDM-09 シナリオ C: `--no-ipc` フラグ指定時に
///   stdout 1 行目が "daemon: unknown (--no-ipc)"、
///   stdout 2 行目が "autostart: enabled" または "autostart: disabled"、
///   かつ常に exit 0 で終了すること。
///
/// 設計根拠: docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md §AC-DDM-09 シナリオ C
/// REQ-DDM-012 §設計原則（情報提供のみ、副作用なし）
#[test]
#[serial]
fn sc_ddm_002_tc003_ac09_daemon_status_no_ipc_always_exit_0() {
    let home = make_home_dir();

    // ケース A: autostart 未登録状態
    {
        let output = Command::cargo_bin("shikomi")
            .expect("shikomi binary")
            .env("HOME", home.path())
            .args(["daemon", "status", "--no-ipc"])
            .output()
            .expect("実行失敗");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "AC-DDM-09: daemon status は常に exit 0 であるべき。\
             exit={:?} stdout={stdout} stderr={stderr}",
            output.status.code()
        );
        let lines: Vec<&str> = stdout.lines().collect();
        assert!(
            lines.len() >= 2,
            "AC-DDM-09: stdout は 2 行以上であるべき。\nstdout:\n{stdout}"
        );
        assert_eq!(
            lines[0], "daemon: unknown (--no-ipc)",
            "AC-DDM-09: 1 行目は 'daemon: unknown (--no-ipc)' であるべき"
        );
        assert_eq!(
            lines[1], "autostart: disabled",
            "AC-DDM-09: autostart 未登録状態では 2 行目が 'autostart: disabled' であるべき"
        );
        assert!(
            !stderr.contains("error:"),
            "AC-DDM-09: stderr に 'error:' が含まれるべきでない。stderr:\n{stderr}"
        );
    }

    // ケース B: install 後 → autostart: enabled
    {
        run_daemon_cmd(home.path(), &["install"]).success();

        let output = Command::cargo_bin("shikomi")
            .expect("shikomi binary")
            .env("HOME", home.path())
            .args(["daemon", "status", "--no-ipc"])
            .output()
            .expect("実行失敗");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "AC-DDM-09: daemon status は install 後も exit 0 であるべき"
        );
        let lines: Vec<&str> = stdout.lines().collect();
        assert!(lines.len() >= 2, "AC-DDM-09: stdout は 2 行以上であるべき");
        assert_eq!(lines[0], "daemon: unknown (--no-ipc)");
        assert_eq!(
            lines[1], "autostart: enabled",
            "AC-DDM-09: install 後は 2 行目が 'autostart: enabled' であるべき"
        );
    }
}

/// AC-DDM-09 シナリオ B: daemon 未起動 + autostart 未登録で
///   stdout に "daemon: not running" + "autostart: disabled" が出力され、
///   exit 0 であること。
///
/// REQ-DDM-012 §設計原則（確認できない状態も結果として出力する）
#[test]
#[serial]
fn sc_ddm_002_tc003b_ac09_daemon_status_not_running_when_no_daemon() {
    let home = make_home_dir();
    // XDG_RUNTIME_DIR を存在しないパスに向けて daemon を「not running」にする
    let fake_xdg = TempDir::new().expect("tempdir");
    // sock ファイルを作らないことで「not running」を再現

    let output = Command::cargo_bin("shikomi")
        .expect("shikomi binary")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", fake_xdg.path())
        .args(["daemon", "status"])
        .output()
        .expect("実行失敗");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "AC-DDM-09: daemon status は daemon 未起動でも exit 0 であるべき。\
         exit={:?} stdout={stdout} stderr={stderr}",
        output.status.code()
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 2, "AC-DDM-09: stdout は 2 行以上であるべき");
    assert_eq!(
        lines[0], "daemon: not running",
        "AC-DDM-09: daemon 未起動時の 1 行目は 'daemon: not running' であるべき"
    );
    assert_eq!(
        lines[1], "autostart: disabled",
        "AC-DDM-09: autostart 未登録時の 2 行目は 'autostart: disabled' であるべき"
    );
}

// ---------------------------------------------------------------------------
// SC-DDM-002-TC-004: AC-DDM-10 — install 冪等性（2 回連続で exit 0）
// ---------------------------------------------------------------------------

/// AC-DDM-10: `shikomi daemon install` を 2 回連続実行しても
///   2 回目も exit 0 で成功し、自動起動ファイルが依然として存在すること。
///
/// 設計根拠: docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md §AC-DDM-10
/// REQ-DDM-010 §設計原則（冪等性）
#[test]
#[serial]
fn sc_ddm_002_tc004_ac10_daemon_install_idempotent() {
    let home = make_home_dir();

    // 1 回目
    run_daemon_cmd(home.path(), &["install"])
        .success()
        .stdout(predicate::str::contains("shikomi-daemon autostart enabled"));

    // 2 回目（冪等性確認）
    run_daemon_cmd(home.path(), &["install"])
        .success()
        .stdout(predicate::str::contains("shikomi-daemon autostart enabled"))
        .stderr(predicate::str::contains("error:").not());

    // ファイルが依然として存在すること
    if let Some(autostart_path) = expected_autostart_file(home.path()) {
        assert!(
            autostart_path.exists(),
            "AC-DDM-10: 2 回目 install 後も自動起動ファイルが存在するべき"
        );
    }
}

// ---------------------------------------------------------------------------
// 追加 IT: --quiet フラグで成功メッセージが抑制されること（IT-128 相当）
// ---------------------------------------------------------------------------

/// `--quiet` フラグ指定時に install の成功メッセージが stdout に出力されないこと。
/// tracing は stderr に出力される（--quiet は stdout のみ制御）。
///
/// 設計根拠: docs/features/daemon-default-mode/autostart/test-design/integration.md §TC-IT-127 相当
/// REQ-DDM-010 / detailed-design §run_daemon_subcommand (quiet=true 分岐)
#[test]
#[serial]
fn tc_it_127_quiet_flag_suppresses_install_success_message() {
    let home = make_home_dir();

    let output = Command::cargo_bin("shikomi")
        .expect("shikomi binary")
        .env("HOME", home.path())
        .args(["daemon", "install", "--quiet"])
        .output()
        .expect("実行失敗");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "--quiet install は exit 0 であるべき。exit={:?} stderr={stderr}",
        output.status.code()
    );
    assert!(
        !stdout.contains("shikomi-daemon autostart enabled"),
        "--quiet 時に stdout に成功メッセージが出力されるべきでない。\nstdout:\n{stdout}"
    );
    // exit 0 で成功しているのに stdout が空（hint も出ない）
    assert!(
        stdout.trim().is_empty(),
        "--quiet 時に stdout は空であるべき。\nstdout:\n{stdout}"
    );
    // stderr に error: が含まれないこと
    assert!(
        !stderr.contains("error:"),
        "--quiet install の stderr に 'error:' が含まれるべきでない。\nstderr:\n{stderr}"
    );
}

/// `--quiet` フラグ指定時に uninstall の成功メッセージが stdout に出力されないこと。
///
/// REQ-DDM-011 / detailed-design §run_daemon_subcommand (quiet=true 分岐)
#[test]
#[serial]
fn tc_it_128_quiet_flag_suppresses_uninstall_success_message() {
    let home = make_home_dir();

    // 前提: install 済み
    run_daemon_cmd(home.path(), &["install"]).success();

    let output = Command::cargo_bin("shikomi")
        .expect("shikomi binary")
        .env("HOME", home.path())
        .args(["daemon", "uninstall", "--quiet"])
        .output()
        .expect("実行失敗");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "--quiet uninstall は exit 0 であるべき。exit={:?} stderr={stderr}",
        output.status.code()
    );
    assert!(
        !stdout.contains("shikomi-daemon autostart disabled"),
        "--quiet 時に stdout に成功メッセージが出力されるべきでない。\nstdout:\n{stdout}"
    );
    assert!(
        stdout.trim().is_empty(),
        "--quiet 時に stdout は空であるべき。\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// 追加 IT: uninstall 冪等性（未登録でも exit 0）
// ---------------------------------------------------------------------------

/// `shikomi daemon uninstall` を未登録状態から実行しても exit 0 で成功すること（冪等性）。
///
/// REQ-DDM-011 §設計原則（未登録の場合は冪等、成功扱い）
#[test]
#[serial]
fn tc_it_128b_daemon_uninstall_idempotent_when_not_registered() {
    let home = make_home_dir();
    // install せずに直接 uninstall
    run_daemon_cmd(home.path(), &["uninstall"])
        .success()
        .stdout(predicate::str::contains(
            "shikomi-daemon autostart disabled",
        ))
        .stderr(predicate::str::contains("error:").not());
}

// ---------------------------------------------------------------------------
// 追加 IT: install → status → uninstall → status のサイクル検証
// ---------------------------------------------------------------------------

/// install → status(no-ipc) → uninstall → status(no-ipc) のサイクルで
///   autostart 状態が正しく反映されること。
///
/// REQ-DDM-010〜012 / AC-DDM-07〜09
#[test]
#[serial]
fn tc_it_132_status_reflects_install_uninstall_cycle() {
    let home = make_home_dir();

    // 初期状態: disabled
    {
        let output = Command::cargo_bin("shikomi")
            .expect("shikomi binary")
            .env("HOME", home.path())
            .args(["daemon", "status", "--no-ipc"])
            .output()
            .expect("実行失敗");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("autostart: disabled"),
            "初期状態は disabled であるべき。\nstdout:\n{stdout}"
        );
    }

    // install 後: enabled
    {
        run_daemon_cmd(home.path(), &["install"]).success();
        let output = Command::cargo_bin("shikomi")
            .expect("shikomi binary")
            .env("HOME", home.path())
            .args(["daemon", "status", "--no-ipc"])
            .output()
            .expect("実行失敗");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("autostart: enabled"),
            "install 後は enabled であるべき。\nstdout:\n{stdout}"
        );
    }

    // uninstall 後: disabled に戻る
    {
        run_daemon_cmd(home.path(), &["uninstall"]).success();
        let output = Command::cargo_bin("shikomi")
            .expect("shikomi binary")
            .env("HOME", home.path())
            .args(["daemon", "status", "--no-ipc"])
            .output()
            .expect("実行失敗");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("autostart: disabled"),
            "uninstall 後は disabled に戻るべき。\nstdout:\n{stdout}"
        );
    }
}
