//! 対話プロンプト文言。`remove` の確認プロンプトに使う。

use shikomi_core::RecordId;

use super::Locale;

/// `remove` の確認プロンプト文字列を返す（改行は含まない。readline 側で stdin からの入力を継承）。
#[must_use]
pub fn render_remove_prompt(id: &RecordId, label: Option<&str>, locale: Locale) -> String {
    let label_display = label.unwrap_or("<unknown>");
    match locale {
        Locale::English => format!("Delete record {id} ({label_display})? [y/N]: "),
        Locale::JapaneseEn => {
            format!("レコード {id} ({label_display}) を削除しますか? [y/N]: ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_render_remove_prompt_english_contains_delete_record() {
        let id = RecordId::new(Uuid::now_v7()).unwrap();
        let prompt = render_remove_prompt(&id, Some("label"), Locale::English);
        assert!(prompt.starts_with("Delete record "));
        assert!(prompt.contains("(label)"));
    }

    #[test]
    fn test_render_remove_prompt_japanese_en_contains_japanese_text() {
        let id = RecordId::new(Uuid::now_v7()).unwrap();
        let prompt = render_remove_prompt(&id, Some("lbl"), Locale::JapaneseEn);
        assert!(prompt.contains("削除しますか"));
    }
}
