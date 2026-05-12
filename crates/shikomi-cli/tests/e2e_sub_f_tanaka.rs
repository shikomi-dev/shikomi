//! Sub-F (#44) TC-F-E01 — 田中ペルソナ E2E テスト。
//!
//! ## 設計方針
//! - Issue #79 / SSoT: `docs/features/cli-vault-commands/test-design/e2e.md §13`
//!   および `vault-encryption/test-design/sub-f-cli-subcommands/index.md §15.8`
//! - Linux only (`#[cfg(target_os = "linux")]`): expectrl PTY + Unix シグナル全対応
//! - env seam: `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2`（C-40 allowlist、daemon lib.rs §15.9）
//!   で idle TTL を 2 秒に短縮し、Step 4 発火に 3 秒 sleep を使用する
//! - PTY: C-38 stdin パイプ拒否のため、passphrase 入力は `expectrl` 経由
//! - i18n: §13.5 規定通り `LANG=ja_JP.UTF-8` と `LANG=C` の 2 モードで検証
//!
//! ## #[ignore] 規約（vault-persistence/test-design/integration/changelog.md v8.4 準拠）
//! reason 文字列の必須要素:
//! 1. skip 理由  2. 関連ゲート  3. 設計書クロス参照  4. 解除条件
//!
//! ## tempfile 平文記録禁止（OWASP A02）
//! passphrase などの秘密情報は `assert_cmd` の stdin / PTY 経由でのみ渡す。
//! ファイルに書き出すことは絶対禁止。
//!
//! 設計根拠: `docs/features/cli-vault-commands/test-design/e2e.md §13`
//! 対応 Issue: #79

mod common;
mod helpers;

use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[allow(dead_code)] // `#[cfg(target_os = "linux")]` テスト外では未使用
fn shikomi(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin shikomi");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

// -----------------------------------------------------------------------
// TC-F-E01（English locale）: 田中ペルソナ E2E 7 ステップ（LANG=C）
//
// 設計根拠: e2e.md §13.4 / §15.8
//
// 解除条件:
//   1. daemon V2 IPC `Encrypt` ハンドラ実装（TC-F-I01 unlock 条件）
//      → `shikomi vault encrypt` でプレーン vault を暗号化 vault に変換できること
//   2. daemon V2 IPC `ChangePassword` ハンドラ実装（TC-F-I05 unlock 条件）
//      → Step 6 で `shikomi vault change-password` が成功すること
//   3. `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` env seam が daemon lib.rs で有効
//      → daemon が 2 秒でアイドルロックを実行すること（Issue #79 本 PR で実装）
//   4. encrypted vault fixture（`shikomi vault encrypt` 経由での seed vault 生成）
//      → create_encrypted_vault() は fake fixture のため unlock 不可
// -----------------------------------------------------------------------
#[test]
#[cfg(target_os = "linux")]
#[ignore = "requires: (1) daemon V2 Encrypt IPC handler not yet implemented \
            (sub-f-daemon-v2-handler gate, test-design e2e.md §13, TC-F-I01 unlock condition); \
            (2) daemon V2 ChangePassword IPC handler \
            (sub-f-daemon-v2-handler gate, test-design e2e.md §13, TC-F-I05 unlock condition); \
            (3) real encrypted vault fixture with known passphrase; \
            unlock condition: all TC-F-I01/I04/I05 #[ignore] resolved + seed vault available"]
fn tc_f_e01_english_tanaka_persona_7step_idle_lock() {
    // ステップ 1: daemon を `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` env で起動
    // C-40: debug build 限定 env seam（TC-F-S05/S06 で静的検証済み）
    let vault_dir = TempDir::new().expect("tempdir");

    // TODO(unlock): seed vault の作成（vault encrypt 実装後に差し替え）
    // 現時点: create_encrypted_vault() は fake fixture のため unlock 不可
    // common::fixtures::create_encrypted_vault(vault_dir.path()).expect("fixture");

    #[cfg(debug_assertions)]
    let daemon = helpers::DaemonSpawn::new(vault_dir.path())
        .expect("daemon spawn")
        .with_idle_threshold(2);

    // ステップ 2: shikomi vault unlock（expectrl PTY 経由 passphrase 入力）
    // TODO(unlock): expectrl PTY で passphrase = "test-passphrase" を入力
    // 期待: exit 0 + "vault unlocked"（LANG=C MSG-S03）

    // ステップ 3: shikomi list — exit 0 + [encrypted, unlocked] バナー
    // TODO(unlock): vault unlock 実装後にコメント解除
    // shikomi(vault_dir.path())
    //     .env("LANG", "C")
    //     .envs(daemon.env_args())
    //     .args(["list"])
    //     .assert()
    //     .code(0)
    //     .stdout(predicate::str::contains("[encrypted, unlocked]"));

    // ステップ 4: sleep 3 → shikomi list → exit 3（idle auto-lock by env seam）
    std::thread::sleep(Duration::from_secs(3));
    // TODO(unlock): vault unlock 実装後にコメント解除
    // shikomi(vault_dir.path())
    //     .env("LANG", "C")
    //     .envs(daemon.env_args())
    //     .args(["list"])
    //     .assert()
    //     .code(3)
    //     .stderr(predicate::str::contains("vault unlocked").not());

    // ステップ 5: shikomi vault unlock 再入力 — MSG-S03「vault unlocked」確認
    // TODO(unlock): expectrl PTY

    // ステップ 6: shikomi vault change-password — MSG-S05「master password changed」確認
    // TODO(unlock): expectrl PTY

    // ステップ 7: shikomi list（再 unlock なし、cache 維持確認）
    // TODO(unlock): vault unlock + change-password 実装後にコメント解除
    // shikomi(vault_dir.path())
    //     .env("LANG", "C")
    //     .envs(daemon.env_args())
    //     .args(["list"])
    //     .assert()
    //     .code(0)
    //     .stdout(predicate::str::contains("[encrypted, unlocked]"));

    // Cleanup: daemon に SIGTERM（DaemonSpawn::drop で自動送信）
    #[cfg(debug_assertions)]
    drop(daemon);
    drop(vault_dir);
}

// -----------------------------------------------------------------------
// TC-F-E01（Japanese locale）: 田中ペルソナ E2E 7 ステップ（LANG=ja_JP.UTF-8）
// §13.5 i18n 2 モード検証 — 日本語モードで MSG-S03/S04/S05 文言確認
//
// 設計根拠: e2e.md §13.5
//
// 解除条件: tc_f_e01_english_tanaka_persona_7step_idle_lock と同じ
// -----------------------------------------------------------------------
#[test]
#[cfg(target_os = "linux")]
#[ignore = "requires: (1) daemon V2 Encrypt IPC handler not yet implemented \
            (sub-f-daemon-v2-handler gate, test-design e2e.md §13.5, TC-F-I01 unlock condition); \
            (2) daemon V2 ChangePassword IPC handler \
            (sub-f-daemon-v2-handler gate, test-design e2e.md §13.5, TC-F-I05 unlock condition); \
            (3) known-passphrase encrypted vault fixture; \
            i18n 2モード LANG=ja_JP.UTF-8: MSG-S03/S04/S05 日本語文言確認 (e2e.md §13.5); \
            unlock condition: all TC-F-I01/I04/I05 #[ignore] resolved + seed vault available"]
fn tc_f_e01_japanese_tanaka_persona_7step_idle_lock() {
    // §13.5: LANG=ja_JP.UTF-8 モードで MSG-S03/S04/S05 日本語文言を確認する
    let vault_dir = TempDir::new().expect("tempdir");

    #[cfg(debug_assertions)]
    let daemon = helpers::DaemonSpawn::new(vault_dir.path())
        .expect("daemon spawn")
        .with_idle_threshold(2);

    // ステップ 2: vault unlock — MSG-S03「vault のロックを解除しました」確認
    // ステップ 3: list
    // ステップ 4: idle lock — MSG-S09(c) 日本語文言確認
    std::thread::sleep(Duration::from_secs(3));
    // ステップ 5: vault unlock 再 — MSG-S03 日本語
    // ステップ 6: change-password — MSG-S05「マスターパスワードを変更しました」確認
    // ステップ 7: list（cache 維持）

    // TODO(unlock): 各ステップ expectrl PTY + assert_cmd 実装
    // 上記は全て #[ignore] が解除されてから実装する

    #[cfg(debug_assertions)]
    drop(daemon);
    drop(vault_dir);
}

// -----------------------------------------------------------------------
// 未使用 import の suppress（#[ignore] テストでも型チェックを通すため）
// -----------------------------------------------------------------------
#[allow(dead_code)]
fn _use_predicates() {
    let _ = predicate::str::contains("");
}
