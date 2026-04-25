//! レコード ID（UUIDv7 newtype）。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::{DomainError, InvalidRecordIdReason};

// -------------------------------------------------------------------
// RecordId
// -------------------------------------------------------------------

/// レコードを一意に識別する `UUIDv7` newtype。
///
/// `UUIDv7` 以外のバージョン・nil UUID は構築時に拒否される（Fail Fast）。
///
/// IPC 経路では `String`（UUIDv7 表記）として送受信する（`Display` / `FromStr` 整合）。
///
/// `Hash` は `IpcVaultRepository` の差分検出（`HashSet` / `HashMap` キー利用）で
/// 必要となる。Newtype 内部の `Uuid` は既に `Hash` 実装済みのため、derive で完結する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordId {
    inner: Uuid,
}

impl Serialize for RecordId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.inner)
    }
}

impl<'de> Deserialize<'de> for RecordId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl RecordId {
    /// `Uuid` から `RecordId` を構築する。
    ///
    /// # Errors
    /// - `UUIDv7` でない場合: `DomainError::InvalidRecordId(WrongVersion)`
    /// - nil UUID の場合: `DomainError::InvalidRecordId(NilUuid)`
    pub fn new(uuid: Uuid) -> Result<Self, DomainError> {
        if uuid.is_nil() {
            return Err(DomainError::InvalidRecordId(InvalidRecordIdReason::NilUuid));
        }
        // uuid::Version::SortRand は UUIDv7
        match uuid.get_version() {
            Some(uuid::Version::SortRand) => {}
            other => {
                let actual = other.map_or(0, |v| v as u8);
                return Err(DomainError::InvalidRecordId(
                    InvalidRecordIdReason::WrongVersion { actual },
                ));
            }
        }
        Ok(Self { inner: uuid })
    }

    /// UUID 文字列から `RecordId` を構築する。
    ///
    /// # Errors
    /// - パース失敗: `DomainError::InvalidRecordId(ParseError)`
    /// - `UUIDv7` 以外・nil UUID: 同上の対応バリアント
    pub fn try_from_str(s: &str) -> Result<Self, DomainError> {
        let uuid = Uuid::parse_str(s).map_err(|e| {
            DomainError::InvalidRecordId(InvalidRecordIdReason::ParseError(e.to_string()))
        })?;
        Self::new(uuid)
    }

    /// 内包する `Uuid` への参照を返す。
    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.inner
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl FromStr for RecordId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn valid_uuid_v7() -> Uuid {
        Uuid::now_v7()
    }

    #[test]
    fn test_new_with_valid_uuidv7_ok() {
        assert!(RecordId::new(valid_uuid_v7()).is_ok());
    }

    #[test]
    fn test_new_with_uuidv4_returns_wrong_version_error() {
        let uuid_v4 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let err = RecordId::new(uuid_v4).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordId(crate::error::InvalidRecordIdReason::WrongVersion {
                actual: 4
            })
        ));
    }

    #[test]
    fn test_new_with_nil_uuid_returns_nil_uuid_error() {
        let err = RecordId::new(Uuid::nil()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordId(crate::error::InvalidRecordIdReason::NilUuid)
        ));
    }

    #[test]
    fn test_try_from_str_with_valid_uuidv7_string_ok() {
        // A valid UUIDv7 string (version=7, variant=10xx)
        let result = RecordId::try_from_str("01234567-0123-7000-8000-0123456789ab");
        assert!(result.is_ok(), "expected Ok but got {:?}", result);
    }

    #[test]
    fn test_try_from_str_with_invalid_string_returns_parse_error() {
        let err = RecordId::try_from_str("not-a-uuid").unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordId(crate::error::InvalidRecordIdReason::ParseError(_))
        ));
    }

    #[test]
    fn test_as_uuid_returns_stored_uuid() {
        let uuid = valid_uuid_v7();
        let id = RecordId::new(uuid).unwrap();
        assert_eq!(id.as_uuid(), &uuid);
    }
}
