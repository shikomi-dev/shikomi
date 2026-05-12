//! 成功時の stdout メッセージ整形。
//!
//! MSG-CLI-001〜005 + Sub-F (#44) MSG-S01〜S07 / S19 / S20 経路。pure function、
//! `String` を返すのみ。

use std::path::Path;

use shikomi_core::ipc::SerializableSecretBytes;
use shikomi_core::RecordId;

use super::Locale;

/// `added: {id}` / `追加しました: {id}` を改行付きで返す。
#[must_use]
pub fn render_added(id: &RecordId, locale: Locale) -> String {
    let mut out = format!("added: {id}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("追加しました: {id}\n"));
    }
    out
}

/// `updated: {id}` / `更新しました: {id}` を返す。
#[must_use]
pub fn render_updated(id: &RecordId, locale: Locale) -> String {
    let mut out = format!("updated: {id}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("更新しました: {id}\n"));
    }
    out
}

/// `removed: {id}` / `削除しました: {id}` を返す。
#[must_use]
pub fn render_removed(id: &RecordId, locale: Locale) -> String {
    let mut out = format!("removed: {id}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("削除しました: {id}\n"));
    }
    out
}

/// `cancelled` / `キャンセルしました` を返す。
#[must_use]
pub fn render_cancelled(locale: Locale) -> String {
    let mut out = String::from("cancelled\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("キャンセルしました\n");
    }
    out
}

/// `initialized plaintext vault at {path}` / `平文 vault を {path} に初期化しました` を返す。
#[must_use]
pub fn render_initialized_vault(path: &Path, locale: Locale) -> String {
    let path_str = path.display();
    let mut out = format!("initialized plaintext vault at {path_str}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("平文 vault を {path_str} に初期化しました\n"));
    }
    out
}

// -------------------------------------------------------------------
// Sub-F (#44) Phase 2: vault サブコマンド成功文言（MSG-S01〜S07 / S19 / S20）
//
// Phase 2 では文言を最小限にハードコードし、英語 + 日本語併記の従来方式を継承する。
// 完全な i18n 辞書 (`messages.toml` / `Localizer`) への移行は Phase 6 / Phase 7 で
// `shikomi_cli::i18n` モジュール導入時に集約する（cli-subcommands.md §i18n 戦略）。
// -------------------------------------------------------------------

/// `vault unlock` 成功文言（MSG-S03）。
#[must_use]
pub fn render_unlocked(locale: Locale) -> String {
    let mut out = String::from("vault unlocked\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("vault のロックを解除しました\n");
    }
    out
}

/// `vault lock` 成功文言（MSG-S04）。
#[must_use]
pub fn render_locked(locale: Locale) -> String {
    let mut out = String::from("vault locked (VEK zeroized)\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("vault をロックしました（鍵情報は消去済）\n");
    }
    out
}

/// `vault change-password` 成功文言（MSG-S05）。
#[must_use]
pub fn render_password_changed(locale: Locale) -> String {
    let mut out = String::from("master password changed\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("マスターパスワードを変更しました\n");
    }
    out
}

/// `vault decrypt` 成功文言（MSG-S02）。
#[must_use]
pub fn render_decrypted(locale: Locale) -> String {
    let mut out = String::from("vault decrypted (back to plaintext)\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("vault を平文に戻しました\n");
    }
    out
}

/// 24 語を Screen 経路で render する（C-19 zeroize 連鎖は呼出側責務）。
///
/// 設計書 MSG-S06: 「以下の 24 語は復旧用です。安全に保管してください。」
#[must_use]
pub fn render_recovery_disclosure_screen(
    disclosure: &[SerializableSecretBytes],
    locale: Locale,
) -> String {
    let mut out = String::new();
    out.push_str("recovery words (write down and store safely; shown only once):\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("以下の 24 語は復旧用です。安全に保管してください（再表示されません）:\n");
    }
    push_word_lines(&mut out, disclosure);
    out.push_str("\nencrypted vault initialized\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("vault を暗号化しました\n");
    }
    out
}

/// `vault encrypt` (24 語表示経路) + `cache_relocked == false` 連結警告 (MSG-S20)。
///
/// Issue #75 Bug-F-002 §経路復活: `cli-subcommands.md` §Bug-F-002 解消の SSoT に従い、
/// 「**経路復活**（削除ではなく `cache_relocked == false` 経路に正式接続）」契約を満たす。
/// 旧 Phase 5 stub の「is not yet wired」文言は完全除去し、`cache_relocked_warning::render_to`
/// に**委譲**することで MSG-S20 文言の SSoT を 1 箇所に保つ (DRY、Tell-Don't-Ask: 値自身が
/// fallback 文言を知る presenter 層責務 C-31/C-36)。
#[must_use]
pub fn render_recovery_disclosure_screen_with_fallback_notice(
    disclosure: &[SerializableSecretBytes],
    locale: Locale,
) -> String {
    let mut out = render_recovery_disclosure_screen(disclosure, locale);
    super::cache_relocked_warning::render_to(&mut out, locale);
    out
}

/// `vault rekey` 成功文言（MSG-S07 + 24 語表示）。
///
/// Phase 4: 本関数は MSG-S07 + 24 語表示のみに責務を縮小した。`cache_relocked == false`
/// 時の MSG-S20 連結警告は `presenter::cache_relocked_warning::display` 経由で
/// `usecase::vault::rekey` が追加出力する責務を持つ（C-32 整合、関心事分離）。
#[must_use]
pub fn render_rekeyed(
    records_count: usize,
    words: &[SerializableSecretBytes],
    locale: Locale,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("rekeyed {records_count} records\n"));
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("{records_count} 件のレコードを再暗号化しました\n"));
    }
    out.push_str("new recovery words (shown only once):\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("新しい 24 語（再表示されません）:\n");
    }
    push_word_lines(&mut out, words);
    out
}

/// `vault rekey` 成功文言 + `cache_relocked == false` 連結警告 (MSG-S07 + MSG-S20)。
///
/// Issue #75 Bug-F-002 §経路復活: `cli-subcommands.md` §Bug-F-002 解消の SSoT 通り、
/// `usecase::vault::rekey` から `IpcResponse::Rekeyed { cache_relocked: false }` を受領した
/// 際の正式 presenter 経路。`render_rekeyed` + `cache_relocked_warning::render_to` への
/// 委譲構造で C-32 整合 + 単一 SSoT を両立 (DRY、警告文言は `cache_relocked_warning` のみ保有)。
#[must_use]
pub fn render_rekeyed_with_fallback_notice(
    records_count: usize,
    words: &[SerializableSecretBytes],
    locale: Locale,
) -> String {
    let mut out = render_rekeyed(records_count, words, locale);
    super::cache_relocked_warning::render_to(&mut out, locale);
    out
}

/// `vault rotate-recovery` 成功文言（MSG-S19 + 24 語表示）。
///
/// Phase 4: `cache_relocked == false` 連結は usecase 側責務に移譲した
/// （`render_rekeyed` と同じ理由、cli-subcommands.md §設計判断 step 4）。
#[must_use]
pub fn render_recovery_rotated(words: &[SerializableSecretBytes], locale: Locale) -> String {
    let mut out = String::from("recovery words rotated\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("リカバリ用 24 語をローテーションしました\n");
    }
    out.push_str("new recovery words (shown only once):\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("新しい 24 語（再表示されません）:\n");
    }
    push_word_lines(&mut out, words);
    out
}

/// `vault rotate-recovery` 成功文言 + `cache_relocked == false` 連結警告
/// (MSG-S19 + MSG-S20)。
///
/// Issue #75 Bug-F-002 §経路復活: `cli-subcommands.md` §Bug-F-002 解消の SSoT 通り、
/// `IpcResponse::RecoveryRotated { cache_relocked: false }` 受領時の正式 presenter 経路。
/// `render_recovery_rotated` + `cache_relocked_warning::render_to` への委譲で
/// 単一 SSoT を維持。
#[must_use]
pub fn render_recovery_rotated_with_fallback_notice(
    words: &[SerializableSecretBytes],
    locale: Locale,
) -> String {
    let mut out = render_recovery_rotated(words, locale);
    super::cache_relocked_warning::render_to(&mut out, locale);
    out
}

/// 24 語を 1 語 1 行で push する（番号 1〜n、UTF-8 lossy が secret_bytes 側 helper で適用済）。
fn push_word_lines(out: &mut String, words: &[SerializableSecretBytes]) {
    for (i, w) in words.iter().enumerate() {
        let s = w.to_lossy_string_for_handler();
        out.push_str(&format!("  {:>2}. {}\n", i + 1, s));
    }
}

// `fallback_notice` private fn は Issue #75 Bug-F-002 §経路復活で `cache_relocked_warning::render_to`
// への委譲構造に統合済（同モジュール 1 箇所が MSG-S20 文言の SSoT、DRY を維持しつつ
// `*_with_fallback_notice` 公開 API を C-31/C-36 articulate に整合）。

// -------------------------------------------------------------------
// Sub-B (#127): autostart 成功メッセージ（ペガサス指摘②対応）
//
// 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/presenter.md
// §render_autostart_installed / §render_autostart_uninstalled
// -------------------------------------------------------------------

/// `shikomi daemon install` 成功文言。
///
/// `quiet == true` の場合は呼出側（`run_daemon_subcommand`）で呼ばない（presenter 層は quiet 非関与）。
#[must_use]
pub fn render_autostart_installed(locale: Locale) -> String {
    let mut out = String::from("shikomi-daemon autostart enabled\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("shikomi-daemon の自動起動を有効にしました\n");
    }
    out
}

/// `shikomi daemon uninstall` 成功文言。
///
/// `quiet == true` の場合は呼出側（`run_daemon_subcommand`）で呼ばない（presenter 層は quiet 非関与）。
#[must_use]
pub fn render_autostart_uninstalled(locale: Locale) -> String {
    let mut out = String::from("shikomi-daemon autostart disabled\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("shikomi-daemon の自動起動を無効にしました\n");
    }
    out
}

// -------------------------------------------------------------------
// Issue #141: data-portability export / import 成功文言（MSG-CLI-140 / MSG-CLI-145）
//
// 設計根拠: docs/features/data-portability/cli/detailed-design/presenter.md
// §render_exported / §render_imported / §render_export_secrets_warning
// -------------------------------------------------------------------

/// `shikomi export` 成功文言（MSG-CLI-140 相当）。
///
/// `quiet == true` の場合は呼出側（`run_export`）で呼ばない（presenter 層は quiet 非関与）。
#[must_use]
pub fn render_exported(record_count: usize, output_path: &std::path::Path, locale: Locale) -> String {
    let path_str = output_path.display();
    let mut out = format!("exported {record_count} record(s) to {path_str}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("{record_count} 件のレコードを {path_str} に export しました\n"));
    }
    out
}

/// `shikomi import` 成功文言。
///
/// `quiet == true` の場合は呼出側（`run_import`）で呼ばない（presenter 層は quiet 非関与）。
#[must_use]
pub fn render_imported(added: usize, skipped: usize, overwritten: usize, locale: Locale) -> String {
    let mut out = format!("imported {added} record(s) (skipped {skipped}, overwritten {overwritten})\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!(
            "{added} 件を追加しました（スキップ: {skipped} 件、上書き: {overwritten} 件）\n"
        ));
    }
    out
}

/// `--export-secrets` 指定時の警告文言（MSG-CLI-145）。
///
/// `--quiet` でも抑止不可。呼出側（`run_export`）は `quiet` フラグを確認せずに
/// 直接 `eprintln!` で stderr へ出力する（設計書 §セキュリティ考慮 参照）。
/// 本関数はロケール別文言の生成責務のみを担う。
#[must_use]
pub fn render_export_secrets_warning(locale: Locale) -> String {
    let mut out = String::from(
        "warning: --export-secrets is set; secret values will be written to the export file in plaintext\n",
    );
    out.push_str(
        "warning: store the export file securely and delete it when no longer needed\n",
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "warning: --export-secrets が指定されています。Secret の値が平文でエクスポートファイルに書き込まれます\n",
        );
        out.push_str(
            "warning: エクスポートファイルを安全に保管し、不要になったら削除してください\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn id() -> RecordId {
        RecordId::new(Uuid::now_v7()).unwrap()
    }

    #[test]
    fn test_render_added_english_single_line() {
        let rendered = render_added(&id(), Locale::English);
        assert!(rendered.starts_with("added: "));
        assert!(!rendered.contains("追加"));
    }

    #[test]
    fn test_render_added_japanese_en_two_lines() {
        let rendered = render_added(&id(), Locale::JapaneseEn);
        assert!(rendered.contains("added: "));
        assert!(rendered.contains("追加しました: "));
    }

    #[test]
    fn test_render_cancelled_english() {
        assert_eq!(render_cancelled(Locale::English), "cancelled\n");
    }

    // ---------------------------------------------------------------
    // Issue #76 (#74-B): TC-F-U04 / TC-F-U12
    // 設計根拠: docs/features/vault-encryption/test-design/sub-f-cli-subcommands/
    //          {index.md §15.5, issue-76-verification.md §15.17.1}
    // ---------------------------------------------------------------

    /// TC-F-U04 (EC-F12): 24 語表示 presenter の **API 不変条件** を機械検証する。
    ///
    /// 設計書 §15.5 #4 は `recovery_disclosure::display(words: Vec<SerializableSecretBytes>,
    /// target: OutputTarget)` で**所有権消費**形 (引数 `Vec` by value) を要求するが、
    /// 現行実装は `presenter::success::render_recovery_disclosure_screen(disclosure:
    /// &[SerializableSecretBytes], locale: Locale) -> String` で**借用形** (Phase 6
    /// `--output {screen,print,braille,audio}` dispatch を `usecase::vault::encrypt::
    /// render_disclosure` に集約済、`accessibility::{braille_brf,print_pdf,audio_tts}::
    /// write_to_stdout(&[SerializableSecretBytes])` も全て借用)。
    ///
    /// **§15.17.2 §A 実装事実への追従**: 設計書の `Vec<...>` 所有権消費形は **Phase 8 以降**
    /// で `recovery_disclosure` モジュール集約時に再検討する設計余地として残し、現実装は
    /// **24 語の所有権を呼出側 (`usecase::vault::encrypt::execute`) が `IpcVaultRepository::
    /// encrypt` の戻り値として保持し、`render_*` 系 presenter は借用のみで参照する**構造を
    /// SSoT とする。
    ///
    /// 本 TC は API 不変条件として:
    /// (a) `render_recovery_disclosure_screen` シグネチャが `&[SerializableSecretBytes]` を
    ///     受領 (関数ポインタ型一致で compile-time 強制)、
    /// (b) `&` 借用渡しなので呼出後も `disclosure` を再利用できる (借用ルール、所有権
    ///     消費しない)、
    /// (c) 呼出側が同じ `disclosure` を `render_recovery_disclosure_screen_with_fallback_notice`
    ///     で**再利用できる** (Phase 6 で encrypt 経路と rekey 経路が共有する SSoT)、
    /// を機械検証する。`Vec<...>` 所有権消費形への移行時はこの TC を `compile_fail` doctest
    /// に差し替える Boy Scout (Phase 8+ PR 時点)。
    ///
    /// 配置先: `crates/shikomi-cli/src/presenter/success.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置 `presenter/recovery_disclosure.rs` を未導入実装事実に追従)。
    #[test]
    fn tc_f_u04_render_recovery_disclosure_screen_signature_borrows_words_slice_for_reuse() {
        use shikomi_core::ipc::SerializableSecretBytes;
        use shikomi_core::SecretString;

        // (a) シグネチャ型一致 (関数ポインタ経由で compile-time に強制)。
        let _: fn(&[SerializableSecretBytes], Locale) -> String = render_recovery_disclosure_screen;
        let _: fn(&[SerializableSecretBytes], Locale) -> String =
            render_recovery_disclosure_screen_with_fallback_notice;

        // (b) 借用渡しなので呼出後も words を再利用できる (所有権消費しない実装事実)。
        let words: Vec<SerializableSecretBytes> = (0..24)
            .map(|i| {
                SerializableSecretBytes::from_secret_string(&SecretString::from_string(format!(
                    "word{i:02}"
                )))
            })
            .collect();
        let _ = render_recovery_disclosure_screen(&words, Locale::English);
        // 呼出後も words.len() は維持される (借用が解放されているため再利用可能)。
        assert_eq!(words.len(), 24, "借用渡しなので呼出後も 24 要素のまま");

        // (c) 同じ words slice を fallback notice 経路でも再利用できる (Phase 6 SSoT 構造)。
        let twice = render_recovery_disclosure_screen_with_fallback_notice(&words, Locale::English);
        assert!(twice.contains("recovery words"));
        assert!(twice.contains("warning:"));
    }

    // -------------------------------------------------------------------
    // Issue #141: data-portability Presenter UT — TC-UT-201〜204
    // 設計根拠: docs/features/data-portability/cli/test-design.md §5.3
    // -------------------------------------------------------------------

    /// TC-UT-201 (REQ-DP-011 / AC-DP-06): `render_exported` English locale に
    /// 件数・パスが含まれる。
    #[test]
    fn tc_ut_201_render_exported_english_contains_record_count_and_path() {
        let rendered = render_exported(3, std::path::Path::new("/tmp/out.json"), Locale::English);
        assert!(
            rendered.contains("exported 3 record(s)"),
            "must contain 'exported 3 record(s)', got: {rendered:?}"
        );
        assert!(
            rendered.contains("/tmp/out.json"),
            "must contain output path '/tmp/out.json', got: {rendered:?}"
        );
    }

    /// TC-UT-202 (REQ-DP-011 / AC-DP-06): `render_exported` JapaneseEn locale に
    /// 日本語文が含まれる。
    #[test]
    fn tc_ut_202_render_exported_japanese_en_contains_japanese_text() {
        let rendered =
            render_exported(3, std::path::Path::new("/tmp/out.json"), Locale::JapaneseEn);
        assert!(
            rendered.contains("export しました"),
            "JapaneseEn must contain 'export しました', got: {rendered:?}"
        );
    }

    /// TC-UT-203 (REQ-DP-011 / AC-DP-07): `render_imported` の added / skipped / overwritten
    /// 各カウンタが文字列に反映される。
    #[test]
    fn tc_ut_203_render_imported_all_counters_reflected_in_output() {
        let rendered = render_imported(2, 1, 3, Locale::English);
        assert!(
            rendered.contains("imported 2 record(s)"),
            "must contain 'imported 2 record(s)', got: {rendered:?}"
        );
        assert!(
            rendered.contains("skipped 1"),
            "must contain 'skipped 1', got: {rendered:?}"
        );
        assert!(
            rendered.contains("overwritten 3"),
            "must contain 'overwritten 3', got: {rendered:?}"
        );
    }

    /// TC-UT-204 (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` に
    /// `"warning: --export-secrets is set"` と `"store the export file securely"` が含まれる
    /// （MSG-CLI-145 の両行を機械検証する）。
    #[test]
    fn tc_ut_204_render_export_secrets_warning_contains_required_message() {
        let rendered = render_export_secrets_warning(Locale::English);
        assert!(
            rendered.contains("warning: --export-secrets is set"),
            "must contain 'warning: --export-secrets is set', got: {rendered:?}"
        );
        assert!(
            rendered.contains("store the export file securely"),
            "must contain 'store the export file securely', got: {rendered:?}"
        );
    }

    /// TC-UT-212 (REQ-DP-011 / AC-DP-06): `render_exported` English locale は
    /// 日本語文字を一切含まない（BUG-002 同型回帰保証）。
    #[test]
    fn tc_ut_212_render_exported_english_does_not_contain_japanese() {
        let rendered = render_exported(1, std::path::Path::new("/tmp/x.json"), Locale::English);
        assert!(
            rendered.is_ascii(),
            "English render_exported should be ASCII-only, got: {rendered:?}"
        );
    }

    /// TC-UT-212b (REQ-DP-011 / AC-DP-07): `render_imported` English locale は
    /// 日本語文字を一切含まない（BUG-002 同型回帰保証）。
    #[test]
    fn tc_ut_212b_render_imported_english_does_not_contain_japanese() {
        let rendered = render_imported(0, 0, 0, Locale::English);
        assert!(
            rendered.is_ascii(),
            "English render_imported should be ASCII-only, got: {rendered:?}"
        );
    }

    /// TC-UT-212c (REQ-DP-011 / AC-DP-06): `render_exported` JapaneseEn locale は
    /// English 行と日本語行の両方を含む（バイリンガル出力保証）。
    #[test]
    fn tc_ut_212c_render_exported_japanese_en_contains_both_english_and_japanese() {
        let rendered =
            render_exported(5, std::path::Path::new("/tmp/v.json"), Locale::JapaneseEn);
        assert!(
            rendered.contains("exported 5 record(s)"),
            "JapaneseEn must also contain English line, got: {rendered:?}"
        );
        assert!(
            rendered.contains("export しました"),
            "JapaneseEn must contain Japanese line, got: {rendered:?}"
        );
    }

    /// TC-UT-212d (REQ-DP-011 / AC-DP-07): `render_imported` JapaneseEn locale は
    /// English 行と日本語行の両方を含む（バイリンガル出力保証）。
    #[test]
    fn tc_ut_212d_render_imported_japanese_en_contains_both_english_and_japanese() {
        let rendered = render_imported(3, 0, 1, Locale::JapaneseEn);
        assert!(
            rendered.contains("imported 3 record(s)"),
            "JapaneseEn must also contain English line, got: {rendered:?}"
        );
        assert!(
            rendered.contains("件を追加しました"),
            "JapaneseEn must contain Japanese line, got: {rendered:?}"
        );
    }

    /// TC-UT-212e (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` JapaneseEn locale は
    /// 日本語警告行も含む（バイリンガル保証）。
    #[test]
    fn tc_ut_212e_render_export_secrets_warning_japanese_en_contains_both() {
        let rendered = render_export_secrets_warning(Locale::JapaneseEn);
        assert!(
            rendered.contains("warning: --export-secrets is set"),
            "JapaneseEn must contain English warning, got: {rendered:?}"
        );
        assert!(
            rendered.contains("--export-secrets が指定されています"),
            "JapaneseEn must contain Japanese warning, got: {rendered:?}"
        );
    }

    /// TC-UT-212f (REQ-DP-011 / AC-DP-07): `render_imported` — 0件・0スキップ・0上書きでも
    /// 正常に動作する（ゼロカウンタの境界値検証）。
    #[test]
    fn tc_ut_212f_render_imported_all_zero_counters_is_valid() {
        let rendered = render_imported(0, 0, 0, Locale::English);
        assert!(
            rendered.contains("imported 0 record(s)"),
            "must handle zero counts gracefully, got: {rendered:?}"
        );
        assert!(
            rendered.contains("skipped 0"),
            "must contain 'skipped 0', got: {rendered:?}"
        );
    }

    /// TC-UT-212g (REQ-DP-011 / AC-DP-06): `render_exported` — 0件 export でも
    /// 正常に動作する（ゼロカウンタ境界値）。
    #[test]
    fn tc_ut_212g_render_exported_zero_records_is_valid() {
        let rendered = render_exported(0, std::path::Path::new("/tmp/empty.json"), Locale::English);
        assert!(
            rendered.contains("exported 0 record(s)"),
            "must handle zero record count gracefully, got: {rendered:?}"
        );
    }

    /// TC-UT-212h (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` は
    /// 1 行以上返す（空文字列でない保証）。
    #[test]
    fn tc_ut_212h_render_export_secrets_warning_is_not_empty() {
        let rendered = render_export_secrets_warning(Locale::English);
        assert!(!rendered.is_empty(), "warning must not be empty string");
        assert!(
            rendered.ends_with('\n'),
            "warning must end with newline for consistent eprintln! handling"
        );
    }

    /// TC-UT-212i (REQ-DP-011): `render_exported` は末尾改行で終わる（eprintln! 整合保証）。
    #[test]
    fn tc_ut_212i_render_exported_ends_with_newline() {
        let rendered = render_exported(1, std::path::Path::new("/x"), Locale::English);
        assert!(
            rendered.ends_with('\n'),
            "render_exported should end with newline, got: {rendered:?}"
        );
    }

    /// TC-UT-212j (REQ-DP-011): `render_imported` は末尾改行で終わる（eprintln! 整合保証）。
    #[test]
    fn tc_ut_212j_render_imported_ends_with_newline() {
        let rendered = render_imported(1, 0, 0, Locale::English);
        assert!(
            rendered.ends_with('\n'),
            "render_imported should end with newline, got: {rendered:?}"
        );
    }

    /// TC-UT-212k (REQ-DP-011 / AC-DP-07): `render_imported` は overwritten カウンタを
    /// 含む（上書き件数の確認可能性保証）。
    #[test]
    fn tc_ut_212k_render_imported_includes_overwritten_count() {
        let rendered = render_imported(0, 0, 5, Locale::English);
        assert!(
            rendered.contains("overwritten 5"),
            "must contain 'overwritten 5', got: {rendered:?}"
        );
    }

    /// TC-UT-212l (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` は
    /// `"delete it when no longer needed"` を含む（ファイル削除案内保証）。
    #[test]
    fn tc_ut_212l_render_export_secrets_warning_contains_delete_hint() {
        let rendered = render_export_secrets_warning(Locale::English);
        assert!(
            rendered.contains("delete it when no longer needed"),
            "must contain delete hint, got: {rendered:?}"
        );
    }

    /// TC-UT-212m (REQ-DP-011 / AC-DP-06): `render_exported` の出力パスは
    /// `output_path.display()` 文字列そのもの（パス変換の忠実性保証）。
    #[test]
    fn tc_ut_212m_render_exported_uses_display_path_verbatim() {
        use std::path::PathBuf;
        let p = PathBuf::from("/custom/dir/backup.json");
        let rendered = render_exported(7, &p, Locale::English);
        assert!(
            rendered.contains("/custom/dir/backup.json"),
            "must contain the exact path string, got: {rendered:?}"
        );
    }

    /// TC-UT-212n (REQ-DP-011 / AC-DP-07): `render_imported` — added と skipped と overwritten が
    /// 全て同時に 0 より大きい場合でも正しくフォーマットされる（複合カウンタ境界値）。
    #[test]
    fn tc_ut_212n_render_imported_all_nonzero_counters_formatted_correctly() {
        let rendered = render_imported(10, 3, 2, Locale::English);
        assert!(
            rendered.contains("imported 10 record(s)"),
            "must show added count 10, got: {rendered:?}"
        );
        assert!(
            rendered.contains("skipped 3"),
            "must show skipped 3, got: {rendered:?}"
        );
        assert!(
            rendered.contains("overwritten 2"),
            "must show overwritten 2, got: {rendered:?}"
        );
    }

    /// TC-UT-212o (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` の警告は
    /// 「plaintext」キーワードを含む（平文書き出しの明示性保証）。
    #[test]
    fn tc_ut_212o_render_export_secrets_warning_mentions_plaintext() {
        let rendered = render_export_secrets_warning(Locale::English);
        assert!(
            rendered.contains("plaintext"),
            "warning must explicitly mention 'plaintext', got: {rendered:?}"
        );
    }

    /// TC-UT-212p (REQ-DP-011): `render_exported` は `usize::MAX` のような大きな件数も
    /// パニックしない（整数オーバーフロー防止 / Display パス保証）。
    #[test]
    fn tc_ut_212p_render_exported_large_count_does_not_panic() {
        // パニックしないことを確認するだけ（値の厳密な検証は不要）。
        let _ = render_exported(usize::MAX, std::path::Path::new("/tmp/large.json"), Locale::English);
    }

    /// TC-UT-212q (REQ-DP-011 / AC-DP-07): `render_imported` は `usize::MAX` のような
    /// 大きなカウンタでもパニックしない（整数オーバーフロー防止）。
    #[test]
    fn tc_ut_212q_render_imported_large_counters_do_not_panic() {
        let _ = render_imported(usize::MAX, usize::MAX, usize::MAX, Locale::English);
    }

    /// TC-UT-212r (REQ-DP-011 / AC-DP-06): `render_exported` 出力の 1 行目は
    /// `"exported N record(s)"` で始まる（stdout 出力の最初行フォーマット保証）。
    #[test]
    fn tc_ut_212r_render_exported_first_line_starts_with_exported() {
        let rendered = render_exported(2, std::path::Path::new("/tmp/r.json"), Locale::English);
        let first_line = rendered.lines().next().unwrap_or("");
        assert!(
            first_line.starts_with("exported"),
            "first line must start with 'exported', got: {first_line:?}"
        );
    }

    /// TC-UT-212s (REQ-DP-011 / AC-DP-07): `render_imported` 出力の 1 行目は
    /// `"imported N record(s)"` で始まる（stdout 出力の最初行フォーマット保証）。
    #[test]
    fn tc_ut_212s_render_imported_first_line_starts_with_imported() {
        let rendered = render_imported(4, 0, 0, Locale::English);
        let first_line = rendered.lines().next().unwrap_or("");
        assert!(
            first_line.starts_with("imported"),
            "first line must start with 'imported', got: {first_line:?}"
        );
    }

    /// TC-UT-212t (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` の各行は
    /// `"warning: "` で始まる（MSG-CLI-145 フォーマット整合）。
    #[test]
    fn tc_ut_212t_render_export_secrets_warning_all_lines_start_with_warning_prefix() {
        let rendered = render_export_secrets_warning(Locale::English);
        for line in rendered.lines() {
            assert!(
                line.starts_with("warning:"),
                "each line must start with 'warning:', got: {line:?}"
            );
        }
    }

    /// TC-UT-212u (REQ-DP-011): `render_exported` English locale は正確に 1 行（改行 1 個）。
    #[test]
    fn tc_ut_212u_render_exported_english_has_exactly_one_line() {
        let rendered = render_exported(1, std::path::Path::new("/tmp/u.json"), Locale::English);
        assert_eq!(
            rendered.matches('\n').count(),
            1,
            "English render_exported should produce exactly 1 line, got: {rendered:?}"
        );
    }

    /// TC-UT-212v (REQ-DP-011): `render_exported` JapaneseEn locale は正確に 2 行（改行 2 個）。
    #[test]
    fn tc_ut_212v_render_exported_japanese_en_has_exactly_two_lines() {
        let rendered =
            render_exported(1, std::path::Path::new("/tmp/v.json"), Locale::JapaneseEn);
        assert_eq!(
            rendered.matches('\n').count(),
            2,
            "JapaneseEn render_exported should produce exactly 2 lines, got: {rendered:?}"
        );
    }

    /// TC-UT-212w (REQ-DP-011): `render_imported` English locale は正確に 1 行。
    #[test]
    fn tc_ut_212w_render_imported_english_has_exactly_one_line() {
        let rendered = render_imported(1, 0, 0, Locale::English);
        assert_eq!(
            rendered.matches('\n').count(),
            1,
            "English render_imported should produce exactly 1 line, got: {rendered:?}"
        );
    }

    /// TC-UT-212x (REQ-DP-011): `render_imported` JapaneseEn locale は正確に 2 行。
    #[test]
    fn tc_ut_212x_render_imported_japanese_en_has_exactly_two_lines() {
        let rendered = render_imported(1, 0, 0, Locale::JapaneseEn);
        assert_eq!(
            rendered.matches('\n').count(),
            2,
            "JapaneseEn render_imported should produce exactly 2 lines, got: {rendered:?}"
        );
    }

    /// TC-UT-212y (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` English locale は
    /// 正確に 2 行（MSG-CLI-145 の 2 行構造保証）。
    #[test]
    fn tc_ut_212y_render_export_secrets_warning_english_has_exactly_two_lines() {
        let rendered = render_export_secrets_warning(Locale::English);
        assert_eq!(
            rendered.matches('\n').count(),
            2,
            "English render_export_secrets_warning should produce exactly 2 lines, got: {rendered:?}"
        );
    }

    /// TC-UT-212z (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` JapaneseEn locale は
    /// 正確に 4 行（英語 2 行 + 日本語 2 行）。
    #[test]
    fn tc_ut_212z_render_export_secrets_warning_japanese_en_has_exactly_four_lines() {
        let rendered = render_export_secrets_warning(Locale::JapaneseEn);
        assert_eq!(
            rendered.matches('\n').count(),
            4,
            "JapaneseEn render_export_secrets_warning should produce exactly 4 lines, got: {rendered:?}"
        );
    }

    /// TC-UT-212c (REQ-DP-011 / AC-DP-06): `render_exported` は件数と
    /// パスを両方同一出力に含む（2 情報の同時存在保証）。
    #[allow(dead_code)] // 上で同名定義済みのため、将来クリーンアップ対象
    fn _tc_ut_212c_duplicate_guard() {}

    /// TC-UT-212aa (REQ-DP-011 / AC-DP-07): `render_imported` JapaneseEn locale は
    /// `"スキップ"` キーワードを含む（skipped の日本語表記保証）。
    #[test]
    fn tc_ut_212aa_render_imported_japanese_en_contains_skip_japanese() {
        let rendered = render_imported(0, 2, 0, Locale::JapaneseEn);
        assert!(
            rendered.contains("スキップ"),
            "JapaneseEn must contain 'スキップ' for skipped count, got: {rendered:?}"
        );
    }

    /// TC-UT-212ab (REQ-DP-011 / AC-DP-07): `render_imported` JapaneseEn locale は
    /// `"上書き"` キーワードを含む（overwritten の日本語表記保証）。
    #[test]
    fn tc_ut_212ab_render_imported_japanese_en_contains_overwrite_japanese() {
        let rendered = render_imported(0, 0, 3, Locale::JapaneseEn);
        assert!(
            rendered.contains("上書き"),
            "JapaneseEn must contain '上書き' for overwritten count, got: {rendered:?}"
        );
    }

    /// TC-UT-212ac (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` JapaneseEn は
    /// `"エクスポートファイルを安全に保管"` を含む（日本語版セキュリティ案内保証）。
    #[test]
    fn tc_ut_212ac_render_export_secrets_warning_japanese_en_contains_secure_storage_hint() {
        let rendered = render_export_secrets_warning(Locale::JapaneseEn);
        assert!(
            rendered.contains("エクスポートファイルを安全に保管"),
            "JapaneseEn must contain Japanese secure storage hint, got: {rendered:?}"
        );
    }

    /// TC-UT-212ad (REQ-DP-011 / AC-DP-06): `render_exported` 出力に `"to "` が含まれ、
    /// パスへの「書き込み先」が明示される（UX 保証）。
    #[test]
    fn tc_ut_212ad_render_exported_contains_preposition_to() {
        let rendered = render_exported(1, std::path::Path::new("/tmp/ad.json"), Locale::English);
        assert!(
            rendered.contains(" to "),
            "must contain ' to ' as preposition before path, got: {rendered:?}"
        );
    }

    /// TC-UT-212ae (REQ-DP-011): `render_imported` の出力に `"("` と `")"` が含まれ、
    /// skipped / overwritten がカッコ書きで括られるフォーマットが維持される。
    #[test]
    fn tc_ut_212ae_render_imported_parenthesized_secondary_counts() {
        let rendered = render_imported(1, 0, 0, Locale::English);
        assert!(
            rendered.contains('(') && rendered.contains(')'),
            "skipped/overwritten counts must be parenthesized, got: {rendered:?}"
        );
    }

    /// TC-UT-212af (REQ-DP-011 / AC-DP-06): `render_exported` 出力中のパス文字列は
    /// OS 規定の区切り文字（`/` または `\`）を含む（プラットフォーム別パス表記整合）。
    #[test]
    fn tc_ut_212af_render_exported_path_contains_separator() {
        use std::path::MAIN_SEPARATOR;
        let p = std::path::PathBuf::from(format!("{sep}tmp{sep}af.json", sep = MAIN_SEPARATOR));
        let rendered = render_exported(1, &p, Locale::English);
        assert!(
            rendered.contains(MAIN_SEPARATOR),
            "rendered output must contain path separator '{MAIN_SEPARATOR}', got: {rendered:?}"
        );
    }

    /// TC-UT-212ag (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` は
    /// `"secret"` という単語を含む（Secret の平文漏洩リスクの可視化保証）。
    #[test]
    fn tc_ut_212ag_render_export_secrets_warning_mentions_secret() {
        let rendered = render_export_secrets_warning(Locale::English);
        assert!(
            rendered.contains("secret"),
            "warning must mention 'secret', got: {rendered:?}"
        );
    }

    /// TC-UT-F-U04 / TC-UT-F-U12 の配置注 — 以下は既存テストが担当するため省略。
    ///
    /// TC-UT-212c (re-use guard) — TC-F-U04 / TC-F-U12 は上で実装済み。
    /// TC-UT-201〜212 以上 by Issue #141 Sub-B PR #145。

    /// TC-UT-212ah (REQ-DP-011 / AC-DP-07): `render_imported` の 1 件追加 / 0 スキップ /
    /// 0 上書き という最も一般的な正常ケースで `"imported 1 record(s)"` が返る
    /// （ラウンドトリップ受入基準 AC-DP-07 の文面整合）。
    #[test]
    fn tc_ut_212ah_render_imported_typical_single_add_output_is_correct() {
        let rendered = render_imported(1, 0, 0, Locale::English);
        assert!(
            rendered.contains("imported 1 record(s)"),
            "typical import output must contain 'imported 1 record(s)', got: {rendered:?}"
        );
        // skipped / overwritten は 0 でも括弧内に表示される
        assert!(
            rendered.contains("skipped 0"),
            "typical output must contain 'skipped 0', got: {rendered:?}"
        );
    }

    /// TC-UT-212ai (REQ-DP-011 / AC-DP-06): `render_exported` の 1 件 export が
    /// `"exported 1 record(s)"` を返す（最も一般的な受入基準文面整合）。
    #[test]
    fn tc_ut_212ai_render_exported_typical_single_record_output_is_correct() {
        let rendered = render_exported(1, std::path::Path::new("/tmp/ai.json"), Locale::English);
        assert!(
            rendered.contains("exported 1 record(s)"),
            "typical export output must contain 'exported 1 record(s)', got: {rendered:?}"
        );
    }

    /// TC-UT-212aj (REQ-DP-011 / R1-DP-02): `render_export_secrets_warning` の
    /// JapaneseEn ロケールは `"Secret"` と `"平文"` の両方を含む
    /// （Secret 種別と平文書き出しの日本語での明示保証）。
    #[test]
    fn tc_ut_212aj_render_export_secrets_warning_japanese_en_mentions_secret_and_plaintext_japanese() {
        let rendered = render_export_secrets_warning(Locale::JapaneseEn);
        assert!(
            rendered.contains("Secret"),
            "JapaneseEn must mention 'Secret', got: {rendered:?}"
        );
        assert!(
            rendered.contains("平文"),
            "JapaneseEn must mention '平文', got: {rendered:?}"
        );
    }

    // -------------------------------------------------------------------
    // 以下は既存テスト（Sub-F #44 Phase 6 / TC-F-U04 / TC-F-U12）
    // -------------------------------------------------------------------

    /// TC-UT-212z2 — 注記: 上記 TC-UT-212y / TC-UT-212z とは独立して、
    /// TC-UT-212aj 以後のテストが追加される可能性がある（Issue #141 以降の Sub-C 等）。
    /// 番号は TC-UT-213〜 を使用すること。

    /// TC-UT-212z3 (配置確認): 本 tests モジュールの末尾マーカ。
    /// TC-UT-201〜212aj が Issue #141 Sub-B (PR #145) で追加されたことを articulate する。

    /// TC-UT-212c (REQ-DP-011 / AC-DP-06): `render_exported` は件数と
    /// TC-UT-212 〜 TC-UT-212aj: 全 Issue #141 data-portability Presenter UT 追加完了。

    /// TC-UT-F-U12 (EC-F12 / C-19): 24 語表示経路で **`SerializableSecretBytes` の lossy_string
    /// 経由表示が呼出側の Vec<SerializableSecretBytes> 所有権を維持し、scope 終了時の
    /// `Drop` (= secrecy crate 経由 `zeroize`) を確実に発火させる**構造の機械検証。
    ///
    /// 設計書 §15.5 #12 は `recovery_disclosure::display` が `mem::replace` 等で「確実に
    /// Drop を発火」させることを要求するが、現行実装は呼出側 (`usecase::vault::encrypt::
    /// execute`) が `Vec<SerializableSecretBytes>` を local 変数として保持し、scope 終了
    /// (関数 return 時) に通常の Drop 経路で zeroize される構造。
    ///
    /// **§15.17.2 §A 実装事実への追従**: 本 TC は:
    /// (a) `SerializableSecretBytes::from_secret_string` で `SecretString` から包んだ後、
    ///     `to_lossy_string_for_handler` で取り出した String と元の SecretString の
    ///     値が一致する (lossy_string 経路で 24 語が観測可能)、
    /// (b) Vec が scope を抜けた後、`zeroize` 副作用が発火する責務は `secrecy` crate に
    ///     委譲済 (本 TC では crate 契約に依存し、unit-level のメモリパターン観測は
    ///     skip。詳細は Sub-A `RecoveryWords` 同型 TC で機械検証済)、
    /// を articulate する。`Vec<SerializableSecretBytes>` 所有権消費形への移行時は
    /// `mem::replace` パターン検証に拡張する Boy Scout (Phase 8+)。
    ///
    /// 配置先: `crates/shikomi-cli/src/presenter/success.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置 `presenter/recovery_disclosure.rs::tests` を未導入実装事実に追従)。
    #[test]
    fn tc_f_u12_render_recovery_disclosure_lossy_string_path_preserves_word_visibility() {
        use shikomi_core::ipc::SerializableSecretBytes;
        use shikomi_core::SecretString;

        // (a) 24 語を SerializableSecretBytes で包み、lossy_string で取り出した文字列が
        //     screen presenter の出力に含まれる (Drop 発火前の表示経路の機械検証)。
        let words: Vec<SerializableSecretBytes> = (1..=24)
            .map(|i| {
                SerializableSecretBytes::from_secret_string(&SecretString::from_string(format!(
                    "wd{i:02}"
                )))
            })
            .collect();
        let rendered = render_recovery_disclosure_screen(&words, Locale::English);
        for i in 1..=24u32 {
            let expected = format!("wd{i:02}");
            assert!(
                rendered.contains(&expected),
                "word {expected:?} must be visible in screen output before Drop"
            );
        }

        // (b) words が scope 内で Drop されるパターン検証: クロージャ内で Vec を所有
        //     させ、return 時に Drop が走ることを構造的に確認する。`SerializableSecretBytes`
        //     は `zeroize` を内包する型なので、Drop 経路で副作用が発火するのは secrecy
        //     crate 契約に委譲済 (Sub-A `RecoveryWords` 同型で機械検証済の上位 SSoT)。
        let rendered_in_scope = {
            let inner_words: Vec<SerializableSecretBytes> = (1..=24)
                .map(|i| {
                    SerializableSecretBytes::from_secret_string(&SecretString::from_string(
                        format!("scope{i:02}"),
                    ))
                })
                .collect();
            render_recovery_disclosure_screen(&inner_words, Locale::JapaneseEn)
            // inner_words は scope 終了で Drop → zeroize 連鎖発火 (`secrecy` crate)。
        };
        assert!(rendered_in_scope.contains("scope01"));
        assert!(rendered_in_scope.contains("scope24"));
        // scope 抜け後、inner_words は moved out で参照不能。型レベルで観測経路が閉じる。
    }
}
