//! E2E テスト — vault パス優先順位
//!
//! 対応 REQ: REQ-CLI-005
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-060 / 061 / 062
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §9`
//!
//! env を操作するため `#[serial]` でプロセス内の並列衝突を避ける。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use tempfile::TempDir;

use common::tighten_perms_unix;

fn fresh_dir() -> TempDir {
    let d = TempDir::new().unwrap();
    tighten_perms_unix(d.path());
    d
}

fn add_one(dir: &std::path::Path, label: &str, value: &str) -> String {
    let mut cmd = Command::cargo_bin("shikomi").unwrap();
    let out = cmd
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .args([
            "--vault-dir",
            dir.to_str().unwrap(),
            "add",
            "--kind",
            "text",
            "--label",
            label,
            "--value",
            value,
        ])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    stdout
        .lines()
        .find(|l| l.starts_with("added: "))
        .unwrap()
        .trim_start_matches("added: ")
        .trim()
        .to_owned()
}

// -------------------------------------------------------------------
// TC-E2E-060: --vault-dir は env var より優先
// -------------------------------------------------------------------

#[test]
#[serial]
fn tc_e2e_060_flag_overrides_env_var() {
    let dir_a = fresh_dir();
    let dir_b = fresh_dir();
    let _id_a = add_one(dir_a.path(), "A_LABEL", "A_VAL");
    // B を空のままに

    let mut cmd = Command::cargo_bin("shikomi").unwrap();
    cmd.env("SHIKOMI_VAULT_DIR", dir_b.path())
        .env_remove("LANG")
        .args(["--vault-dir", dir_a.path().to_str().unwrap(), "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("A_LABEL"));
}

// -------------------------------------------------------------------
// TC-E2E-061: env var は OS デフォルトより優先
// -------------------------------------------------------------------

#[test]
#[serial]
fn tc_e2e_061_env_var_is_consumed_when_flag_absent() {
    let dir_a = fresh_dir();
    let _id = add_one(dir_a.path(), "E_LABEL", "E_VAL");

    let mut cmd = Command::cargo_bin("shikomi").unwrap();
    cmd.env("SHIKOMI_VAULT_DIR", dir_a.path())
        .env_remove("LANG")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("E_LABEL"));
}

// -------------------------------------------------------------------
// TC-E2E-062: フラグも env もない → OS デフォルト解決に失敗しないこと
// -------------------------------------------------------------------

#[test]
#[serial]
fn tc_e2e_062_os_default_resolution_does_not_crash() {
    // HOME を tempdir に向け、XDG_DATA_HOME も設定して OS デフォルトを安定化。
    let home = fresh_dir();
    let xdg_data_home = home.path().join("share");
    std::fs::create_dir_all(&xdg_data_home).unwrap();

    let mut cmd = Command::cargo_bin("shikomi").unwrap();
    // `--vault-dir` も env もなし → `dirs::data_dir()` 経由で OS デフォルトを解決
    let assert = cmd
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", &xdg_data_home)
        .arg("list")
        .assert();

    // 期待: exit 1（vault 未初期化）または exit 0（空 vault）
    let code = assert.get_output().status.code();
    // 2 (SystemError) / 3 (EncryptionUnsupported) になるとパス解決失敗の兆候
    assert!(
        matches!(code, Some(0) | Some(1)),
        "OS default resolution should succeed; got exit code {code:?}"
    );
}
