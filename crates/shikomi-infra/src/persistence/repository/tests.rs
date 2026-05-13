//! `SqliteVaultRepository` ユニットテスト群。

use shikomi_core::{Vault, VaultHeader, VaultVersion};
use tempfile::TempDir;

use super::SqliteVaultRepository;
use crate::persistence::{error::PersistenceError, VaultRepository};

// ---------------------------------------------------------------------------
// TC-U-REPO-01: prepare_dir — 存在しないディレクトリを作成する
// ---------------------------------------------------------------------------

#[test]
fn tc_u_repo_01_prepare_dir_creates_directory() {
    let base = TempDir::new().unwrap();
    let vault_dir = base.path().join("vault_new");

    // 事前条件: ディレクトリが存在しない
    assert!(!vault_dir.exists(), "事前条件: ディレクトリが未作成");

    let repo = SqliteVaultRepository::from_directory(&vault_dir).unwrap();
    repo.prepare_dir().expect("prepare_dir は成功すべき");

    assert!(
        vault_dir.is_dir(),
        "prepare_dir 後: ディレクトリが作成されるべき"
    );
}

// ---------------------------------------------------------------------------
// TC-U-REPO-02: prepare_dir — 既存ディレクトリに冪等
// ---------------------------------------------------------------------------

#[test]
fn tc_u_repo_02_prepare_dir_is_idempotent_on_existing_dir() {
    let dir = TempDir::new().unwrap();
    let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();

    // 2 回呼び出しても失敗しない
    repo.prepare_dir()
        .expect("1 回目: prepare_dir は成功すべき");
    repo.prepare_dir()
        .expect("2 回目: prepare_dir は冪等であるべき");
}

// ---------------------------------------------------------------------------
// TC-UT-140〜142: SqliteVaultRepository::load_or_create_plaintext
// 設計書: docs/features/daemon-ipc/test-design/unit.md §2.18
// REQ-DAEMON-028 / Issue #80
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// TC-UT-140: vault.db 存在時 — 既存 vault をロードして返す（ログ出力なし）
// ---------------------------------------------------------------------------

#[test]
#[tracing_test::traced_test]
fn tc_ut_140_load_or_create_plaintext_returns_existing_vault_without_log() {
    // REQ-DAEMON-028 / Issue #80
    let dir = TempDir::new().unwrap();
    let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
    repo.prepare_dir().expect("prepare_dir は成功すべき");

    // 事前: vault.db を保存
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, time::OffsetDateTime::now_utc())
        .expect("header は常に有効");
    let initial_vault = Vault::new(header);
    repo.save(&initial_vault).expect("初期 vault の保存");

    // act
    let result = repo.load_or_create_plaintext();

    // assert
    let vault = result.expect("既存 vault.db があれば Ok が返るべき");
    assert!(vault.records().is_empty(), "初期 vault はレコード 0 件");
    assert!(
        matches!(
            vault.protection_mode(),
            shikomi_core::ProtectionMode::Plaintext
        ),
        "初期 vault は plaintext モードであるべき"
    );
    // vault.db が既存なので生成ログは出力されない
    assert!(
        !logs_contain("vault not found"),
        "既存 vault.db がある場合、生成ログは出力されるべきでない"
    );
}

// ---------------------------------------------------------------------------
// TC-UT-141: vault.db 不在 — 空 plaintext vault を生成してログ出力
// ---------------------------------------------------------------------------

#[test]
#[tracing_test::traced_test]
fn tc_ut_141_load_or_create_plaintext_creates_empty_vault_when_absent() {
    // REQ-DAEMON-028 / Issue #80
    let dir = TempDir::new().unwrap();
    let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
    repo.prepare_dir().expect("prepare_dir は成功すべき");
    // vault.db を意図的に作成しない

    // act
    let result = repo.load_or_create_plaintext();

    // assert
    let vault = result.expect("vault.db 不在でも Ok (空 vault 生成) が返るべき");
    assert!(
        vault.records().is_empty(),
        "新規生成した vault はレコード 0 件であるべき"
    );
    assert!(
        matches!(
            vault.protection_mode(),
            shikomi_core::ProtectionMode::Plaintext
        ),
        "新規生成した vault は plaintext モードであるべき"
    );
    assert!(
        logs_contain("vault not found; created new plaintext vault at"),
        "生成ログ 'vault not found; created new plaintext vault at' が出力されるべき"
    );
    assert!(
        logs_contain("hint: to enable encryption"),
        "暗号化ヒントログが出力されるべき"
    );

    // 横串アサート（冪等性）: vault.db 生成後に再呼出しても Ok が返る
    let vault2 = repo
        .load_or_create_plaintext()
        .expect("2 回目の呼出し: 既存 vault.db を load して Ok が返るべき");
    assert!(
        vault2.records().is_empty(),
        "2 回目の呼出し: 既存 vault のレコード 0 件"
    );
}

// ---------------------------------------------------------------------------
// TC-UT-142: 書き込み不可ディレクトリ（Unix 限定）— InvalidPermission を返す
// ---------------------------------------------------------------------------
//
// ディレクトリのパーミッションを 0o500（読み取り専用）に変更後に `load_or_create_plaintext`
// を呼ぶと `PersistenceError::InvalidPermission` が返る。
//
// NOTE: `load_or_create_plaintext` 内部で最初に呼ばれる `load()` が `load_inner()` 経由で
// `PermissionGuard::verify_dir` を実行する。`0o500 != 0o700` を検出した時点で
// `PersistenceError::InvalidPermission` で早期 Fail Fast し、`save` フェーズには到達しない。
//
// NOTE (Windows): write-protected directory test skipped on Windows (requires admin privileges)

#[cfg(unix)]
#[test]
fn tc_ut_142_load_or_create_plaintext_returns_invalid_permission_when_dir_not_writable() {
    // REQ-DAEMON-028 / Issue #80
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
    // vault.db は作成しない
    // ディレクトリを 0o500（所有者 r-x）に変更して書き込み不可にする
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500))
        .expect("chmod 0o500 に成功すべき");

    // act
    let result = repo.load_or_create_plaintext();

    // cleanup: TempDir::drop() が削除できるようパーミッション復元
    let _ = std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700));

    // assert: verify_dir が先に InvalidPermission を返す（Io(_) ではない）
    assert!(
        matches!(result, Err(PersistenceError::InvalidPermission { .. })),
        "InvalidPermission が返るべきだが: {:?}",
        result.err()
    );
}
