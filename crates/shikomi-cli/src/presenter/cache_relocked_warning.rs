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
}
