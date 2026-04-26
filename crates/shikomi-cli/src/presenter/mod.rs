//! 出力整形層（pure function、副作用なし）。
//!
//! `Locale` を明示引数で受け取る Dependency Injection 形式。`std::env::var("LANG")` を
//! presenter 内部で呼ばない（テスト再現性維持、`run()` 起動時に 1 度だけ `detect_from_env`）。

pub mod cache_relocked_warning;
pub mod error;
pub mod list;
pub mod mode_banner;
pub mod prompt;
pub mod success;
pub mod warning;

// -------------------------------------------------------------------
// Locale
// -------------------------------------------------------------------

/// i18n ロケール。`LANG` 環境変数の先頭 2 文字を大文字小文字無視で判定する。
///
/// 設計根拠: docs/features/cli-vault-commands/detailed-design/data-structures.md
/// §`Locale` 検出ルール
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    /// 英語のみ（`LANG=C` / `en_*` / 未設定 / 空）
    English,
    /// 英語 + 日本語併記（`LANG=ja_*` / `ja` / `JA_*`）
    JapaneseEn,
}

impl Locale {
    /// `LANG` 環境変数から Locale を決定する（副作用あり、`run()` 起動時に 1 度だけ呼ぶ）。
    #[must_use]
    pub fn detect_from_env() -> Self {
        let raw = std::env::var("LANG").ok();
        Self::detect_from_lang_env_value(raw.as_deref())
    }

    /// pure 版：`LANG` の値（`Option<&str>`）から Locale を決定する。
    #[must_use]
    pub fn detect_from_lang_env_value(value: Option<&str>) -> Self {
        match value {
            Some(s) if s.len() >= 2 && s[..2].eq_ignore_ascii_case("ja") => Self::JapaneseEn,
            _ => Self::English,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_from_lang_env_value_ja_jp_utf8_maps_to_japanese_en() {
        assert_eq!(
            Locale::detect_from_lang_env_value(Some("ja_JP.UTF-8")),
            Locale::JapaneseEn
        );
    }

    #[test]
    fn test_detect_from_lang_env_value_ja_short_maps_to_japanese_en() {
        assert_eq!(
            Locale::detect_from_lang_env_value(Some("ja")),
            Locale::JapaneseEn
        );
    }

    #[test]
    fn test_detect_from_lang_env_value_upper_ja_maps_to_japanese_en() {
        assert_eq!(
            Locale::detect_from_lang_env_value(Some("JA_JP")),
            Locale::JapaneseEn
        );
    }

    #[test]
    fn test_detect_from_lang_env_value_en_us_maps_to_english() {
        assert_eq!(
            Locale::detect_from_lang_env_value(Some("en_US.UTF-8")),
            Locale::English
        );
    }

    #[test]
    fn test_detect_from_lang_env_value_c_locale_maps_to_english() {
        assert_eq!(
            Locale::detect_from_lang_env_value(Some("C")),
            Locale::English
        );
    }

    #[test]
    fn test_detect_from_lang_env_value_none_maps_to_english() {
        assert_eq!(Locale::detect_from_lang_env_value(None), Locale::English);
    }

    #[test]
    fn test_detect_from_lang_env_value_empty_maps_to_english() {
        assert_eq!(
            Locale::detect_from_lang_env_value(Some("")),
            Locale::English
        );
    }
}
