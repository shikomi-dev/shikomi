//! 警告メッセージ（stderr）。MSG-CLI-050。

use super::Locale;

/// `--value` 経由の secret 入力が shell 履歴に残る可能性を警告する。
#[must_use]
pub fn render_shell_history_warning(locale: Locale) -> String {
    let mut out = String::from(
        "warning: '--value' for a secret leaks into shell history; prefer '--stdin'\n",
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "警告: secret を --value で渡すと shell 履歴に残ります。--stdin を推奨します\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_shell_history_warning_english_contains_stdin_hint() {
        let rendered = render_shell_history_warning(Locale::English);
        assert!(rendered.contains("--stdin"));
        assert!(!rendered.contains("警告"));
    }

    #[test]
    fn test_render_shell_history_warning_japanese_en_contains_both() {
        let rendered = render_shell_history_warning(Locale::JapaneseEn);
        assert!(rendered.contains("--stdin"));
        assert!(rendered.contains("警告"));
    }
}
