//! `list` の表出力整形。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/public-api.md
//! §`shikomi_cli::presenter::list`
//! + docs/features/vault-encryption/detailed-design/cli-subcommands.md §C-37
//!
//! - ID カラムは UUIDv7 全長 36 文字（トランケートしない、
//!   basic-design/index.md §UX設計 の案 A 採用）
//! - label / value は 40 char で切り詰め。超過時に `…` を付与
//! - **Sub-F (#44) Phase 3 / C-37**: 先頭行に保護モードバナー (`[plaintext]` /
//!   `[encrypted, locked]` / `[encrypted, unlocked]` / `[unknown]`) を必ず描画する。
//!   `render_list` シグネチャに `protection_mode: ProtectionModeBanner` を**必須引数**
//!   として持たせ、Default 値や `Option` を許さない設計で型レベルに強制する。

use shikomi_core::ipc::ProtectionModeBanner;

use super::{mode_banner, Locale};
use crate::view::{RecordView, ValueView, LIST_VALUE_PREVIEW_MAX};

const ID_WIDTH: usize = 36;
const KIND_WIDTH: usize = 6;
const LABEL_WIDTH: usize = 40;
const TRUNCATION_SUFFIX: &str = "…";
const MASKED_STR: &str = "****";

/// `list` 結果を表形式に整形する。
///
/// `protection_mode` は **Sub-F (#44) C-37 で必須引数化** された。`Option` や
/// `Default::default()` を持たせず、呼出側 (`lib::run_list`) は Sqlite/IPC 経路の
/// 双方で `ProtectionModeBanner` を**必ず**算出して渡す責務を持つ (型レベル強制)。
/// `color_enabled` は `NO_COLOR` 環境変数 / 非 TTY / `--quiet` 判定済みの値を渡す
/// (presenter は env を読まない、テスト再現性維持)。
#[must_use]
pub fn render_list(
    views: &[RecordView],
    protection_mode: ProtectionModeBanner,
    color_enabled: bool,
    locale: Locale,
) -> String {
    let banner = mode_banner::display(protection_mode, color_enabled);
    if views.is_empty() {
        // 空 vault でもバナーは先頭に出す (REQ-S16: 保護状態は常に明示)。
        return format!("{banner}{}", render_empty(locale));
    }

    let mut out = String::from(&banner);
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

/// 空 vault の出力（バナー無し、`render_list` から呼ばれる時はバナーが prefix される）。
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
    fn test_render_list_empty_falls_back_to_render_empty_with_banner() {
        let out = render_list(&[], ProtectionModeBanner::Plaintext, false, Locale::English);
        assert!(
            out.starts_with("[plaintext]\n"),
            "banner must be the first line"
        );
        assert!(
            out.ends_with("no records\n"),
            "empty body must follow the banner"
        );
    }

    #[test]
    fn test_render_list_single_secret_value_is_masked() {
        let rec = make_record(RecordKind::Secret, "pw", "SUPER-SECRET-VALUE");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(
            &[view],
            ProtectionModeBanner::Plaintext,
            false,
            Locale::English,
        );
        assert!(rendered.contains("secret"));
        assert!(rendered.contains("****"));
        assert!(!rendered.contains("SUPER-SECRET-VALUE"));
    }

    #[test]
    fn test_render_list_text_value_rendered_verbatim_when_short() {
        let rec = make_record(RecordKind::Text, "url", "https://x");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(
            &[view],
            ProtectionModeBanner::Plaintext,
            false,
            Locale::English,
        );
        assert!(rendered.contains("https://x"));
    }

    #[test]
    fn test_render_list_id_column_width_is_full_uuid_length() {
        let rec = make_record(RecordKind::Text, "l", "v");
        let uuid_len = rec.id().to_string().chars().count();
        assert_eq!(uuid_len, 36, "UUIDv7 display should be 36 chars");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(
            &[view],
            ProtectionModeBanner::Plaintext,
            false,
            Locale::English,
        );
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
        let rendered = render_list(
            &[view],
            ProtectionModeBanner::Plaintext,
            false,
            Locale::English,
        );
        assert!(rendered.contains(TRUNCATION_SUFFIX));
    }

    /// Sub-F (#44) Phase 3 / C-37: バナーは必ず先頭行に出力される (REQ-S16)。
    #[test]
    fn test_render_list_encrypted_locked_banner_is_first_line() {
        let rec = make_record(RecordKind::Text, "l", "v");
        let view = RecordView::from_record(&rec);
        let rendered = render_list(
            &[view],
            ProtectionModeBanner::EncryptedLocked,
            false,
            Locale::English,
        );
        assert!(
            rendered.starts_with("[encrypted, locked]\n"),
            "banner must be first line, got: {rendered:?}"
        );
    }

    /// 4 variants 全てでバナーが先頭に出ること (REQ-S16 / C-37)。
    #[test]
    fn test_render_list_banner_emitted_for_all_variants() {
        let rec = make_record(RecordKind::Text, "l", "v");
        let view = RecordView::from_record(&rec);
        for (mode, expected_label) in [
            (ProtectionModeBanner::Plaintext, "[plaintext]"),
            (ProtectionModeBanner::EncryptedLocked, "[encrypted, locked]"),
            (
                ProtectionModeBanner::EncryptedUnlocked,
                "[encrypted, unlocked]",
            ),
            (ProtectionModeBanner::Unknown, "[unknown]"),
        ] {
            let rendered = render_list(std::slice::from_ref(&view), mode, false, Locale::English);
            assert!(
                rendered.starts_with(&format!("{expected_label}\n")),
                "banner mismatch for {mode:?}, got: {rendered:?}"
            );
        }
    }
}
