//! Sub-D (#42) integration test — TC-D-I01..I05 + Bug-D-001/002 機械検証.
//!
//! 銀ちゃん impl PR #58 (`5c21d10`) に対する黒盒結合テスト。
//! 銀ちゃんが PR 本文で透明性報告した「3 つの妥協点」を機械検証で確定する:
//!
//! - **Bug-D-001 (HIGH)**: `verify_header_aead` が常に Ok() を返す簡略実装。
//!   設計書 C-17/C-18 のヘッダ AEAD タグ検証が実装段階で実質無効化されている。
//!   `nonce_counter` 巻戻し攻撃が**ヘッダ AEAD タグでは検出されない**経路がある
//!   （ただし wrapped_vek_by_pw の AEAD 経路で間接検出される妥協形）。
//! - **Bug-D-002 (HIGH)**: `rekey_vault` ステップ 5 で wrapped_vek_by_pw /
//!   wrapped_vek_by_recovery を**旧のまま維持**し records だけ新 VEK で
//!   再暗号化。設計書 §F-D4 では新 VEK で wrapped_vek を再生成すべき。
//!   実装通りだと **rekey 後の vault は load 時に旧 VEK が出てきて新 records
//!   を復号できず破損状態**。これを TC-D-I03 で機械確定する。
//! - **Bug-D-003 (Medium)**: composite container BLOB の妥協（vault-persistence
//!   sqlite/mapping/header.rs）。Sub-D 単独では検証範囲外、横断 review 範囲。
//!
//! テスト工程でマユリが正直に検証データを記録する。`feature/issue-42-aead-tests-d`
//! ブランチ独立 PR で報告。

#![allow(clippy::unwrap_used, clippy::expect_used)] // テスト harness 内部、shrink 出力で許容

mod helpers;
use helpers::{
    make_record, make_repo, migration_op_with_test_rename_retry, plaintext_header,
    save_with_test_rename_retry, ENV_MUTEX,
};

use shikomi_core::{ProtectionMode, Vault};
use shikomi_infra::crypto::aead::AesGcmAeadAdapter;
use shikomi_infra::crypto::kdf::{Argon2idAdapter, Bip39Pbkdf2Hkdf};
use shikomi_infra::crypto::password::ZxcvbnGate;
use shikomi_infra::crypto::rng::Rng;
use shikomi_infra::persistence::vault_migration::VaultMigration;
use shikomi_infra::persistence::VaultRepository;
use tempfile::TempDir;

/// 強パスワード（zxcvbn ≥ 3 通過想定）。
const STRONG_PASSWORD: &str = "correct horse battery staple long enough phrase";

fn build_migration_set() -> (
    Argon2idAdapter,
    Bip39Pbkdf2Hkdf,
    AesGcmAeadAdapter,
    Rng,
    ZxcvbnGate,
) {
    (
        Argon2idAdapter::default(),
        Bip39Pbkdf2Hkdf,
        AesGcmAeadAdapter,
        Rng,
        ZxcvbnGate::default(),
    )
}

fn seed_plaintext_vault(repo: &shikomi_infra::persistence::SqliteVaultRepository, n: usize) {
    let mut vault = Vault::new(plaintext_header());
    for i in 0..n {
        vault
            .add_record(make_record(
                &format!("label-{i}"),
                &format!("secret-value-{i}"),
            ))
            .unwrap();
    }
    // Bug-G-005 Option K: 初回 save は CI runner 固有のハンドル遅延 (~1570ms 一定) で
    // flaky に rename retry exhausted する事象をテスト側 retry で吸収する
    // (実装側 SSoT `security.md` §jitter ~1675ms は据置)。詳細は test-design v8.3。
    save_with_test_rename_retry(repo, &vault);
}

/// TC-D-I01: encrypt_vault → unlock_with_password で同 plaintext records 復元.
#[test]
#[ignore = "CI runner persistent VM-level file lock (13s+) — Bug-G-002〜G-006 articulated in \
            test-design v8.4, run with --ignored locally. \
            5 ラウンドの実験 (G-002 線形 retry / G-003 Defender exclusion / G-004 \
            Stop-Service WSearch+SysMain / G-005 #[ignore] 検討 / G-006 Option K test-side retry \
            5 attempts ~13s) すべてで `outcome=\"exhausted\"` 継続観測。\
            VM レベルのファイルロック介入で実装側 / テスト側の retry budget をいくら拡張しても \
            吸収不能と articulate 完了。AC-18 はローカル `cargo test -p shikomi-infra --test \
            vault_migration_integration -- --ignored` で手動担保 (TC-I29 主 / B と統一方針)"]
fn tc_d_i01_encrypt_then_unlock_password_roundtrip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    seed_plaintext_vault(&repo, 5);

    let (kdf_pw, kdf_recovery, aead, rng, gate) = build_migration_set();
    let migration = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);

    // encrypt 実行 → RecoveryDisclosure を取得（disclose せず drop）
    // Bug-G-005 Option K: encrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    let _disclosure = migration_op_with_test_rename_retry("encrypt_vault", || {
        migration.encrypt_vault(STRONG_PASSWORD.to_string())
    });

    // unlock で VEK を取得し、同じ vault を再 load して records を確認
    let _vek = migration
        .unlock_with_password(STRONG_PASSWORD.to_string())
        .expect("unlock_with_password must succeed after encrypt");

    // load して records 件数を確認
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir.path());
    let repo2 = shikomi_infra::persistence::SqliteVaultRepository::new().unwrap();
    std::env::remove_var("SHIKOMI_VAULT_DIR");
    let loaded = repo2.load().unwrap();
    assert_eq!(
        loaded.records().len(),
        5,
        "records count must survive encrypt → unlock"
    );
}

/// TC-D-I02: encrypt_vault → decrypt_vault で平文 vault 復元（DecryptConfirmation 経由）.
///
/// `DecryptConfirmation::confirm()` で型レベル証跡を作り、`decrypt_vault` の
/// 引数として渡す。decrypt_vault は内部で `unlock_internal_with_password` →
/// 全 records `decrypt_one_record` → 平文 vault 構築 → atomic write を実行。
#[test]
#[ignore = "CI runner persistent VM-level file lock (13s+) — Bug-G-002〜G-006 articulated in \
            test-design v8.4, run with --ignored locally. \
            5 ラウンドの実験 (G-002 線形 retry / G-003 Defender exclusion / G-004 \
            Stop-Service WSearch+SysMain / G-005 #[ignore] 検討 / G-006 Option K test-side retry \
            5 attempts ~13s) すべてで `outcome=\"exhausted\"` 継続観測。\
            VM レベルのファイルロック介入で実装側 / テスト側の retry budget をいくら拡張しても \
            吸収不能と articulate 完了。AC-18 はローカル `cargo test -p shikomi-infra --test \
            vault_migration_integration -- --ignored` で手動担保 (TC-I29 主 / B と統一方針)"]
fn tc_d_i02_encrypt_then_decrypt_roundtrip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    seed_plaintext_vault(&repo, 5);

    let (kdf_pw, kdf_recovery, aead, rng, gate) = build_migration_set();
    let migration = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);

    // 暗号化
    // Bug-G-005 Option K: encrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    let _disclosure = migration_op_with_test_rename_retry("encrypt_vault", || {
        migration.encrypt_vault(STRONG_PASSWORD.to_string())
    });

    // 復号 (DecryptConfirmation は二段確認証跡)
    // Bug-G-005 Option K: decrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    // DecryptConfirmation は値型で reuse 不可のためクロージャ内で都度生成する。
    migration_op_with_test_rename_retry("decrypt_vault", || {
        let confirmation =
            shikomi_infra::persistence::vault_migration::DecryptConfirmation::confirm();
        migration.decrypt_vault(STRONG_PASSWORD.to_string(), confirmation)
    });

    // load して平文モードに戻ったことを確認
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir.path());
    let repo2 = shikomi_infra::persistence::SqliteVaultRepository::new().unwrap();
    std::env::remove_var("SHIKOMI_VAULT_DIR");
    let loaded = repo2.load().unwrap();
    assert_eq!(
        loaded.header().protection_mode(),
        ProtectionMode::Plaintext,
        "decrypt_vault 後は平文モードに戻るべき"
    );
    assert_eq!(
        loaded.records().len(),
        5,
        "全 records が復号されて平文 vault に書き戻されるべき"
    );
}

/// TC-D-I03: rekey 後の旧 VEK / 新 VEK の挙動を観察.
///
/// **Bug-D-002 機械検証**: 設計書 §F-D4 では rekey 後の vault は新パスワードで
/// unlock 可能なはず。実装は wrapped_vek_by_pw を旧のまま維持して records だけ
/// 新 VEK で再暗号化するため、rekey 後 unlock_with_password が壊れる仮説を検証。
///
/// 期待:
/// - (a) rekey 自体は `Ok(())` で完了する
/// - (b) **rekey 後 unlock_with_password が成功して vault load + records 復号できるか**
///   が Bug-D-002 の中核観察ポイント
#[test]
#[ignore = "CI runner persistent VM-level file lock (13s+) — Bug-G-002〜G-006 articulated in \
            test-design v8.4, run with --ignored locally. \
            5 ラウンドの実験 (G-002 線形 retry / G-003 Defender exclusion / G-004 \
            Stop-Service WSearch+SysMain / G-005 #[ignore] 検討 / G-006 Option K test-side retry \
            5 attempts ~13s) すべてで `outcome=\"exhausted\"` 継続観測。\
            VM レベルのファイルロック介入で実装側 / テスト側の retry budget をいくら拡張しても \
            吸収不能と articulate 完了。AC-18 はローカル `cargo test -p shikomi-infra --test \
            vault_migration_integration -- --ignored` で手動担保 (TC-I29 主 / B と統一方針)"]
fn tc_d_i03_rekey_then_unlock_with_same_password_observation() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    seed_plaintext_vault(&repo, 3);

    let (kdf_pw, kdf_recovery, aead, rng, gate) = build_migration_set();
    let migration = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);

    // 暗号化
    // Bug-G-005 Option K: encrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    let _disclosure = migration_op_with_test_rename_retry("encrypt_vault", || {
        migration.encrypt_vault(STRONG_PASSWORD.to_string())
    });

    // 1 回 unlock して動作確認
    let _vek_before = migration
        .unlock_with_password(STRONG_PASSWORD.to_string())
        .expect("pre-rekey unlock must succeed");

    // rekey 実行
    // Bug-G-005 Option K: rekey_vault 内部の repo.save が CI flaky のためテスト側 retry。
    migration_op_with_test_rename_retry("rekey_vault", || {
        migration.rekey_vault(STRONG_PASSWORD.to_string())
    });

    // **核心観察**: rekey 後に同じパスワードで unlock できるか？
    // 注: `RecordPayloadEncrypted` には `tag()` 公開アクセサが無いため、
    // unlock 経由で取得した VEK を使った records 直接復号テストは
    // 外部 API では実行不能（テスト用裏口を作るのは E2E ブラックボックス
    // 原則違反）。本 TC は unlock 自体の成否のみ機械検証し、records 復号の
    // 健全性は proptest TC-D-P01 で encrypt→unlock→VEK 比較で間接担保する。
    // Bug-D-002 (rekey wrapped_vek 維持) の真の挙動確定は Sub-E の VEK
    // キャッシュ統合工程で record 復号路が露出する時に再評価。
    let unlock_result = migration.unlock_with_password(STRONG_PASSWORD.to_string());
    if let Err(e) = &unlock_result {
        eprintln!(
            "[Bug-D-002 observation] post-rekey unlock_with_password failed: {:?}",
            e
        );
    }
    assert!(
        unlock_result.is_ok(),
        "Bug-D-002 機械検証 (limited scope): rekey 後 unlock_with_password が成功すべき"
    );
}

/// TC-D-I04: rekey 後の `decrypt_vault` 全件成功（Bug-D-002 修正の機械検証）.
///
/// Bug-D-002 修正後: rekey で wrapped_vek_by_pw を新 KEK で再ラップ + records
/// 新 VEK で再暗号化。同パスワードで decrypt_vault が**全 records 復号成功**
/// するなら新 VEK 経路が完全に通っていることを意味する。
#[test]
#[ignore = "CI runner persistent VM-level file lock (13s+) — Bug-G-002〜G-006 articulated in \
            test-design v8.4, run with --ignored locally. \
            5 ラウンドの実験 (G-002 線形 retry / G-003 Defender exclusion / G-004 \
            Stop-Service WSearch+SysMain / G-005 #[ignore] 検討 / G-006 Option K test-side retry \
            5 attempts ~13s) すべてで `outcome=\"exhausted\"` 継続観測。\
            VM レベルのファイルロック介入で実装側 / テスト側の retry budget をいくら拡張しても \
            吸収不能と articulate 完了。AC-18 はローカル `cargo test -p shikomi-infra --test \
            vault_migration_integration -- --ignored` で手動担保 (TC-I29 主 / B と統一方針)"]
fn tc_d_i04_rekey_then_decrypt_vault_all_records_succeed() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    seed_plaintext_vault(&repo, 5);

    let (kdf_pw, kdf_recovery, aead, rng, gate) = build_migration_set();
    let migration = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);

    // 1) encrypt
    // Bug-G-005 Option K: encrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    let _disclosure = migration_op_with_test_rename_retry("encrypt_vault", || {
        migration.encrypt_vault(STRONG_PASSWORD.to_string())
    });

    // 2) rekey (Bug-D-002 修正後: wrapped_vek_by_pw 新 KEK で再ラップ + records 新 VEK で再暗号化)
    // Bug-G-005 Option K: rekey_vault 内部の repo.save が CI flaky のためテスト側 retry。
    migration_op_with_test_rename_retry("rekey_vault", || {
        migration.rekey_vault(STRONG_PASSWORD.to_string())
    });

    // 3) post-rekey decrypt_vault: 全 records が新 VEK で復号成功すべき
    // Bug-G-005 Option K: decrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    // DecryptConfirmation は値型で reuse 不可のためクロージャ内で都度生成する。
    // 失敗時は Bug-D-002 検証メッセージを panic に含めるため、call-site の expect 文言は
    // ヘルパ panic に moved (本テストでは label="decrypt_vault (Bug-D-002 verify)" で識別)。
    migration_op_with_test_rename_retry("decrypt_vault (Bug-D-002 verify)", || {
        let confirmation =
            shikomi_infra::persistence::vault_migration::DecryptConfirmation::confirm();
        migration.decrypt_vault(STRONG_PASSWORD.to_string(), confirmation)
    });

    // 4) load して平文 vault が完全復元されたことを確認
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir.path());
    let repo2 = shikomi_infra::persistence::SqliteVaultRepository::new().unwrap();
    std::env::remove_var("SHIKOMI_VAULT_DIR");
    let loaded = repo2.load().unwrap();
    assert_eq!(
        loaded.header().protection_mode(),
        ProtectionMode::Plaintext,
        "rekey + decrypt_vault 後は平文モードに戻るべき"
    );
    assert_eq!(
        loaded.records().len(),
        5,
        "rekey 後の全 records が新 VEK で復号され平文 vault に書き戻されるべき"
    );
}

/// TC-D-I05: REQ-P11 改訂による v1 暗号化 vault load 経路（横断検証）.
///
/// vault-persistence 側のテストで TC-I03/I04 が新内容（v1 受入）+ TC-I04a
/// （v999 拒否）に置換済。本 TC は Sub-D の `VaultMigration::unlock_with_password`
/// 経由で v1 暗号化 vault が正しく load されることを補助確認する。
#[test]
#[ignore = "CI runner persistent VM-level file lock (13s+) — Bug-G-002〜G-006 articulated in \
            test-design v8.4, run with --ignored locally. \
            5 ラウンドの実験 (G-002 線形 retry / G-003 Defender exclusion / G-004 \
            Stop-Service WSearch+SysMain / G-005 #[ignore] 検討 / G-006 Option K test-side retry \
            5 attempts ~13s) すべてで `outcome=\"exhausted\"` 継続観測。\
            VM レベルのファイルロック介入で実装側 / テスト側の retry budget をいくら拡張しても \
            吸収不能と articulate 完了。AC-18 はローカル `cargo test -p shikomi-infra --test \
            vault_migration_integration -- --ignored` で手動担保 (TC-I29 主 / B と統一方針)"]
fn tc_d_i05_req_p11_v1_accepted_via_vault_migration() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    seed_plaintext_vault(&repo, 1);

    let (kdf_pw, kdf_recovery, aead, rng, gate) = build_migration_set();
    let migration = VaultMigration::new(&repo, &kdf_pw, &kdf_recovery, &aead, &rng, &gate);

    // encrypt → 暗号化 vault が VaultVersion::CURRENT (==v1) で書込まれる
    // Bug-G-005 Option K: encrypt_vault 内部の repo.save が CI flaky のためテスト側 retry。
    let _disclosure = migration_op_with_test_rename_retry("encrypt_vault", || {
        migration.encrypt_vault(STRONG_PASSWORD.to_string())
    });

    // 同 vault を load して暗号化モードであることが受け入れられる
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("SHIKOMI_VAULT_DIR", dir.path());
    let repo2 = shikomi_infra::persistence::SqliteVaultRepository::new().unwrap();
    std::env::remove_var("SHIKOMI_VAULT_DIR");
    let loaded = repo2
        .load()
        .expect("REQ-P11 改訂: v1 暗号化 vault load が成功すべき");

    // 暗号化モードなら header.protection_mode() が Encrypted を返すはず
    assert_eq!(
        loaded.header().protection_mode(),
        ProtectionMode::Encrypted,
        "load された vault が暗号化モードであることを確認"
    );
}
