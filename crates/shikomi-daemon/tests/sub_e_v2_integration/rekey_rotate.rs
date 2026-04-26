//! TC-E-I06 / TC-E-I08 / TC-E-I06b: change_password / rekey / rotate_recovery
//! + atomic 整合性 + `cache_relocked` (EC-3 / EC-4 / EC-9).
//!
//! 設計書 §14.4 / §14.6 SSoT + ペガサス工程5指摘①「成功と偽る lock」解消:
//!
//! - TC-E-I06: `ChangePassword` で VEK 不変 (REQ-S10 O(1))、cache.is_unlocked 維持
//! - TC-E-I06b: `RotateRecovery` で atomic save 成功 + 再 unlock 成功 →
//!   `cache_relocked: true` で返却、Lie-Then-Surprise 経路を構造的に拒絶
//! - TC-E-I08: rekey + recovery rotation atomic 後の旧 mnemonic 経路 Fail Kindly 維持
//!   + `cache_relocked: true` 確認 (銀時 `143e8eb` 修正取り込み)

use crate::common::{fresh_repo, tighten_perms_unix};
use crate::helpers::{
    encrypt_existing_vault, fast_argon2_adapter, handshake_v2, run_dispatch, secret,
    seed_plaintext_vault, STRONG_PASSWORD,
};
use shikomi_core::ipc::{IpcRequest, IpcResponse, SerializableSecretBytes};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use shikomi_infra::crypto::aead::AesGcmAeadAdapter;
use shikomi_infra::crypto::kdf::Bip39Pbkdf2Hkdf;
use shikomi_infra::crypto::password::ZxcvbnGate;
use shikomi_infra::crypto::rng::Rng;
use shikomi_infra::persistence::vault_migration::VaultMigration;
use shikomi_infra::persistence::VaultRepository;
use tokio::sync::Mutex;

// =====================================================================
// TC-E-I06: change_password で VEK 不変 (REQ-S10 O(1))
// =====================================================================

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
// TC-E-I06b: RotateRecovery で cache_relocked=true (EC-4 + ペガサス工程5)
// =====================================================================
//
// 設計書 §14.4 EC-4: rekey + recovery rotation atomic で `RecoveryRotated`
// 応答を返す。本 TC は応答に **`cache_relocked: true`** フィールドが含まれ、
// daemon 内 `VekCache` が再 unlock されている (`is_unlocked == true`) ことを
// 機械固定する (Lie-Then-Surprise 経路の構造防衛)。

#[tokio::test]
async fn tc_e_i06b_rotate_recovery_returns_cache_relocked_true() {
    let (dir, repo) = fresh_repo();
    tighten_perms_unix(dir.path());
    seed_plaintext_vault(&repo, 2);
    encrypt_existing_vault(&repo);

    let cache = VekCache::new();
    let backoff = Mutex::new(UnlockBackoff::new());

    // unlock (RotateRecovery は cache.is_unlocked == true が前提、C-22)
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

    // RotateRecovery
    let resp = run_dispatch(
        &repo,
        &cache,
        &backoff,
        handshake_v2(),
        IpcRequest::RotateRecovery {
            master_password: secret(STRONG_PASSWORD),
        },
    )
    .await;

    match resp {
        IpcResponse::RecoveryRotated {
            words,
            cache_relocked,
        } => {
            assert_eq!(words.len(), 24, "RecoveryRotated must contain 24 words");
            assert!(
                cache_relocked,
                "rotate_recovery 正常経路では cache_relocked=true (atomic save + 再unlock 成功)"
            );
        }
        other => panic!("expected RecoveryRotated, got {other:?}"),
    }

    // 後続の read/write IPC が VaultLocked を返さない (cache が Unlocked 維持)
    assert!(
        cache.is_unlocked().await,
        "rotate_recovery 後 cache は再 unlock されている (cache_relocked=true と整合)"
    );
}

// =====================================================================
// TC-E-I08: rekey atomic 整合性 + 旧 mnemonic 経路 Fail Kindly (EC-9)
// =====================================================================

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

    // 旧 mnemonic を取り出す (テスト目的、本番では disclose は CLI 表示で 1 回限り)
    let old_words = initial_disclosure.disclose();
    let old_words_array: [String; 24] = old_words.as_slice().to_vec().try_into().unwrap();
    drop(old_words);

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
    // 銀時 `143e8eb` の `cache_relocked: bool` フィールド追加に対応:
    // 正常経路では cache_relocked=true (atomic save 成功 + 再 unlock 成功)。
    // ペガサス工程5指摘の Lie-Then-Surprise 経路を構造的に拒絶する。
    let new_words: Vec<SerializableSecretBytes> = match resp {
        IpcResponse::Rekeyed {
            records_count,
            words,
            cache_relocked,
        } => {
            assert_eq!(records_count, 4, "rekey must re-encrypt all 4 records");
            assert!(
                cache_relocked,
                "rekey 正常経路では cache_relocked=true (atomic save + 再unlock 成功)"
            );
            words
        }
        other => panic!("expected Rekeyed, got {other:?}"),
    };
    assert_eq!(new_words.len(), 24, "Rekeyed must contain 24 words");

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

    // 旧 mnemonic では unlock 失敗 (atomic 整合性破壊ウィンドウゼロ、EC-9)
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
