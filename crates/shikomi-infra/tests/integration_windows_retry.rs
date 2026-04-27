//! vault-persistence 結合テスト — TC-I29 (Windows AtomicWrite rename retry)
//!
//! テスト設計書: docs/features/vault-persistence/test-design/integration.md §TC-I29
//! 対応 Issue: #65
//!
//! 本ファイルの 3 ケースは Issue #65 の `cfg(windows)` 限定 rename retry 補強の
//! 機能 / DoS 兆候 / 監査ログ 3 経路 (pending / succeeded / exhausted) を
//! 一気通貫で検証する。`tracing_test::traced_test` で監査ログを直接観測し、
//! daemon 側 subscriber の上位通報経路が「emit 側で発火可能」であることまで担保する。

#![cfg(windows)]

mod helpers;

use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use helpers::{make_plaintext_vault, make_repo};
use shikomi_infra::persistence::{AtomicWriteStage, PersistenceError, VaultRepository};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// 内部定数 — TC-I29 の race 設定
// ---------------------------------------------------------------------------

/// `share_mode(0)` = `FILE_SHARE_NONE`。
///
/// Windows の `CreateFileW` は既定で `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`
/// を立てるため、`std::fs::OpenOptions` 標準では rename race を再現できない。
/// `OpenOptionsExt::share_mode(0)` で全 share ビットを落とすことで `MoveFileExW` の
/// `MOVEFILE_REPLACE_EXISTING` 経路を `ERROR_ACCESS_DENIED (5)` で確実に弾く。
const FILE_SHARE_NONE: u32 = 0;

/// TC-I29 主検証の補助スレッド保持時間 (ms)。
///
/// `1〜2 回目 retry (経過 ~50–150ms) で吸収される設計` (test-design §TC-I29)。
const TC_I29_HOLD_MS: u64 = 150;

/// TC-I29-A (DoS 兆候 / retry exhausted) の補助スレッド保持時間 (ms)。
///
/// `jitter 込み最悪 ~375ms` を確実に超え、5 回 retry を全敗に追い込む値。
/// 600ms に固定して CI ランナーの揺らぎ (sleep 精度 ±20ms) も吸収する。
const TC_I29_EXHAUST_HOLD_MS: u64 = 600;

/// TC-I29 主検証の経過時間上限 (ms)。
///
/// 純粋な retry 上限は 375ms (50ms × 5 + jitter ±25ms × 5)。これに `write_new` の
/// SQLite 初期化 (~30ms) + 補助スレッド spawn / channel 同期 (~30ms) +
/// CI ランナー (windows-latest) の sleep 精度揺らぎ (~150ms) の余裕を上乗せして
/// 750ms を上限契約として採用する。これを超えるなら retry 設計の上限契約違反
/// (`security.md §jitter` の SSoT 上限値違反) を疑う。
const TC_I29_DEADLINE_MS: u128 = 750;

// ---------------------------------------------------------------------------
// 補助関数
// ---------------------------------------------------------------------------

/// `vault.db` を `share_mode(0)` (FILE_SHARE_NONE) で `hold_ms` だけ排他 open し、
/// 取得開始を ready チャネルで通知する補助スレッドを起動する。
///
/// メインスレッドは `ready_rx.recv()` で「補助スレッドが排他 open を確実に取得した」
/// ことを確認してから `repo.save()` を呼ぶ。これにより race の決定性が担保される
/// (test-design §TC-I29 実装上の注意)。
fn spawn_exclusive_holder(
    path: PathBuf,
    hold_ms: u64,
) -> (thread::JoinHandle<()>, mpsc::Receiver<()>) {
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let f = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_NONE)
            .open(&path)
            .expect("補助スレッド: vault.db を share_mode(0) で open できなかった");
        let _ = ready_tx.send(());
        thread::sleep(Duration::from_millis(hold_ms));
        drop(f);
    });
    (handle, ready_rx)
}

// ---------------------------------------------------------------------------
// TC-I29: 並行 read open 中の rename race を retry が吸収して save が成功する
// ---------------------------------------------------------------------------

/// TC-I29 — 補助スレッドが `vault.db` を 150ms 間 share_mode(0) で保持している
/// 最中に `repo.save()` を発火させ、`cfg(windows)` 限定 rename retry が
/// race を吸収して `Ok(())` を返すことを検証する。
///
/// 検証する観点:
/// 1. `repo.save()` が `Ok(())` を返す — retry が機能していなければ
///    `Err(AtomicWriteFailed { stage: Rename, source: code:5 })` で fail する
/// 2. 経過時間が `750ms` 以内 — `security.md §jitter` の SSoT 上限値
///    `~375ms` + CI ランナー余裕 (sleep 精度 / spawn / channel) で計上
/// 3. `repo.load()` で復元した vault が新内容と一致する — `.new → vault.db`
///    の置換が完了している (rename 成功の振る舞い側証跡)
/// 4. 監査ログに `outcome="pending"` と `outcome="succeeded"` が記録される
///    — Issue #65 §retry 監査ログ の発火経路 2 / 3 を直接観測
/// 5. `outcome="exhausted"` が**含まれない** — 本 TC は retry 成功経路
///
/// 設計書: docs/features/vault-persistence/test-design/integration.md §TC-I29
/// AC-19 (Issue #65 retry 補強) 対応。
#[test]
#[tracing_test::traced_test]
fn tc_i29_aux_thread_holds_150ms_save_succeeds_within_375ms_window() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    // 1. 初期 vault を save (vault.db を物理存在状態にする)
    let initial = make_plaintext_vault(1);
    repo.save(&initial).expect("初期 save が失敗");
    let vault_db = dir.path().join("vault.db");
    assert!(vault_db.exists(), "初期 save 後に vault.db が存在しない");

    // 2. 補助スレッドで vault.db を share_mode(0) で 150ms 排他 open
    let (handle, ready) = spawn_exclusive_holder(vault_db.clone(), TC_I29_HOLD_MS);
    ready.recv().expect("補助スレッド ready 受信失敗");

    // 3. メインスレッドで別内容を save → retry が race を吸収
    let new_vault = make_plaintext_vault(2);
    let started = Instant::now();
    let result = repo.save(&new_vault);
    let elapsed = started.elapsed();

    // 補助スレッドを join (リソースリーク防止)
    handle.join().expect("補助スレッド join 失敗");

    // 検証 1: save が Ok を返す (retry が race を吸収した)
    assert!(
        result.is_ok(),
        "save が失敗 (retry 不発): {:?}",
        result.err()
    );

    // 検証 2: 上限契約 (~375ms + ランナー余裕 = 750ms) を超過していない
    assert!(
        elapsed.as_millis() < TC_I29_DEADLINE_MS,
        "save 経過 {} ms が上限契約 {} ms を超過 (security.md §jitter SSoT 上限違反の疑い)",
        elapsed.as_millis(),
        TC_I29_DEADLINE_MS,
    );

    // 検証 3: rename が成立して .new が vault.db に反映されている (振る舞い検証)
    let loaded = repo.load().expect("save 後の load 失敗");
    assert_eq!(
        loaded.records().len(),
        2,
        "rename が成立しておらず .new が反映されていない"
    );

    // 検証 4: 監査ログに rename retry の pending / succeeded 経路が記録されている
    assert!(
        logs_contain("persistence: rename retry event"),
        "rename retry イベント (warn レベル) がログに発火していない \
         — Audit::retry_event の配線漏れの疑い",
    );
    assert!(
        logs_contain(r#"outcome="pending""#),
        "outcome=\"pending\" が見当たらない (retry 試行直前の監査が発火していない)"
    );
    assert!(
        logs_contain(r#"outcome="succeeded""#),
        "outcome=\"succeeded\" が見当たらない (retry の rename 成功直後の監査が発火していない)"
    );
    assert!(
        logs_contain(r#"stage="rename""#),
        "stage=\"rename\" が見当たらない (retry イベントの stage ラベルが SSoT 値と不一致)"
    );

    // 検証 5: 本 TC は retry 成功経路なので exhausted は出てはいけない
    assert!(
        !logs_contain(r#"outcome="exhausted""#),
        "outcome=\"exhausted\" が記録されている (本 TC は retry 成功経路のはず)"
    );
    assert!(
        !logs_contain("rename retry exhausted"),
        "exhausted error イベントが発火している (retry 内部状態の bug 疑い)"
    );
}

// ---------------------------------------------------------------------------
// TC-I29-A: retry が 5 回全敗で `AtomicWriteFailed { stage: Rename }` を返し、
//           `outcome="exhausted"` が error レベルで監査ログに発火する
//           (DoS 兆候 / OWASP A09 上位通報の emit 側責務)
// ---------------------------------------------------------------------------

/// TC-I29-A — 補助スレッドが `vault.db` を 600ms 間 share_mode(0) で保持し、
/// retry の上限契約 ~375ms を超過させることで 5 回 retry を全敗に追い込む。
///
/// 検証する観点:
/// 1. `repo.save()` が `Err(AtomicWriteFailed { stage: Rename })` を返す
///    — retry 上限契約 (security.md §jitter SSoT) を超えた race は意図通り fail fast
/// 2. 監査ログに `outcome="exhausted"` が **error レベル** で発火している
///    — daemon 側 subscriber の DoS 兆候上位通報 (OWASP A09) の起点
/// 3. `"rename retry exhausted"` メッセージが含まれている
///    — Audit::retry_event の error 分岐 (`outcome == "exhausted"` 経路) が機能している
///
/// 設計書: docs/features/vault-persistence/basic-design/security.md
///         §atomic write の二次防衛線 §retry 監査ログ §rename retry 全敗
/// AC-19 (Issue #65 retry 補強、DoS 兆候側) 対応。
#[test]
#[tracing_test::traced_test]
fn tc_i29_a_aux_thread_holds_600ms_save_fails_with_rename_exhausted() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    // 初期 vault を save
    repo.save(&make_plaintext_vault(1)).expect("初期 save");
    let vault_db = dir.path().join("vault.db");

    // 補助スレッドで 600ms 排他 open (>375ms で retry を 5 回全敗させる)
    let (handle, ready) = spawn_exclusive_holder(vault_db, TC_I29_EXHAUST_HOLD_MS);
    ready.recv().expect("補助スレッド ready 受信失敗");

    // 別内容を save → retry 全敗で AtomicWriteFailed { stage: Rename }
    let new_vault = make_plaintext_vault(2);
    let result = repo.save(&new_vault);

    handle.join().expect("補助スレッド join 失敗");

    // 検証 1: save が AtomicWriteFailed { stage: Rename } で失敗 (fail fast)
    match result {
        Err(PersistenceError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            source,
        }) => {
            // raw_os_error が retry 対象 (5 / 32 / 33) のいずれかであることまで担保
            assert!(
                matches!(source.raw_os_error(), Some(5 | 32 | 33)),
                "raw_os_error が retry 対象外 (5/32/33): {:?}",
                source.raw_os_error()
            );
        }
        other => panic!(
            "AtomicWriteFailed {{ stage: Rename }} を期待したが {:?}",
            other,
        ),
    }

    // 検証 2: 監査ログに exhausted 経路が error レベルで発火している
    assert!(
        logs_contain("rename retry exhausted"),
        "exhausted error イベントが発火していない (Audit::retry_event の error 分岐配線漏れ)"
    );
    assert!(
        logs_contain(r#"outcome="exhausted""#),
        "outcome=\"exhausted\" が見当たらない (5 回全敗時の監査が発火していない)"
    );

    // 検証 3: 5 回全敗なので pending は最低 5 回出ている (attempt=1〜5 が全部 emit される)
    // tracing_test の logs_contain は部分一致なので attempt 個別カウントは
    // tracing_test では困難。pending の存在のみ確認 (本 TC の主目的は exhausted の発火検証)
    assert!(
        logs_contain(r#"outcome="pending""#),
        "outcome=\"pending\" が見当たらない (retry 試行直前の監査が発火していない)"
    );
    // 全敗経路では succeeded は出てはいけない
    assert!(
        !logs_contain(r#"outcome="succeeded""#),
        "outcome=\"succeeded\" が記録されている (本 TC は retry 全敗経路のはず)"
    );
}

// ---------------------------------------------------------------------------
// TC-I29-B: 補助スレッド不在 (race 無し) では retry 経路に入らず、
//           pending / succeeded / exhausted いずれの監査ログも emit されない
//           (= 1 回目の rename が即成功する正常経路の確認)
// ---------------------------------------------------------------------------

/// TC-I29-B — race の無い通常 save では `windows_rename_retry` 自体が呼ばれず、
/// `Audit::retry_event` の 3 経路はいずれも emit されない (正常系の sanity check)。
///
/// 設計書: docs/features/vault-persistence/detailed-design/flows.md §`save` step 7
/// (Win 1 回目 rename 成功時は retry 経路に入らない契約)。
///
/// この TC は retry 経路への偽 emit (= retry してないのに監査ログを出してしまう bug) を
/// 検出する。Issue #65 修正後の `rename_atomic` 内 `is_windows_transient_rename_error` 判定が
/// 1 回目成功時に retry に分岐しないことを emit 側証跡で確認する。
#[test]
#[tracing_test::traced_test]
fn tc_i29_b_no_race_save_emits_no_retry_audit_events() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    // 通常の save (race 無し、初回作成)
    let vault = make_plaintext_vault(3);
    repo.save(&vault).expect("通常 save が失敗");

    // 別内容で再 save (race 無し、置換)
    let updated = make_plaintext_vault(5);
    repo.save(&updated).expect("置換 save が失敗");

    // 監査ログに retry 経路が一切 emit されていないこと
    assert!(
        !logs_contain("persistence: rename retry event"),
        "race 無しなのに rename retry イベントが emit された (偽 retry 配線の疑い)"
    );
    assert!(
        !logs_contain("rename retry exhausted"),
        "race 無しなのに exhausted が emit された (retry 経路の制御フローバグ疑い)"
    );
    assert!(
        !logs_contain(r#"outcome="pending""#),
        "race 無しなのに outcome=\"pending\" が emit された"
    );
    assert!(
        !logs_contain(r#"outcome="succeeded""#),
        "race 無しなのに outcome=\"succeeded\" が emit された"
    );
    assert!(
        !logs_contain(r#"outcome="exhausted""#),
        "race 無しなのに outcome=\"exhausted\" が emit された"
    );
}
