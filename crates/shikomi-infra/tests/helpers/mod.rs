#![allow(dead_code)] // needed: not all helpers used in all test binaries

use std::path::Path;
use std::sync::Mutex;

use shikomi_core::{
    Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
    VaultVersion,
};
use shikomi_infra::persistence::SqliteVaultRepository;
use time::OffsetDateTime;
use uuid::Uuid;

/// 環境変数 `SHIKOMI_VAULT_DIR` へのアクセスをプロセス全体で直列化するミューテックス。
///
/// `with_dir` は `pub(crate)` のため外部テストからアクセス不可。
/// `ENV_MUTEX` を介してプロセス内での環境変数アクセスを直列化し、
/// `SHIKOMI_VAULT_DIR` を設定した後に `new()` を呼ぶことで同等の動作を実現する。
pub static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// tempdir を使った `SqliteVaultRepository` を構築する。
///
/// `SHIKOMI_VAULT_DIR` 環境変数経由で `SqliteVaultRepository::new()` を呼び出す。
/// `ENV_MUTEX` でプロセス内アクセスを直列化し、並列テストでの env 競合を防ぐ。
pub fn make_repo(dir: &Path) -> SqliteVaultRepository {
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir);
    let repo = SqliteVaultRepository::new().unwrap();
    std::env::remove_var("SHIKOMI_VAULT_DIR");
    repo
}

/// 平文モードの `VaultHeader` を作る。
pub fn plaintext_header() -> VaultHeader {
    VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap()
}

/// 平文 `Record` を 1 件作る。
pub fn make_record(label: &str, value: &str) -> Record {
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
pub fn make_plaintext_vault(n: usize) -> Vault {
    let mut vault = Vault::new(plaintext_header());
    for i in 0..n {
        vault
            .add_record(make_record(&format!("label-{i}"), &format!("value-{i}")))
            .unwrap();
    }
    vault
}
