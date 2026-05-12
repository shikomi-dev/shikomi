//! Sub-F vault アクセシビリティ出力 結合テスト（TC-F-A01〜A06）。
//!
//! ## 責務
//! `shikomi vault encrypt --output {print,braille,audio}` のアクセシビリティ出力経路と
//! `SHIKOMI_ACCESSIBILITY=1` 自動切替・umask 077 ファイル権限・Locked vault 認可バイパス防衛を
//! 結合経路で検証する。
//!
//! ## ファイルガード
//! `#![cfg(unix)]` により Windows CI では全 TC がコンパイル対象外となる。
//! `expectrl` PTY および `umask` は Unix 専用のため個別 `#[ignore]` は不要（3-OS matrix のうち
//! Ubuntu + macOS でのみ実行される）。
//!
//! ## #[ignore] 規約（vault-persistence/test-design/integration/changelog.md v8.4 準拠）
//! reason 文字列の必須要素:
//! 1. skip 理由  2. 関連ゲート  3. 設計書クロス参照  4. 解除条件
//!
//! ## 実装注意（OWASP A02）
//! TC-F-A03: fake TTS バイナリへのニーモニックテキスト渡しを tempfile に平文記録することは
//! **禁止**（vault secret 漏洩リスク）。spawn 確認は CLI の `stdout pid: N` 形式出力で行う。
//!
//! 設計根拠: `docs/features/cli-vault-commands/test-design/integration.md §11`
//! 対応 Issue: #78

#![cfg(unix)]

mod common;
mod helpers;

use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

use assert_cmd::Command as AssertCommand;
use common::fixtures;
use common::tighten_perms_unix;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// 共通ヘルパー
// ---------------------------------------------------------------------------

/// `shikomi --vault-dir <dir>` ベースの Command を返す。
fn shikomi_with_vault_dir(dir: &Path) -> AssertCommand {
    let mut cmd = AssertCommand::cargo_bin("shikomi").expect("cargo_bin");
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir);
    cmd
}

/// 暗号化済み vault を持つ TempDir を返す。
fn setup_encrypted_vault() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    fixtures::create_encrypted_vault(dir.path()).expect("create encrypted vault");
    dir
}

/// 平文 vault を持つ TempDir を返す（`add` で vault.db を初期化）。
///
/// OWASP A02 陰性確認のため `SECRET_TEST_VALUE` を値とする secret レコードを投入する。
/// TC-F-A01 / TC-F-A02 の `.windows(secret_marker.len()).all(|w| w != secret_marker)` が
/// 有意な検査になるようフィクスチャ側でもマーカーを実際に vault に書き込む。
fn setup_plaintext_vault() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    shikomi_with_vault_dir(dir.path())
        .args(["add", "--kind", "text", "--label", "L", "--value", "V"])
        .assert()
        .success();
    // OWASP A02 陰性確認: stdout に secret 値が漏洩しないことを検証する意味のある assert にするため
    // 実際に "SECRET_TEST_VALUE" を vault に投入する（TC-F-A01 / TC-F-A02 で照合）。
    shikomi_with_vault_dir(dir.path())
        .args(["add", "--kind", "secret", "--label", "S", "--stdin"])
        .write_stdin("SECRET_TEST_VALUE\n")
        .assert()
        .success();
    dir
}

// ---------------------------------------------------------------------------
// TC-F-A01: vault encrypt --output print → PDF バイト列 + OWASP A02 陰性確認
// 設計根拠: integration.md §11.4
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires Sub-F daemon V2 IPC handler VaultEncrypt (PDF output path) — not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs; \
            expectrl PTY passphrase input requires handler to respond \
            (test-design integration.md §11.4, \
            unlock condition: implement VaultEncrypt IPC handler returning PDF bytes via stdout)"]
fn tc_f_a01_vault_encrypt_output_print_produces_pdf_bytes() {
    // 前提: plaintext vault + DaemonSpawn
    let dir = setup_plaintext_vault();
    let daemon = helpers::DaemonSpawn::new(dir.path()).expect("daemon spawn");

    // `shikomi vault encrypt --output print` (expectrl PTY 経由 passphrase 入力)
    // → exit 0 + stdout バイト列に %PDF-1.7 magic byte + %%EOF 終端 marker 含有
    let output = shikomi_with_vault_dir(dir.path())
        .envs(daemon.env_args())
        .args(["vault", "encrypt", "--output", "print"])
        .output()
        .expect("shikomi spawn");

    assert!(
        output.status.success(),
        "expected exit 0 for --output print"
    );

    // %PDF-1.7 magic byte (EC-F1 / SSoT §15.7 A01)
    assert!(
        output.stdout.windows(7).any(|w| w == b"%PDF-1."),
        "stdout must start with %PDF-1.x magic byte"
    );
    // %%EOF 終端 marker
    assert!(
        output.stdout.windows(5).any(|w| w == b"%%EOF"),
        "stdout must contain %%EOF terminator"
    );

    // OWASP A02: vault secret 値のバイト列が PDF stdout に含まれないこと（TC-F-A02 と対称）
    // setup_plaintext_vault() で投入した "SECRET_TEST_VALUE" が全長 17 バイトで現れないことを確認。
    // `.windows(3)` + 3 バイト前方一致は不十分（"SEC" 程度で false negative が多すぎる）。
    let secret_marker = b"SECRET_TEST_VALUE";
    assert!(
        output
            .stdout
            .windows(secret_marker.len())
            .all(|w| w != secret_marker),
        "stdout must not contain vault secret bytes (OWASP A02)"
    );
}

// ---------------------------------------------------------------------------
// TC-F-A02: vault encrypt --output braille → BRF バイト列 + OWASP A02 陰性確認
// 設計根拠: integration.md §11.4
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires Sub-F daemon V2 IPC handler VaultEncrypt (BRF output path) — not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs; \
            expectrl PTY passphrase input requires handler to respond \
            (test-design integration.md §11.4, \
            unlock condition: implement VaultEncrypt IPC handler returning BRF bytes via stdout)"]
fn tc_f_a02_vault_encrypt_output_braille_produces_brf_bytes() {
    // 前提: plaintext vault + DaemonSpawn
    let dir = setup_plaintext_vault();
    let daemon = helpers::DaemonSpawn::new(dir.path()).expect("daemon spawn");

    // `shikomi vault encrypt --output braille` (expectrl PTY 経由)
    // → exit 0 + stdout に Unicode braille 範囲 (U+2800..U+28FF) のコードポイント含有
    let output = shikomi_with_vault_dir(dir.path())
        .envs(daemon.env_args())
        .args(["vault", "encrypt", "--output", "braille"])
        .output()
        .expect("shikomi spawn");

    assert!(
        output.status.success(),
        "expected exit 0 for --output braille"
    );

    // Unicode braille 範囲 (U+2800..U+28FF) のコードポイントが stdout に含まれることを確認
    // (または ASCII BRF 行末 \r\n 形式)
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let has_braille = stdout_str
        .chars()
        .any(|c| ('\u{2800}'..='\u{28FF}').contains(&c));
    let has_brf_crlf = output.stdout.windows(2).any(|w| w == b"\r\n");
    assert!(
        has_braille || has_brf_crlf,
        "stdout must contain Unicode braille codepoints (U+2800..U+28FF) or ASCII BRF CRLF"
    );

    // OWASP A02: vault secret 値のバイト列が BRF stdout に含まれないこと
    let secret_marker = b"SECRET_TEST_VALUE";
    assert!(
        output
            .stdout
            .windows(secret_marker.len())
            .all(|w| w != secret_marker),
        "BRF stdout must not contain vault secret bytes (OWASP A02)"
    );
}

// ---------------------------------------------------------------------------
// TC-F-A03: vault encrypt --output audio → TTS spawn + pid:N stdout + env allowlist
// 設計根拠: integration.md §11.3 / §11.4
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires fake TTS binary in PATH (audio spawn gate, \
            test-design integration.md §11.3, \
            unlock condition: add fake_say fixture to tests/helpers/ and register in CI workflow)"]
fn tc_f_a03_vault_encrypt_output_audio_spawns_tts_with_pid() {
    // 前提: plaintext vault + DaemonSpawn + fake say/espeak が PATH 先頭に配置されていること
    // OWASP A02 実装注意: fake TTS へのニーモニック渡しを tempfile に平文記録することは禁止。
    // spawn 確認は CLI の `stdout pid: N` 形式出力で行う（§11.3 実装注意）。
    let dir = setup_plaintext_vault();
    let daemon = helpers::DaemonSpawn::new(dir.path()).expect("daemon spawn");

    // dictation 学習 prefs の mtime を事前記録（run 後の変化なし確認、OWASP A02 §11.3）
    // macOS: ~/Library/Preferences/com.apple.SpeechRecognitionServer.plist
    // Linux: ~/.local/share/speech-dispatcher / ~/.config/speech-dispatcher
    let home = std::env::var("HOME").unwrap_or_default();
    let dictation_pref_rels: &[&str] = &[
        "Library/Preferences/com.apple.SpeechRecognitionServer.plist",
        ".local/share/speech-dispatcher",
        ".config/speech-dispatcher",
    ];
    let pref_mtimes_before: Vec<(std::path::PathBuf, Option<std::time::SystemTime>)> =
        dictation_pref_rels
            .iter()
            .map(|rel| {
                let p = std::path::Path::new(&home).join(rel);
                let mtime = p.metadata().ok().and_then(|m| m.modified().ok());
                (p, mtime)
            })
            .collect();

    let output = shikomi_with_vault_dir(dir.path())
        .envs(daemon.env_args())
        .args(["vault", "encrypt", "--output", "audio"])
        .output()
        .expect("shikomi spawn");

    assert!(
        output.status.success(),
        "expected exit 0 for --output audio"
    );

    // stdout に `pid: N` 形式（整数）で TTS サブプロセス ID が出力されること（§11.4 TC-F-A03）
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("pid:") || stdout.contains("pid: "),
        "stdout must contain 'pid: N' TTS subprocess ID: {:?}",
        stdout
    );
    // pid: 以降が整数であることを確認
    if let Some(pid_part) = stdout.split("pid:").nth(1) {
        let pid_str = pid_part.split_whitespace().next().unwrap_or("");
        assert!(
            pid_str.parse::<u32>().is_ok(),
            "pid value after 'pid:' must be a valid integer: {:?}",
            pid_str
        );
    }

    // env allowlist 通過確認: CLI が spawn した TTS プロセスに余分な env が渡されていない
    // (fake TTS バイナリが自身の env を stdout に出力する実装を前提とする)
    let shikomi_internal_env_leaked =
        stdout.contains("SHIKOMI_") || stdout.contains("XDG_RUNTIME_DIR=");
    assert!(
        !shikomi_internal_env_leaked,
        "TTS subprocess must not receive shikomi internal env vars (env allowlist violation, OWASP A02)"
    );

    // dictation 学習 prefs 汚染なし: fake TTS が prefs ファイルに書き込まないこと (OWASP A02、§11.3)
    // 事前 mtime と事後 mtime を比較し、変化があった場合は prefs 汚染とみなして失敗させる。
    for (path, mtime_before) in &pref_mtimes_before {
        let mtime_after = path.metadata().ok().and_then(|m| m.modified().ok());
        assert_eq!(
            mtime_before, &mtime_after,
            "dictation prefs must not be modified by fake TTS subprocess (OWASP A02 §11.3): {:?}",
            path
        );
    }
}

// ---------------------------------------------------------------------------
// TC-F-A04: SHIKOMI_ACCESSIBILITY=1 自動切替 → いずれかの出力形式 + 情報漏洩なし
// 設計根拠: integration.md §11.3 / §11.4
// ---------------------------------------------------------------------------

#[test]
#[ignore = "SHIKOMI_ACCESSIBILITY=1 auto-select may trigger audio path requiring TTS binary (audio auto-select gate, \
            test-design integration.md §11.3, \
            unlock condition: implementation guarantees print/braille fallback when TTS unavailable, \
            or fake TTS registered in CI)"]
fn tc_f_a04_accessibility_env_auto_selects_output_format() {
    // 前提: plaintext vault + DaemonSpawn + SHIKOMI_ACCESSIBILITY=1
    let dir = setup_plaintext_vault();
    let daemon = helpers::DaemonSpawn::new(dir.path()).expect("daemon spawn");

    let output = shikomi_with_vault_dir(dir.path())
        .envs(daemon.env_args())
        .env("SHIKOMI_ACCESSIBILITY", "1")
        .args(["vault", "encrypt"]) // --output フラグなし（自動切替）
        .output()
        .expect("shikomi spawn");

    assert!(
        output.status.success(),
        "expected exit 0 with SHIKOMI_ACCESSIBILITY=1"
    );

    // print / braille / audio のいずれかの出力形式が stdout / stderr に現れること
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let has_pdf_magic = output.stdout.windows(7).any(|w| w == b"%PDF-1.");
    let has_braille = stdout
        .chars()
        .any(|c| ('\u{2800}'..='\u{28FF}').contains(&c));
    let has_pid = stdout.contains("pid:") || stderr.contains("pid:");
    assert!(
        has_pdf_magic || has_braille || has_pid,
        "stdout/stderr must show at least one output format (print/braille/audio) with SHIKOMI_ACCESSIBILITY=1"
    );

    // レコード内容が平文で stdout / stderr に露出しないこと（grep 0 件）
    assert!(
        !stdout.contains("SECRET_TEST_VALUE") && !stderr.contains("SECRET_TEST_VALUE"),
        "stdout/stderr must not expose vault record content (OWASP A02)"
    );
}

// ---------------------------------------------------------------------------
// TC-F-A05: umask 077 + > out.pdf リダイレクト → mode 0o600 + /tmp 中間ファイルなし
// 設計根拠: integration.md §11.2 / §11.4
// ---------------------------------------------------------------------------

#[test]
#[allow(unsafe_code)]
// pre_exec は unsafe fn（設計書 §11.2: CommandExt::pre_exec unsafe ブロック使用を明記）
#[ignore = "requires Sub-F daemon V2 IPC handler VaultEncrypt (umask 077 + file redirect path) — not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs; \
            CommandExt::pre_exec unsafe umask(0o077) requires handler to produce PDF bytes \
            (test-design integration.md §11.4, \
            unlock condition: implement VaultEncrypt IPC handler)"]
fn tc_f_a05_vault_encrypt_output_print_respects_umask_077() {
    use std::os::unix::fs::PermissionsExt as _;

    // 前提: plaintext vault + DaemonSpawn
    let dir = setup_plaintext_vault();
    let daemon = helpers::DaemonSpawn::new(dir.path()).expect("daemon spawn");
    let out_pdf = dir.path().join("out.pdf");

    // `CommandExt::pre_exec` で umask(0o077) を子プロセス前に設定してから実行
    // (`> out.pdf` リダイレクトは std::process::Command では直接できないため、
    //  stdout を ファイルにリダイレクトする File を使う)
    // 設計書 §11.2: unix::process::CommandExt::pre_exec + unsafe { libc::umask(0o077) }
    let file = std::fs::File::create(&out_pdf).expect("create out.pdf");
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin("shikomi"));
    cmd.env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .arg("--vault-dir")
        .arg(dir.path())
        .envs(daemon.env_args())
        .args(["vault", "encrypt", "--output", "print"])
        .stdout(file);

    // SAFETY: umask は非同期シグナルセーフな syscall。pre_exec 内の unsafe ブロックで
    // umask(0o077) を設定し、子プロセスが生成するファイルを owner read/write のみに制限する。
    // 設計書 §11.2 CommandExt::pre_exec 使用を明記。
    unsafe {
        cmd.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }

    let status = cmd.status().expect("shikomi spawn");
    assert!(
        status.success(),
        "expected exit 0 for --output print with umask 077"
    );

    // out.pdf の mode が 0o600 であることを確認（umask 0o077 の反映）
    let mode = std::fs::metadata(&out_pdf)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "out.pdf mode must be 0o600 (umask 077), got {mode:#o}"
    );

    // /tmp 以下に vault.db 関連の中間ファイルが生成されていないこと（全バリアント確認）
    // SQLite が生成する -wal / -shm / -journal と作業中間ファイル .tmp / .new を含む
    // 全パターンを検査する（single-file チェックでは -wal 漏洩を見逃す）。
    // § 11.1 memory-only 出力契約。
    let vault_db_variants = [
        "vault.db",
        "vault.db-wal",
        "vault.db-shm",
        "vault.db-journal",
        "vault.db.tmp",
        "vault.db.new",
    ];
    for variant in &vault_db_variants {
        let p = std::path::Path::new("/tmp").join(variant);
        assert!(
            !p.exists(),
            "/tmp/{variant} must not be created (memory-only output required, §11.1)"
        );
    }
}

// ---------------------------------------------------------------------------
// TC-F-A06: Locked vault + --output print → 拒否（OWASP A01 認可バイパス防衛）
// 設計根拠: integration.md §11.4 / §11.6
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires Sub-F daemon V2 Locked state — VaultEncrypt IPC handler not yet implemented \
            in crates/shikomi-daemon/src/ipc/handler/mod.rs (locked-vault gate, \
            test-design integration.md §11.4, \
            unlock condition: implement VaultEncrypt + VaultUnlock + VaultLock IPC handlers)"]
fn tc_f_a06_locked_vault_encrypt_output_print_rejected() {
    // 前提: 暗号化 vault (Locked) + DaemonSpawn
    // Locked 状態は VaultUnlock → VaultLock サイクルで確立（handlers 未実装のため #[ignore]）
    let dir = setup_encrypted_vault();
    let daemon = helpers::DaemonSpawn::new(dir.path()).expect("daemon spawn");

    let output = shikomi_with_vault_dir(dir.path())
        .envs(daemon.env_args())
        .args(["vault", "encrypt", "--output", "print"])
        .output()
        .expect("shikomi spawn");

    // exit 1 以上（AlreadyEncrypted または VaultLocked 由来エラー）
    assert!(
        !output.status.success(),
        "shikomi vault encrypt on Locked vault must not exit 0 (OWASP A01)"
    );
    // シグナル終了（SIGKILL 等）時は `status.code()` が None を返す。
    // None の場合も非ゼロ終了として扱うため unwrap_or(1) とする。
    // unwrap_or(0) では SIGKILL 終了が exit 0 と判定され assert が false positive を起こす。
    let exit_code = output.status.code().unwrap_or(1);
    assert!(
        exit_code >= 1,
        "expected exit code >= 1 for Locked vault, got {exit_code}"
    );

    // OWASP A01: stdout に %PDF-1.7 バイト列が含まれないこと
    // (Locked vault でアクセシビリティ出力が生成されない、認可バイパス防衛確認)
    assert!(
        !output.stdout.windows(7).any(|w| w == b"%PDF-1."),
        "stdout must not contain %PDF-1.x bytes when vault is Locked (OWASP A01 authz bypass defense)"
    );

    // エラー文言確認（AlreadyEncrypted または VaultLocked 由来）
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("AlreadyEncrypted")
            || stderr.contains("VaultLocked")
            || stdout.contains("AlreadyEncrypted")
            || stdout.contains("VaultLocked"),
        "error output must contain AlreadyEncrypted or VaultLocked: stderr={:?} stdout={:?}",
        stderr,
        stdout
    );
}
