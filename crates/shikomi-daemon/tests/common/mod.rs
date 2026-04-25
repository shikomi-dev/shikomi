//! daemon 結合テスト共通ヘルパー。
//!
//! - `fresh_repo`: `tempfile::TempDir` + `SqliteVaultRepository` のペアを返す
//! - `empty_vault_mutex`: `Arc<Mutex<Vault>>` を空の平文 vault で初期化
//! - `fixed_time`: 決定的テスト用の固定時刻
//! - `peer_mock`: `PeerCredentialSource` trait の in-test 実装
//!
//! 設計根拠: `docs/features/daemon-ipc/test-design/integration.md §8 + §8.1`
//! 対応 Issue: #26

#![allow(dead_code)] // 各 integration test file で部分的に使用されるため

use std::sync::Arc;

use shikomi_core::{Vault, VaultHeader, VaultVersion};
use shikomi_infra::persistence::SqliteVaultRepository;
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::sync::Mutex;

pub mod peer_mock;

/// 決定的テスト用の固定時刻（UNIX_EPOCH + 1 時間）。
#[must_use]
pub fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
}

/// 一時ディレクトリ + `SqliteVaultRepository` のペア。
pub fn fresh_repo() -> (TempDir, SqliteVaultRepository) {
    let dir = TempDir::new().expect("failed to create tempdir");
    tighten_perms_unix(dir.path());
    let repo =
        SqliteVaultRepository::from_directory(dir.path()).expect("failed to create repository");
    (dir, repo)
}

/// 空の平文 `Vault` を `Arc<Mutex<Vault>>` で返す。
pub fn empty_vault_mutex() -> Arc<Mutex<Vault>> {
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_time()).unwrap();
    Arc::new(Mutex::new(Vault::new(header)))
}

/// Unix: TempDir のパーミッションを 0700 に揃える（infra の要求）。
#[cfg(unix)]
pub fn tighten_perms_unix(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(path, perms).expect("chmod 0700");
}

#[cfg(not(unix))]
pub fn tighten_perms_unix(_path: &std::path::Path) {}
