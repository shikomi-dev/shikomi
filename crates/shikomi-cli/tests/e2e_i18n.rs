//! E2E テスト — i18n（英日 2 段 / `LANG=C` 英語のみ）
//!
//! 対応 REQ: REQ-CLI-008
//! 対応 Issue: #20
//! 対応 TC: TC-E2E-070 / TC-E2E-071
//! 設計書: `docs/features/cli-vault-commands/test-design/e2e.md §10`

mod common;

use assert_cmd::Command;
use tempfile::TempDir;

use common::tighten_perms_unix;

fn empty_vault_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    tighten_perms_unix(dir.path());
    dir
}

fn shikomi_with_lang(dir: &std::path::Path, lang: &str) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env("LANG", lang)
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

// -------------------------------------------------------------------
// TC-E2E-070: LANG=ja_JP.UTF-8 → 英日 2 段
// -------------------------------------------------------------------

#[test]
fn tc_e2e_070_japanese_locale_produces_english_and_japanese_lines() {
    let dir = empty_vault_dir();
    // vault 未初期化 → エラーで両 locale の文言を誘発
    let out = shikomi_with_lang(dir.path(), "ja_JP.UTF-8")
        .arg("list")
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("vault not initialized"),
        "missing english line: {stderr}"
    );
    assert!(
        stderr.contains("vault が初期化されていません"),
        "missing japanese line: {stderr}"
    );
}

// -------------------------------------------------------------------
// TC-E2E-071: LANG=C → 英語のみ（日本語が一切出ない）
// -------------------------------------------------------------------

#[test]
fn tc_e2e_071_c_locale_produces_english_only() {
    let dir = empty_vault_dir();
    let out = shikomi_with_lang(dir.path(), "C")
        .arg("list")
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(
        stderr.contains("vault not initialized"),
        "missing english line: {stderr}"
    );
    assert!(
        !stderr.contains("vault が"),
        "unexpected japanese line: {stderr}"
    );
    // `LANG=C` 配下では日本語文字（ひらがな・カタカナ）を一切出さない
    let has_hiragana = stderr
        .chars()
        .any(|c| ('\u{3040}'..='\u{309F}').contains(&c));
    assert!(!has_hiragana, "stderr contains hiragana: {stderr}");
}
