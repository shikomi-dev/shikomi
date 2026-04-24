//! E2E テスト — vault 未初期化
//!
//! 対応 REQ: REQ-CLI-010, MSG-CLI-104, MSG-CLI-005
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-050〜052
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §8`

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

fn empty_vault_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    dir
}

// -------------------------------------------------------------------
// TC-E2E-050: vault 未初期化で list → exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_050_list_on_uninitialized_vault_exits_with_code_one() {
    let dir = empty_vault_dir();
    let out = shikomi(dir.path()).arg("list").assert().code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("vault not initialized") || stderr.contains("not initialized"),
        "stderr should contain 'vault not initialized': {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-051: vault 未初期化で add → 自動初期化 + added: uuid
// -------------------------------------------------------------------

#[test]
fn tc_e2e_051_add_on_uninitialized_vault_auto_initializes() {
    let dir = empty_vault_dir();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        stdout.contains("initialized plaintext vault"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("added: "), "stdout: {stdout}");

    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("L").and(predicate::str::contains("V")));
}

// -------------------------------------------------------------------
// TC-E2E-052: vault 未初期化で edit / remove → exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_052_edit_on_uninitialized_vault_exits_with_code_one() {
    let dir = empty_vault_dir();
    let out = shikomi(dir.path())
        .args([
            "edit",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--label",
            "L",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("not initialized"), "stderr: {stderr}");
}

#[test]
fn tc_e2e_052_remove_on_uninitialized_vault_exits_with_code_one() {
    let dir = empty_vault_dir();
    let out = shikomi(dir.path())
        .args([
            "remove",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--yes",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("not initialized"), "stderr: {stderr}");
}
