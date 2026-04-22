//! vault フォーマットバージョン管理。

use crate::error::DomainError;

// -------------------------------------------------------------------
// VaultVersion
// -------------------------------------------------------------------

/// vault ファイルフォーマットのバージョンを表す newtype。
///
/// `CURRENT` (1) と `MIN_SUPPORTED` (1) の範囲でのみ有効。
/// 現時点では 1 のみが有効なバージョン。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VaultVersion(u16);

impl VaultVersion {
    /// 現在サポートする最新バージョン。新規 vault 作成時はこの値を使う。
    pub const CURRENT: Self = Self(1);

    /// 読み込みに対応する最小バージョン。これより古い vault は拒否する。
    pub const MIN_SUPPORTED: Self = Self(1);

    /// `u16` 値から `VaultVersion` を生成する。
    ///
    /// # Errors
    /// `MIN_SUPPORTED..=CURRENT` の範囲外の場合 `DomainError::UnsupportedVaultVersion` を返す。
    pub fn try_new(value: u16) -> Result<Self, DomainError> {
        if value < Self::MIN_SUPPORTED.0 || value > Self::CURRENT.0 {
            return Err(DomainError::UnsupportedVaultVersion(value));
        }
        Ok(Self(value))
    }

    /// 内包する `u16` 値を返す。
    #[must_use]
    pub fn value(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_new_current_version_ok() {
        assert!(VaultVersion::try_new(1).is_ok());
    }

    #[test]
    fn test_try_new_below_min_supported_returns_error() {
        let err = VaultVersion::try_new(0).unwrap_err();
        assert!(matches!(err, DomainError::UnsupportedVaultVersion(0)));
    }

    #[test]
    fn test_try_new_above_current_returns_error() {
        let err = VaultVersion::try_new(2).unwrap_err();
        assert!(matches!(err, DomainError::UnsupportedVaultVersion(2)));
    }

    #[test]
    fn test_value_returns_inner_u16() {
        let v = VaultVersion::try_new(1).unwrap();
        assert_eq!(v.value(), 1u16);
    }
}
