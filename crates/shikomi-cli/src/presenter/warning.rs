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

    /// TC-UT-156 (REQ-DDM-003 / AC-DDM-05): MSG-CLI-051 文言・`render_ipc_opt_in_notice` 関数が
    /// ソースコードに存在しないことを `include_str!` で静的検査する。
    ///
    /// NOTE: forbidden 文字列は連結形式で定義し、テスト自身がフォールスポジティブを引かないようにする。
    #[test]
    fn tc_ut_156_render_ipc_opt_in_notice_does_not_exist_in_source() {
        // src から tests ブロックを除いた非テスト部分だけを確認する。
        // `include_str!` は test 関数自身も含むため、assert 文字列リテラルに forbidden 単語を
        // 直接置かず、連結で組み立てる（フォールスポジティブ回避）。
        let src = include_str!("warning.rs");

        // 非テストコード行（// または /// コメント以外、cfg(test) ブロック外）を抽出する。
        // 単純化: cfg(test) ブロック前の行のみを対象とする。
        let non_test_src: String = {
            let mut in_test = false;
            src.lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("#[cfg(test)]") {
                        in_test = true;
                    }
                    !in_test
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        // forbidden: "render_ipc_opt_in_notice" 関数定義
        let forbidden_fn = ["render_ipc_opt_in", "notice"].concat();
        assert!(
            !non_test_src.contains(&forbidden_fn),
            "{forbidden_fn} must be deleted (REQ-DDM-003 / MSG-CLI-051 廃止)"
        );

        // forbidden: "MSG-CLI-051"
        let forbidden_msg = ["MSG-CLI-0", "51"].concat();
        assert!(
            !non_test_src.contains(&forbidden_msg),
            "{forbidden_msg} must not appear in warning.rs (廃止)"
        );

        // forbidden: "IPC mode"
        let forbidden_ipc_mode = ["IPC ", "mode"].concat();
        assert!(
            !non_test_src.contains(&forbidden_ipc_mode),
            "Phase 1 '{forbidden_ipc_mode}' string must not appear (MSG-CLI-051 廃止)"
        );

        // forbidden: "opt-in"
        let forbidden_opt_in = ["opt", "-in"].concat();
        assert!(
            !non_test_src.contains(&forbidden_opt_in),
            "Phase 1 '{forbidden_opt_in}' string must not appear in warning.rs (MSG-CLI-051 廃止)"
        );
    }

    /// TC-UT-157 (REQ-DDM-003 / AC-DDM-05): `render_shell_history_warning` と
    /// `render_vault_ipc_forced_note` のいずれも MSG-CLI-051 文言を出力しない。
    #[test]
    fn tc_ut_157_no_warning_function_outputs_msg_cli_051_wording() {
        for locale in [Locale::English, Locale::JapaneseEn] {
            let history_warn = render_shell_history_warning(locale);
            for forbidden in ["IPC mode", "--ipc", "opt-in", "MSG-CLI-051"] {
                assert!(
                    !history_warn.contains(forbidden),
                    "render_shell_history_warning({locale:?}) must not contain '{forbidden}'"
                );
            }
            let vault_note = render_vault_ipc_forced_note(locale);
            // MSG-CLI-052 text は "--ipc" を含まない（"--no-ipc" は含む）
            assert!(
                !vault_note.contains("IPC mode"),
                "render_vault_ipc_forced_note({locale:?}) must not contain 'IPC mode'"
            );
            assert!(
                !vault_note.contains("opt-in"),
                "render_vault_ipc_forced_note({locale:?}) must not contain 'opt-in'"
            );
            assert!(
                !vault_note.contains("MSG-CLI-051"),
                "render_vault_ipc_forced_note({locale:?}) must not contain 'MSG-CLI-051'"
            );
        }
    }
}
