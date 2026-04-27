//! `cache_relocked: false` 時の MSG-S20 連結警告（Sub-F #44 Phase 4、C-32 整合）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/basic-design/ux-and-msg.md
//!   §cache_relocked: false の UX 設計判断 / §文言の不変条件 (a)(b)(c)
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §設計判断: `cache_relocked: false` 経路の CLI 表示分岐（C-32 整合）
//!
//! 責務分離:
//! - 本モジュールは MSG-S20 警告 + 再 unlock 案内のみを返す pure function。
//! - `presenter::success::render_rekeyed` / `render_recovery_rotated` は MSG-S07 /
//!   MSG-S19 + 24 語表示のみに責務を縮小し、`cache_relocked` 分岐を持たない。
//! - `usecase::vault::{rekey, rotate_recovery}` が `outcome.cache_relocked == false`
//!   を観測した時にのみ本モジュールの `display` を追加で stdout に出力する。
//!
//! 不変条件:
//! - 終了コードは Sub-F C-31 / C-36 に従い `0` のまま（operation 成功）。
//! - `Locale::JapaneseEn` でも英文を先に出し、和文を改行で続ける（success.rs と同じ二段書式）。
//! - 副作用なし。`std::env` も `std::io` も触らない。

use super::Locale;

/// MSG-S20 連結警告 + 再 unlock 案内を返す。
///
/// 既存の `presenter::success::render_rekeyed` 等が末尾に追記しても
/// 不自然にならないよう、先頭に空行（`\n`）を 1 つ含める。
#[must_use]
pub fn display(locale: Locale) -> String {
    let mut out = String::new();
    render_to(&mut out, locale);
    out
}

/// 既存の `String` バッファに連結する low-level 形式。
///
/// `render_rekeyed_with_fallback_notice` のように複数 presenter モジュールを
/// 連結したい呼出側が `String` を再確保せずに済むよう公開する。
pub fn render_to(out: &mut String, locale: Locale) {
    out.push('\n');
    out.push_str(
        "warning: rekey/rotation succeeded but the unlock cache could not be refreshed.\n",
    );
    out.push_str("hint: run `shikomi vault unlock` again before the next operation.\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "警告: 鍵情報の再キャッシュに失敗しました。次の操作前に `shikomi vault unlock` を再度実行してください。\n",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_english_contains_msg_s20_keywords() {
        let s = display(Locale::English);
        assert!(
            s.contains("warning:"),
            "english warning marker missing: {s}"
        );
        assert!(
            s.contains("unlock cache could not be refreshed"),
            "MSG-S20 english body missing: {s}"
        );
        assert!(
            s.contains("shikomi vault unlock"),
            "english unlock hint missing: {s}"
        );
        assert!(!s.contains("警告"), "japanese leaked into english: {s}");
    }

    #[test]
    fn test_display_japanese_en_contains_both_languages() {
        let s = display(Locale::JapaneseEn);
        assert!(s.contains("warning:"), "english marker missing: {s}");
        assert!(s.contains("警告:"), "japanese marker missing: {s}");
        assert!(
            s.contains("`shikomi vault unlock` を再度実行"),
            "japanese unlock hint missing: {s}"
        );
    }

    #[test]
    fn test_display_starts_with_newline_for_concatenation() {
        // 既存 24 語ブロックの末尾に空行を挟んで読みやすくする契約。
        assert!(display(Locale::English).starts_with('\n'));
        assert!(display(Locale::JapaneseEn).starts_with('\n'));
    }

    #[test]
    fn test_render_to_appends_in_place() {
        let mut buf = String::from("prefix\n");
        render_to(&mut buf, Locale::English);
        assert!(buf.starts_with("prefix\n\n"));
        assert!(buf.contains("warning:"));
    }

    /// TC-F-U06 (C-32 / C-36 / EC-F6): `cache_relocked: false` 経路で `vault rekey` /
    /// `rotate-recovery` 完了後に追記される **MSG-S07 / MSG-S19 完了文言 + MSG-S20「次
    /// の操作前に `shikomi vault unlock` を再度実行してください」連結**を機械検証
    /// する。
    ///
    /// 設計書 §15.5 #6 の検証手段に合わせて:
    /// (a) English locale で MSG-S20 英文の必須キーワード (`unlock cache could not
    ///     be refreshed`、`shikomi vault unlock`) を含む、
    /// (b) JapaneseEn locale で MSG-S20 英文 + 和文 2 段が連結される、
    /// (c) `success::render_rekeyed_with_fallback_notice` が **MSG-S07** + 24 語表示 +
    ///     MSG-S20 連結警告を生成する経路で、本モジュール `display` と同じ MSG-S20
    ///     文言を保つ (DRY、SSoT 1 箇所、Issue #75 Bug-F-002 §経路復活整合)、
    /// を articulate する。終了コード 0 を返す呼出側責務 (C-31 / C-36) は本 unit test
    /// では検証せず、結合 TC-F-I07c で間接担保。
    ///
    /// 配置先: `crates/shikomi-cli/src/presenter/cache_relocked_warning.rs::tests`
    /// (issue-76-verification.md §15.17.1 推奨配置と一致)。
    #[test]
    fn tc_f_u06_display_concatenates_msg_s20_warning_for_both_locales() {
        // (a) English: warning + unlock hint の必須キーワード。
        let en = display(Locale::English);
        assert!(
            en.contains("warning:"),
            "english warning marker missing: {en}"
        );
        assert!(
            en.contains("unlock cache could not be refreshed"),
            "MSG-S20 english body missing: {en}"
        );
        assert!(
            en.contains("shikomi vault unlock"),
            "english unlock hint missing: {en}"
        );

        // (b) JapaneseEn: 英 + 日 2 段の MSG-S20 連結。
        let ja = display(Locale::JapaneseEn);
        assert!(
            ja.contains("warning:"),
            "english marker missing in ja: {ja}"
        );
        assert!(ja.contains("警告:"), "japanese marker missing: {ja}");
        assert!(
            ja.contains("`shikomi vault unlock` を再度実行"),
            "japanese unlock hint missing: {ja}"
        );

        // (c) `success::render_rekeyed_with_fallback_notice` 経由で SSoT 整合: MSG-S07 完了
        // 文言 (`rekeyed N records`) + 24 語表示の後に **本モジュールの MSG-S20 文言が同じ
        // 文言で連結**される (DRY、Issue #75 Bug-F-002 §経路復活整合)。空 24 語スライスで
        // 構造のみ検証する (zeroize 連鎖は TC-F-U12 範疇)。
        let rekeyed_with_fallback =
            crate::presenter::success::render_rekeyed_with_fallback_notice(7, &[], Locale::English);
        assert!(
            rekeyed_with_fallback.contains("rekeyed 7 records"),
            "MSG-S07 完了文言 (records_count) が連結されるべき: {rekeyed_with_fallback}"
        );
        assert!(
            rekeyed_with_fallback.contains("unlock cache could not be refreshed"),
            "MSG-S20 警告文言が `render_rekeyed_with_fallback_notice` 経由で連結されるべき (DRY、SSoT 1 箇所): {rekeyed_with_fallback}"
        );
    }
}
