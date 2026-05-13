//! Presenter への出力 DTO。
//!
//! Record から射影する表示用ビュー。Secret kind は `ValueView::Masked` に変換し、
//! Text kind は `shikomi_core::Record::text_preview` 経由で平文プレビュー文字列を取り出す。
//! `shikomi-cli/src/` 内で secret 値を直接取り出す呼び出しを行わない契約を満たす
//! ための委譲（CI grep 対象：シークレット経路監査）。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/data-structures.md
//! §`ValueView` の構築ルール、basic-design/security.md §シークレット経路監査

use shikomi_core::{Record, RecordId, RecordKind, RecordLabel};

/// `list` プレビューの最大文字数（char 単位）。
pub const LIST_VALUE_PREVIEW_MAX: usize = 40;

// -------------------------------------------------------------------
// ValueView
// -------------------------------------------------------------------

/// レコード値の表示用ビュー。Secret は常にマスク、Text はプレビュー文字列を保持。
#[derive(Debug, Clone)]
pub enum ValueView {
    /// Text レコードの平文プレビュー（先頭 `LIST_VALUE_PREVIEW_MAX` char）
    Plain(String),
    /// Secret レコード。マスク表示（`****`）に置き換えられる。
    Masked,
}

// -------------------------------------------------------------------
// RecordView
// -------------------------------------------------------------------

/// `list` 表示用の Record 射影。
#[derive(Debug, Clone)]
pub struct RecordView {
    pub id: RecordId,
    pub kind: RecordKind,
    pub label: RecordLabel,
    pub value: ValueView,
}

impl RecordView {
    /// ドメイン Record から表示用ビューを構築する。
    ///
    /// - Secret kind → `ValueView::Masked`
    /// - Text kind → `Record::text_preview(LIST_VALUE_PREVIEW_MAX)` を呼び、`Some(s)` なら `Plain(s)`、
    ///   `None`（想定外、暗号化等）なら `Masked` フォールバック（防御的）
    ///
    /// 本メソッド内で secret 値を直接取り出す呼び出しを行わない（`shikomi_core::Record::text_preview`
    /// に委譲し、露出経路を core 内部に封じる。
    /// `docs/features/cli-vault-commands/basic-design/security.md §シークレット経路監査` 参照）。
    #[must_use]
    pub fn from_record(record: &Record) -> Self {
        let value = match record.kind() {
            RecordKind::Secret => ValueView::Masked,
            RecordKind::Text => record
                .text_preview(LIST_VALUE_PREVIEW_MAX)
                .map_or(ValueView::Masked, ValueView::Plain),
        };
        Self {
            id: record.id().clone(),
            kind: record.kind(),
            label: record.label().clone(),
            value,
        }
    }
}

// -------------------------------------------------------------------
// テスト
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{RecordLabel, RecordPayload, SecretString};
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn make_id() -> RecordId {
        RecordId::new(Uuid::now_v7()).unwrap()
    }

    #[test]
    fn test_record_view_from_record_secret_kind_yields_masked() {
        let record = Record::new(
            make_id(),
            RecordKind::Secret,
            RecordLabel::try_new("pw".to_owned()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("super-secret".to_owned())),
            OffsetDateTime::UNIX_EPOCH,
        );
        let view = RecordView::from_record(&record);
        assert!(matches!(view.value, ValueView::Masked));
    }

    #[test]
    fn test_record_view_from_record_text_kind_yields_plain() {
        let record = Record::new(
            make_id(),
            RecordKind::Text,
            RecordLabel::try_new("url".to_owned()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("https://example.com".to_owned())),
            OffsetDateTime::UNIX_EPOCH,
        );
        let view = RecordView::from_record(&record);
        match view.value {
            ValueView::Plain(s) => assert_eq!(s, "https://example.com"),
            ValueView::Masked => panic!("expected Plain, got Masked"),
        }
    }

    #[test]
    fn test_record_view_from_record_text_long_value_truncates_to_40_chars() {
        let long = "a".repeat(100);
        let record = Record::new(
            make_id(),
            RecordKind::Text,
            RecordLabel::try_new("l".to_owned()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string(long)),
            OffsetDateTime::UNIX_EPOCH,
        );
        let view = RecordView::from_record(&record);
        match view.value {
            ValueView::Plain(s) => assert_eq!(s.chars().count(), LIST_VALUE_PREVIEW_MAX),
            ValueView::Masked => panic!("expected Plain"),
        }
    }
}
