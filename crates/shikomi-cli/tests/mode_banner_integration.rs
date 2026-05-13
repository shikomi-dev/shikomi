//! Sub-F mode banner 結合テスト（TC-F-I10a〜d）。
//!
//! ## 責務
//! `shikomi list` が plaintext / encrypted-locked / encrypted-unlocked の 3 状態と
//! `NO_COLOR=1` 環境変数に対して正しいバナーを出力することを結合経路で検証する。
//!
//! ## ユニットテストとの棲み分け
//! `unit.md §5（TC-UT-050〜053）` は `presenter::list::render_list` の pure function
//! テスト（副作用なし）。本ファイルは CLI バイナリ全体の結合経路テスト（exit code /
//! stdout / stderr / env var 伝播を含む）。詳細は integration.md §10.5 参照。
//!
//! ## #[ignore] 規約
//! reason 文字列の必須要素: ① skip 理由 ② 関連ゲート ③ 設計書クロス参照 ④ 解除条件
//!
//! 設計根拠: `docs/features/cli-vault-commands/test-design/integration.md §10.4.6 / §10.6`
//! 対応 Issue: #77

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

use common::tighten_perms_unix;

// ---------------------------------------------------------------------------
// 共通ヘルパー
// ---------------------------------------------------------------------------

/// `shikomi --vault-dir <dir>` ベースの Command を返す。
/// `SHIKOMI_VAULT_DIR` / `LANG` 残留を除去してテスト間干渉を防ぐ。
fn shikomi_with_vault_dir(dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--no-ipc")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

/// 0700 パーミッションの一時ディレクトリを作成し、`add` で平文 vault を初期化して返す。
///
/// `shikomi list` が `VaultNotInitialized` で失敗しないよう `vault.db` を事前生成する。
fn setup_plaintext_vault() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    // 平文 vault を初期化（add が vault.db を自動生成）
    shikomi_with_vault_dir(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
        .assert()
        .success();
    dir
}

// ---------------------------------------------------------------------------
// TC-F-I10a: plaintext vault + `shikomi list` → exit 0 + `[plaintext]` バナー
// 設計根拠: integration.md §10.4.6
// ---------------------------------------------------------------------------

#[test]
fn tc_f_i10a_list_on_plaintext_vault_shows_plaintext_banner() {
    // 前提: DaemonSpawn 不要。plaintext vault（`vault.db` 存在）を用意する。
    let dir = setup_plaintext_vault();

    shikomi_with_vault_dir(dir.path())
        .arg("list")
        .assert()
        .success() // exit 0
        .stdout(predicate::str::contains("[plaintext]")); // EC-F9 / REQ-S16
}

// ---------------------------------------------------------------------------
// TC-F-I10b: encrypted vault (Locked) + `shikomi list` → exit 3 + `[encrypted, locked]`
// 設計根拠: integration.md §10.4.6
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows, covered by Unix CI \
              (test-design integration.md §10.3, \
              unlock condition: port to Windows Console API)"
)]
#[ignore = "requires Sub-F daemon V2 Locked state — Unlock/Lock IPC handlers not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs; \
            Locked state cannot be established without VaultLock IPC handler \
            (test-design integration.md §10.4.6, \
            unlock condition: implement VaultUnlock + VaultLock IPC handlers)"]
fn tc_f_i10b_list_on_locked_encrypted_vault_shows_locked_banner() {
    // DaemonSpawn + Unlock → Lock サイクルで Locked 状態を確立してから
    // `shikomi list` → exit 3 + `[encrypted, locked]` バナーを検証する。
    unimplemented!(
        "TC-F-I10b: requires VaultUnlock + VaultLock IPC handlers; see integration.md §10.4.6"
    );
}

// ---------------------------------------------------------------------------
// TC-F-I10c: encrypted vault (Unlocked) + `shikomi list` → exit 0 + `[encrypted, unlocked]`
// 設計根拠: integration.md §10.4.6
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows, covered by Unix CI \
              (test-design integration.md §10.3, \
              unlock condition: port to Windows Console API)"
)]
#[ignore = "requires Sub-F daemon V2 Unlocked state — VaultUnlock IPC handler not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs; \
            Unlocked state cannot be established without VaultUnlock IPC handler \
            (test-design integration.md §10.4.6, \
            unlock condition: implement VaultUnlock IPC handler)"]
fn tc_f_i10c_list_on_unlocked_encrypted_vault_shows_unlocked_banner() {
    // DaemonSpawn + Unlock で Unlocked 状態を確立してから
    // `shikomi list` → exit 0 + `[encrypted, unlocked]` バナーを検証する。
    unimplemented!("TC-F-I10c: requires VaultUnlock IPC handler; see integration.md §10.4.6");
}

// ---------------------------------------------------------------------------
// TC-F-I10d: plaintext vault + `NO_COLOR=1` → `[plaintext]` 含有かつ ANSI escape なし
// 設計根拠: integration.md §10.4.6
// ---------------------------------------------------------------------------

#[test]
fn tc_f_i10d_list_with_no_color_env_suppresses_ansi_escapes() {
    // 前提: DaemonSpawn 不要。plaintext vault（`vault.db` 存在）を用意する。
    let dir = setup_plaintext_vault();

    let output = shikomi_with_vault_dir(dir.path())
        .env("NO_COLOR", "1") // https://no-color.org — ANSI 出力抑制
        .arg("list")
        .assert()
        .success() // exit 0
        .stdout(predicate::str::contains("[plaintext]")) // EC-F9 / REQ-S16
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(
        !stdout.contains("\x1b["),
        "NO_COLOR=1 のとき stdout に ANSI escape sequence を含んではならない: {:?}",
        stdout
    );
}
