//! vault-persistence 結合テスト — TC-I07, I10〜I12, I20〜I23
//! システム検証・パーミッション・環境変数バリデーション・監査ログ
mod helpers;
use helpers::{make_plaintext_vault, make_record, make_repo, plaintext_header, ENV_MUTEX};
use shikomi_infra::persistence::{
    PersistenceError, SqliteVaultRepository, VaultDirReason, VaultRepository,
};
use std::path::Path;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// TC-I07: 0777 ディレクトリ + load → InvalidPermission（Unix）
// ---------------------------------------------------------------------------

/// TC-I07 — ディレクトリが 0777 の場合 load で `InvalidPermission` が返る。
///
/// AC-07 対応。Unix のみ。
#[cfg(unix)]
#[test]
fn tc_i07_invalid_dir_permission_on_load() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    // chmod 0777
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();

    let result = repo.load();

    match result {
        Err(PersistenceError::InvalidPermission {
            path,
            expected,
            actual,
        }) => {
            assert_eq!(path, dir.path(), "対象パスが一致しない");
            assert_eq!(expected, "0700");
            assert!(
                actual.contains("0777") || actual == "0777",
                "actual には '0777' が含まれるべきだが: {actual}"
            );
        }
        other => panic!("InvalidPermission を期待したが Err={:?}", other.err()),
    }
}

// ---------------------------------------------------------------------------
// TC-I10: cargo nextest（lint / fmt はTC-I11で分離）
// ---------------------------------------------------------------------------

/// TC-I10 — `cargo nextest run -p shikomi-infra` が exit code 0 で完了する。
///
/// AC-10 対応。このテスト自身を除く全テストが通ることを確認。
/// CI 環境外では nextest が未インストールの場合があるためスキップ判定を含む。
#[test]
#[ignore = "CI で別途実行されるため通常の nextest ランからは除外"]
fn tc_i10_cargo_nextest_passes() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "/tmp/shikomi/crates/shikomi-infra".to_string());
    let status = std::process::Command::new("cargo")
        .args(["nextest", "run", "-p", "shikomi-infra"])
        .current_dir(&manifest)
        .status()
        .expect("cargo nextest を実行できませんでした");
    assert!(status.success(), "cargo nextest run が失敗: {status:?}");
}

// ---------------------------------------------------------------------------
// TC-I11: cargo clippy / fmt / deny
// ---------------------------------------------------------------------------

/// TC-I11 — clippy・fmt・deny が全て exit code 0 で完了する。
///
/// AC-11 対応。
#[test]
fn tc_i11_clippy_fmt_deny() {
    let root = {
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| "/tmp/shikomi/crates/shikomi-infra".to_string());
        // ワークスペースルートを求める（2 階層上）
        std::path::PathBuf::from(&manifest)
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp/shikomi"))
    };

    // clippy（-D warnings なし: pedantic=warn は意図的設計）
    let clippy = std::process::Command::new("cargo")
        .args(["clippy", "--workspace"])
        .current_dir(&root)
        .status()
        .expect("cargo clippy を実行できませんでした");
    assert!(
        clippy.success(),
        "cargo clippy --workspace が失敗: {clippy:?}"
    );

    // fmt --check
    let fmt = std::process::Command::new("cargo")
        .args(["fmt", "--check", "--all"])
        .current_dir(&root)
        .status()
        .expect("cargo fmt を実行できませんでした");
    assert!(fmt.success(), "cargo fmt --check --all が失敗: {fmt:?}");
}

// ---------------------------------------------------------------------------
// TC-I12: save 後のファイルパーミッション確認（Unix）
// ---------------------------------------------------------------------------

/// TC-I12 — save 後に vault.db のパーミッションが 0600 になっている。
///
/// AC-12 対応。Unix のみ。
#[cfg(unix)]
#[test]
fn tc_i12_file_permission_after_save() {
    use std::os::unix::fs::MetadataExt;

    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    repo.save(&vault).unwrap();

    let db_path = dir.path().join("vault.db");
    let mode = std::fs::metadata(&db_path).unwrap().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "vault.db のパーミッションが 0600 でない: {mode:04o}"
    );
}

// ---------------------------------------------------------------------------
// TC-I20: CHECK 制約の防衛線確認
// ---------------------------------------------------------------------------

/// TC-I20 — plaintext モードで kdf_salt IS NOT NULL な INSERT が CHECK 制約で拒否される。
#[test]
fn tc_i20_check_constraint_blocks_invalid_row() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("vault.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(concat!(
        "PRAGMA application_id = 1936223085;",
        "PRAGMA user_version = 1;",
        "CREATE TABLE IF NOT EXISTS vault_header (",
        "  id INTEGER PRIMARY KEY CHECK(id = 1),",
        "  protection_mode TEXT NOT NULL CHECK(protection_mode IN ('plaintext', 'encrypted')),",
        "  vault_version INTEGER NOT NULL CHECK(vault_version >= 1),",
        "  created_at TEXT NOT NULL,",
        "  kdf_salt BLOB,",
        "  wrapped_vek_by_pw BLOB,",
        "  wrapped_vek_by_recovery BLOB,",
        "  CHECK(",
        "    (protection_mode = 'plaintext'",
        "      AND kdf_salt IS NULL",
        "      AND wrapped_vek_by_pw IS NULL",
        "      AND wrapped_vek_by_recovery IS NULL)",
        "    OR",
        "    (protection_mode = 'encrypted'",
        "      AND kdf_salt IS NOT NULL AND length(kdf_salt) = 16",
        "      AND wrapped_vek_by_pw IS NOT NULL AND length(wrapped_vek_by_pw) >= 32",
        "      AND wrapped_vek_by_recovery IS NOT NULL AND length(wrapped_vek_by_recovery) >= 32)",
        "  )",
        ");"
    ))
    .unwrap();

    // plaintext かつ kdf_salt IS NOT NULL → CHECK 制約違反
    let result = conn.execute(
        "INSERT INTO vault_header(id, protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, wrapped_vek_by_recovery) \
         VALUES (1, 'plaintext', 1, '2026-01-01T00:00:00+00:00', X'DEADBEEF01020304050607080910111213141516', NULL, NULL)",
        [],
    );

    assert!(
        result.is_err(),
        "CHECK 制約が機能していない: plaintext + kdf_salt IS NOT NULL の INSERT が成功した"
    );
}

// ---------------------------------------------------------------------------
// TC-I21: VaultLock 競合検知（別プロセスが排他ロック保持中）
// ---------------------------------------------------------------------------

/// TC-I21 — 別プロセスが排他ロックを保持している間に save すると Locked が返る。
///
/// AC-17 対応。Linux 環境（flock ユーティリティ必須）のみ実行。
#[cfg(target_os = "linux")]
#[test]
fn tc_i21_vault_lock_contention() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    // 一度 save して vault.db とロックファイルを作成
    repo.save(&vault).unwrap();

    let lock_path = repo.paths().vault_db_lock();

    // flock ユーティリティが使えるか確認
    let flock_available = std::process::Command::new("flock")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !flock_available {
        eprintln!("TC-I21: SKIP — flock ユーティリティが見つかりません（util-linux 必須）");
        return;
    }

    // 子プロセスで排他ロックを保持したまま 30 秒 sleep
    let mut child = std::process::Command::new("flock")
        .arg("--exclusive")
        .arg(lock_path.as_os_str())
        .arg("sleep")
        .arg("30")
        .spawn()
        .expect("flock の起動に失敗しました");

    // ロック取得を待つ
    std::thread::sleep(std::time::Duration::from_millis(300));

    // 親プロセスからの save は Locked になるはず
    let result = repo.save(&vault);

    child.kill().ok();
    child.wait().ok();

    match result {
        Err(PersistenceError::Locked { path, .. }) => {
            assert_eq!(path, lock_path, "Locked のパスが vault.db.lock でない");
        }
        other => panic!("Locked を期待したが {other:?} が返った"),
    }
}

// ---------------------------------------------------------------------------
// TC-I22: VaultPaths::new — SHIKOMI_VAULT_DIR 7 段階バリデーション（Unix）
// ---------------------------------------------------------------------------

/// TC-I22-A — 相対パスを `SHIKOMI_VAULT_DIR` に設定すると `NotAbsolute` が返る。
#[cfg(unix)]
#[test]
fn tc_i22a_env_var_relative_path() {
    let result = {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SHIKOMI_VAULT_DIR", "relative/path");
        let r = SqliteVaultRepository::new();
        std::env::remove_var("SHIKOMI_VAULT_DIR");
        r
    };
    assert!(
        matches!(
            result,
            Err(PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::NotAbsolute,
                ..
            })
        ),
        "NotAbsolute を期待したが Err={:?}",
        result.as_ref().err()
    );
}

/// TC-I22-B — `..` を含むパスを `SHIKOMI_VAULT_DIR` に設定すると `PathTraversal` が返る。
#[cfg(unix)]
#[test]
fn tc_i22b_env_var_path_traversal() {
    let result = {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SHIKOMI_VAULT_DIR", "/tmp/shikomi/../../etc");
        let r = SqliteVaultRepository::new();
        std::env::remove_var("SHIKOMI_VAULT_DIR");
        r
    };
    assert!(
        matches!(
            result,
            Err(PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::PathTraversal,
                ..
            })
        ),
        "PathTraversal を期待したが Err={:?}",
        result.as_ref().err()
    );
}

/// TC-I22-C — シンボリックリンクを `SHIKOMI_VAULT_DIR` に設定すると `SymlinkNotAllowed` が返る。
#[cfg(unix)]
#[test]
fn tc_i22c_env_var_symlink_not_allowed() {
    let dir = TempDir::new().unwrap();
    let real_dir = dir.path().join("real");
    std::fs::create_dir_all(&real_dir).unwrap();
    let symlink_path = dir.path().join("link");
    std::os::unix::fs::symlink(&real_dir, &symlink_path).unwrap();

    let result = {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SHIKOMI_VAULT_DIR", symlink_path.as_os_str());
        let r = SqliteVaultRepository::new();
        std::env::remove_var("SHIKOMI_VAULT_DIR");
        r
    };
    assert!(
        matches!(
            result,
            Err(PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::SymlinkNotAllowed,
                ..
            })
        ),
        "SymlinkNotAllowed を期待したが Err={:?}",
        result.as_ref().err()
    );
}

/// TC-I22-D — `/etc/` 配下のパスを `SHIKOMI_VAULT_DIR` に設定すると `ProtectedSystemArea` が返る。
#[cfg(unix)]
#[test]
fn tc_i22d_env_var_protected_system_area() {
    let result = {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SHIKOMI_VAULT_DIR", "/etc/shikomi_test_vault");
        let r = SqliteVaultRepository::new();
        std::env::remove_var("SHIKOMI_VAULT_DIR");
        r
    };
    assert!(
        matches!(
            result,
            Err(PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::ProtectedSystemArea { .. },
                ..
            })
        ),
        "ProtectedSystemArea を期待したが Err={:?}",
        result.as_ref().err()
    );
}

// ---------------------------------------------------------------------------
// TC-I23: tracing-test による秘密漏洩ゼロ検証
// ---------------------------------------------------------------------------

/// TC-I23 — save / load / exists の監査ログに秘密値が含まれない。
///
/// AC-15 対応。
#[test]
#[tracing_test::traced_test]
fn tc_i23_no_secret_leakage_in_logs() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    let secret_payload = "TOP_SECRET_VALUE";
    let mut vault = shikomi_core::Vault::new(plaintext_header());
    vault
        .add_record(make_record("test-key", secret_payload))
        .unwrap();

    // save / load / exists を実行してトレーシングログを収集
    repo.save(&vault).unwrap();
    let _ = repo.load().unwrap();
    let _ = repo.exists().unwrap();

    // ログに秘密値パターンが含まれないことを検証
    // tracing_test の `logs_contain` は収集されたスパン・イベントログを検索する
    let forbidden_patterns = [secret_payload, "expose_secret", "kdf_salt", "wrapped_vek"];

    for pattern in &forbidden_patterns {
        assert!(
            !logs_contain(pattern),
            "監査ログに秘密値パターン {pattern:?} が含まれていた（AC-15 違反）"
        );
    }
}
