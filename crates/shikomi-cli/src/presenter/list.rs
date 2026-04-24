//! `list` の表出力整形。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/public-api.md
//! §`shikomi_cli::presenter::list`
//!
//! - ID カラムは UUIDv7 全長 36 文字（トランケートしない、
//!   basic-design/index.md §UX設計 の案 A 採用）
//! - label / value は 40 char で切り詰め。超過時に `…` を付与

use super::Locale;
use crate::view::{RecordView, ValueView, LIST_VALUE_PREVIEW_MAX};

const ID_WIDTH: usize = 36;
const KIND_WIDTH: usize = 6;
const LABEL_WIDTH: usize = 40;
const TRUNCATION_SUFFIX: &str = "…";
const MASKED_STR: &str = "****";

/// `list` 結果を表形式に整形する。
#[must_use]
pub fn render_list(views: &[RecordView], locale: Locale) -> String {
    if views.is_empty() {
        return render_empty(locale);
    }

    let mut out = String::new();
    // ヘッダ
    out.push_str(&format!(
        "{:<id$}  {:<kind$}  {:<label$}  VALUE\n",
        "ID",
        "KIND",
        "LABEL",
        id = ID_WIDTH,
        kind = KIND_WIDTH,
        label = LABEL_WIDTH,
    ));
    out.push_str(&format!(
        "{:<id$}  {:<kind$}  {:<label$}  -----\n",
        "--",
        "----",
        "-----",
        id = ID_WIDTH,
        kind = KIND_WIDTH,
        label = LABEL_WIDTH,
    ));

    for view in views {
        let kind = kind_str(view);
        let label = truncate_chars(view.label.as_str(), LABEL_WIDTH);
        let value = match &view.value {
            ValueView::Masked => MASKED_STR.to_owned(),
            ValueView::Plain(s) => truncate_chars(s, LIST_VALUE_PREVIEW_MAX),
        };
        out.push_str(&format!(
            "{:<id$}  {:<kind$}  {:<label$}  {}\n",
            view.id,
            kind,
            label,
            value,
            id = ID_WIDTH,
            kind = KIND_WIDTH,
            label = LABEL_WIDTH,
        ));
    }
    out
}

/// 空 vault の出力。
#[must_use]
pub fn render_empty(locale: Locale) -> String {
    let mut out = String::from("no records\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("レコードはありません\n");
    }
    out
}

fn kind_str(view: &RecordView) -> &'static str {
    match view.kind {
        shikomi_core::RecordKind::Text => "text",
        shikomi_core::RecordKind::Secret => "secret",
    }
}

/// char 単位で `max` まで切り詰める。超過した場合は末尾に `…` を付ける（そのぶん 1 char 減らす）。
fn truncate_chars(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        return s.to_owned();
    }
    if max == 0 {
        return String::new();
    }
    let taken: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{taken}{TRUNCATION_SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString};
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn make_record(kind: RecordKind, label: &str, value: &str) -> Record {
        Record::new(
            RecordId::new(Uuid::now_v7()).unwrap(),
            kind,
            RecordLabel::try_new(label.to_owned()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string(value.to_owned())),
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn test_render_empty_english() {
        assert_eq!(render_empty(Locale::English), "no records\n");
    }

    #[test]
    fn test_render_list_empty_falls_back_to_render_empty() {
        assert_eq!(render_list(&[], Locale::English), "no records\n");
    }

    #[test]
    fn test_render_list_single_secret_value_is_masked() {
        let rec = make_record(RecordKind::Secret, "pw", "SUPER-SECRET-VALUE");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(&[view], Locale::English);
        assert!(rendered.contains("secret"));
        assert!(rendered.contains("****"));
        assert!(!rendered.contains("SUPER-SECRET-VALUE"));
    }

    #[test]
    fn test_render_list_text_value_rendered_verbatim_when_short() {
        let rec = make_record(RecordKind::Text, "url", "https://x");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(&[view], Locale::English);
        assert!(rendered.contains("https://x"));
    }

    #[test]
    fn test_render_list_id_column_width_is_full_uuid_length() {
        let rec = make_record(RecordKind::Text, "l", "v");
        let uuid_len = rec.id().to_string().chars().count();
        assert_eq!(uuid_len, 36, "UUIDv7 display should be 36 chars");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(&[view], Locale::English);
        // 出力にそのまま 36 文字 UUID が含まれ、トランケートされていないこと
        assert!(rendered.contains(&rec.id().to_string()));
    }

    #[test]
    fn test_truncate_chars_under_limit_returns_original() {
        assert_eq!(truncate_chars("abc", 10), "abc");
    }

    #[test]
    fn test_truncate_chars_over_limit_appends_ellipsis() {
        let out = truncate_chars("abcdefghij", 5);
        assert_eq!(out.chars().count(), 5);
        assert!(out.ends_with(TRUNCATION_SUFFIX));
    }

    #[test]
    fn test_render_list_label_exceeding_40_chars_is_truncated_with_ellipsis() {
        let long_label = "a".repeat(60);
        let rec = make_record(RecordKind::Text, &long_label, "v");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(&[view], Locale::English);
        assert!(rendered.contains(TRUNCATION_SUFFIX));
    }
}
