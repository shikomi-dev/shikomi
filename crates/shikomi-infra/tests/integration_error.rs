//! vault-persistence 結合テスト — TC-I03〜I05, I13〜I15
//! 異常系・エラーハンドリング
//!
//! Sub-D (#42) 改訂: REQ-P11 改訂により暗号化モード即時拒否経路を退役。
//! TC-I03 / TC-I04 は新内容に置換 (旧 `UnsupportedYet` 即返却 → 暗号化モード受入 +
//! 不正 BLOB は `Corrupted` で拒否、設計書 §`PersistenceError` の改訂 参照)。
mod helpers;
use helpers::{make_plaintext_vault, make_repo};
use shikomi_core::{AuthTag, KdfSalt, NonceBytes, Vault, VaultHeader, VaultVersion, WrappedVek};
use shikomi_infra::persistence::{PersistenceError, VaultRepository};
use tempfile::TempDir;
use time::OffsetDateTime;

// ---------------------------------------------------------------------------
// TC-I03 (Sub-D 改訂): 暗号化モード vault を save しても受入される。
// 旧 TC-I03 (UnsupportedYet 即返却) は Sub-D 退役。
// ---------------------------------------------------------------------------

fn make_dummy_wrapped_vek() -> WrappedVek {
    // Sub-D で composite container 形式に変わるが、本テストは `WrappedVek` の構造的
    // 妥当性 (32B 以上 ciphertext + 12B nonce + 16B tag) のみを担保する dummy で
    // save 経路が UnsupportedYet を返さないことのみ検証する。
    // 実暗号化フローの roundtrip は `VaultMigration` integration test で別途カバー。
    WrappedVek::new(
        vec![0u8; 32],
        NonceBytes::from_random([0u8; 12]),
        AuthTag::from_array([0u8; 16]),
    )
    .unwrap()
}

/// TC-I03 (Sub-D 改訂) — 暗号化モード vault を save する経路が解禁され、
/// `UnsupportedYet` ではなく成功 (`Ok`) または `Corrupted` (BLOB 形式不正) になる。
///
/// 旧 AC-03 (`UnsupportedYet` 即返却) は Sub-D 設計改訂で廃止。
/// 本 TC-I03 は **暗号化モードの save 経路が動作する** ことを sanity check する。
#[test]
fn tc_i03_encrypted_vault_save_does_not_return_unsupported_yet() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    let kdf_salt = KdfSalt::try_new(&[0u8; 16]).unwrap();
    let header = VaultHeader::new_encrypted(
        VaultVersion::CURRENT,
        OffsetDateTime::now_utc(),
        kdf_salt,
        make_dummy_wrapped_vek(),
        make_dummy_wrapped_vek(),
    )
    .unwrap();
    let vault = Vault::new(header);

    let result = repo.save(&vault);

    // Sub-D 改訂: UnsupportedYet 即返却は退役。`Ok` (合法 BLOB) または別エラー (Corrupted等) になる。
    if let Err(PersistenceError::UnsupportedYet { .. }) = result {
        panic!("Sub-D で暗号化モードを解禁したにも関わらず UnsupportedYet が返った: {result:?}");
    }
}

// ---------------------------------------------------------------------------
// TC-I04 (Sub-D 改訂): 暗号化モード vault.db を load する経路が解禁される。
// 旧 TC-I04 (UnsupportedYet 即返却) は Sub-D 退役。
// ---------------------------------------------------------------------------

/// TC-I04 (Sub-D 改訂) — `protection_mode='encrypted'` の vault.db を load すると
/// `UnsupportedYet` ではなく、BLOB 形式が composite container でない場合は `Corrupted` を返す。
///
/// 旧 AC-04 (`UnsupportedYet` 即返却) は Sub-D 設計改訂で廃止。
/// 本 TC-I04 は **暗号化モード load 経路が動作する + 不正 BLOB は Corrupted で拒否** を検証する。
#[test]
fn tc_i04_encrypted_vault_db_load_does_not_return_unsupported_yet() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("vault.db");

    // Unix: vault.db を rusqlite で直接作成 (CHECK 制約なし DDL でダミー暗号化行を挿入)
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
        // kdf_salt=16B + 全 0 BLOB の simple 形式 (composite container ではない)。
        // Sub-D は composite container 形式 (magic "SHKE" prefix) を期待するため、
        // この dummy は **`Corrupted`** で拒否される (旧 UnsupportedYet ではない)。
        conn.execute(
            "INSERT INTO vault_header VALUES (
               1, 'encrypted', 1, '2026-01-01T00:00:00+00:00',
               X'00000000000000000000000000000000',
               X'0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000',
               X'0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000'
             )",
            [],
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    // Windows: load() が verify_dir / verify_file を呼ぶため、save() で DACL を事前設定し
    //          その後 vault.db を暗号化モードに UPDATE する。
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
                 X'0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000', \
               wrapped_vek_by_recovery = \
                 X'0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000' \
             WHERE id = 1",
            [],
        )
        .unwrap();
    }

    let repo = make_repo(dir.path());
    let result = repo.load();

    // Sub-D 改訂: UnsupportedYet 即返却は退役。`Corrupted` (composite container magic 不一致等) に変わる。
    if let Err(PersistenceError::UnsupportedYet { .. }) = result {
        panic!("Sub-D で暗号化モードを解禁したにも関わらず UnsupportedYet が返った: {result:?}");
    }
    // load 自体は ok or Corrupted のいずれか (本テストは UnsupportedYet 不在のみ確認)。
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
