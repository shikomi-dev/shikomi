//! E2E テスト — `shikomi export` / `shikomi import`
//!
//! 対応 AC: AC-DP-06 / AC-DP-07 / AC-DP-08 / AC-DP-09 / AC-DP-10
//! 対応 TC: TC-E2E-DP-001〜012
//! 設計書: `docs/features/data-portability/cli/test-design.md §5.1`
//! 対応 Issue: #141

mod common;

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

use common::tighten_perms_unix;

// -------------------------------------------------------------------
// ヘルパー
// -------------------------------------------------------------------

/// vault ディレクトリに `--no-ipc --vault-dir <dir>` を付与した Command を返す。
fn shikomi(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--no-ipc")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

/// TempDir を作成して Unix パーミッションを 0700 に設定する。
fn setup_vault_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    dir
}

/// vault に Text レコードを 1 件追加する。成功が前提。
fn add_text_record(dir: &Path, label: &str, value: &str) {
    shikomi(dir)
        .args(["add", "--kind", "text", "--label", label, "--value", value])
        .assert()
        .success();
}

/// vault に Secret レコードを 1 件追加する。
fn add_secret_record(dir: &Path, label: &str, value: &str) {
    shikomi(dir)
        .args([
            "add", "--kind", "secret", "--label", label, "--value", value,
        ])
        .assert()
        .success();
}

/// vault から export して JSON ファイルパスを返す。成功が前提。
fn export_vault(dir: &Path, out_path: &PathBuf) {
    shikomi(dir)
        .args(["export", "--output", out_path.to_str().unwrap()])
        .assert()
        .success();
}

// -------------------------------------------------------------------
// TC-E2E-DP-001: export 正常 — format_version:1 を含む JSON が書き込まれる
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_001_export_succeeds_and_writes_json_with_format_version_1() {
    let dir = setup_vault_dir();
    add_text_record(dir.path(), "my-label", "my-value");
    let out = dir.path().join("out.json");

    shikomi(dir.path())
        .args(["export", "--output", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("exported 1 record(s)"));

    // ファイルが存在し format_version:1 を含む
    assert!(out.exists(), "export file should exist");
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("\"format_version\": 1"),
        "should contain format_version: 1, got: {content}"
    );
    // tagged union 構造
    assert!(content.contains("\"kind\""), "should contain 'kind' key");
}

// -------------------------------------------------------------------
// TC-E2E-DP-002: export → import ラウンドトリップ — 同一レコードが vault B に存在する
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_002_export_import_roundtrip_same_record_in_target_vault() {
    // vault A
    let dir_a = setup_vault_dir();
    add_text_record(dir_a.path(), "roundtrip-label", "roundtrip-value");
    let out = dir_a.path().join("export.json");
    export_vault(dir_a.path(), &out);

    // vault B（新規）
    let dir_b = setup_vault_dir();
    shikomi(dir_b.path())
        .args(["import", "--input", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported 1 record(s)"));

    // vault B の list でラベルが見える
    shikomi(dir_b.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("roundtrip-label"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-003: --export-secrets なし export → import で MSG-CLI-144 exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_003_import_redacted_secret_record_exits_1_with_msg_cli_144() {
    let dir = setup_vault_dir();
    add_secret_record(dir.path(), "secret-label", "s3cr3t");
    let out = dir.path().join("out.json");
    // --export-secrets なし → Secret が {"kind":"redacted"} になる
    export_vault(dir.path(), &out);

    let dir_b = setup_vault_dir();
    shikomi(dir_b.path())
        .args(["import", "--input", out.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot import record"))
        .stderr(predicate::str::contains("payload is redacted"))
        .stderr(predicate::str::contains("re-export"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-004: import --on-conflict skip — 衝突レコードをスキップする
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_004_import_on_conflict_skip_skips_conflicting_records() {
    let dir = setup_vault_dir();
    // A, B の 2 件追加
    add_text_record(dir.path(), "label-a", "value-a");
    add_text_record(dir.path(), "label-b", "value-b");
    let out = dir.path().join("export.json");
    export_vault(dir.path(), &out);

    // 同じ vault に import --on-conflict skip（A, B は衝突のためスキップ）
    shikomi(dir.path())
        .args([
            "import",
            "--input",
            out.to_str().unwrap(),
            "--on-conflict",
            "skip",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped 2"))
        .stdout(predicate::str::contains("imported 0 record(s)"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-005: 同一ファイルを 2 回 import — 2 回目に MSG-CLI-142 exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_005_second_import_all_conflict_exits_1_with_msg_cli_142() {
    let dir_a = setup_vault_dir();
    add_text_record(dir_a.path(), "conflict-label", "val");
    let out = dir_a.path().join("export.json");
    export_vault(dir_a.path(), &out);

    let dir_b = setup_vault_dir();
    // 1 回目: 成功
    shikomi(dir_b.path())
        .args(["import", "--input", out.to_str().unwrap()])
        .assert()
        .success();

    // 2 回目: 全件衝突 → MSG-CLI-142
    shikomi(dir_b.path())
        .args(["import", "--input", out.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("import conflict"))
        .stderr(predicate::str::contains("already exist in vault"))
        .stderr(predicate::str::contains("--on-conflict skip"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-006: vault ロック済み → export で MSG-CLI-140 exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_006_export_on_locked_vault_exits_1_with_msg_cli_140() {
    let dir = setup_vault_dir();
    // 暗号化 vault を作成（ロック済み）
    common::fixtures::create_encrypted_vault(dir.path()).expect("create_encrypted_vault");
    let out = dir.path().join("out.json");

    shikomi(dir.path())
        .args(["export", "--output", out.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("vault is locked"))
        .stderr(predicate::str::contains("unlock"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-007: --force なし + 出力先ファイル既存 → MSG-CLI-141 exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_007_export_existing_file_without_force_exits_1_with_msg_cli_141() {
    let dir = setup_vault_dir();
    add_text_record(dir.path(), "lbl", "val");
    let out = dir.path().join("existing.json");
    // 先に出力先ファイルを作成しておく
    std::fs::write(&out, "{}").unwrap();

    shikomi(dir.path())
        .args(["export", "--output", out.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "export output file already exists",
        ))
        .stderr(predicate::str::contains("--force"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-008: --export-secrets + --quiet でも MSG-CLI-145 が stderr に出る
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_008_export_secrets_warning_not_suppressed_by_quiet() {
    let dir = setup_vault_dir();
    add_secret_record(dir.path(), "sec", "password");
    let out = dir.path().join("out.json");

    shikomi(dir.path())
        .args([
            "--quiet",
            "export",
            "--output",
            out.to_str().unwrap(),
            "--export-secrets",
        ])
        .assert()
        .success()
        // --quiet でも MSG-CLI-145 は stderr に出る
        .stderr(predicate::str::contains("warning: --export-secrets is set"))
        // --quiet なので stdout には成功メッセージが出ない
        .stdout(predicate::str::is_empty());
}

// -------------------------------------------------------------------
// TC-E2E-DP-009: import --on-conflict overwrite — 衝突レコードを上書きする
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_009_import_on_conflict_overwrite_replaces_existing_record() {
    let dir = setup_vault_dir();
    add_text_record(dir.path(), "ow-label", "old-value");
    let out = dir.path().join("old.json");
    export_vault(dir.path(), &out);

    // 新しい値で別 vault に "old.json" の old-value を import overwirte
    // 検証: 同じ vault でoverwriteした後 list で overwritten になること
    // まず別の vault で import → list → "old-value" 確認
    let dir_b = setup_vault_dir();
    shikomi(dir_b.path())
        .args(["import", "--input", out.to_str().unwrap()])
        .assert()
        .success();

    // 更新されたファイルを同じ vault B に --on-conflict overwrite で再 import
    shikomi(dir_b.path())
        .args([
            "import",
            "--input",
            out.to_str().unwrap(),
            "--on-conflict",
            "overwrite",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("overwritten 1"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-010: 不正 JSON ファイル → import で MSG-CLI-143 exit 1
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_010_import_broken_json_exits_1_with_msg_cli_143() {
    let dir = setup_vault_dir();
    let broken = dir.path().join("broken.json");
    std::fs::write(&broken, "{not valid json").unwrap();

    shikomi(dir.path())
        .args(["import", "--input", broken.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("failed to parse import file"))
        .stderr(predicate::str::contains("format_version"));
}

// -------------------------------------------------------------------
// TC-E2E-DP-011: --force + 出力先ファイル既存 → 上書き成功
// -------------------------------------------------------------------

#[test]
fn tc_e2e_dp_011_export_with_force_overwrites_existing_file() {
    let dir = setup_vault_dir();
    add_text_record(dir.path(), "lbl", "val");
    let out = dir.path().join("output.json");
    // 旧ファイルを作成しておく
    std::fs::write(&out, "old content").unwrap();

    shikomi(dir.path())
        .args(["export", "--output", out.to_str().unwrap(), "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("exported"));

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("\"format_version\": 1"),
        "should be overwritten with valid JSON"
    );
}

// -------------------------------------------------------------------
// TC-E2E-DP-012: export ファイルのパーミッションが 0600（Unix のみ）
// -------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn tc_e2e_dp_012_export_file_permission_is_0600_on_unix() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = setup_vault_dir();
    add_text_record(dir.path(), "perm-test", "value");
    let out = dir.path().join("perm_test.json");

    shikomi(dir.path())
        .args(["export", "--output", out.to_str().unwrap()])
        .assert()
        .success();

    let mode = std::fs::metadata(&out).unwrap().permissions().mode();
    // 下位 9 ビットが 0o600 = rw------- であること
    assert_eq!(
        mode & 0o777,
        0o600,
        "export file should have 0600 permissions, got {mode:o}"
    );
}
