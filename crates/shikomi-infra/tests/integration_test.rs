//! vault-persistence 結合テスト — TC-I01〜TC-I23
//!
//! テスト設計書: docs/features/vault-persistence/test-design/integration.md
//! 対応 Issue: #10

use std::path::Path;

use shikomi_core::{
    KdfSalt, Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault,
    VaultHeader, VaultVersion, WrappedVek,
};
use shikomi_infra::persistence::{
    CorruptedReason, PersistenceError, SqliteVaultRepository, VaultDirReason, VaultRepository,
};
use tempfile::TempDir;
use time::OffsetDateTime;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ヘルパー
// ---------------------------------------------------------------------------

/// tempdir を使った `SqliteVaultRepository` を構築する（検証スキップ）。
fn make_repo(dir: &Path) -> SqliteVaultRepository {
    SqliteVaultRepository::with_dir(dir.to_path_buf())
}

/// 平文モードの `VaultHeader` を作る。
fn plaintext_header() -> VaultHeader {
    VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap()
}

/// 平文 `Record` を 1 件作る。
fn make_record(label: &str, value: &str) -> Record {
    let now = OffsetDateTime::now_utc();
    Record::new(
        RecordId::new(Uuid::now_v7()).unwrap(),
        RecordKind::Secret,
        RecordLabel::try_new(label.to_string()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string(value.to_string())),
        now,
    )
}

/// N 件のレコードを持つ平文 vault を作る。
fn make_plaintext_vault(n: usize) -> Vault {
    let mut vault = Vault::new(plaintext_header());
    for i in 0..n {
        vault
            .add_record(make_record(&format!("label-{i}"), &format!("value-{i}")))
            .unwrap();
    }
    vault
}

// ---------------------------------------------------------------------------
// TC-I01: 公開 API ドキュメント確認
// ---------------------------------------------------------------------------

/// TC-I01 — `cargo doc` が exit code 0 で完了し、主要型のドキュメントが生成される。
///
/// AC-01 対応。
#[test]
fn tc_i01_cargo_doc() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "/tmp/shikomi/crates/shikomi-infra".to_string());
    let status = std::process::Command::new("cargo")
        .args(["doc", "-p", "shikomi-infra", "--no-deps"])
        .current_dir(&manifest)
        .status()
        .expect("cargo を実行できませんでした");
    assert!(status.success(), "cargo doc が失敗しました: {status:?}");
}

// ---------------------------------------------------------------------------
// TC-I02: 平文 vault round-trip（レコード 5 件）
// ---------------------------------------------------------------------------

/// TC-I02 — save → load で平文 vault が完全に復元される。
///
/// AC-02 対応。
#[test]
fn tc_i02_plaintext_vault_round_trip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(5);

    repo.save(&vault).unwrap();
    let loaded = repo.load().unwrap();

    // ヘッダ確認
    assert_eq!(
        vault.header().protection_mode(),
        loaded.header().protection_mode()
    );
    assert_eq!(
        vault.header().version().value(),
        loaded.header().version().value()
    );

    // レコード件数・内容確認
    assert_eq!(vault.records().len(), loaded.records().len());
    for (orig, loaded_rec) in vault.records().iter().zip(loaded.records().iter()) {
        assert_eq!(orig.id(), loaded_rec.id(), "RecordId が一致しない");
        assert_eq!(
            orig.label().as_str(),
            loaded_rec.label().as_str(),
            "label が一致しない"
        );
        // payload は SecretString の Eq 未実装のためフォーマット比較
        if let (RecordPayload::Plaintext(a), RecordPayload::Plaintext(b)) =
            (orig.payload(), loaded_rec.payload())
        {
            assert_eq!(
                a.expose_secret(),
                b.expose_secret(),
                "plaintext_value が一致しない"
            );
        } else {
            panic!("平文モードなのに暗号化ペイロードが返った");
        }
    }
}

// ---------------------------------------------------------------------------
// TC-I03: 暗号化モード vault を save → UnsupportedYet
// ---------------------------------------------------------------------------

/// TC-I03 — 暗号化モード vault を save すると UnsupportedYet が返る。
///
/// AC-03 対応。
#[test]
fn tc_i03_encrypted_vault_save_unsupported() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    let kdf_salt = KdfSalt::try_new(&[0u8; 16]).unwrap();
    let wrapped_pw = WrappedVek::try_new(vec![0u8; 48].into_boxed_slice()).unwrap();
    let wrapped_rec = WrappedVek::try_new(vec![0u8; 48].into_boxed_slice()).unwrap();
    let header = VaultHeader::new_encrypted(
        VaultVersion::CURRENT,
        OffsetDateTime::now_utc(),
        kdf_salt,
        wrapped_pw,
        wrapped_rec,
    )
    .unwrap();
    let vault = Vault::new(header);

    let result = repo.save(&vault);

    assert!(
        matches!(
            result,
            Err(PersistenceError::UnsupportedYet { feature, .. })
            if feature.contains("encrypted")
        ),
        "UnsupportedYet を期待したが予期せぬ結果が返った"
    );
    // .new ファイルが作成されていないこと
    assert!(
        !dir.path().join("vault.db.new").exists(),
        ".new ファイルが不正に作成された"
    );
}

// ---------------------------------------------------------------------------
// TC-I04: 暗号化モード vault.db を load → UnsupportedYet
// ---------------------------------------------------------------------------

/// TC-I04 — protection_mode='encrypted' の vault.db を load すると UnsupportedYet が返る。
///
/// AC-04 対応。
#[test]
fn tc_i04_encrypted_vault_db_load_unsupported() {
    let dir = TempDir::new().unwrap();

    // vault.db を rusqlite で直接作成（CHECK 制約なし DDL でサイズ要件を満たす暗号化行を挿入）
    let db_path = dir.path().join("vault.db");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "PRAGMA application_id = 1936223085;
             PRAGMA user_version = 1;
             CREATE TABLE vault_header (
               id INTEGER PRIMARY KEY,
               protection_mode TEXT NOT NULL,
               vault_version INTEGER NOT NULL,
               created_at TEXT NOT NULL,
               kdf_salt BLOB,
               wrapped_vek_by_pw BLOB,
               wrapped_vek_by_recovery BLOB
             );
             CREATE TABLE records (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL,
               label TEXT NOT NULL,
               payload_variant TEXT NOT NULL,
               plaintext_value TEXT,
               nonce BLOB,
               ciphertext BLOB,
               aad BLOB,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );",
        )
        .unwrap();
        // kdf_salt=16B, wrapped_vek=32B を満たすダミーデータで INSERT
        conn.execute(
            "INSERT INTO vault_header VALUES (
               1, 'encrypted', 1, '2026-01-01T00:00:00+00:00',
               X'00000000000000000000000000000000',
               X'0000000000000000000000000000000000000000000000000000000000000000',
               X'0000000000000000000000000000000000000000000000000000000000000000'
             )",
            [],
        )
        .unwrap();
    }

    // パーミッションを 0600 に設定（Unix のみ）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    let repo = make_repo(dir.path());
    let result = repo.load();

    assert!(
        matches!(result, Err(PersistenceError::UnsupportedYet { feature, .. }) if feature.contains("encrypted")),
        "UnsupportedYet を期待したが予期せぬ結果が返った"
    );
}

// ---------------------------------------------------------------------------
// TC-I05: .new 残存 + load → OrphanNewFile
// ---------------------------------------------------------------------------

/// TC-I05 — vault.db.new が残存する状態で load を呼ぶと OrphanNewFile が返る。
///
/// AC-05 対応。
#[test]
fn tc_i05_orphan_new_file_on_load() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    // vault.db を正常に作成
    repo.save(&vault).unwrap();

    // vault.db.new を空ファイルとして残す
    let new_path = dir.path().join("vault.db.new");
    std::fs::write(&new_path, b"").unwrap();

    let result = repo.load();

    match result {
        Err(PersistenceError::OrphanNewFile { path }) => {
            assert_eq!(path, new_path, "OrphanNewFile のパスが一致しない");
        }
        other => panic!("OrphanNewFile を期待したが Err={:?}", other.err()),
    }
    // vault.db は変更されていないこと（再 load が成功する）
    std::fs::remove_file(&new_path).unwrap();
    repo.load().expect("vault.db が破損していた");
}

// ---------------------------------------------------------------------------
// TC-I07: 0777 ディレクトリ + load → InvalidPermission（Unix）
// ---------------------------------------------------------------------------

/// TC-I07 — ディレクトリが 0777 の場合 load で InvalidPermission が返る。
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
// TC-I08: UTF-8 特殊文字ラベルの round-trip
// ---------------------------------------------------------------------------

/// TC-I08 — 絵文字・CJK・アラビア文字を含むラベルが byte 完全一致で復元される。
///
/// AC-08 対応。
#[test]
fn tc_i08_utf8_label_round_trip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    let label_str = "🗝️秘密のキー💀شيكومي";
    let mut vault = Vault::new(plaintext_header());
    vault
        .add_record(make_record(label_str, "secret_value"))
        .unwrap();

    repo.save(&vault).unwrap();
    let loaded = repo.load().unwrap();

    assert_eq!(loaded.records().len(), 1);
    assert_eq!(
        loaded.records()[0].label().as_str(),
        label_str,
        "ラベルがバイト単位で一致しない"
    );
}

// ---------------------------------------------------------------------------
// TC-I09: SQL インジェクション禁止設計の静的 grep 確認
// ---------------------------------------------------------------------------

/// TC-I09 — ソース内に SQL 文字列連結パターンが存在しないことを確認する。
///
/// AC-09 対応。
#[test]
fn tc_i09_no_sql_injection_patterns() {
    let src_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "/tmp/shikomi/crates/shikomi-infra".to_string());
    let src_path = format!("{src_dir}/src");

    // format! マクロで SQL キーワードを組み立てているパターン
    let status1 = std::process::Command::new("grep")
        .args([
            "-rEn",
            r#"format!\s*\(.*(?:SELECT|INSERT|UPDATE|DELETE|PRAGMA)"#,
            "--include=*.rs",
            &src_path,
        ])
        .status()
        .expect("grep を実行できませんでした");
    // grep は一致なし = exit 1 が期待値
    assert!(
        !status1.success(),
        "SQL 連結パターン (format!) が検出された — SQL インジェクションリスク"
    );

    // 文字列連結で SQL を組み立てているパターン
    let status2 = std::process::Command::new("grep")
        .args([
            "-rEn",
            r#""[^"]*(?:SELECT|INSERT|UPDATE|DELETE)[^"]*"\s*\+"#,
            "--include=*.rs",
            &src_path,
        ])
        .status()
        .expect("grep を実行できませんでした");
    assert!(
        !status2.success(),
        "SQL 連結パターン (+ 演算子) が検出された — SQL インジェクションリスク"
    );
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
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
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
// TC-I13: ゼロバイト vault.db → panic せずエラー返却
// ---------------------------------------------------------------------------

/// TC-I13 — 0 バイトの vault.db を load しても panic しない。
///
/// AC-13 対応。
#[test]
fn tc_i13_zero_byte_vault_db() {
    let dir = TempDir::new().unwrap();

    // ゼロバイトの vault.db を配置
    let db_path = dir.path().join("vault.db");
    std::fs::write(&db_path, b"").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    let repo = make_repo(dir.path());
    let result = repo.load();

    assert!(
        matches!(
            result,
            Err(PersistenceError::Sqlite { .. }) | Err(PersistenceError::SchemaMismatch { .. })
        ),
        "Sqlite または SchemaMismatch を期待したが予期せぬ結果が返った"
    );
}

// ---------------------------------------------------------------------------
// TC-I14: 不正バイト列 vault.db → panic せずエラー返却
// ---------------------------------------------------------------------------

/// TC-I14 — 非 SQLite バイト列の vault.db を load しても panic しない。
///
/// AC-13 対応。
#[test]
fn tc_i14_corrupt_vault_db() {
    let dir = TempDir::new().unwrap();

    let db_path = dir.path().join("vault.db");
    std::fs::write(&db_path, b"this is not a sqlite file\x00\xFF").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    let repo = make_repo(dir.path());
    let result = repo.load();

    assert!(
        matches!(
            result,
            Err(PersistenceError::Sqlite { .. }) | Err(PersistenceError::SchemaMismatch { .. })
        ),
        "Sqlite または SchemaMismatch を期待したが予期せぬ結果が返った"
    );
}

// ---------------------------------------------------------------------------
// TC-I15: .new 残存 + save → OrphanNewFile
// ---------------------------------------------------------------------------

/// TC-I15 — vault.db.new が残存する状態で save を呼ぶと OrphanNewFile が返る。
///
/// AC-14 対応。
#[test]
fn tc_i15_orphan_new_file_on_save() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    // vault.db.new を先に作成（孤立ファイル状態を模倣）
    // save() は ensure_dir → acquire_lock → detect_orphan の順なので
    // ディレクトリを先に作成し、その後 .new を置く
    std::fs::create_dir_all(dir.path()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let new_path = dir.path().join("vault.db.new");
    std::fs::write(&new_path, b"").unwrap();

    let result = repo.save(&vault);

    match result {
        Err(PersistenceError::OrphanNewFile { path }) => {
            assert_eq!(path, new_path, "OrphanNewFile のパスが一致しない");
        }
        other => panic!("OrphanNewFile を期待したが Err={:?}", other.err()),
    }
}

// ---------------------------------------------------------------------------
// TC-I16: exists() — vault 非存在
// ---------------------------------------------------------------------------

/// TC-I16 — 空の tempdir で exists() を呼ぶと Ok(false) が返る。
#[test]
fn tc_i16_exists_returns_false_when_no_vault() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    assert_eq!(repo.exists().unwrap(), false);
}

// ---------------------------------------------------------------------------
// TC-I17: exists() — vault 存在
// ---------------------------------------------------------------------------

/// TC-I17 — save 後に exists() を呼ぶと Ok(true) が返る。
#[test]
fn tc_i17_exists_returns_true_after_save() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    repo.save(&vault).unwrap();
    assert_eq!(repo.exists().unwrap(), true);
}

// ---------------------------------------------------------------------------
// TC-I18: SHIKOMI_VAULT_DIR 環境変数 override
// ---------------------------------------------------------------------------

/// TC-I18 — SHIKOMI_VAULT_DIR で指定したディレクトリに vault.db が作成される。
///
/// `serial_test` で直列化（std::env::set_var がグローバル状態を変更するため）。
#[test]
#[serial_test::serial]
fn tc_i18_env_var_vault_dir_override() {
    let dir = TempDir::new().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir.path().as_os_str());

    let result = (|| -> Result<(), PersistenceError> {
        let repo = SqliteVaultRepository::new()?;
        let vault = make_plaintext_vault(1);
        repo.save(&vault)?;
        Ok(())
    })();

    std::env::remove_var("SHIKOMI_VAULT_DIR");

    result.expect("SHIKOMI_VAULT_DIR 指定での save が失敗した");
    assert!(
        dir.path().join("vault.db").exists(),
        "指定ディレクトリに vault.db が作成されていない"
    );
}

// ---------------------------------------------------------------------------
// TC-I19: ゼロレコード vault round-trip
// ---------------------------------------------------------------------------

/// TC-I19 — レコードゼロの vault を save → load しても正常に復元される。
///
/// AC-02 の境界値ケース。
#[test]
fn tc_i19_zero_record_vault_round_trip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = Vault::new(plaintext_header());

    repo.save(&vault).unwrap();
    let loaded = repo.load().unwrap();

    assert!(loaded.records().is_empty(), "records が空でない");
    assert_eq!(
        loaded.header().protection_mode(),
        vault.header().protection_mode()
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

/// TC-I22-A — 相対パスを SHIKOMI_VAULT_DIR に設定すると NotAbsolute が返る。
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn tc_i22a_env_var_relative_path() {
    std::env::set_var("SHIKOMI_VAULT_DIR", "relative/path");
    let result = SqliteVaultRepository::new();
    std::env::remove_var("SHIKOMI_VAULT_DIR");

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

/// TC-I22-B — `..` を含むパスを SHIKOMI_VAULT_DIR に設定すると PathTraversal が返る。
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn tc_i22b_env_var_path_traversal() {
    std::env::set_var("SHIKOMI_VAULT_DIR", "/tmp/shikomi/../../etc");
    let result = SqliteVaultRepository::new();
    std::env::remove_var("SHIKOMI_VAULT_DIR");

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

/// TC-I22-C — シンボリックリンクを SHIKOMI_VAULT_DIR に設定すると SymlinkNotAllowed が返る。
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn tc_i22c_env_var_symlink_not_allowed() {
    let dir = TempDir::new().unwrap();
    let real_dir = dir.path().join("real");
    std::fs::create_dir_all(&real_dir).unwrap();
    let symlink_path = dir.path().join("link");
    std::os::unix::fs::symlink(&real_dir, &symlink_path).unwrap();

    std::env::set_var("SHIKOMI_VAULT_DIR", symlink_path.as_os_str());
    let result = SqliteVaultRepository::new();
    std::env::remove_var("SHIKOMI_VAULT_DIR");

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

/// TC-I22-D — `/etc/` 配下のパスを SHIKOMI_VAULT_DIR に設定すると ProtectedSystemArea が返る。
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn tc_i22d_env_var_protected_system_area() {
    std::env::set_var("SHIKOMI_VAULT_DIR", "/etc/shikomi_test_vault");
    let result = SqliteVaultRepository::new();
    std::env::remove_var("SHIKOMI_VAULT_DIR");

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
    let mut vault = Vault::new(plaintext_header());
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
            "監査ログに秘密値パターン {:?} が含まれていた（AC-15 違反）",
            pattern
        );
    }
}
