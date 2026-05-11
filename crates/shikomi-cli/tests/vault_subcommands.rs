//! Sub-F vault サブコマンド 結合テスト（TC-F-I01〜I12）。
//!
//! ## 設計方針
//! - エントリポイント: `assert_cmd::Command::cargo_bin("shikomi")` 実バイナリ
//! - daemon 依存: `helpers::DaemonSpawn` 経由で実子プロセス起動（Unix 限定）
//! - TTY 入力: `expectrl` PTY ライブラリ（Unix 限定 dev-dep）
//!
//! ## #[ignore] 規約（vault-persistence/test-design/integration/changelog.md v8.4 準拠）
//! reason 文字列の必須要素:
//! 1. skip 理由  2. 関連ゲート  3. 設計書クロス参照  4. 解除条件
//!
//! 設計根拠: `docs/features/cli-vault-commands/test-design/integration.md §10`
//! 対応 Issue: #77

mod common;
mod helpers;

use std::path::Path;

use assert_cmd::Command;
use common::fixtures;
use common::tighten_perms_unix;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// 共通ヘルパー
// ---------------------------------------------------------------------------

fn shikomi_with_vault_dir(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

fn setup_encrypted_vault() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    fixtures::create_encrypted_vault(dir.path()).expect("create encrypted vault");
    dir
}

// ---------------------------------------------------------------------------
// TC-F-I01: vault encrypt — expectrl PTY 経由パスワード入力
// 設計根拠: integration.md §10.4.1
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows, covered by Unix CI \
              (3-OS matrix design intent: TC-F-I* PTY path is Unix+macOS only, \
              test-design integration.md §10.3, \
              unlock condition: port to Windows Console API)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Encrypt — not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs \
            (test-design integration.md §10.4.1, \
            unlock condition: implement VaultEncrypt IPC handler)"]
fn tc_f_i01_vault_encrypt_via_pty_succeeds() {
    // DaemonSpawn + expectrl PTY で `shikomi vault encrypt --output screen` を実行し
    // EC-F1 / REQ-S15 を結合経路で検証する。
    // 実装条件: daemon V2 IpcRequest::VaultEncrypt ハンドラ実装後に #[ignore] 解除。
    unimplemented!("TC-F-I01: daemon V2 Encrypt handler required; see integration.md §10.4.1");
}

// ---------------------------------------------------------------------------
// TC-F-I02: vault decrypt — expectrl PTY
// TC-F-I02b: paste 検出 (< 30ms)
// 設計根拠: integration.md §10.4.1
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Decrypt — not yet implemented \
            (test-design integration.md §10.4.1, \
            unlock condition: implement VaultDecrypt IPC handler)"]
fn tc_f_i02_vault_decrypt_via_pty_succeeds() {
    unimplemented!("TC-F-I02");
}

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Decrypt + expectrl paste timing \
            (test-design integration.md §10.4.1, \
            unlock condition: implement VaultDecrypt + C-34 paste detection)"]
fn tc_f_i02b_paste_detection_fast_input_rejected() {
    unimplemented!("TC-F-I02b");
}

// ---------------------------------------------------------------------------
// TC-F-I03: vault unlock — 正常系 + backoff (exit 2)
// TC-F-I03b: recovery 経路 + RecoveryRequired (exit 5)
// 設計根拠: integration.md §10.4.2
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Unlock — not yet implemented \
            (test-design integration.md §10.4.2, \
            unlock condition: implement VaultUnlock IPC handler)"]
fn tc_f_i03_vault_unlock_correct_password_exits_zero() {
    unimplemented!("TC-F-I03");
}

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Unlock (recovery path) — not yet implemented \
            (test-design integration.md §10.4.2, \
            unlock condition: implement VaultUnlock recovery IPC handler)"]
fn tc_f_i03b_vault_unlock_recovery_path() {
    unimplemented!("TC-F-I03b");
}

// ---------------------------------------------------------------------------
// TC-F-I04: vault lock
// 設計根拠: integration.md §10.4.2
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Lock — not yet implemented \
            (test-design integration.md §10.4.2, \
            unlock condition: implement VaultLock IPC handler)"]
fn tc_f_i04_vault_lock_exits_zero_and_subsequent_list_exits_three() {
    unimplemented!("TC-F-I04");
}

// ---------------------------------------------------------------------------
// TC-F-I05: vault change-password
// 設計根拠: integration.md §10.4.3
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler ChangePassword — not yet implemented \
            (test-design integration.md §10.4.3, \
            unlock condition: implement VaultChangePassword IPC handler)"]
fn tc_f_i05_vault_change_password_cache_maintained() {
    unimplemented!("TC-F-I05");
}

// ---------------------------------------------------------------------------
// TC-F-I06: vault encrypt — AlreadyEncrypted 防衛 (C-35)
// 設計根拠: integration.md §10.4.3
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Encrypt (C-35 disclose once guard) — not yet implemented \
            (test-design integration.md §10.4.3, \
            unlock condition: implement AlreadyEncrypted guard in Encrypt handler)"]
fn tc_f_i06_vault_encrypt_already_encrypted_returns_error() {
    unimplemented!("TC-F-I06");
}

// ---------------------------------------------------------------------------
// TC-F-I07: vault rekey — cache_relocked=true 経路
// TC-F-I07c: fault injection (C-40 allowlist, #[cfg(debug_assertions)])
// 設計根拠: integration.md §10.4.4
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Rekey — not yet implemented \
            (test-design integration.md §10.4.4, \
            unlock condition: implement VaultRekey IPC handler)"]
fn tc_f_i07_vault_rekey_cache_maintained() {
    unimplemented!("TC-F-I07");
}

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[cfg_attr(
    not(debug_assertions),
    ignore = "requires debug build (C-40 allowlist gate, test-design integration.md §10.3, \
              unlock condition: SHIKOMI_DAEMON_FORCE_RELOCK_FAIL extended to release builds by explicit flag)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler Rekey + force_relock_fail fault injection — not yet implemented \
            (test-design integration.md §10.4.4, \
            unlock condition: implement Rekey handler + C-40 env seam in daemon)"]
fn tc_f_i07c_vault_rekey_cache_relocked_fault_injection() {
    unimplemented!("TC-F-I07c");
}

// ---------------------------------------------------------------------------
// TC-F-I08: vault rotate-recovery
// 設計根拠: integration.md §10.4.5
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 IPC handler RotateRecovery — not yet implemented \
            (test-design integration.md §10.4.5, \
            unlock condition: implement VaultRotateRecovery IPC handler)"]
fn tc_f_i08_vault_rotate_recovery_cache_maintained() {
    unimplemented!("TC-F-I08");
}

// ---------------------------------------------------------------------------
// TC-F-I09: Locked 状態での `shikomi list` — exit 3 + 情報漏洩防衛
// TC-F-I09b: Locked 状態での CRUD — 全て exit 3
// 設計根拠: integration.md §10.4.5
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 Locked state (Unlock → Lock cycle via V2 IPC) — handlers not yet implemented \
            (test-design integration.md §10.4.5, \
            unlock condition: implement Unlock + Lock IPC handlers to establish Locked state)"]
fn tc_f_i09_list_on_locked_vault_exits_three_no_data_leak() {
    unimplemented!("TC-F-I09");
}

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "expectrl PTY not available on Windows (test-design integration.md §10.3)"
)]
#[ignore = "requires Sub-F daemon V2 Locked state for CRUD operations — handlers not yet implemented \
            (test-design integration.md §10.4.5, \
            unlock condition: implement Unlock + Lock IPC handlers)"]
fn tc_f_i09b_crud_on_locked_vault_all_exit_three() {
    unimplemented!("TC-F-I09b");
}

// ---------------------------------------------------------------------------
// TC-F-I11a: インジェクション境界値 — SQL インジェクション
// 設計根拠: integration.md §10.4.8
// ---------------------------------------------------------------------------
//
// NOTE: RecordLabel::try_new はセミコロン・ダッシュを制御文字として拒否しない（空文字
// / 256 グラフェーム超 / 制御文字のみを拒否）。設計書は exit 1 + InvalidLabel を期待しているが、
// 現在の実装では `"; DROP TABLE records;--"` は有効なラベルとして受け入れられる。
// OWASP A03 防御は rusqlite パラメータバインディングで担保されており records テーブルは
// 消えない（本質的な安全保証は維持）が、設計書の期待 exit code (1) と実装 (0) が乖離している。
// 設計書の RecordLabel バリデーション仕様を修正するか、テスト期待値を変更した後に
// #[ignore] を解除すること。

#[test]
#[ignore = "design expects exit 1 (CliError::InvalidLabel) for SQL injection label, \
            but RecordLabel::try_new accepts semicolons (only rejects empty/too-long/control-chars); \
            actual SQL injection protection is via rusqlite parameterized queries — \
            records table is intact but label is accepted (exit 0, not exit 1); \
            (test-design integration.md §10.4.8, \
            unlock condition: revise RecordLabel validator or update expected behavior in §10.4.8)"]
fn tc_f_i11a_sql_injection_label_rejected() {
    unimplemented!("TC-F-I11a");
}

// ---------------------------------------------------------------------------
// TC-F-I11b: インジェクション境界値 — パストラバーサル（SHIKOMI_VAULT_DIR）
// 設計根拠: integration.md §10.4.8
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "path traversal boundary is /etc (Unix only), Windows equivalent covered by \
              VaultPaths::new unit test \
              (test-design integration.md §10.4.8, \
              unlock condition: add Windows-specific traversal boundary TC)"
)]
fn tc_f_i11b_path_traversal_via_env_var_rejected() {
    // 前提: daemon 不要。SHIKOMI_VAULT_DIR に `..` を含む非絶対パスを設定する。
    // VaultPaths::new が NotAbsolute / PathTraversal で弾き、/etc/ 配下に vault.db が
    // 生成されないことを確認する（OWASP A03 パストラバーサル防衛）。

    // --- ① 相対パス（`..` を含む）はパストラバーサル境界として拒否される ---
    let out = Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env("SHIKOMI_VAULT_DIR", "../../../../etc/passwd")
        .env_remove("LANG")
        .arg("list")
        .output()
        .expect("shikomi spawn");

    // 非ゼロ終了を確認（PersistenceError::InvalidVaultDir → CliError::Persistence → exit 2）
    assert!(
        !out.status.success(),
        "shikomi list with traversal vault dir must exit non-zero"
    );

    // /etc/ 配下に vault.db が生成されていないことを確認
    assert!(
        !std::path::Path::new("/etc/passwd/vault.db").exists(),
        "vault.db must not be created under /etc/"
    );
    assert!(
        !std::path::Path::new("/etc/vault.db").exists(),
        "vault.db must not be created under /etc/"
    );

    // --- ② シェルメタ文字を含む VAULT_DIR も拒否される ---
    let out2 = Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env("SHIKOMI_VAULT_DIR", "$(echo /tmp/evil)")
        .env_remove("LANG")
        .arg("list")
        .output()
        .expect("shikomi spawn");

    assert!(
        !out2.status.success(),
        "shikomi list with shell-metachar vault dir must exit non-zero"
    );
}

// ---------------------------------------------------------------------------
// TC-F-I12: stdin パイプ拒否 — C-38 NonInteractivePassword
// 設計根拠: integration.md §10.4.7
// ---------------------------------------------------------------------------
//
// `assert_cmd::Command::write_stdin` で非 TTY パイプ経路を模擬する。
// `shikomi vault unlock` は password::prompt() の is_stdin_tty() チェックで
// DaemonSpawn への IPC 接続より先に NonInteractivePassword を返す経路が必要。
// ただし run_vault は connect_vault_ipc (IPC ハンドシェイク) を先に実行するため、
// daemon が起動していない場合は DaemonNotRunning で失敗する。
// 現時点では daemon バイナリのビルドが前提になるため #[ignore] とする。

// BUG-112-01 修正: `DaemonSpawn` は `#[cfg(unix)]` 限定シンボルのため、
// `#[cfg_attr(target_os = "windows", ignore)]` のランタイムスキップでは
// Windows コンパイル時に E0433 が発生する。`#[cfg(unix)]` でコンパイル自体を除外する。
#[test]
#[cfg(unix)]
#[ignore = "requires pre-built shikomi-daemon binary and running DaemonSpawn for IPC handshake; \
            run `cargo build -p shikomi-daemon` first, then test verifies NonInteractivePassword \
            is returned before IPC Unlock call (C-38, test-design integration.md §10.4.7, \
            unlock condition: add daemon build step to just test-cli or CI workflow)"]
fn tc_f_i12_stdin_pipe_to_vault_unlock_rejected() {
    let encrypted_dir = setup_encrypted_vault();

    // DaemonSpawn (requires shikomi-daemon binary to be pre-built)
    let daemon = helpers::DaemonSpawn::new(encrypted_dir.path()).expect("daemon spawn");

    // stdin パイプ経由でパスワード送信 → NonInteractivePassword (exit 1)
    shikomi_with_vault_dir(encrypted_dir.path())
        .envs(daemon.env_args())
        .args(["vault", "unlock"])
        .write_stdin("strong-password\n")
        .assert()
        .code(1); // CliError::NonInteractivePassword → ExitCode::UserError (1)
}
