#![allow(dead_code)] // needed: not all helpers used in all test binaries

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use shikomi_core::{
    Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
    VaultVersion,
};
use shikomi_infra::persistence::vault_migration::MigrationError;
use shikomi_infra::persistence::{
    AtomicWriteStage, PersistenceError, SqliteVaultRepository, VaultRepository,
};
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

// ---------------------------------------------------------------------------
// Bug-G-005 Option K: テスト側 rename retry ヘルパ
// ---------------------------------------------------------------------------
//
// 経緯:
//   Win CI ランナー (windows-latest) で `vault.db` ハンドルに対する
//   原因不明の遅延 (~1570ms 一定、Bug-G-002〜004 で 3 ラウンド articulate) により、
//   実装側の指数バックオフ retry budget (`security.md` §jitter SSoT 最悪 ~1675ms) 内側でも
//   `vault_migration_integration` 5 件 + TC-I29 主 / TC-I29-B が `outcome=exhausted` で
//   flaky に fail する事象を、**テスト側の retry でのみ** 吸収する (Option K)。
//
// 設計原則:
//   - **実装側 SSoT は据置** — `security.md` §jitter 「最悪 ~1675ms」は本番ユーザー契約の
//     ままで一切変更しない。本ヘルパは CI 環境特性に対するテスト fixture であり、
//     「実装の仕様外」を補正する compromise として articulate する
//     (`docs/features/vault-persistence/test-design/integration.md` v8.3)。
//   - **責務分離** — 本番 race 吸収は実装側 retry が担当、CI 異常ハンドル遅延は
//     テスト fixture が担当。両者を混ぜない。
//   - **対象を絞る** — `AtomicWriteStage::Rename` の rename retry exhausted 経路のみ retry。
//     その他 (`Sqlite` / 非 Rename stage / domain error 等) は即 panic で本来の test 失敗を
//     露出させる (Fail Fast、テストゲートの責務を曇らせない)。
//   - **線形バックオフ** — `500ms × attempt` (500/1000/1500/2000ms、最大 4 回再試行 = 5 attempts)。
//     CI 観測の 1570ms 遅延を attempt 3〜4 で確実に超える設定。
//
// 解除条件:
//   `cargo test --ignored` 専用 CI ジョブ整備、または真犯人 (rusqlite handle 遅延 / MDE /
//   AMSI / 未知 filter driver 等) が別 PR で根治された場合に本ヘルパを撤去する
//   (test-design v8.3 §運用ルール)。

/// テスト側 retry の最大試行回数 (1 回目 + 4 回再試行 = 5 attempts)。
const TEST_RETRY_MAX_ATTEMPTS: u32 = 5;

/// テスト側 retry の線形バックオフ単位 (各試行で `unit × attempt` ms スリープ)。
///
/// CI 実測の 1570ms 一定遅延を attempt 2〜4 (累積 500+1000+1500=3000ms) で確実に超える設定。
const TEST_RETRY_BACKOFF_UNIT_MS: u64 = 500;

/// rename retry 共通の retry loop コア（DRY、Bug-G-005 Option K）。
///
/// `save_with_test_rename_retry` / `migration_op_with_test_rename_retry` の重複していた
/// retry loop / 線形バックオフ計算 / panic 文言を本関数に集約する
/// （ペテルギウス再レビュー指摘 §DRY違反）。
///
/// 引数:
/// - `label`: 失敗時 panic / `eprintln!` 文言に挿入する操作識別子
/// - `op`: 試行する操作。`Ok(T)` で即返却、`Err(E)` で is_retryable に委譲
/// - `is_retryable`: `Err(E)` が rename retry exhausted 経路（再試行価値あり）か判定する述語
///
/// retry 戦略:
/// - 最大 `TEST_RETRY_MAX_ATTEMPTS` 回試行
/// - 線形バックオフ `TEST_RETRY_BACKOFF_UNIT_MS × attempt` ms
/// - `is_retryable` が false のエラー / 最終 attempt 失敗 → 即 panic（Fail Fast）
///
/// # Panics
///
/// - 全 attempt で retry 失敗: panic で test fail
/// - `is_retryable` が false のエラー: 即 panic で test fail
fn retry_with_backoff<T, E: std::fmt::Debug>(
    label: &str,
    mut op: impl FnMut() -> Result<T, E>,
    is_retryable: impl Fn(&E) -> bool,
) -> T {
    for attempt in 1..=TEST_RETRY_MAX_ATTEMPTS {
        match op() {
            Ok(v) => return v,
            Err(ref e) if attempt < TEST_RETRY_MAX_ATTEMPTS && is_retryable(e) => {
                let backoff =
                    Duration::from_millis(TEST_RETRY_BACKOFF_UNIT_MS * u64::from(attempt));
                eprintln!(
                    "[Bug-G-005 Option K test-retry] {label} attempt {attempt}/{TEST_RETRY_MAX_ATTEMPTS} \
                     hit rename retry exhausted ({e:?}); sleeping {backoff:?} before retry"
                );
                std::thread::sleep(backoff);
            }
            Err(e) => panic!(
                "[Bug-G-005 Option K test-retry] {label} failed (non-retryable or final attempt {attempt}/{TEST_RETRY_MAX_ATTEMPTS}): {e:?}"
            ),
        }
    }
    unreachable!("loop guarantees return on Ok or panic on Err with attempt >= MAX")
}

/// `repo.save()` をテスト側 retry でラップする。
///
/// `PersistenceError::AtomicWriteFailed { stage: Rename, .. }` のみ retry し、
/// 他のエラーは即 panic する (Bug-G-005 Option K、`integration.md` v8.3)。
///
/// # Panics
///
/// - 全 attempt で rename retry exhausted: panic で test fail
/// - 非 rename エラー: 即 panic で test fail
pub fn save_with_test_rename_retry(repo: &SqliteVaultRepository, vault: &Vault) {
    retry_with_backoff(
        "save",
        || repo.save(vault),
        |e| {
            matches!(
                e,
                PersistenceError::AtomicWriteFailed {
                    stage: AtomicWriteStage::Rename,
                    ..
                }
            )
        },
    );
}

/// `MigrationError` を返す `VaultMigration` 系メソッドをテスト側 retry でラップする。
///
/// `MigrationError::AtomicWriteFailed { stage: Rename, .. }` および
/// `MigrationError::Persistence(PersistenceError::AtomicWriteFailed { stage: Rename, .. })`
/// の双方を retry 対象とする (vault_migration 内で `?` 経由か直接構築かの違いを
/// 吸収するため)。他のエラーは即 panic で test fail させる (Bug-G-005 Option K)。
///
/// # Panics
///
/// - 全 attempt で rename retry exhausted: panic で test fail
/// - 非 rename エラー: 即 panic で test fail
pub fn migration_op_with_test_rename_retry<T>(
    label: &str,
    op: impl FnMut() -> Result<T, MigrationError>,
) -> T {
    retry_with_backoff(label, op, is_migration_rename_retry_exhausted)
}

/// `MigrationError` が rename retry exhausted 経路かを判定する。
///
/// `vault_migration` 内で `?` 経由で来る場合は `Persistence(PersistenceError::AtomicWriteFailed)`、
/// 直接構築される場合は `AtomicWriteFailed { ... }` と異なる variant を経由するため、双方を網羅する。
fn is_migration_rename_retry_exhausted(err: &MigrationError) -> bool {
    matches!(
        err,
        MigrationError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            ..
        } | MigrationError::Persistence(PersistenceError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            ..
        })
    )
}
