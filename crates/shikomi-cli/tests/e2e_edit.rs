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

// -------------------------------------------------------------------
// REG-E2E-edit-secret-*: ペテルギウス指摘①（secret echo leak）回帰テスト
//
// 実装根拠: `lib.rs::run_edit`（commit 87e1b66）で既存 kind を事前 load し、
// Secret なら `read_password` / Text なら `read_line` に切り替える設計。
// assert_cmd からは TTY を提供できないため `is_stdin_tty() == false` 経路で
// secret 値が stdout/stderr に漏れないこと + Fail Fast 経路を検証する。
// -------------------------------------------------------------------

/// stdin pipe された値を edit で新しい secret 値としてセットできることを、
/// secret 値が stdout / stderr のどこにも現れないこと（REQ-CLI-007）と合わせて検証。
#[test]
fn reg_e2e_edit_secret_record_via_stdin_never_echoes_and_roundtrips_as_masked() {
    // 前提: secret レコードを 1 件 add
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    let out = shikomi(dir.path())
        .args(["add", "--kind", "secret", "--label", "S", "--stdin"])
        .write_stdin("OLD_SECRET_TEST_VALUE\n")
        .assert()
        .success();
    let uuid = String::from_utf8_lossy(&out.get_output().stdout)
        .lines()
        .find(|l| l.starts_with("added: "))
        .unwrap()
        .trim_start_matches("added: ")
        .trim()
        .to_owned();

    // edit --stdin で新値を注入
    let out = shikomi(dir.path())
        .args(["edit", "--id", &uuid, "--stdin"])
        .write_stdin("NEW_SECRET_TEST_VALUE\n")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    // stdout/stderr に旧値も新値も一切出ない
    assert!(!stdout.contains("OLD_SECRET_TEST_VALUE"));
    assert!(!stdout.contains("NEW_SECRET_TEST_VALUE"));
    assert!(!stderr.contains("OLD_SECRET_TEST_VALUE"));
    assert!(!stderr.contains("NEW_SECRET_TEST_VALUE"));
    assert!(stdout.contains("updated: "));

    // ラウンドトリップ: list で Masked + 新旧どちらの secret 値も含まれない
    shikomi(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("****")
                .and(predicate::str::contains("OLD_SECRET_TEST_VALUE").not())
                .and(predicate::str::contains("NEW_SECRET_TEST_VALUE").not()),
        );
}

/// 既存 kind が Secret のとき `--value` 直接指定すると shell 履歴警告が stderr に出る。
/// 実装は `run_edit` で事前 load → `existing_kind == Secret && args.value.is_some()` で
/// `render_shell_history_warning` を呼ぶ（ペテルギウス指摘③の YAGNI 解消で `locale`
/// 引数が実際に使われる経路となる）。
#[test]
fn reg_e2e_edit_secret_record_with_value_flag_emits_shell_history_warning_on_stderr() {
    // 前提: secret レコードを add（stdin 経由で初期値は漏洩させない）
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    let out = shikomi(dir.path())
        .args(["add", "--kind", "secret", "--label", "S", "--stdin"])
        .write_stdin("INIT_SECRET\n")
        .assert()
        .success();
    let uuid = String::from_utf8_lossy(&out.get_output().stdout)
        .lines()
        .find(|l| l.starts_with("added: "))
        .unwrap()
        .trim_start_matches("added: ")
        .trim()
        .to_owned();

    let out = shikomi(dir.path())
        .args(["edit", "--id", &uuid, "--value", "P"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("warning") && stderr.to_lowercase().contains("shell"),
        "stderr should emit shell-history warning for secret+--value: {stderr}"
    );
}

/// 既存 kind が Text のときは `--value` 指定でも shell 履歴警告は出さない。
/// （shell 履歴リスクは secret のみに該当する設計）。
#[test]
fn reg_e2e_edit_text_record_with_value_flag_does_not_emit_shell_history_warning() {
    let (dir, uuid) = setup_vault_with_record("L", "V");
    let out = shikomi(dir.path())
        .args(["edit", "--id", &uuid, "--value", "V2"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        !stderr.to_lowercase().contains("shell"),
        "Text kind edit must not emit shell-history warning: {stderr}"
    );
}

// -------------------------------------------------------------------
// REG-E2E-edit-failfast-*: ペテルギウス指摘②（Fail Fast 違反）の
// 事前 load 経路における回帰テスト。
//
// `run_edit` は `--value` / `--stdin` 指定時のみ事前 load する。この経路での
// VaultNotInitialized / EncryptionUnsupported / RecordNotFound が正しく
// Fail Fast で exit するかを確認する（既存 TC-E2E-024/041/052 は `--label` のみ
// の経路＝事前 load しない経路をカバー）。
// -------------------------------------------------------------------

/// `--value` 指定 + vault 未初期化 → 事前 load の exists() チェックで
/// VaultNotInitialized → exit 1
#[test]
fn reg_e2e_edit_with_value_on_uninitialized_vault_exits_with_code_one() {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    let out = shikomi(dir.path())
        .args([
            "edit",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--value",
            "X",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("vault not initialized"),
        "stderr should mention vault not initialized: {stderr}"
    );
}

/// `--value` 指定 + 存在しない id（vault は初期化済み）→ 事前 load の
/// find_record で RecordNotFound → exit 1
#[test]
fn reg_e2e_edit_with_value_on_missing_id_exits_with_code_one() {
    let (dir, _uuid) = setup_vault_with_record("L", "V");
    let out = shikomi(dir.path())
        .args([
            "edit",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--value",
            "X",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("record not found"),
        "stderr should contain 'record not found': {stderr}"
    );
}

/// `--value` 指定 + 暗号化 vault → 事前 load の protection_mode で
/// EncryptionUnsupported → exit 3（BUG-001 と同じ契約、別経路で再確認）
#[test]
fn reg_e2e_edit_with_value_on_encrypted_vault_exits_with_code_three() {
    use common::fixtures;
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    fixtures::create_encrypted_vault(dir.path()).unwrap();
    let out = shikomi(dir.path())
        .args([
            "edit",
            "--id",
            "018f0000-0000-7000-8000-000000000000",
            "--value",
            "X",
        ])
        .assert()
        .code(3);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("encrypt"),
        "stderr should mention encryption: {stderr}"
    );
}
