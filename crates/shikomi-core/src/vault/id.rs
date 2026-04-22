//! レコード ID（UUIDv7 newtype）。

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

use crate::error::{DomainError, InvalidRecordIdReason};

// -------------------------------------------------------------------
// RecordId
// -------------------------------------------------------------------

/// レコードを一意に識別する `UUIDv7` newtype。
///
/// `UUIDv7` 以外のバージョン・nil UUID は構築時に拒否される（Fail Fast）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordId {
    inner: Uuid,
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
