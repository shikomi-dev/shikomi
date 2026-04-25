//! vault 集約ルートと関連型の公開インターフェース。

pub mod crypto_data;
pub mod header;
pub mod id;
pub mod nonce;
pub mod protection_mode;
pub mod record;
pub mod version;

pub use crypto_data::{Aad, AuthTag, CipherText, KdfSalt, WrappedVek};
pub use header::{VaultHeader, VaultHeaderEncrypted, VaultHeaderPlaintext};
pub use id::RecordId;
pub use nonce::{NonceBytes, NonceCounter};
pub use protection_mode::ProtectionMode;
pub use record::{Record, RecordKind, RecordLabel, RecordPayload, RecordPayloadEncrypted};
pub use version::VaultVersion;

use crate::crypto::Vek;
use crate::error::{DomainError, VaultConsistencyReason};

// -------------------------------------------------------------------
// VekProvider trait
// -------------------------------------------------------------------

/// VEK（Vault Encryption Key）の再生成・再暗号化を担うプロバイダ trait。
///
/// 実装は `shikomi-infra` に置く（Dependency Inversion）。
/// `shikomi-core` はこの trait シグネチャのみを所有し、暗号実装に依存しない。
///
/// Sub-A 改訂 (Boy Scout Rule): 引数型 `&SecretBytes` を `&Vek` に置換。
/// `Vek` は Clone 禁止のため、`reencrypt_all` は `&mut self, records` のみを取り、
/// 内部で `self.new_vek()` を呼び出して新 VEK を参照する責務を負う
/// (借用衝突回避。Sub-B で具象実装を新設する際の前提)。
pub trait VekProvider {
    /// プロバイダが保持する新 VEK への参照を返す。
    fn new_vek(&self) -> &Vek;

    /// 全レコードを新 VEK で再暗号化する（in-place）。
    ///
    /// 実装は内部で `self.new_vek()` を呼び出して新 VEK を参照する。
    /// 部分失敗した場合は `DomainError::VaultConsistencyError(RekeyPartialFailure)` を返す。
    /// 呼び出し元は `SQLite` トランザクションでアトミック更新を保証すること。
    ///
    /// # Errors
    /// 再暗号化失敗時に `DomainError` を返す。
    fn reencrypt_all(&mut self, records: &mut [Record]) -> Result<(), DomainError>;

    /// 新 VEK をパスワード由来の KEK でラップした `WrappedVek` を返す。
    ///
    /// # Errors
    /// KDF / 暗号化失敗時に `DomainError` を返す。
    fn derive_new_wrapped_pw(&self, vek: &Vek) -> Result<WrappedVek, DomainError>;

    /// 新 VEK をリカバリ由来の KEK でラップした `WrappedVek` を返す。
    ///
    /// # Errors
    /// KDF / 暗号化失敗時に `DomainError` を返す。
    fn derive_new_wrapped_recovery(&self, vek: &Vek) -> Result<WrappedVek, DomainError>;
}

// -------------------------------------------------------------------
// Vault
// -------------------------------------------------------------------

/// vault 集約ルート。
///
/// ヘッダの保護モードと全レコードペイロードの整合性を `add_record` / `update_record`
/// で常に保証する（Fail Fast）。
/// レコード ID の一意性も集約自身が強制する。
pub struct Vault {
    header: VaultHeader,
    records: Vec<Record>,
}

impl Vault {
    /// 空のレコードリストで `Vault` を構築する。
    ///
    /// 空集合はヘッダと常に整合するため、この構築は失敗しない。
    #[must_use]
    pub fn new(header: VaultHeader) -> Self {
        Self {
            header,
            records: Vec::new(),
        }
    }

    /// ヘッダが表す保護モードを返す。
    #[must_use]
    pub fn protection_mode(&self) -> ProtectionMode {
        self.header.protection_mode()
    }

    /// vault ヘッダへの参照を返す。
    #[must_use]
    pub fn header(&self) -> &VaultHeader {
        &self.header
    }

    /// 全レコードのスライスを返す。
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// 指定した ID を持つレコードへの参照を返す。存在しない場合は `None`。
    #[must_use]
    pub fn find_record(&self, id: &RecordId) -> Option<&Record> {
        self.records.iter().find(|r| r.id() == id)
    }

    /// レコードを追加する。
    ///
    /// # Errors
    /// - 保護モードとペイロードが一致しない: `DomainError::VaultConsistencyError(ModeMismatch)`
    /// - 同一 ID のレコードが既に存在する: `DomainError::VaultConsistencyError(DuplicateId)`
    pub fn add_record(&mut self, record: Record) -> Result<(), DomainError> {
        let vault_mode = self.protection_mode();
        let record_mode = record.payload().variant_mode();
        if vault_mode != record_mode {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::ModeMismatch {
                    vault_mode,
                    record_mode,
                },
            ));
        }
        if self.records.iter().any(|r| r.id() == record.id()) {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::DuplicateId(record.id().clone()),
            ));
        }
        self.records.push(record);
        Ok(())
    }

    /// 指定した ID のレコードを削除して返す。
    ///
    /// # Errors
    /// 該当 ID が存在しない場合 `DomainError::VaultConsistencyError(RecordNotFound)` を返す。
    pub fn remove_record(&mut self, id: &RecordId) -> Result<Record, DomainError> {
        let pos = self
            .records
            .iter()
            .position(|r| r.id() == id)
            .ok_or_else(|| {
                DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(
                    id.clone(),
                ))
            })?;
        Ok(self.records.remove(pos))
    }

    /// 指定した ID のレコードに `updater` クロージャを適用して更新する。
    ///
    /// `updater` には元レコードの `clone` を渡す。
    /// `updater` 失敗・モード不一致の場合も `self.records` は変更されない（Fail Fast）。
    ///
    /// # Errors
    /// - 該当 ID が存在しない: `DomainError::VaultConsistencyError(RecordNotFound)`
    /// - updater が `Err` を返した場合: その `DomainError`
    /// - 更新後のペイロードモードが vault と不一致: `DomainError::VaultConsistencyError(ModeMismatch)`
    pub fn update_record<F>(&mut self, id: &RecordId, updater: F) -> Result<(), DomainError>
    where
        F: FnOnce(Record) -> Result<Record, DomainError>,
    {
        let pos = self
            .records
            .iter()
            .position(|r| r.id() == id)
            .ok_or_else(|| {
                DomainError::VaultConsistencyError(VaultConsistencyReason::RecordNotFound(
                    id.clone(),
                ))
            })?;

        // clone を updater に渡す。updater 失敗・モード不一致でも self.records[pos] は untouched。
        let new_record = updater(self.records[pos].clone())?;

        let vault_mode = self.protection_mode();
        let record_mode = new_record.payload().variant_mode();
        if vault_mode != record_mode {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::ModeMismatch {
                    vault_mode,
                    record_mode,
                },
            ));
        }

        self.records[pos] = new_record;
        Ok(())
    }

    /// VEK を再生成し、全レコードを再暗号化する（rekey）。
    ///
    /// 平文 vault に対しては失敗する。
    /// 再暗号化は `VekProvider` に委譲し、`shikomi-core` は暗号実装に依存しない。
    ///
    /// # Errors
    /// - 平文 vault に対する rekey: `DomainError::VaultConsistencyError(RekeyInPlaintextMode)`
    /// - 再暗号化失敗: `DomainError::VaultConsistencyError(RekeyPartialFailure)` 等
    pub fn rekey_with<P: VekProvider>(&mut self, provider: &mut P) -> Result<(), DomainError> {
        if self.protection_mode() != ProtectionMode::Encrypted {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::RekeyInPlaintextMode,
            ));
        }

        // Vek は Clone 禁止 (Sub-A 契約 C-2)。`new_vek()` の参照を順次の借用で消費する。
        // derive_new_wrapped_* は &self、reencrypt_all は &mut self を取るため借用衝突しない。
        let new_wrapped_pw = provider.derive_new_wrapped_pw(provider.new_vek())?;
        let new_wrapped_recovery = provider.derive_new_wrapped_recovery(provider.new_vek())?;
        provider.reencrypt_all(&mut self.records)?;

        // ヘッダの wrapped VEK を更新
        if let VaultHeader::Encrypted(ref mut enc) = self.header {
            enc.replace_wrapped_veks(new_wrapped_pw, new_wrapped_recovery);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
