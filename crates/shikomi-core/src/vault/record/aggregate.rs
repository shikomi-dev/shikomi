//! `Record` エンティティと永続化用 rehydration コンストラクタ。

use time::OffsetDateTime;

use crate::error::{DomainError, VaultConsistencyReason};
use crate::vault::id::RecordId;

use super::kind::RecordKind;
use super::label::RecordLabel;
use super::payload::RecordPayload;

// -------------------------------------------------------------------
// ユーティリティ（モジュール非公開）
// -------------------------------------------------------------------

/// `OffsetDateTime` をマイクロ秒精度に切り捨てる（サブマイクロ秒は切り捨て）。
///
/// `Record::new` / `Record::rehydrate` / `Record::with_updated_*` が内部で呼び出す。
/// 永続化（`SQLite` RFC3339）と AAD 計算のラウンドトリップを保証するため。
fn truncate_to_microsecond(dt: OffsetDateTime) -> OffsetDateTime {
    // nanosecond() は [0, 999_999_999]。% 1_000 でサブマイクロ秒 ns を取り出す。
    let sub_micro_ns = i64::from(dt.nanosecond() % 1_000);
    dt - time::Duration::nanoseconds(sub_micro_ns)
}

// -------------------------------------------------------------------
// Record
// -------------------------------------------------------------------

/// vault 内のレコードエンティティ。
///
/// `Record::new` に渡す引数は全て検証済み型のみ受け付けるため、
/// 構築自体は失敗しない（Fail Fast は各引数の型構築時に行われる）。
#[derive(Debug, Clone)]
pub struct Record {
    id: RecordId,
    kind: RecordKind,
    label: RecordLabel,
    payload: RecordPayload,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl Record {
    /// レコードを構築する。
    ///
    /// `created_at = updated_at = now`（マイクロ秒精度に切り捨て済み）。
    #[must_use]
    pub fn new(
        id: RecordId,
        kind: RecordKind,
        label: RecordLabel,
        payload: RecordPayload,
        now: OffsetDateTime,
    ) -> Self {
        let ts = truncate_to_microsecond(now);
        Self {
            id,
            kind,
            label,
            payload,
            created_at: ts,
            updated_at: ts,
        }
    }

    /// 永続化層からレコードを復元する（rehydration コンストラクタ）。
    ///
    /// `Record::new` と異なり `created_at` / `updated_at` を引数から直接設定する。
    /// サブマイクロ秒成分はマイクロ秒精度に切り捨てる。
    /// 副作用なし・ビジネスロジックなし。時刻順序の検証のみ行う。
    ///
    /// # Errors
    /// `updated_at < created_at` の場合 `DomainError::VaultConsistencyError(InvalidUpdatedAt)`
    pub fn rehydrate(
        id: RecordId,
        kind: RecordKind,
        label: RecordLabel,
        payload: RecordPayload,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let created_at = truncate_to_microsecond(created_at);
        let updated_at = truncate_to_microsecond(updated_at);
        if updated_at < created_at {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::InvalidUpdatedAt,
            ));
        }
        Ok(Self {
            id,
            kind,
            label,
            payload,
            created_at,
            updated_at,
        })
    }

    /// レコード ID への参照を返す。
    #[must_use]
    pub fn id(&self) -> &RecordId {
        &self.id
    }

    /// レコード種別を返す。
    #[must_use]
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// ラベルへの参照を返す。
    #[must_use]
    pub fn label(&self) -> &RecordLabel {
        &self.label
    }

    /// ペイロードへの参照を返す。
    #[must_use]
    pub fn payload(&self) -> &RecordPayload {
        &self.payload
    }

    /// 作成時刻を返す（マイクロ秒精度）。
    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// 最終更新時刻を返す（マイクロ秒精度）。
    #[must_use]
    pub fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }

    /// ラベルを更新した新しい `Record` を返す（self を消費）。
    ///
    /// # Errors
    /// `now < self.created_at` の場合 `DomainError::VaultConsistencyError(InvalidUpdatedAt)` を返す。
    pub fn with_updated_label(
        mut self,
        label: RecordLabel,
        now: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let ts = truncate_to_microsecond(now);
        if ts < self.created_at {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::InvalidUpdatedAt,
            ));
        }
        self.label = label;
        self.updated_at = ts;
        Ok(self)
    }

    /// ペイロードを更新した新しい `Record` を返す（self を消費）。
    ///
    /// 内部で `updated_at` をマイクロ秒精度に切り捨てる。
    ///
    /// # Errors
    /// `now < self.created_at` の場合 `DomainError::VaultConsistencyError(InvalidUpdatedAt)` を返す。
    pub fn with_updated_payload(
        mut self,
        payload: RecordPayload,
        now: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let ts = truncate_to_microsecond(now);
        if ts < self.created_at {
            return Err(DomainError::VaultConsistencyError(
                VaultConsistencyReason::InvalidUpdatedAt,
            ));
        }
        self.payload = payload;
        self.updated_at = ts;
        Ok(self)
    }

    /// Text レコードの平文プレビュー（先頭 `max_chars` char）を返す。
    ///
    /// - `RecordKind::Text` かつ `RecordPayload::Plaintext(SecretString)` のとき `Some(先頭 N char)`
    /// - それ以外（`Secret` kind / `Encrypted` variant）は `None`
    ///
    /// `SecretString::expose_secret()` は本関数内で完結する（`shikomi-cli` から呼ばせないための
    /// 集約側 read-only アクセサ。`docs/features/cli-vault-commands/basic-design/security.md
    /// §expose_secret 経路監査` 参照）。grapheme 境界は考慮せず char 単位で切る（CLI プレビュー用途）。
    ///
    /// 境界: `max_chars == 0` は `Some("".to_owned())` を返す。`max_chars` が文字列長を超える
    /// 場合は全文を返す。
    #[must_use]
    pub fn text_preview(&self, max_chars: usize) -> Option<String> {
        match (self.kind, &self.payload) {
            (RecordKind::Text, RecordPayload::Plaintext(secret)) => {
                Some(secret.expose_secret().chars().take(max_chars).collect())
            }
            _ => None,
        }
    }
}
