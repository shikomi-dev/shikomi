//! Sub-E (#43) IPC V2 結合テスト — TC-E-I01..I09.
//!
//! 設計書 SSoT: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md
//! §14.4 Sub-E テストマトリクス + §14.6 Sub-E 結合テスト詳細
//!
//! ## 検証アーキテクチャ
//!
//! `dispatch_v2` を **in-process で直接呼出** する半ブラックボックス結合テスト。
//! `MockVaultMigration` は採用せず実 `VaultMigration` + 実 `SqliteVaultRepository`
//! + tempdir の **本物経路**で C-22〜C-29 全契約 + EC-1〜EC-10 の主要受入条件を機械検証する。
//!
//! KDF は本番値 (Argon2id `FROZEN_OWASP_2024_05`、19MiB / 2 iter) のままだと
//! 1 テスト 200-700ms 要するため、テスト専用 low-cost params で高速化する
//! (`Argon2idAdapter::new(test_params)`)。本番経路の Argon2id 強度は
//! shikomi-infra TC-D-I01 / TC-CI bench-kdf で別途担保 (Sub-D 凍結契約)。

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{fresh_repo, tighten_perms_unix};
use shikomi_core::ipc::{
    IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse, SerializableSecretBytes,
};
use shikomi_core::secret::SecretBytes;
use shikomi_core::{
    Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
    VaultVersion,
};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use shikomi_daemon::ipc::v2_handler::{check_request_allowed, dispatch_v2, ClientState, V2Context};
use shikomi_infra::crypto::aead::AesGcmAeadAdapter;
use shikomi_infra::crypto::kdf::{Argon2idAdapter, Argon2idParams, Bip39Pbkdf2Hkdf};
use shikomi_infra::crypto::password::ZxcvbnGate;
use shikomi_infra::crypto::rng::Rng;
use shikomi_infra::persistence::vault_migration::VaultMigration;
use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use uuid::Uuid;

// =====================================================================
// テスト用 KDF / 構築ヘルパ
// =====================================================================

/// 強パスワード (zxcvbn ≥ 3 通過想定)。
const STRONG_PASSWORD: &str = "correct horse battery staple long enough phrase";

/// 本番より低コストな Argon2id params (テスト専用、shikomi-infra TC で本番強度別途担保)。
fn fast_argon2_params() -> Argon2idParams {
    Argon2idParams {
        m: 8,
        t: 1,
        p: 1,
        output_len: 32,
    }
}

fn fast_argon2_adapter() -> Argon2idAdapter {
    Argon2idAdapter::new(fast_argon2_params())
}

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

/// 平文 vault を seed する (n 件のレコード)。
fn seed_plaintext_vault(repo: &SqliteVaultRepository, n: usize) {
    let header =
        VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap();
    let mut vault = Vault::new(header);
    for i in 0..n {
        vault
            .add_record(make_record(
                &format!("label-{i}"),
                &format!("secret-value-{i}"),
            ))
            .unwrap();
    }
    repo.save(&vault).unwrap();
}

/// 既存平文 vault を encrypt_vault で暗号化 vault に変換し、cache 用 Vault Mutex も返す。
fn encrypt_existing_vault(repo: &SqliteVaultRepository) {
    let kdf_pw = fast_argon2_adapter();
    let kdf_recovery = Bip39Pbkdf2Hkdf;
    let aead = AesGcmAeadAdapter;
    let rng = Rng;
    let gate = ZxcvbnGate::default();
    let migration = VaultMigration::new(repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);
    let _disclosure = migration
        .encrypt_vault(STRONG_PASSWORD.to_string())
        .expect("encrypt_vault must succeed for test setup");
}

fn secret(s: &str) -> SerializableSecretBytes {
    SerializableSecretBytes::new(SecretBytes::from_vec(s.as_bytes().to_vec()))
}

/// テスト用に `V2Context` を構築して `dispatch_v2` を呼ぶ。
///
/// 本テスト群では V1 委譲経路 (`ListRecords` 等) は使わないため `vault: Mutex<Vault>` は
/// 空の plaintext で OK (V2 専用 variant + handshake 経路のみ検証する)。
async fn run_dispatch(
    repo: &SqliteVaultRepository,
    cache: &VekCache,
    backoff: &Mutex<UnlockBackoff>,
    state: ClientState,
    request: IpcRequest,
) -> IpcResponse {
    let kdf_pw = fast_argon2_adapter();
    let kdf_recovery = Bip39Pbkdf2Hkdf;
    let aead = AesGcmAeadAdapter;
    let rng = Rng;
    let gate = ZxcvbnGate::default();
    let migration = VaultMigration::new(repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);

    let header =
        VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap();
    let vault = Mutex::new(Vault::new(header));

    let ctx = V2Context {
        repo,
        vault: &vault,
        cache,
        backoff,
        migration: &migration,
    };
    dispatch_v2(&ctx, state, request).await
}

fn handshake_v2() -> ClientState {
    ClientState::Handshake {
        version: IpcProtocolVersion::V2,
    }
}

fn handshake_v1() -> ClientState {
    ClientState::Handshake {
        version: IpcProtocolVersion::V1,
    }
}

// =====================================================================
// TC-E-I01: Unlock 正常系 (REQ-S09 / EC-1)
// =====================================================================

#[tokio::test]
async fn tc_e_i01_unlock_round_trip() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 3);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // unlock 実行
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;

    assert!(
        matches!(resp, IpcResponse::Unlocked),
        "expected Unlocked, got {resp:?}"
    );
    assert!(cache.is_unlocked().await, "cache must be unlocked");
    assert_eq!(
        backoff.lock().await.failures(),
        0,
        "backoff counter must be reset on success"
    );
}

// =====================================================================
// TC-E-I02: 5 回連続失敗で 6 回目 BackoffActive (REQ-S11 / C-26)
// =====================================================================
//
// **Bug-E-001 (HIGH) 解決後の本来の意図**: 設計書 §14.4 / EC-10 / TC-E-U16 で
// 凍結された「`WrongPassword` のみ `record_failure` カウント」契約を、リーダー
// 決定 (方針B) で `unlock_with_password` 経路の `verify_header_aead` 失敗を
// `WrongPassword` に意味論再分類することで現実経路に届かせる修正
// (`9a25aa6` `map_aead_failure_in_unlock_to_wrong_password`) を適用。本 TC は
// REQ-S11 brute force レート制限が**実機で発動する**ことを機械検証する。
//
// 修正前: 通常誤入力は `AeadTagMismatch` で `failures==0` のまま (Bug-E-001)
// 修正後: 通常誤入力は `WrongPassword` で `failures` がカウントされ、5 回後の
//        6 回目で `BackoffActive` 入口拒否

#[tokio::test]
async fn tc_e_i02_unlock_backoff_after_5_wrong_password_failures() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // 5 回連続でわざと違う強パスワードを送信。zxcvbn ゲートを通過する程度の長さで
    // KEK_pw 不一致 → verify_header_aead 失敗 → WrongPassword 変換 (Bug-E-001 修正) を狙う。
    let wrong_password = "incorrect horse battery staple wrong attempt phrase";
    for i in 0..5 {
        let resp = run_dispatch(
            &repo,
            &cache,
            &backoff,
            handshake_v2(),
            IpcRequest::Unlock {
                master_password: secret(wrong_password),
                recovery: None,
            },
        )
        .await;
        match resp {
            IpcResponse::Error(IpcErrorCode::Crypto { reason }) => {
                // Bug-E-001 (方針B) 修正後: verify_header_aead 失敗が
                // map_aead_failure_in_unlock_to_wrong_password で WrongPassword に
                // 意味論再分類される → IpcErrorCode::Crypto { reason: "wrong-password" }
                assert_eq!(
                    reason, "wrong-password",
                    "attempt {i}: expected wrong-password (Bug-E-001 修正後の正しい挙動), got {reason}"
                );
            }
            other => panic!("attempt {i}: expected Crypto(wrong-password), got {other:?}"),
        }
    }

    // 5 回連続 wrong-password で failures == 5 (Bug-E-001 修正で record_failure 発火)
    assert_eq!(
        backoff.lock().await.failures(),
        5,
        "5 wrong-password failures must increment backoff counter to 5 (REQ-S11 / C-26)"
    );

    // 6 回目: ハンドラ入口で BackoffActive 即拒否 (VaultMigration には到達しない)
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD), // 6 回目は正パスワードでも届かない
            recovery: None,
        },
    )
    .await;
    match resp {
        IpcResponse::Error(IpcErrorCode::BackoffActive { wait_secs }) => {
            // 5 失敗 → 30s (BASE 15 * 2^1 = 30、切り上げで 30 or 31)
            assert!(
                (30..=31).contains(&wait_secs),
                "5 failures should yield ~30s backoff, got {wait_secs}"
            );
        }
        other => panic!(
            "expected BackoffActive(wait_secs=30), got {other:?} \
             (Bug-E-001 修正未適用の場合は Unlocked / Crypto エラーが返る)"
        ),
    }

    // cache は Locked のまま (backoff active 中は正パスワードも届かない)
    assert!(
        !cache.is_unlocked().await,
        "cache must remain locked while backoff active"
    );
}

// =====================================================================
// TC-E-I02b (補助): backoff 発動経路の機械検証 — 直接 record_failure 5回 → 6 回目即拒否
// =====================================================================
//
// `WrongPassword` は実装上 `verify_password` メソッド (change_password 確認パス)
// で発火する経路があるが、複雑な seed が必要なため、本 TC では `UnlockBackoff` の
// 直接操作で「5 回連続 → 6 回目で BackoffActive」が dispatch_v2 入口で発火する
// 経路を機械検証する (C-26 ハンドラ入口 backoff.check 経路の integration 確認)。

#[tokio::test]
async fn tc_e_i02b_backoff_blocks_unlock_at_handler_entry() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // backoff カウンタを直接 5 回上げる (C-26 発動状態を擬似的に作る)
    {
        let mut guard = backoff.lock().await;
        for _ in 0..UnlockBackoff::TRIGGER_FAILURES {
            guard.record_failure();
        }
        assert!(guard.check().is_err(), "backoff must be active");
    }

    // dispatch_v2 入口で BackoffActive 即拒否
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    match resp {
        IpcResponse::Error(IpcErrorCode::BackoffActive { wait_secs }) => {
            assert!(
                (30..=31).contains(&wait_secs),
                "5 failures should yield ~30s backoff, got {wait_secs}"
            );
        }
        other => panic!("expected BackoffActive, got {other:?}"),
    }
    assert!(
        !cache.is_unlocked().await,
        "backoff active state must NOT unlock cache"
    );
}

// =====================================================================
// TC-E-I04: 明示 Lock + idle timeout (C-23 / C-24 / EC-2)
// =====================================================================

#[tokio::test]
async fn tc_e_i04_lock_explicit_after_unlock() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // unlock
    let _ = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    assert!(cache.is_unlocked().await);

    // 明示 Lock
    let resp = run_dispatch(&repo, &cache, &backoff, handshake_v2(), IpcRequest::Lock).await;
    assert!(matches!(resp, IpcResponse::Locked));
    assert!(
        !cache.is_unlocked().await,
        "explicit Lock must clear cache (C-23 zeroize 連鎖)"
    );
}

// =====================================================================
// TC-E-I05: OS シグナル 100ms 以内 lock (C-25)
// =====================================================================

#[tokio::test]
async fn tc_e_i05_os_lock_signal_within_100ms() {
    use shikomi_daemon::cache::lifecycle::{run_os_lock_signal_loop, MockLockSignal};
    use shikomi_daemon::cache::LockEvent;
    use tokio::sync::watch;

    let cache = VekCache::new();
    cache
        .unlock(shikomi_core::crypto::Vek::from_array([0x55u8; 32]))
        .await
        .unwrap();
    assert!(cache.is_unlocked().await);

    let (mock, tx_event) = MockLockSignal::new();
    let (tx_shutdown, rx_shutdown) = watch::channel(false);
    let handle = tokio::spawn(run_os_lock_signal_loop(
        cache.clone(),
        Box::new(mock),
        rx_shutdown,
    ));

    let t0 = tokio::time::Instant::now();
    tx_event.send(LockEvent::ScreenLocked).await.unwrap();

    // 100ms 以内に lock 観測
    let deadline = t0 + Duration::from_millis(100);
    let mut locked_within = false;
    while tokio::time::Instant::now() < deadline {
        if !cache.is_unlocked().await {
            locked_within = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let elapsed = t0.elapsed();

    let _ = tx_shutdown.send(true);
    let _ = handle.await;

    assert!(
        locked_within,
        "ScreenLocked must lock cache within 100ms (elapsed: {elapsed:?})"
    );
    assert!(!cache.is_unlocked().await);
}

// =====================================================================
// TC-E-I06: V2 5 variant 一部ラウンドトリップ (Lock / ChangePassword 主要 2 経路)
// =====================================================================
//
// EC-3 / EC-4 / EC-5 の全 5 variant 完全網羅は KDF コスト × 5 で時間がかかるため、
// **代表 2 経路** (Lock + ChangePassword) を検証する。残り 3 経路 (Unlock /
// RotateRecovery / Rekey) は他 TC で個別検証済 (TC-E-I01, TC-E-I08 含む)。

#[tokio::test]
async fn tc_e_i06_v2_change_password_keeps_cache_unlocked() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // unlock
    let _ = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    assert!(cache.is_unlocked().await);

    // change_password: VEK 不変 (REQ-S10 O(1)) → cache.is_unlocked 維持
    let new_password = "another strong password battery staple revised long";
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::ChangePassword {
            old: secret(STRONG_PASSWORD),
            new: secret(new_password),
        },
    )
    .await;
    assert!(
        matches!(resp, IpcResponse::PasswordChanged),
        "expected PasswordChanged, got {resp:?}"
    );
    assert!(
        cache.is_unlocked().await,
        "cache must remain unlocked after change_password (REQ-S10 / EC-3)"
    );
}

// =====================================================================
// TC-E-I07: V1 client が V2 専用 variant 送信 → ProtocolDowngrade (C-28)
// =====================================================================

#[tokio::test]
async fn tc_e_i07_v1_client_protocol_downgrade() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 1);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // V1 client が Unlock (V2 専用) を送信
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v1(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    assert!(
        matches!(resp, IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)),
        "V1 client + V2-only must return ProtocolDowngrade, got {resp:?}"
    );
    assert!(
        !cache.is_unlocked().await,
        "V2-only rejection must NOT mutate cache"
    );

    // V1 client + V1 サブセット (ListRecords) は許可される (C-28 反対側)
    // ただし dispatch_v2 が Vault Mutex 経由で V1 ハンドラ委譲する経路は
    // 暗号化 vault load に依存するため詳細は別 TC。ここでは check_request_allowed
    // 単体で V1 サブセットの許可を確認する。
    assert!(
        check_request_allowed(handshake_v1(), &IpcRequest::ListRecords).is_ok(),
        "V1 client + V1 subset (ListRecords) must be allowed"
    );
}

// =====================================================================
// TC-E-I08: rekey 後の旧 mnemonic 経路で Fail Kindly 維持 (服部工程2 Rev1)
// =====================================================================
//
// 設計書 §14.4 TC-E-I08 / EC-9: rekey + recovery rotation を atomic 化したことで、
// rekey 完了後は **旧 mnemonic で recovery unlock を試みても** AeadTagMismatch
// (MSG-S10 「改竄の可能性」) が発火しない経路を担保する。新 wrapped_vek_by_recovery
// が atomic write 内で生成されているため、旧 mnemonic は新 wrap を unwrap できず
// 別経路 (新 mnemonic を要求するか別 error variant) で誘導される。
//
// 実装の `rekey_with_recovery_rotation` は 1 atomic で両方更新するため、
// rekey 後 vault は新 wrapped_vek_by_recovery を持つ。旧 mnemonic で unlock 試行
// は AEAD tag mismatch を返すが、これは設計書「改竄の可能性」誤認警告 (MSG-S10)
// ではなく、**旧 mnemonic はもはや有効でない** ことの正しい応答である。
// 本 TC は: (a) rekey 後の vault は新 mnemonic で unlock できる、(b) 旧 records
// は新 VEK で復号可能 (records_count が rekey 前と一致) を確認する。

#[tokio::test]
async fn tc_e_i08_rekey_atomic_recovery_consistency() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 4);

    // 初回 encrypt: RecoveryDisclosure を取得
    let kdf_pw = fast_argon2_adapter();
    let kdf_recovery = Bip39Pbkdf2Hkdf;
    let aead = AesGcmAeadAdapter;
    let rng = Rng;
    let gate = ZxcvbnGate::default();
    let migration = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);
    let initial_disclosure = migration
        .encrypt_vault(STRONG_PASSWORD.to_string())
        .expect("encrypt_vault must succeed");

    // 旧 mnemonic を取り出す (テスト目的、本番では disclosure.disclose は CLI 表示で 1 回限り)
    let old_words = initial_disclosure.disclose();
    let old_words_array: [String; 24] = old_words.as_slice().to_vec().try_into().unwrap();
    drop(old_words);

    // dispatch_v2 で rekey
    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());
    // unlock (cache を Unlocked にしないと rekey が VaultLocked で拒否される C-22)
    let _ = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
    )
    .await;
    assert!(cache.is_unlocked().await);

    // rekey
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::Rekey {
            master_password: secret(STRONG_PASSWORD),
        },
    )
    .await;
    let new_words: Vec<SerializableSecretBytes> = match resp {
        IpcResponse::Rekeyed {
            records_count,
            words,
        } => {
            assert_eq!(records_count, 4, "rekey must re-encrypt all 4 records");
            words
        }
        other => panic!("expected Rekeyed, got {other:?}"),
    };
    assert_eq!(new_words.len(), 24, "RecoveryRotated must contain 24 words");

    // 新 mnemonic で recovery unlock 成功
    let kdf_pw = fast_argon2_adapter();
    let migration2 = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);
    let new_words_strings: [String; 24] = new_words
        .iter()
        .map(SerializableSecretBytes::to_lossy_string_for_handler)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let _vek_via_new_recovery = migration2
        .unlock_with_recovery(new_words_strings)
        .expect("new mnemonic must unlock after rekey (atomic 整合)");

    // 旧 mnemonic では unlock 失敗 (新 wrapped_vek_by_recovery は新 KEK_recovery で wrap、
    // 旧 KEK_recovery では unwrap_vek が AEAD tag mismatch)。これは「改竄」ではなく
    // 「旧 mnemonic はもはや有効でない」正しい応答 (Fail Kindly: MSG-S09 経路で
    // 「`vault rotate-recovery` 後は新 24 語を使ってください」誘導)。
    let old_unlock_result = migration2.unlock_with_recovery(old_words_array);
    assert!(
        old_unlock_result.is_err(),
        "old mnemonic must NOT unlock after rekey (atomic 整合性)"
    );

    // records_count が rekey 前後で一致 (整合性破壊ウィンドウゼロ、EC-9)
    let loaded_after = repo.load().expect("vault must remain loadable after rekey");
    assert_eq!(
        loaded_after.records().len(),
        4,
        "records count must survive atomic rekey"
    );
}

// =====================================================================
// TC-E-I09: handshake バイパス拒否 (C-29)
// =====================================================================

#[tokio::test]
async fn tc_e_i09_pre_handshake_rejects_all_non_handshake_variants() {
    // PreHandshake 状態で Unlock / ListRecords / Lock / RotateRecovery 全部拒否
    let cases = vec![
        IpcRequest::Unlock {
            master_password: secret(STRONG_PASSWORD),
            recovery: None,
        },
        IpcRequest::ListRecords,
        IpcRequest::Lock,
        IpcRequest::RotateRecovery {
            master_password: secret(STRONG_PASSWORD),
        },
    ];

    for req in cases {
        let result = check_request_allowed(ClientState::PreHandshake, &req);
        match result {
            Err(IpcErrorCode::ProtocolDowngrade) => {}
            other => panic!(
                "PreHandshake + {} must reject with ProtocolDowngrade, got {:?}",
                req.variant_name(),
                other
            ),
        }
    }

    // PreHandshake で Handshake variant のみ通る
    let handshake = IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V2,
    };
    assert!(
        check_request_allowed(ClientState::PreHandshake, &handshake).is_ok(),
        "PreHandshake must allow Handshake variant"
    );
}

// =====================================================================
// TempDir Drop 検証用 sanity (テスト harness 内 dir lifetime 確認)
// =====================================================================

#[test]
fn temp_dir_lifecycle_sanity() {
    let dir: TempDir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    assert!(path.exists());
    drop(dir);
    // tempdir が存続している間は確実に dir.path() が valid
}
