//! Sub-E (#43) IPC V2 結合テスト 共通ヘルパ.
//!
//! `tests/sub_e_v2_integration.rs` のエントリから `mod helpers;` で取り込まれる。
//! 各 sub module (backoff / unlock / lock_lifecycle / handshake / rekey_rotate /
//! sanity) は `use super::helpers::...` で共通の構築関数 / 定数を利用する。
//!
//! ペガサス工程5指摘対応 (ファイル分割): 単一 693 行を `sub_e_v2_integration/` 配下
//! に責務別 module として再編、共通定数・ヘルパは本ファイルに一元化 (DRY)。

#![allow(dead_code)] // 各 sub module で部分的に利用される

use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest, IpcResponse, SerializableSecretBytes};
use shikomi_core::secret::SecretBytes;
use shikomi_core::{
    Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
    VaultVersion,
};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use shikomi_daemon::ipc::v2_handler::{dispatch_v2, ClientState, V2Context};
use shikomi_infra::crypto::aead::AesGcmAeadAdapter;
use shikomi_infra::crypto::kdf::{Argon2idAdapter, Argon2idParams, Bip39Pbkdf2Hkdf};
use shikomi_infra::crypto::password::ZxcvbnGate;
use shikomi_infra::crypto::rng::Rng;
use shikomi_infra::persistence::vault_migration::VaultMigration;
use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
use time::OffsetDateTime;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 強パスワード (zxcvbn ≥ 3 通過想定)。
pub const STRONG_PASSWORD: &str = "correct horse battery staple long enough phrase";

/// 本番より低コストな Argon2id params (テスト専用、shikomi-infra TC で本番強度別途担保)。
pub fn fast_argon2_params() -> Argon2idParams {
    Argon2idParams {
        m: 8,
        t: 1,
        p: 1,
        output_len: 32,
    }
}

pub fn fast_argon2_adapter() -> Argon2idAdapter {
    Argon2idAdapter::new(fast_argon2_params())
}

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

/// 平文 vault を seed する (n 件のレコード)。
pub fn seed_plaintext_vault(repo: &SqliteVaultRepository, n: usize) {
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

/// 既存平文 vault を `encrypt_vault` で暗号化 vault に変換する。
pub fn encrypt_existing_vault(repo: &SqliteVaultRepository) {
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

pub fn secret(s: &str) -> SerializableSecretBytes {
    SerializableSecretBytes::new(SecretBytes::from_vec(s.as_bytes().to_vec()))
}

/// テスト用に `V2Context` を構築して `dispatch_v2` を呼ぶ。
///
/// 本テスト群では V1 委譲経路 (`ListRecords` 等) は使わないため `vault: Mutex<Vault>` は
/// 空の plaintext で OK (V2 専用 variant + handshake 経路のみ検証する)。
pub async fn run_dispatch(
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

pub fn handshake_v2() -> ClientState {
    ClientState::Handshake {
        version: IpcProtocolVersion::V2,
    }
}

pub fn handshake_v1() -> ClientState {
    ClientState::Handshake {
        version: IpcProtocolVersion::V1,
    }
}
