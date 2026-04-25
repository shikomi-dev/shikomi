//! 警告メッセージ（stderr）。MSG-CLI-050 / MSG-CLI-051。

use super::Locale;

/// `--value` 経由の secret 入力が shell 履歴に残る可能性を警告する（MSG-CLI-050）。
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

/// `--ipc` opt-in 起動時に MSG-CLI-051 を返す。
///
/// `--ipc` は preview 機能で、Phase 1.5 で list/add/edit/remove の 4 操作を支える。
/// 現段階では既定の SQLite 直接アクセス経路と挙動が異なる可能性があるため、
/// `--quiet` 指定時を除き必ず stderr に注意を出す。
///
/// 設計根拠: docs/features/cli-vault-commands/basic-design/error.md §MSG-CLI-051
#[must_use]
pub fn render_ipc_opt_in_notice(locale: Locale) -> String {
    let mut out = String::from(
        "warning: --ipc is an opt-in preview routing all vault operations through shikomi-daemon\n",
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "警告: --ipc はプレビュー機能で、全ての vault 操作を shikomi-daemon 経由で行います\n",
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

    #[test]
    fn test_render_ipc_opt_in_notice_english_only_does_not_contain_japanese() {
        let rendered = render_ipc_opt_in_notice(Locale::English);
        assert!(rendered.contains("--ipc"));
        assert!(rendered.contains("preview"));
        assert!(!rendered.contains("警告"));
    }

    #[test]
    fn test_render_ipc_opt_in_notice_japanese_en_contains_both() {
        let rendered = render_ipc_opt_in_notice(Locale::JapaneseEn);
        assert!(rendered.contains("--ipc"));
        assert!(rendered.contains("preview"));
        assert!(rendered.contains("警告"));
    }
}
