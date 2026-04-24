//! E2E テスト — `shikomi remove`
//!
//! 対応 REQ: REQ-CLI-004, REQ-CLI-011, MSG-CLI-105
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-030〜033
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §6`
//!
//! NOTE: TC-E2E-030（擬似 TTY 必要）は `expectrl`/`rexpect` 依存を避けるため
//! `#[ignore]` でスキップする（設計書 §6 の注記通り）。TC-E2E-031（非 TTY + --yes 無し）
//! を主検証とする。

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

fn setup_vault_with_record() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
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
// TC-E2E-030: remove — 擬似 TTY での 'y' 確認（CI で動作しない可能性のため ignore）
// -------------------------------------------------------------------

#[test]
#[ignore = "pseudo-tty (expectrl/rexpect) not used; main assertion is TC-E2E-031"]
fn tc_e2e_030_remove_pseudo_tty_confirmation_y_deletes_record() {
    // 擬似 TTY 実装は Phase 2 で検討。主受入基準は TC-E2E-031 で満たす。
}

// -------------------------------------------------------------------
// TC-E2E-031: remove — 非 TTY + --yes 無しで exit 1（主検証）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_031_remove_non_tty_without_yes_refuses_and_exits_one() {
    let (dir, uuid) = setup_vault_with_record();
    let out = shikomi(dir.path())
        .args(["remove", "--id", &uuid])
        // .write_stdin("") でパイプ stdin を接続 → is_terminal() false
        .write_stdin("")
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("--yes") || stderr.contains("non-interactive") || stderr.contains("refusing"),
        "stderr should mention --yes requirement: {stderr}"
    );

    // レコードが残存している（ラウンドトリップ検証）
    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(uuid.as_str()));
}

// -------------------------------------------------------------------
// TC-E2E-032: remove --yes で確認なし削除 → list で不在
// -------------------------------------------------------------------

#[test]
fn tc_e2e_032_remove_with_yes_deletes_record_and_disappears_from_list() {
    let (dir, uuid) = setup_vault_with_record();
    shikomi(dir.path())
        .args(["remove", "--id", &uuid, "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed: ").and(predicate::str::contains(uuid.as_str())));

    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(uuid.as_str()).not());
}

// -------------------------------------------------------------------
// TC-E2E-033: remove --yes で存在しない id → exit 1 + record not found
// -------------------------------------------------------------------

#[test]
fn tc_e2e_033_remove_missing_id_with_yes_reports_record_not_found() {
    let (dir, _uuid) = setup_vault_with_record();
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
    assert!(
        stderr.contains("record not found"),
        "stderr should contain 'record not found': {stderr}"
    );
}
