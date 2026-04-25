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
/// 文面は **「daemon 経由経路（preview）であること」+「既定経路は引き続き
/// SQLite 直結であること」** を必ず併記する（ユーザに「`--ipc` を外せば安定
/// 経路に戻れる」逃げ道を提示する UX 規約）。`--quiet` 指定時を除き必ず stderr
/// に出す。
///
/// 設計根拠: docs/features/daemon-ipc/requirements.md §MSG-CLI-051
#[must_use]
pub fn render_ipc_opt_in_notice(locale: Locale) -> String {
    let mut out = String::from(
        "warning: --ipc routes operations through shikomi-daemon (preview); default path remains direct SQLite\n",
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "警告: --ipc は shikomi-daemon 経由経路（プレビュー）。既定経路は引き続き SQLite 直結です\n",
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
    fn test_render_ipc_opt_in_notice_english_matches_spec_wording() {
        // 設計書 docs/features/daemon-ipc/requirements.md §MSG-CLI-051 と
        // 完全一致する英文行を保持する契約。
        let rendered = render_ipc_opt_in_notice(Locale::English);
        assert!(rendered.contains(
            "warning: --ipc routes operations through shikomi-daemon (preview); default path remains direct SQLite"
        ));
        // 「逃げ道（default path remains direct SQLite）」の文言が落ちていない
        // ことの個別保証——preview 警告の核 UX 規約
        assert!(rendered.contains("default path remains direct SQLite"));
        // 英ロケールでは日本語行を出さない
        assert!(!rendered.contains("警告"));
    }

    #[test]
    fn test_render_ipc_opt_in_notice_japanese_en_contains_both_spec_wordings() {
        let rendered = render_ipc_opt_in_notice(Locale::JapaneseEn);
        // 英文行（先頭に出る）
        assert!(rendered.contains("default path remains direct SQLite"));
        // 日本語行（既定経路の逃げ道明示）
        assert!(rendered.contains(
            "警告: --ipc は shikomi-daemon 経由経路（プレビュー）。既定経路は引き続き SQLite 直結です"
        ));
        assert!(rendered.contains("既定経路は引き続き SQLite 直結"));
    }
}
