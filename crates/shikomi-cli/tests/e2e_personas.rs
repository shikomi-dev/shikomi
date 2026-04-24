//! E2E テスト — ペルソナシナリオ統合
//!
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-100 (SCN-A) / TC-E2E-101 (SCN-B) / TC-E2E-102 (SCN-C)
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §11`
//!         `docs/features/cli-vault-commands/requirements-analysis.md §ペルソナ`

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

fn extract_added_uuid(stdout: &str) -> String {
    stdout
        .lines()
        .find(|l| l.starts_with("added: "))
        .expect("added line")
        .trim_start_matches("added: ")
        .trim()
        .to_owned()
}

fn fresh_dir() -> TempDir {
    let d = TempDir::new().unwrap();
    tighten_perms_unix(d.path());
    d
}

// -------------------------------------------------------------------
// TC-E2E-100: SCN-A 山田美咲ライフサイクル統合
// -------------------------------------------------------------------

#[test]
fn tc_e2e_100_scn_a_fullstack_engineer_record_lifecycle() {
    let dir = fresh_dir();

    // (1) add text
    let out1 = shikomi(dir.path())
        .args([
            "add",
            "--kind",
            "text",
            "--label",
            "SSH: prod",
            "--value",
            "ssh -J bastion prod",
        ])
        .assert()
        .success();
    let text_uuid = extract_added_uuid(&String::from_utf8_lossy(&out1.get_output().stdout));

    // (2) add secret via stdin
    let out2 = shikomi(dir.path())
        .args(["add", "--kind", "secret", "--label", "AWS_KEY", "--stdin"])
        .write_stdin("SECRET_TEST_VALUE\n")
        .assert()
        .success();
    let secret_uuid = extract_added_uuid(&String::from_utf8_lossy(&out2.get_output().stdout));
    assert!(!String::from_utf8_lossy(&out2.get_output().stdout).contains("SECRET_TEST_VALUE"));
    assert!(!String::from_utf8_lossy(&out2.get_output().stderr).contains("SECRET_TEST_VALUE"));

    // (3) list で 2 件確認、Secret マスク、SECRET_TEST_VALUE 不在
    shikomi(dir.path()).arg("list").assert().success().stdout(
        predicate::str::contains(&text_uuid)
            .and(predicate::str::contains(&secret_uuid))
            .and(predicate::str::contains("****"))
            .and(predicate::str::contains("SECRET_TEST_VALUE").not()),
    );

    // (4) edit label 更新
    shikomi(dir.path())
        .args(["edit", "--id", &text_uuid, "--label", "SSH: prod-v2"])
        .assert()
        .success();

    // (5) list で更新反映確認
    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("SSH: prod-v2"));

    // (6) remove secret_uuid
    shikomi(dir.path())
        .args(["remove", "--id", &secret_uuid, "--yes"])
        .assert()
        .success();

    // (7) list で 1 件残存、SECRET_TEST_VALUE 不在
    shikomi(dir.path()).arg("list").assert().success().stdout(
        predicate::str::contains(&text_uuid)
            .and(predicate::str::contains(&secret_uuid).not())
            .and(predicate::str::contains("SECRET_TEST_VALUE").not()),
    );
}

// -------------------------------------------------------------------
// TC-E2E-101: SCN-B 田中俊介初心者保護（日本語併記 + 非 TTY 削除拒否）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_101_scn_b_non_interactive_removal_is_refused_with_japanese_hint() {
    let dir = fresh_dir();
    // セットアップ: add 1 件
    let out = shikomi(dir.path())
        .args([
            "add",
            "--kind",
            "text",
            "--label",
            "TAN",
            "--value",
            "田中のメモ",
        ])
        .assert()
        .success();
    let uuid = extract_added_uuid(&String::from_utf8_lossy(&out.get_output().stdout));

    // (1) 日本語併記 list 確認
    let mut cmd = Command::cargo_bin("shikomi").unwrap();
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env("LANG", "ja_JP.UTF-8")
        .args(["--vault-dir", dir.path().to_str().unwrap(), "list"])
        .assert()
        .success();

    // (2) 非 TTY + --yes 無しで remove → 拒否 + 日本語ヒント
    let mut cmd = Command::cargo_bin("shikomi").unwrap();
    let out = cmd
        .env_remove("SHIKOMI_VAULT_DIR")
        .env("LANG", "ja_JP.UTF-8")
        .args([
            "--vault-dir",
            dir.path().to_str().unwrap(),
            "remove",
            "--id",
            &uuid,
        ])
        .write_stdin("")
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("--yes"), "stderr: {stderr}");
    assert!(
        stderr.contains("再実行") || stderr.contains("確認"),
        "stderr should contain Japanese hint: {stderr}"
    );

    // 続けて list で当該レコード残存確認
    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(uuid.as_str()));
}

// -------------------------------------------------------------------
// TC-E2E-102: SCN-C 自己記述性
// -------------------------------------------------------------------

#[test]
fn tc_e2e_102_scn_c_help_and_version_messages_are_well_formed() {
    // (a) shikomi --help → 全サブコマンド一覧
    let out = Command::cargo_bin("shikomi")
        .unwrap()
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for sub in ["list", "add", "edit", "remove"] {
        assert!(
            stdout.contains(sub),
            "--help missing subcommand '{sub}': {stdout}"
        );
    }

    // (b) --version
    let out = Command::cargo_bin("shikomi")
        .unwrap()
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--version")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let pkg_version = env!("CARGO_PKG_VERSION");
    assert!(
        stdout.contains(pkg_version),
        "--version should match CARGO_PKG_VERSION={pkg_version}: {stdout}"
    );

    // (c) add --help → --kind / --label / --value / --stdin
    let out = Command::cargo_bin("shikomi")
        .unwrap()
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .args(["add", "--help"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for flag in ["--kind", "--label", "--value", "--stdin"] {
        assert!(
            stdout.contains(flag),
            "add --help missing flag '{flag}': {stdout}"
        );
    }

    // (d) edit --help → --kind は含まない（Phase 1 スコープ外）
    let out = Command::cargo_bin("shikomi")
        .unwrap()
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .args(["edit", "--help"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        !stdout.contains("--kind"),
        "edit --help MUST NOT contain --kind (Phase 1 scope): {stdout}"
    );
}
