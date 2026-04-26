//! `VaultMigration` — vault 平文⇄暗号化マイグレーション service (Sub-D 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//! §`VaultMigration` (6 メソッド)
//!
//! ## 6 メソッド
//!
//! - `encrypt_vault`: 平文 vault → 暗号化 vault 初回マイグレーション (F-D1)
//! - `decrypt_vault`: 暗号化 vault → 平文 vault 戻し (F-D3、`DecryptConfirmation` 強制)
//! - `unlock_with_password`: 暗号化 vault パスワード経路 unlock → `Vek` 復元 (F-D2)
//! - `unlock_with_recovery`: 暗号化 vault リカバリ経路 unlock → `Vek` 復元 (F-D2)
//! - `rekey_vault`: VEK 入替 + 全レコード再暗号化 (F-D4、NonceCounter LIMIT 到達時)
//! - `change_password`: マスターパスワード変更 (F-D5、O(1)、VEK 不変)
//!
//! ## DI 構造
//!
//! 設計書 §`VaultMigration::DI`: 6 依存を `&'a` で受ける (`SqliteVaultRepository` /
//! `Argon2idAdapter` / `Bip39Pbkdf2Hkdf` / `AesGcmAeadAdapter` / `Rng` / `ZxcvbnGate`)。
//! 後続 Sub-E でテスト容易性のため trait 境界化を検討する余地を残す (現段階は具体型固定)。
//!
//! ## Clean Architecture
//!
//! - shikomi-core 側型のみを消費 (`Vek` / `MasterPassword` / `RecoveryMnemonic` /
//!   `Verified<Plaintext>` / `Aad` / `WrappedVek` / `Record` / `Vault` / `VaultHeader`)。
//! - shikomi-core への新規依存追加なし。
//! - `aes-gcm` / `argon2` / `bip39` の直接 import なし
//!   (Sub-B/C アダプタ経由のみ、TC-D-S04 sub-d-static-checks.sh で grep 検証)。

use shikomi_core::crypto::{
    HeaderAeadKey, MasterPassword, Plaintext, RecoveryMnemonic, Vek, Verified,
};
use shikomi_core::error::CryptoError;
use shikomi_core::{
    Aad, AuthTag, CipherText, NonceBytes, NonceCounter, ProtectionMode, Record, RecordPayload,
    RecordPayloadEncrypted, SecretString, Vault, VaultHeader, VaultVersion,
};
use time::OffsetDateTime;

use crate::crypto::aead::AesGcmAeadAdapter;
use crate::crypto::kdf::{Argon2idAdapter, Bip39Pbkdf2Hkdf};
use crate::crypto::password::ZxcvbnGate;
use crate::crypto::rng::Rng;
use crate::persistence::error::PersistenceError;
use crate::persistence::repository::SqliteVaultRepository;
use crate::persistence::VaultRepository;

use super::confirmation::DecryptConfirmation;
use super::error::MigrationError;
use super::header::{canonical_aad_bytes, HeaderAeadEnvelope, KdfParams, VaultEncryptedHeader};
use super::recovery::RecoveryDisclosure;
use super::storage::{decode_vault_to_encrypted_header, encode_encrypted_header_for_storage};

/// vault マイグレーション service。
///
/// 設計書 §DI 構造: 全依存を `&'a` で受ける無状態集約。
/// 後続 Sub-E (#43) でテスト容易性のため trait 境界化を検討するが、Sub-D 段階では
/// 具体型固定で開始する (Sub-D 完了優先、trait 化は Sub-E でのリファクタ余地)。
pub struct VaultMigration<'a> {
    /// vault 永続化リポジトリ。
    pub repository: &'a SqliteVaultRepository,
    /// パスワード経路 KDF (Argon2id)。
    pub kdf_pw: &'a Argon2idAdapter,
    /// リカバリ経路 KDF (BIP-39 + PBKDF2 + HKDF)。
    pub kdf_recovery: &'a Bip39Pbkdf2Hkdf,
    /// AEAD アダプタ (AES-256-GCM)。
    pub aead: &'a AesGcmAeadAdapter,
    /// CSPRNG エントリ点。
    pub rng: &'a Rng,
    /// パスワード強度ゲート (zxcvbn)。
    pub gate: &'a ZxcvbnGate,
}

impl<'a> VaultMigration<'a> {
    /// `VaultMigration` を構築する。
    #[must_use]
    pub fn new(
        repository: &'a SqliteVaultRepository,
        kdf_pw: &'a Argon2idAdapter,
        kdf_recovery: &'a Bip39Pbkdf2Hkdf,
        aead: &'a AesGcmAeadAdapter,
        rng: &'a Rng,
        gate: &'a ZxcvbnGate,
    ) -> Self {
        Self {
            repository,
            kdf_pw,
            kdf_recovery,
            aead,
            rng,
            gate,
        }
    }

    // ---------------------------------------------------------------
    // F-D1: encrypt_vault
    // ---------------------------------------------------------------

    /// 平文 vault → 暗号化 vault 初回マイグレーション (F-D1)。
    ///
    /// # Errors
    ///
    /// - 弱パスワード: `CryptoError::WeakPassword` → `MigrationError::Crypto(...)`
    /// - 既に暗号化 vault: `MigrationError::AlreadyEncrypted`
    /// - KDF 失敗: `MigrationError::Crypto(CryptoError::KdfFailed)`
    /// - AEAD 失敗 (構造的に到達しない): `MigrationError::Crypto(AeadTagMismatch)`
    /// - atomic write 失敗: `MigrationError::AtomicWriteFailed { stage }` (原状復帰済)
    pub fn encrypt_vault(&self, password: String) -> Result<RecoveryDisclosure, MigrationError> {
        // 1. MasterPassword 構築 (zxcvbn 強度ゲート)
        let master_password = MasterPassword::new(password, self.gate)?;

        // 2. 既存平文 vault load
        let plaintext_vault = self.repository.load()?;
        if plaintext_vault.protection_mode() != ProtectionMode::Plaintext {
            return Err(MigrationError::AlreadyEncrypted);
        }

        // 3. KdfSalt / VEK / Mnemonic 生成
        let kdf_salt = self.rng.generate_kdf_salt();
        let vek = self.rng.generate_vek();
        let mnemonic = build_recovery_mnemonic(self.rng)?;

        // 4. KEK_pw / KEK_recovery 派生
        let kek_pw = self.kdf_pw.derive_kek_pw(&master_password, &kdf_salt)?;
        let kek_recovery = self.kdf_recovery.derive_kek_recovery(&mnemonic)?;

        // 5. wrapped_vek_by_pw / wrapped_vek_by_recovery 構築
        let wrapped_vek_by_pw =
            self.aead
                .wrap_vek(&kek_pw, &self.rng.generate_nonce_bytes(), &vek)?;
        let wrapped_vek_by_recovery =
            self.aead
                .wrap_vek(&kek_recovery, &self.rng.generate_nonce_bytes(), &vek)?;

        // 6. 各レコードを AEAD 暗号化
        let target_version = plaintext_vault.header().version();
        let target_created_at = plaintext_vault.header().created_at();
        let mut encrypted_records: Vec<Record> =
            Vec::with_capacity(plaintext_vault.records().len());
        for record in plaintext_vault.records() {
            let encrypted_record =
                encrypt_one_record(self.aead, self.rng, &vek, target_version, record)?;
            encrypted_records.push(encrypted_record);
        }

        // 7. ヘッダ AEAD envelope 構築 (空 ciphertext + AAD = canonical_bytes_for_aad)
        let nonce_counter = NonceCounter::new();
        let kdf_params = KdfParams::FROZEN;
        let header_envelope = build_header_envelope(
            self.aead,
            self.rng,
            &kek_pw,
            target_version,
            target_created_at,
            &kdf_salt,
            &wrapped_vek_by_pw,
            &wrapped_vek_by_recovery,
            &nonce_counter,
            kdf_params,
        )?;

        // 8. 完成形 VaultEncryptedHeader 構築
        let encrypted_header = VaultEncryptedHeader::new(
            target_version,
            target_created_at,
            kdf_salt,
            wrapped_vek_by_pw,
            wrapped_vek_by_recovery,
            nonce_counter,
            kdf_params,
            header_envelope,
        );

        // 9. shikomi-core Vault 集約に詰めて save
        let vault_to_save = build_encrypted_vault(&encrypted_header, encrypted_records)?;
        self.repository
            .save(&vault_to_save)
            .map_err(map_persistence_error)?;

        // 10. RecoveryDisclosure 返却 (呼出側が 1 度だけ disclose 表示)
        Ok(RecoveryDisclosure::new(mnemonic, OffsetDateTime::now_utc()))
    }

    // ---------------------------------------------------------------
    // F-D3: decrypt_vault
    // ---------------------------------------------------------------

    /// 暗号化 vault → 平文 vault 戻し (F-D3、片方向降格)。
    ///
    /// `_confirm: DecryptConfirmation` 引数を要求 (C-20、`--force` でも省略不可)。
    ///
    /// # Errors
    ///
    /// - 平文 vault: `MigrationError::NotEncrypted`
    /// - パスワード強度ゲート失敗: `CryptoError::WeakPassword`
    /// - AEAD 検証失敗: `MigrationError::Crypto(AeadTagMismatch)`
    /// - UTF-8 不正: `MigrationError::PlaintextNotUtf8`
    /// - atomic write 失敗: `MigrationError::AtomicWriteFailed { stage }`
    pub fn decrypt_vault(
        &self,
        password: String,
        _confirm: DecryptConfirmation,
    ) -> Result<(), MigrationError> {
        // 1. unlock で VEK 復元 + ヘッダ AEAD 検証
        let (encrypted_vault, encrypted_header, vek) =
            self.unlock_internal_with_password(password)?;

        // 2. 全 EncryptedRecord を復号 → PlaintextRecord 構築
        let mut plaintext_records: Vec<Record> =
            Vec::with_capacity(encrypted_vault.records().len());
        for record in encrypted_vault.records() {
            let plaintext_record = decrypt_one_record(self.aead, &vek, record)?;
            plaintext_records.push(plaintext_record);
        }

        // 3. 平文 vault 集約構築
        let plaintext_header =
            VaultHeader::new_plaintext(encrypted_header.version(), encrypted_header.created_at())
                .map_err(MigrationError::Domain)?;
        let mut plaintext_vault = Vault::new(plaintext_header);
        for r in plaintext_records {
            plaintext_vault
                .add_record(r)
                .map_err(MigrationError::Domain)?;
        }

        // 4. atomic write
        self.repository
            .save(&plaintext_vault)
            .map_err(map_persistence_error)?;

        Ok(())
    }

    // ---------------------------------------------------------------
    // F-D2: unlock_with_password
    // ---------------------------------------------------------------

    /// 暗号化 vault パスワード経路 unlock → `Vek` 復元。
    ///
    /// # Errors
    ///
    /// - 強度ゲート失敗: `CryptoError::WeakPassword`
    /// - 平文 vault: `MigrationError::NotEncrypted`
    /// - ヘッダ AEAD 検証失敗 / wrap 復号失敗: `MigrationError::Crypto(AeadTagMismatch)`
    pub fn unlock_with_password(&self, password: String) -> Result<Vek, MigrationError> {
        let (_vault, _header, vek) = self.unlock_internal_with_password(password)?;
        Ok(vek)
    }

    /// `decrypt_vault` 等から内部利用する unlock。Vault / Header / VEK を返す。
    fn unlock_internal_with_password(
        &self,
        password: String,
    ) -> Result<(Vault, VaultEncryptedHeader, Vek), MigrationError> {
        let master_password = MasterPassword::new(password, self.gate)?;

        let loaded_vault = self.repository.load()?;
        if loaded_vault.protection_mode() != ProtectionMode::Encrypted {
            return Err(MigrationError::NotEncrypted);
        }

        let encrypted_header = decode_vault_to_encrypted_header(&loaded_vault)?;
        let kek_pw = self
            .kdf_pw
            .derive_kek_pw(&master_password, encrypted_header.kdf_salt())?;

        // ヘッダ AEAD タグ検証 (C-17/C-18、L1 nonce_counter 巻戻し / kdf_params 改竄
        // / wrapped_vek 入替を AAD 不一致経由で検出)。
        verify_header_aead(self.aead, &encrypted_header, &kek_pw)?;

        // wrapped_vek_by_pw を unwrap → 32B 検証
        let verified = self
            .aead
            .unwrap_vek(&kek_pw, encrypted_header.wrapped_vek_by_pw())?;
        let vek = unwrap_vek_to_32b(verified)?;

        Ok((loaded_vault, encrypted_header, vek))
    }

    // ---------------------------------------------------------------
    // F-D2: unlock_with_recovery
    // ---------------------------------------------------------------

    /// 暗号化 vault リカバリ経路 unlock → `Vek` 復元。
    ///
    /// # Errors
    ///
    /// - 不正 mnemonic: `CryptoError::InvalidMnemonic`
    /// - 平文 vault: `MigrationError::NotEncrypted`
    /// - wrap 復号失敗: `MigrationError::Crypto(AeadTagMismatch)`
    pub fn unlock_with_recovery(&self, words: [String; 24]) -> Result<Vek, MigrationError> {
        let mnemonic = RecoveryMnemonic::from_words(words);
        let kek_recovery = self.kdf_recovery.derive_kek_recovery(&mnemonic)?;

        let loaded_vault = self.repository.load()?;
        if loaded_vault.protection_mode() != ProtectionMode::Encrypted {
            return Err(MigrationError::NotEncrypted);
        }

        let encrypted_header = decode_vault_to_encrypted_header(&loaded_vault)?;
        // リカバリ経路ではヘッダ AEAD タグ検証は **行わない** (header AEAD 鍵は KEK_pw 派生)。

        let verified = self
            .aead
            .unwrap_vek(&kek_recovery, encrypted_header.wrapped_vek_by_recovery())?;
        let vek = unwrap_vek_to_32b(verified)?;

        Ok(vek)
    }

    // ---------------------------------------------------------------
    // F-D4: rekey_vault
    // ---------------------------------------------------------------

    /// VEK 入替 + 全レコード再暗号化 (NonceCounter LIMIT 到達時の必須経路、F-D4)。
    ///
    /// 設計書 §F-D4 step 4-5 通り、新 VEK を **旧 KEK_pw で再 wrap** して
    /// 新 wrapped_vek_by_pw を構築する (records だけ新 VEK で再暗号化して
    /// wrapped_vek を旧のまま維持すると post-rekey unlock で旧 VEK が復元され
    /// records 復号が全件 `AeadTagMismatch` になる、Bug-D-002 対応)。
    ///
    /// **recovery 経路 (`wrapped_vek_by_recovery`) は本 Sub-D 範囲では更新しない**:
    /// recovery rekey は Sub-E IPC 統合で `change_recovery` メソッド追加予定
    /// (新 mnemonic を呼出側に開示するフローが追加で必要なため、本メソッドの
    /// `current_password: String` 1 引数 API では責務が混ざる)。
    /// rekey 後の **recovery 経路 unlock は本実装範囲外** (旧 mnemonic の
    /// wrapped_vek_by_recovery は旧 VEK を unwrap するため、records 復号で失敗する)。
    ///
    /// # Errors
    ///
    /// 各段階のエラーを `MigrationError` に変換して返す。
    pub fn rekey_vault(&self, current_password: String) -> Result<(), MigrationError> {
        // 0. password を rewrap 用にも使うため clone (KEK_pw 再導出に使う)
        let password_for_rewrap = current_password.clone();

        // 1. unlock で旧 VEK と現在の暗号化 vault を取得
        let (loaded_vault, old_header, old_vek) =
            self.unlock_internal_with_password(current_password)?;

        // 2. 新 VEK 生成
        let new_vek = self.rng.generate_vek();

        // 3. 全レコードを旧 VEK → 新 VEK で再暗号化
        let target_version = old_header.version();
        let mut new_records: Vec<Record> = Vec::with_capacity(loaded_vault.records().len());
        for record in loaded_vault.records() {
            // 旧 VEK で復号
            let plaintext_record_intermediate = decrypt_one_record(self.aead, &old_vek, record)?;
            // 新 VEK で再暗号化
            let new_encrypted = encrypt_one_record(
                self.aead,
                self.rng,
                &new_vek,
                target_version,
                &plaintext_record_intermediate,
            )?;
            new_records.push(new_encrypted);
        }

        // 4. nonce_counter リセット
        let new_nonce_counter = NonceCounter::resume(0);

        // 5. 旧 KEK_pw を再導出 (kdf_salt 不変、password 同一なので KEK は同じだが、
        //    rewrap には key 値が必要なため改めて導出する)。
        let master_pw_rewrap = MasterPassword::new(password_for_rewrap, self.gate)?;
        let kek_pw_rewrap = self
            .kdf_pw
            .derive_kek_pw(&master_pw_rewrap, old_header.kdf_salt())?;

        // 6. 新 VEK を旧 KEK_pw で wrap (新 wrapped_vek_by_pw)
        let new_wrapped_pw =
            self.aead
                .wrap_vek(&kek_pw_rewrap, &self.rng.generate_nonce_bytes(), &new_vek)?;

        // 7. ヘッダ AEAD envelope を新 wrapped_pw + 新 nonce_counter + 旧 kdf_salt +
        //    旧 wrapped_recovery で再構築 (AAD は新フィールドで再計算、改竄検出に必要)。
        let header_envelope = build_header_envelope(
            self.aead,
            self.rng,
            &kek_pw_rewrap,
            old_header.version(),
            old_header.created_at(),
            old_header.kdf_salt(),
            &new_wrapped_pw,
            old_header.wrapped_vek_by_recovery(),
            &new_nonce_counter,
            old_header.kdf_params(),
        )?;

        // 8. ヘッダ更新: 新 wrapped_vek_by_pw + 旧 wrapped_vek_by_recovery + 新 nonce_counter
        //    + 新 envelope。NOTE: wrapped_vek_by_recovery は本 Sub-D 範囲では更新しない
        //    (recovery rekey は Sub-E `change_recovery` で対応、メソッド doc 参照)。
        let new_header = VaultEncryptedHeader::new(
            old_header.version(),
            old_header.created_at(),
            old_header.kdf_salt().clone(),
            new_wrapped_pw,
            old_header.wrapped_vek_by_recovery().clone(),
            new_nonce_counter,
            old_header.kdf_params(),
            header_envelope,
        );

        // 9. 新 vault 集約構築 + save
        let vault_to_save = build_encrypted_vault(&new_header, new_records)?;
        self.repository
            .save(&vault_to_save)
            .map_err(map_persistence_error)?;

        // 旧 VEK / 新 VEK / 再導出 KEK_pw は scope 抜けで Drop & zeroize
        drop(old_vek);
        drop(new_vek);
        drop(kek_pw_rewrap);

        Ok(())
    }

    // ---------------------------------------------------------------
    // F-D5: change_password
    // ---------------------------------------------------------------

    /// マスターパスワード変更 (O(1)、VEK 不変、wrapped_vek_by_pw のみ更新、新 salt)。
    ///
    /// REQ-S10 / 設計書 §F-D5。
    ///
    /// # Errors
    ///
    /// - 旧パスワードでの unlock 失敗: `MigrationError::Crypto(AeadTagMismatch)`
    /// - 新パスワード強度ゲート失敗: `CryptoError::WeakPassword`
    pub fn change_password(
        &self,
        old_password: String,
        new_password: String,
    ) -> Result<(), MigrationError> {
        // 1. 旧パスワードで unlock → 旧 VEK と現在のヘッダを取得
        let (loaded_vault, old_header, vek) = self.unlock_internal_with_password(old_password)?;

        // 2. 新 MasterPassword (強度ゲート)
        let new_master_password = MasterPassword::new(new_password, self.gate)?;

        // 3. 新 KdfSalt 生成 (旧 salt 流用禁止)
        let new_kdf_salt = self.rng.generate_kdf_salt();

        // 4. 新 KEK_pw 派生
        let new_kek_pw = self
            .kdf_pw
            .derive_kek_pw(&new_master_password, &new_kdf_salt)?;

        // 5. 新 wrapped_vek_by_pw 構築
        let new_wrapped_vek_by_pw =
            self.aead
                .wrap_vek(&new_kek_pw, &self.rng.generate_nonce_bytes(), &vek)?;

        // 6. ヘッダ AEAD envelope を新 kdf_salt + 新 wrapped_vek_by_pw で再構築
        let nonce_counter = NonceCounter::resume(old_header.nonce_counter().current());
        let header_envelope = build_header_envelope(
            self.aead,
            self.rng,
            &new_kek_pw,
            old_header.version(),
            old_header.created_at(),
            &new_kdf_salt,
            &new_wrapped_vek_by_pw,
            old_header.wrapped_vek_by_recovery(),
            &nonce_counter,
            old_header.kdf_params(),
        )?;

        // 7. 新ヘッダ完成
        let new_header = VaultEncryptedHeader::new(
            old_header.version(),
            old_header.created_at(),
            new_kdf_salt,
            new_wrapped_vek_by_pw,
            old_header.wrapped_vek_by_recovery().clone(),
            nonce_counter,
            old_header.kdf_params(),
            header_envelope,
        );

        // 8. 既存 records はそのまま (VEK 不変、再暗号化不要)
        let preserved_records: Vec<Record> = loaded_vault.records().to_vec();

        // 9. atomic write
        let vault_to_save = build_encrypted_vault(&new_header, preserved_records)?;
        self.repository
            .save(&vault_to_save)
            .map_err(map_persistence_error)?;

        drop(vek);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 内部ヘルパ
// ---------------------------------------------------------------------------

/// `PersistenceError::AtomicWriteFailed` を `MigrationError::AtomicWriteFailed` に
/// 透過させ、それ以外はそのまま `Persistence(_)` に詰める。
fn map_persistence_error(e: PersistenceError) -> MigrationError {
    match e {
        PersistenceError::AtomicWriteFailed { stage, source } => {
            MigrationError::AtomicWriteFailed { stage, source }
        }
        other => MigrationError::Persistence(other),
    }
}

/// `Verified<Plaintext>` の中身を 32B 配列として取り出して `Vek` を構築する。
fn unwrap_vek_to_32b(verified: Verified<Plaintext>) -> Result<Vek, MigrationError> {
    let bytes = verified.into_inner().expose_secret().to_vec();
    if bytes.len() != 32 {
        return Err(MigrationError::Crypto(CryptoError::AeadTagMismatch));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Vek::from_array(arr))
}

/// `RecoveryMnemonic` を CSPRNG エントロピーから構築する。
fn build_recovery_mnemonic(rng: &Rng) -> Result<RecoveryMnemonic, MigrationError> {
    let entropy = rng.generate_mnemonic_entropy();
    let bip39_mnemonic = bip39::Mnemonic::from_entropy(&entropy[..])
        .map_err(|_| MigrationError::Crypto(CryptoError::InvalidMnemonic))?;
    let mut words: [String; 24] = std::array::from_fn(|_| String::new());
    for (i, w) in bip39_mnemonic.words().enumerate() {
        if i >= 24 {
            break;
        }
        words[i] = w.to_string();
    }
    Ok(RecoveryMnemonic::from_words(words))
}

/// 1 record を VEK で AEAD 暗号化し、ciphertext+tag を連結した形式で `Record` を返す。
fn encrypt_one_record(
    aead: &AesGcmAeadAdapter,
    rng: &Rng,
    vek: &Vek,
    target_version: VaultVersion,
    record: &Record,
) -> Result<Record, MigrationError> {
    let plaintext_bytes = match record.payload() {
        RecordPayload::Plaintext(secret) => secret.expose_secret().as_bytes().to_vec(),
        RecordPayload::Encrypted(_) => return Err(MigrationError::AlreadyEncrypted),
    };
    let nonce = rng.generate_nonce_bytes();
    let aad = Aad::new(record.id().clone(), target_version, record.created_at())
        .map_err(MigrationError::Domain)?;
    let (ciphertext, tag) = aead.encrypt_record(vek, &nonce, &aad, &plaintext_bytes)?;
    let merged = concat_ciphertext_and_tag(ciphertext, &tag);
    let encrypted_payload = RecordPayloadEncrypted::new(
        nonce,
        CipherText::try_new(merged.into_boxed_slice()).map_err(MigrationError::Domain)?,
        aad,
    )
    .map_err(MigrationError::Domain)?;
    Record::rehydrate(
        record.id().clone(),
        record.kind(),
        record.label().clone(),
        RecordPayload::Encrypted(encrypted_payload),
        record.created_at(),
        record.updated_at(),
    )
    .map_err(MigrationError::Domain)
}

/// 1 record を VEK で AEAD 復号し、`RecordPayload::Plaintext` の `Record` を返す。
fn decrypt_one_record(
    aead: &AesGcmAeadAdapter,
    vek: &Vek,
    record: &Record,
) -> Result<Record, MigrationError> {
    match record.payload() {
        RecordPayload::Encrypted(enc) => {
            let nonce = enc.nonce().clone();
            let aad = enc.aad().clone();
            let (ct_only, tag) = split_ciphertext_and_tag(enc.ciphertext().as_bytes())?;
            let verified: Verified<Plaintext> =
                aead.decrypt_record(vek, &nonce, &aad, ct_only, &tag)?;
            let plaintext_bytes = verified.into_inner().expose_secret().to_vec();
            let s =
                String::from_utf8(plaintext_bytes).map_err(|_| MigrationError::PlaintextNotUtf8)?;
            let plaintext_payload = RecordPayload::Plaintext(SecretString::from_string(s));
            Record::rehydrate(
                record.id().clone(),
                record.kind(),
                record.label().clone(),
                plaintext_payload,
                record.created_at(),
                record.updated_at(),
            )
            .map_err(MigrationError::Domain)
        }
        RecordPayload::Plaintext(_) => Err(MigrationError::NotEncrypted),
    }
}

/// `EncryptedRecord` 永続化形式: ciphertext と tag を `ct ‖ tag` (16B) 形式に連結する。
fn concat_ciphertext_and_tag(ct: Vec<u8>, tag: &AuthTag) -> Vec<u8> {
    let mut out = ct;
    out.extend_from_slice(tag.as_array());
    out
}

/// 永続化された `ct ‖ tag` (16B) BLOB を分離する。
fn split_ciphertext_and_tag(blob: &[u8]) -> Result<(&[u8], AuthTag), MigrationError> {
    if blob.len() < 16 {
        return Err(MigrationError::Crypto(CryptoError::AeadTagMismatch));
    }
    let split_at = blob.len() - 16;
    let (ct, tag_bytes) = blob.split_at(split_at);
    let mut tag_arr = [0u8; 16];
    tag_arr.copy_from_slice(tag_bytes);
    Ok((ct, AuthTag::from_array(tag_arr)))
}

/// shikomi-core `Vault` 集約に encrypted records を詰める。
fn build_encrypted_vault(
    header: &VaultEncryptedHeader,
    records: Vec<Record>,
) -> Result<Vault, MigrationError> {
    let core_header = encode_encrypted_header_for_storage(header)?;
    let mut vault = Vault::new(core_header);
    for r in records {
        vault.add_record(r).map_err(MigrationError::Domain)?;
    }
    Ok(vault)
}

/// ヘッダ AEAD envelope を構築する (C-17/C-18 正規実装)。
///
/// `canonical_aad_bytes` で組み立てた任意長 AAD を `encrypt_with_raw_aad` に渡し、
/// **空 plaintext + AAD のみ** で AES-256-GCM の 16B authentication tag を取得する
/// (MAC として動作させる用途)。返却 envelope の `ciphertext` は 0 byte 固定、
/// `nonce` 12B + `tag` 16B のみが意味を持つ (改竄検出専用、鍵を運ばない)。
///
/// AAD は `VaultEncryptedHeader::canonical_bytes_for_aad` と完全同型のレイアウトで
/// 構築される (`canonical_aad_bytes` 経由で **header.rs と DRY**)。
///
/// # Errors
///
/// AEAD 内部エラー時 `MigrationError::Crypto(AeadTagMismatch)`。
#[allow(clippy::too_many_arguments)]
fn build_header_envelope(
    aead: &AesGcmAeadAdapter,
    rng: &Rng,
    kek_pw: &shikomi_core::Kek<shikomi_core::crypto::KekKindPw>,
    version: VaultVersion,
    created_at: OffsetDateTime,
    kdf_salt: &shikomi_core::KdfSalt,
    wrapped_pw: &shikomi_core::WrappedVek,
    wrapped_recovery: &shikomi_core::WrappedVek,
    nonce_counter: &NonceCounter,
    kdf_params: KdfParams,
) -> Result<HeaderAeadEnvelope, MigrationError> {
    let aad = canonical_aad_bytes(
        version,
        created_at,
        kdf_salt,
        wrapped_pw,
        wrapped_recovery,
        nonce_counter,
        kdf_params,
    );
    let nonce = rng.generate_nonce_bytes();
    // 空 plaintext + raw AAD で MAC として動作させる (AES-256-GCM の認証付き、
    // ciphertext は 0 byte、tag のみが意味を持つ)。
    let (ciphertext, tag) = aead
        .encrypt_with_raw_aad(kek_pw, &nonce, &aad, &[])
        .map_err(MigrationError::Crypto)?;
    Ok(HeaderAeadEnvelope::new(ciphertext, nonce, tag))
}

/// ヘッダ AEAD タグ検証 (`build_header_envelope` と対称、C-17/C-18 正規実装)。
///
/// `header.canonical_bytes_for_aad()` を AAD に取り、`decrypt_with_raw_aad` で
/// AEAD タグ検証を実施する。検証成功時のみ `Ok(())` を返す。
/// L1 nonce_counter 巻戻し / kdf_params 改竄 / wrapped_vek 入替などは AAD ハッシュ
/// が一致しなくなるため `AeadTagMismatch` で検出される。
///
/// # Errors
///
/// AEAD 検証失敗時 `MigrationError::Crypto(AeadTagMismatch)` (内部詳細秘匿)。
fn verify_header_aead(
    aead: &AesGcmAeadAdapter,
    header: &VaultEncryptedHeader,
    kek_pw: &shikomi_core::Kek<shikomi_core::crypto::KekKindPw>,
) -> Result<(), MigrationError> {
    let aad = header.canonical_bytes_for_aad();
    let envelope = header.header_aead_envelope();
    aead.decrypt_with_raw_aad(
        kek_pw,
        &envelope.nonce,
        &aad,
        &envelope.ciphertext,
        &envelope.tag,
    )
    .map(|_verified| ())
    .map_err(MigrationError::Crypto)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内部ヘルパの sanity check: ciphertext と tag の連結・分離が往復する。
    #[test]
    fn concat_split_ciphertext_and_tag_round_trip() {
        let ct = vec![0xAAu8; 24];
        let tag = AuthTag::from_array([0xBBu8; 16]);
        let merged = concat_ciphertext_and_tag(ct.clone(), &tag);
        assert_eq!(merged.len(), 24 + 16);
        let (ct_back, tag_back) = split_ciphertext_and_tag(&merged).unwrap();
        assert_eq!(ct_back, &ct[..]);
        assert_eq!(tag_back.as_array(), tag.as_array());
    }

    #[test]
    fn split_ciphertext_with_too_short_blob_returns_aead_tag_mismatch() {
        let blob = vec![0u8; 8];
        let err = split_ciphertext_and_tag(&blob).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::Crypto(CryptoError::AeadTagMismatch)
        ));
    }
}
