//! E2E テスト — `shikomi edit`
//!
//! 対応 REQ: REQ-CLI-003, MSG-CLI-100, MSG-CLI-102, MSG-CLI-106
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-020〜025
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §5`

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

fn setup_vault_with_record(label: &str, value: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", label, "--value", value])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let uuid = stdout
        .lines()
        .find(|l| l.starts_with("added: "))
        .expect("added line")
        .trim_start_matches("added: ")
        .trim()
        .to_owned();
    (dir, uuid)
}

// -------------------------------------------------------------------
// TC-E2E-020: edit --label で label のみ更新、list で反映確認
// -------------------------------------------------------------------

#[test]
fn tc_e2e_020_edit_label_only_reflects_in_list() {
    let (dir, uuid) = setup_vault_with_record("OLD", "V");
    shikomi(dir.path())
        .args(["edit", "--id", &uuid, "--label", "NEW_L"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated: ").and(predicate::str::contains(uuid.as_str())));

    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("NEW_L"));
}

// -------------------------------------------------------------------
// TC-E2E-021: edit で --value と --stdin 併用は exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_021_edit_rejects_value_and_stdin_together() {
    let (dir, uuid) = setup_vault_with_record("L", "V");
    let out = shikomi(dir.path())
        .args(["edit", "--id", &uuid, "--value", "X", "--stdin"])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("--value") && stderr.contains("--stdin"));
}

// -------------------------------------------------------------------
// TC-E2E-022: edit フラグ全未指定 → 更新内容なしで exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_022_edit_without_any_update_field_fails() {
    let (dir, uuid) = setup_vault_with_record("L", "V");
    let out = shikomi(dir.path())
        .args(["edit", "--id", &uuid])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("--label") || stderr.contains("required") || stderr.contains("at least"),
        "stderr should mention at-least-one-required: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-023: edit — 不正 UUID
// -------------------------------------------------------------------

#[test]
fn tc_e2e_023_edit_rejects_invalid_uuid() {
    let (dir, _uuid) = setup_vault_with_record("L", "V");
    let out = shikomi(dir.path())
        .args(["edit", "--id", "not-a-uuid", "--label", "L"])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("invalid record id") || stderr.contains("invalid"),
        "stderr should mention invalid id: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-024: edit — 存在しない id
// -------------------------------------------------------------------

#[test]
fn tc_e2e_024_edit_on_missing_id_reports_record_not_found() {
    let (dir, _uuid) = setup_vault_with_record("L", "V");
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
    assert!(
        stderr.contains("record not found"),
        "stderr should contain 'record not found': {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-025: edit --kind は clap レベルで unknown argument
// -------------------------------------------------------------------

#[test]
fn tc_e2e_025_edit_kind_flag_is_unknown_argument() {
    let (dir, uuid) = setup_vault_with_record("L", "V");
    let out = shikomi(dir.path())
        .args(["edit", "--id", &uuid, "--kind", "secret"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    // clap が unexpected argument / unknown flag / --kind を報告する
    assert!(
        stderr.contains("--kind") || stderr.contains("unexpected") || stderr.contains("unknown"),
        "stderr should mention --kind rejection: {stderr}"
    );
}
