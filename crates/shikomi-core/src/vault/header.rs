//! vault ヘッダ（平文 / 暗号化バリアント）。
//!
//! `VaultHeader` は enum で平文 / 暗号化の 2 バリアントを型レベルで排他する。
//! 平文ヘッダには暗号フィールドが存在せず、型で不正状態を排除する（Fail Fast）。

use time::OffsetDateTime;

use crate::error::{DomainError, InvalidVaultHeaderReason};
use crate::vault::crypto_data::{KdfSalt, WrappedVek};
use crate::vault::protection_mode::ProtectionMode;
use crate::vault::version::VaultVersion;

// -------------------------------------------------------------------
// 平文ヘッダ内部型
// -------------------------------------------------------------------

/// 平文モード用 vault ヘッダの内部データ。
#[derive(Debug, Clone)]
pub struct VaultHeaderPlaintext {
    pub(super) version: VaultVersion,
    pub(super) created_at: OffsetDateTime,
}

// -------------------------------------------------------------------
// 暗号化ヘッダ内部型
// -------------------------------------------------------------------

/// 暗号化モード用 vault ヘッダの内部データ。
#[derive(Debug, Clone)]
pub struct VaultHeaderEncrypted {
    pub(super) version: VaultVersion,
    pub(super) created_at: OffsetDateTime,
    pub(super) kdf_salt: KdfSalt,
    pub(super) wrapped_vek_by_pw: WrappedVek,
    pub(super) wrapped_vek_by_recovery: WrappedVek,
}

impl VaultHeaderEncrypted {
    /// rekey 時に wrapped VEK を更新する（vault/mod.rs 内部からのみ呼び出し）。
    pub(super) fn replace_wrapped_veks(
        &mut self,
        wrapped_vek_by_pw: WrappedVek,
        wrapped_vek_by_recovery: WrappedVek,
    ) {
        self.wrapped_vek_by_pw = wrapped_vek_by_pw;
        self.wrapped_vek_by_recovery = wrapped_vek_by_recovery;
    }
}

// -------------------------------------------------------------------
// VaultHeader
// -------------------------------------------------------------------

/// vault ヘッダ。保護モードごとに異なるフィールドを enum バリアントで排他する。
#[derive(Debug, Clone)]
pub enum VaultHeader {
    /// 平文モードのヘッダ（暗号フィールドなし）。
    Plaintext(VaultHeaderPlaintext),
    /// 暗号化モードのヘッダ（KDF ソルト、Wrapped VEK を保持）。
    Encrypted(VaultHeaderEncrypted),
}

impl VaultHeader {
    /// 平文モードのヘッダを構築する。
    ///
    /// # Errors
    /// `version` が対応範囲外の場合 `DomainError::UnsupportedVaultVersion` を返す。
    pub fn new_plaintext(
        version: VaultVersion,
        created_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        // VaultVersion::try_new で既に検証済みだが防御的にチェック
        if version < VaultVersion::MIN_SUPPORTED || version > VaultVersion::CURRENT {
            return Err(DomainError::UnsupportedVaultVersion(version.value()));
        }
        Ok(Self::Plaintext(VaultHeaderPlaintext {
            version,
            created_at,
        }))
    }

    /// 暗号化モードのヘッダを構築する。
    ///
    /// `kdf_salt` / `wrapped_vek_by_pw` / `wrapped_vek_by_recovery` は
    /// それぞれの `try_new` で検証済みの型を渡すこと。
    ///
    /// # Errors
    /// `version` が対応範囲外の場合 `DomainError::UnsupportedVaultVersion` を返す。
    pub fn new_encrypted(
        version: VaultVersion,
        created_at: OffsetDateTime,
        kdf_salt: KdfSalt,
        wrapped_vek_by_pw: WrappedVek,
        wrapped_vek_by_recovery: WrappedVek,
    ) -> Result<Self, DomainError> {
        if version < VaultVersion::MIN_SUPPORTED || version > VaultVersion::CURRENT {
            return Err(DomainError::UnsupportedVaultVersion(version.value()));
        }
        Ok(Self::Encrypted(VaultHeaderEncrypted {
            version,
            created_at,
            kdf_salt,
            wrapped_vek_by_pw,
            wrapped_vek_by_recovery,
        }))
    }

    /// ヘッダが表す保護モードを返す。
    #[must_use]
    pub fn protection_mode(&self) -> ProtectionMode {
        match self {
            Self::Plaintext(_) => ProtectionMode::Plaintext,
            Self::Encrypted(_) => ProtectionMode::Encrypted,
        }
    }

    /// vault のフォーマットバージョンを返す。
    #[must_use]
    pub fn version(&self) -> VaultVersion {
        match self {
            Self::Plaintext(h) => h.version,
            Self::Encrypted(h) => h.version,
        }
    }

    /// vault の作成時刻を返す。
    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        match self {
            Self::Plaintext(h) => h.created_at,
            Self::Encrypted(h) => h.created_at,
        }
    }

    /// 暗号化ヘッダの KDF ソルトへの参照を返す。平文ヘッダの場合は `None`。
    #[must_use]
    pub fn kdf_salt(&self) -> Option<&KdfSalt> {
        match self {
            Self::Plaintext(_) => None,
            Self::Encrypted(h) => Some(&h.kdf_salt),
        }
    }

    /// 暗号化ヘッダのパスワード経路 Wrapped VEK への参照を返す。平文ヘッダは `None`。
    #[must_use]
    pub fn wrapped_vek_by_pw(&self) -> Option<&WrappedVek> {
        match self {
            Self::Plaintext(_) => None,
            Self::Encrypted(h) => Some(&h.wrapped_vek_by_pw),
        }
    }

    /// 暗号化ヘッダのリカバリ経路 Wrapped VEK への参照を返す。平文ヘッダは `None`。
    #[must_use]
    pub fn wrapped_vek_by_recovery(&self) -> Option<&WrappedVek> {
        match self {
            Self::Plaintext(_) => None,
            Self::Encrypted(h) => Some(&h.wrapped_vek_by_recovery),
        }
    }
}

// 不使用の型のみエクスポート - `DomainError` 構築に必要な場合に備えて
// InvalidVaultHeaderReason は error.rs にある
#[allow(unused_imports)]
use InvalidVaultHeaderReason as _;
