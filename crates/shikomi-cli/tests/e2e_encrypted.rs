//! E2E テスト — 暗号化 vault（Fail Fast）
//!
//! 対応 REQ: REQ-CLI-009, MSG-CLI-103
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-040 / TC-E2E-041
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §7`

mod common;

use assert_cmd::Command;
use tempfile::TempDir;

use common::{fixtures, tighten_perms_unix};

fn shikomi(dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

fn setup_encrypted_vault() -> TempDir {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    fixtures::create_encrypted_vault(dir.path()).expect("create encrypted vault");
    dir
}

// -------------------------------------------------------------------
// TC-E2E-040: 暗号化 vault に対する `list` → exit 3 + MSG-CLI-103
// -------------------------------------------------------------------

#[test]
fn tc_e2e_040_list_on_encrypted_vault_exits_with_code_three() {
    let dir = setup_encrypted_vault();
    let out = shikomi(dir.path()).arg("list").assert().code(3);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("encrypt") || stderr.contains("encryption"),
        "stderr should mention encryption: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-041: 暗号化 vault に対する add / edit / remove → exit 3
// -------------------------------------------------------------------

#[test]
fn tc_e2e_041_add_on_encrypted_vault_exits_with_code_three() {
    let dir = setup_encrypted_vault();
    let out = shikomi(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
        .assert()
        .code(3);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("encrypt") || stderr.contains("encryption"),
        "stderr should mention encryption: {stderr}"
    );
}

#[test]
fn tc_e2e_041_edit_on_encrypted_vault_exits_with_code_three() {
    let dir = setup_encrypted_vault();
    let out = shikomi(dir.path())
        .args([
            "edit",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--label",
            "L",
        ])
        .assert()
        .code(3);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("encrypt") || stderr.contains("encryption"),
        "stderr should mention encryption: {stderr}"
    );
}

#[test]
fn tc_e2e_041_remove_on_encrypted_vault_exits_with_code_three() {
    let dir = setup_encrypted_vault();
    let out = shikomi(dir.path())
        .args([
            "remove",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--yes",
        ])
        .assert()
        .code(3);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("encrypt") || stderr.contains("encryption"),
        "stderr should mention encryption: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-040 補: vault 内容が触られていないこと（副作用ゼロ）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_040b_encrypted_vault_db_is_unchanged_after_list_attempt() {
    let dir = setup_encrypted_vault();
    let vault_db = dir.path().join("vault.db");
    let before = std::fs::read(&vault_db).unwrap();

    // 結果の exit code はバグの有無で変わりうるが、vault 内容は変わらない契約。
    let _ = shikomi(dir.path()).arg("list").output();

    let after = std::fs::read(&vault_db).unwrap();
    assert_eq!(before, after, "encrypted vault.db should not be modified");
}
