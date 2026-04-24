//! E2E テスト — `shikomi list`
//!
//! 対応 REQ: REQ-CLI-001, REQ-CLI-007
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-001 / TC-E2E-002 / TC-E2E-003
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §3`

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

use common::tighten_perms_unix;

/// 共通: `shikomi` バイナリに `--vault-dir <dir>` を付けた Command を返す。
/// env は明示的に除去して残留影響を避ける。
fn shikomi(dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

fn setup_vault_dir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    dir
}

// -------------------------------------------------------------------
// TC-E2E-001: list — 空 vault（vault 初期化後に 0 件）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_001_list_empty_vault_prints_no_records_with_exit_zero() {
    let dir = setup_vault_dir();
    // add → remove で 0 件 vault を作る……の代わりに、`add` だけ実行して
    // 直後に `remove --yes` は E2E_remove で扱うので、ここでは add で初期化だけして
    // 1 件ある状態での空ケースは境界値 TC-E2E-002 の観点になってしまう。
    // そのため、先に 1 件 add → id を取って remove で消してから list を呼ぶ。
    let add_out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&add_out.get_output().stdout).to_string();
    let uuid = extract_added_uuid(&stdout).expect("added uuid");
    shikomi(dir.path())
        .args(["remove", "--id", &uuid, "--yes"])
        .assert()
        .success();

    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("no records"));
}

// -------------------------------------------------------------------
// TC-E2E-002: list — 1 件（Text のみ）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_002_list_single_text_record_renders_value_with_header() {
    let dir = setup_vault_dir();
    shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L1", "--value", "V1"])
        .assert()
        .success();

    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("text")
                .and(predicate::str::contains("L1"))
                .and(predicate::str::contains("V1"))
                .and(predicate::str::contains("ID"))
                .and(predicate::str::contains("KIND"))
                .and(predicate::str::contains("LABEL"))
                .and(predicate::str::contains("VALUE")),
        );
}

// -------------------------------------------------------------------
// TC-E2E-003: list — Text + Secret 混在、Secret はマスク表示
// -------------------------------------------------------------------

#[test]
fn tc_e2e_003_list_mixed_records_masks_secret_and_preserves_text() {
    let dir = setup_vault_dir();
    // text: PUBLIC_VAL
    shikomi(dir.path())
        .args([
            "add", "--kind", "text", "--label", "T1", "--value", "PUBLIC_VAL",
        ])
        .assert()
        .success();
    // secret: SECRET_TEST_VALUE via stdin
    shikomi(dir.path())
        .args(["add", "--kind", "secret", "--label", "S1", "--stdin"])
        .write_stdin("SECRET_TEST_VALUE\n")
        .assert()
        .success();

    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("PUBLIC_VAL")
                .and(predicate::str::contains("****"))
                .and(predicate::str::contains("SECRET_TEST_VALUE").not()),
        )
        .stderr(predicate::str::contains("SECRET_TEST_VALUE").not());
}

// -------------------------------------------------------------------
// Helper: `added: <uuid>` から UUID を抽出
// -------------------------------------------------------------------

fn extract_added_uuid(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("added: ") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}
