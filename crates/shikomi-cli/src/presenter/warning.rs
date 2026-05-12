//! 警告・通知メッセージ（stderr）。MSG-CLI-050 / MSG-CLI-052。

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

/// `--no-ipc vault *` 実行時に vault サブコマンドが IPC 強制されることを通知する（MSG-CLI-052）。
///
/// `--no-ipc` フラグを指定しても vault サブコマンドは IPC 経路で動作することをユーザに通知する。
/// `run_vault` 呼び出し前に出力し、daemon 未起動の場合でも note が表示されるようにする。
///
/// 設計根拠: docs/features/daemon-default-mode/cli/detailed-design.md §MSG-CLI-052
#[must_use]
pub fn render_vault_ipc_forced_note(locale: Locale) -> String {
    let mut out = String::from("note: vault commands always use IPC; --no-ipc does not apply\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("注: vault サブコマンドは常に IPC 経由です。--no-ipc は適用されません\n");
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
    fn test_render_vault_ipc_forced_note_english_matches_spec_wording() {
        // 設計書 docs/features/daemon-default-mode/cli/detailed-design.md §MSG-CLI-052 と
        // 完全一致する英文行を保持する契約。
        let rendered = render_vault_ipc_forced_note(Locale::English);
        assert!(
            rendered.contains("note: vault commands always use IPC; --no-ipc does not apply"),
            "MSG-CLI-052 英文が仕様と一致すること: {rendered:?}"
        );
        // 英ロケールでは日本語行を出さない
        assert!(!rendered.contains("注:"));
    }

    #[test]
    fn test_render_vault_ipc_forced_note_japanese_en_contains_both_lines() {
        let rendered = render_vault_ipc_forced_note(Locale::JapaneseEn);
        // 英文行（先頭に出る）
        assert!(
            rendered.contains("note: vault commands always use IPC; --no-ipc does not apply"),
            "英文行が含まれること: {rendered:?}"
        );
        // 日本語行
        assert!(
            rendered
                .contains("注: vault サブコマンドは常に IPC 経由です。--no-ipc は適用されません"),
            "日本語行が含まれること: {rendered:?}"
        );
    }
}
