//! 保護モード（平文 / 暗号化）の列挙型。

use crate::error::DomainError;

// -------------------------------------------------------------------
// ProtectionMode
// -------------------------------------------------------------------

/// vault の保護モード。平文か暗号化かを排他的に表現する。
///
/// 永続化は `as_persisted_str()` / `try_from_persisted_str()` 経由で行う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionMode {
    /// 平文モード。レコードペイロードを暗号化しない。
    Plaintext,
    /// 暗号化モード。レコードペイロードを AES-256-GCM で保護する。
    Encrypted,
}

impl ProtectionMode {
    /// 永続化文字列（`"plaintext"` / `"encrypted"`）を返す。
    #[must_use]
    pub fn as_persisted_str(&self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Encrypted => "encrypted",
        }
    }

    /// 永続化文字列から `ProtectionMode` を復元する。
    ///
    /// # Errors
    /// 未知の文字列の場合 `DomainError::InvalidProtectionMode` を返す。
    pub fn try_from_persisted_str(s: &str) -> Result<Self, DomainError> {
        match s {
            "plaintext" => Ok(Self::Plaintext),
            "encrypted" => Ok(Self::Encrypted),
            other => Err(DomainError::InvalidProtectionMode(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_persisted_str_plaintext_returns_plaintext() {
        assert_eq!(ProtectionMode::Plaintext.as_persisted_str(), "plaintext");
    }

    #[test]
    fn test_as_persisted_str_encrypted_returns_encrypted() {
        assert_eq!(ProtectionMode::Encrypted.as_persisted_str(), "encrypted");
    }

    #[test]
    fn test_try_from_persisted_str_plaintext_ok() {
        assert_eq!(
            ProtectionMode::try_from_persisted_str("plaintext").unwrap(),
            ProtectionMode::Plaintext
        );
    }

    #[test]
    fn test_try_from_persisted_str_encrypted_ok() {
        assert_eq!(
            ProtectionMode::try_from_persisted_str("encrypted").unwrap(),
            ProtectionMode::Encrypted
        );
    }

    #[test]
    fn test_try_from_persisted_str_unknown_returns_invalid_protection_mode() {
        let err = ProtectionMode::try_from_persisted_str("PLAINTEXT").unwrap_err();
        assert!(matches!(err, DomainError::InvalidProtectionMode(_)));
    }
}
