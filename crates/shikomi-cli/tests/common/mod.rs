//! 結合 / E2E テスト共通ヘルパー。
//!
//! - `fresh_repo`: tempfile ベースで `SqliteVaultRepository` を生成
//! - `fixed_time`: 決定的テストのための固定時刻
//! - `fixtures`: 暗号化 vault フィクスチャ生成（`fixtures::create_encrypted_vault`）
//!
//! テスト戦略ガイド準拠: 結合テストは実 SQLite + `tempfile` を使い、
//! モック `VaultRepository` は使わない。
//!
//! トレーサビリティ:
//! - 設計書 `docs/features/cli-vault-commands/test-design/integration.md §3`
//! - 対応 Issue: #20

#![allow(dead_code)] // 各 integration test file で部分的に使用されるため

use std::path::Path;

use shikomi_infra::persistence::SqliteVaultRepository;
use tempfile::TempDir;
use time::OffsetDateTime;

pub mod fixtures;

/// 一時ディレクトリ + `SqliteVaultRepository` のペアを返す。
///
/// 返却される `TempDir` が `Drop` されると一時ディレクトリが削除されるため、
/// 呼び出し側がスコープ中保持すること。
///
/// **permission**: Unix 環境では `TempDir` のデフォルトは 0755 だが、infra 側の
/// `PermissionGuard::verify_dir` は 0700 を要求する。そのため `chmod 0700` に
/// 揃えて `load()` 経路を通す。
pub fn fresh_repo() -> (TempDir, SqliteVaultRepository) {
    let dir = TempDir::new().expect("failed to create tempdir");
    tighten_perms_unix(dir.path());
    let repo =
        SqliteVaultRepository::from_directory(dir.path()).expect("failed to create repository");
    (dir, repo)
}

/// `TempDir` 配下のパーミッションを Unix では `0700` に揃える（no-op on non-Unix）。
#[cfg(unix)]
pub fn tighten_perms_unix(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(path, perms).expect("chmod 0700");
}

#[cfg(not(unix))]
pub fn tighten_perms_unix(_path: &Path) {}

/// 決定的テスト用の固定時刻（UNIX_EPOCH + 1 時間）。
///
/// `SystemTime::now()` に依存しない固定値で、テスト間の再現性を保つ。
#[must_use]
pub fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
}

/// 任意オフセットの固定時刻を返す（`created_at < updated_at` の検証用）。
#[must_use]
pub fn fixed_time_at(hours: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(hours)
}

/// 指定ディレクトリで平文 vault を初期化し、1 件の Text レコードを追加する。
///
/// ラウンドトリップ検証の前提を整える共通セットアップ。追加したレコードの UUID 文字列を返す。
pub fn init_vault_with_one_text_record(dir: &Path, label: &str, value: &str) -> String {
    use shikomi_core::{
        Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
        VaultVersion,
    };
    use shikomi_infra::persistence::VaultRepository;
    use uuid::Uuid;

    let repo = SqliteVaultRepository::from_directory(dir).expect("from_directory");
    let now = fixed_time();
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, now).unwrap();
    let mut vault = Vault::new(header);
    let id = RecordId::new(Uuid::now_v7()).unwrap();
    let record = Record::new(
        id.clone(),
        RecordKind::Text,
        RecordLabel::try_new(label.to_owned()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string(value.to_owned())),
        now,
    );
    vault.add_record(record).unwrap();
    repo.save(&vault).unwrap();
    id.to_string()
}
