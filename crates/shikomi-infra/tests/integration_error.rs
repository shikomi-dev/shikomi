//! vault-persistence 結合テスト — TC-I03〜I05, I13〜I15
//! 異常系・エラーハンドリング
mod helpers;
use helpers::{make_plaintext_vault, make_repo};
use shikomi_core::{KdfSalt, Vault, VaultHeader, VaultVersion, WrappedVek};
use shikomi_infra::persistence::PersistenceError;
use tempfile::TempDir;
use time::OffsetDateTime;

// ---------------------------------------------------------------------------
// TC-I03: 暗号化モード vault を save → UnsupportedYet
// ---------------------------------------------------------------------------

/// TC-I03 — 暗号化モード vault を save すると `UnsupportedYet` が返る。
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

/// TC-I04 — `protection_mode='encrypted'` の vault.db を load すると `UnsupportedYet` が返る。
///
/// AC-04 対応。
#[test]
fn tc_i04_encrypted_vault_db_load_unsupported() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("vault.db");

    // Unix: vault.db を rusqlite で直接作成（CHECK 制約なし DDL でサイズ要件を満たす暗号化行を挿入）
    #[cfg(unix)]
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
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    // Windows: load() が verify_dir / verify_file を呼ぶため、save() で DACL を事前設定し
    //          その後 vault.db を暗号化モードに UPDATE する（CHECK 制約を満足させる）
    #[cfg(windows)]
    {
        let repo_setup = make_repo(dir.path());
        repo_setup.save(&make_plaintext_vault(0)).unwrap();
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE vault_header SET \
               protection_mode = 'encrypted', \
               kdf_salt = X'00000000000000000000000000000000', \
               wrapped_vek_by_pw = \
                 X'0000000000000000000000000000000000000000000000000000000000000000', \
               wrapped_vek_by_recovery = \
                 X'0000000000000000000000000000000000000000000000000000000000000000' \
             WHERE id = 1",
            [],
        )
        .unwrap();
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

/// TC-I05 — vault.db.new が残存する状態で load を呼ぶと `OrphanNewFile` が返る。
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
            // Windows では VaultPaths が canonicalize するため、両パスを正規化して比較する
            // （raw TempDir パスに 8.3 短縮名が含まれる場合と \\?\ プレフィックスの差異を吸収）
            let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
            let canonical_expected = new_path.canonicalize().unwrap_or_else(|_| new_path.clone());
            assert_eq!(
                canonical_path, canonical_expected,
                "OrphanNewFile のパスが一致しない"
            );
        }
        other => panic!("OrphanNewFile を期待したが Err={:?}", other.err()),
    }
    // vault.db は変更されていないこと（再 load が成功する）
    std::fs::remove_file(&new_path).unwrap();
    repo.load().expect("vault.db が破損していた");
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

    // Windows: load() が verify_dir / verify_file を呼ぶため、
    //          save() で DACL を事前設定する（DACL はファイル内容と独立した NTFS 属性）
    #[cfg(windows)]
    {
        let repo_setup = make_repo(dir.path());
        repo_setup.save(&make_plaintext_vault(0)).unwrap();
    }

    // ゼロバイトの vault.db を配置（Windows: DACL は保持されたまま内容のみ上書き）
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

/// TC-I14 — 非 `SQLite` バイト列の vault.db を load しても panic しない。
///
/// AC-13 対応。
#[test]
fn tc_i14_corrupt_vault_db() {
    let dir = TempDir::new().unwrap();

    // Windows: load() が verify_dir / verify_file を呼ぶため、
    //          save() で DACL を事前設定する（DACL はファイル内容と独立した NTFS 属性）
    #[cfg(windows)]
    {
        let repo_setup = make_repo(dir.path());
        repo_setup.save(&make_plaintext_vault(0)).unwrap();
    }

    // 不正バイト列で vault.db を上書き（Windows: DACL は保持されたまま内容のみ上書き）
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

/// TC-I15 — vault.db.new が残存する状態で save を呼ぶと `OrphanNewFile` が返る。
///
/// AC-14 対応。
#[test]
fn tc_i15_orphan_new_file_on_save() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    // vault.db.new を先に作成（孤立ファイル状態を模倣）
    //
    // Windows: ensure_dir が PROTECTED_DACL_SECURITY_INFORMATION をディレクトリに適用すると
    //          既存の「継承 ACE のみ」を持つ子ファイルの ACE が除去され inaccessible になる。
    //          そのため先に save() を呼び出してディレクトリ DACL を確定させてから .new を置く。
    //          確定後に作成したファイルはプロセストークンのデフォルト DACL（explicit ACE）を持つため
    //          二回目の ensure_dir の伝播によって除去されない。
    #[cfg(windows)]
    {
        repo.save(&vault).unwrap();
        // vault.db は残っていてもよい（detect_orphan は .new の存在を先に確認する）
    }

    // Unix: ディレクトリを先に 0700 で作成し、その後 .new を置く
    #[cfg(unix)]
    {
        std::fs::create_dir_all(dir.path()).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    let new_path = dir.path().join("vault.db.new");
    std::fs::write(&new_path, b"").unwrap();

    let result = repo.save(&vault);

    match result {
        Err(PersistenceError::OrphanNewFile { path }) => {
            // Windows では VaultPaths が canonicalize するため、両パスを正規化して比較する
            let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
            let canonical_expected = new_path.canonicalize().unwrap_or_else(|_| new_path.clone());
            assert_eq!(
                canonical_path, canonical_expected,
                "OrphanNewFile のパスが一致しない"
            );
        }
        other => panic!("OrphanNewFile を期待したが Err={:?}", other.err()),
    }
}
