//! E2E テスト — `shikomi add`
//!
//! 対応 REQ: REQ-CLI-002, REQ-CLI-007, MSG-CLI-050, MSG-CLI-100, MSG-CLI-101
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-010〜015
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §4`

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

use common::tighten_perms_unix;

fn shikomi(dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

fn setup_vault_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    dir
}

// -------------------------------------------------------------------
// TC-E2E-010: add --kind text → list でラウンドトリップ確認
// -------------------------------------------------------------------

#[test]
fn tc_e2e_010_add_text_roundtrips_through_list() {
    let dir = setup_vault_dir();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("added: "));
    let uuid_line = stdout
        .lines()
        .find(|l| l.starts_with("added: "))
        .expect("added line");
    let uuid = uuid_line.trim_start_matches("added: ").trim();
    // UUIDv7 全長 36
    assert_eq!(
        uuid.chars().count(),
        36,
        "uuid should be 36 chars, got {uuid}"
    );

    shikomi(dir.path()).arg("list").assert().success().stdout(
        predicate::str::contains(uuid)
            .and(predicate::str::contains("L"))
            .and(predicate::str::contains("V")),
    );
}

// -------------------------------------------------------------------
// TC-E2E-011: add --kind secret --stdin で secret を stdout / stderr に露出しない
// -------------------------------------------------------------------

#[test]
fn tc_e2e_011_add_secret_stdin_never_echoes_secret_marker() {
    let dir = setup_vault_dir();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "secret", "--label", "S", "--stdin"])
        .write_stdin("SECRET_TEST_VALUE\n")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        !stdout.contains("SECRET_TEST_VALUE"),
        "stdout leaked secret: {stdout}"
    );
    assert!(
        !stderr.contains("SECRET_TEST_VALUE"),
        "stderr leaked secret: {stderr}"
    );

    shikomi(dir.path()).arg("list").assert().success().stdout(
        predicate::str::contains("****").and(predicate::str::contains("SECRET_TEST_VALUE").not()),
    );
}

// -------------------------------------------------------------------
// TC-E2E-012: add --kind secret --value → 警告 + exit 0
// -------------------------------------------------------------------

#[test]
fn tc_e2e_012_add_secret_value_warns_on_stderr_and_exits_zero() {
    let dir = setup_vault_dir();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "secret", "--label", "S", "--value", "P"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stdout.contains("added: "));
    assert!(
        stderr.to_lowercase().contains("warning") && stderr.to_lowercase().contains("shell"),
        "stderr should contain shell-history warning: {stderr}"
    );
    // warning 内に secret 値原文が出ないこと
    assert!(
        !stderr.contains(" P ") && !stderr.ends_with("P\n") && !stderr.contains("value: P"),
        "warning text unexpectedly contained secret value: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-013: add で --value と --stdin 併用は exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_013_add_rejects_value_and_stdin_together() {
    let dir = setup_vault_dir();
    let out = shikomi(dir.path())
        .args([
            "add", "--kind", "text", "--label", "L", "--value", "V", "--stdin",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("error:"));
    assert!(
        stderr.contains("--value") && stderr.contains("--stdin"),
        "stderr should mention both flags: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-014: add で --value / --stdin ともに未指定
// -------------------------------------------------------------------

#[test]
fn tc_e2e_014_add_without_value_or_stdin_fails_with_user_error() {
    let dir = setup_vault_dir();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L"])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("error:"));
    assert!(
        stderr.contains("--value") || stderr.contains("--stdin") || stderr.contains("required"),
        "stderr should mention missing value: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-015: add — 不正ラベル（空文字）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_015_add_empty_label_reports_invalid_label() {
    let dir = setup_vault_dir();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "", "--value", "V"])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("invalid label") || stderr.contains("label"),
        "stderr should mention invalid label: {stderr}"
    );
}
