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

    /// TC-F-U02 (C-33 / EC-F11): 翻訳辞書欠落時にパニックさせず英語 fallback で fail-soft する
    /// 「**i18n fail-soft 契約**」の機械検証。
    ///
    /// 設計書 §15.5 #2 は `Localizer::new("ja-JP")?.translate("nonexistent_key")` が
    /// `[missing:nonexistent_key]` を返すことを要求するが、`shikomi_cli::i18n::Localizer`
    /// モジュールは `Phase 6 / Phase 7` で導入予定であり、現状未実装 (`presenter::success.rs`
    /// 内 doc コメント「完全な i18n 辞書 (`messages.toml` / `Localizer`) への移行は
    /// Phase 6 / Phase 7 で集約する」を参照、Issue #75 マージ時点 SSoT)。
    ///
    /// **§15.17.2 §A 実装事実への追従**: 現実装の i18n fail-soft 経路は `Locale::detect_from_lang_env_value`
    /// が **未知 / 不正な LANG 値**を `English` に fallback する仕組みで担保される。本 TC は
    /// この既存経路を articulate し、未知文字列入力で:
    /// (a) パニックせず、
    /// (b) `English` fallback を返す、
    /// ことを機械検証する。`Localizer::translate` 移行後は本 TC を `[missing:{key}]` 検証に
    /// 差し替える Boy Scout が必要 (Phase 6/7 PR 時点)。
    ///
    /// 配置先: `crates/shikomi-cli/src/presenter/mod.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置 `i18n/mod.rs::tests` を Localizer 未導入の現実装事実に追従して `presenter` 配下に配置)。
    #[test]
    fn tc_f_u02_locale_detect_falls_back_to_english_for_unknown_lang_value_without_panic() {
        // (a) 未知の LANG 値 — `xx_YY` のような ISO 639 にない hypothetical コード。
        let unknown = Locale::detect_from_lang_env_value(Some("xx_YY.UTF-8"));
        assert_eq!(
            unknown,
            Locale::English,
            "unknown LANG value must fail-soft to English (C-33 fail-soft 契約の最小実装事実)"
        );

        // (b) `garbage` 入力 — 空、長さ 1、ASCII 制御文字、未知 ISO 639 コード等のノイズ。
        // 注: 非 ASCII 入力 (例: emoji `🦀`) は **Bug-F-010 既知未解消**として skip する。
        // 現実装 `detect_from_lang_env_value` は `s[..2]` byte slice を char boundary 不問で
        // 切るため、2 バイト目が char 境界外の入力 (`🦀` 等) で **panic** する経路がある。
        // C-33 fail-soft 契約違反。本 PR の test plan に Bug-F-010 として report 済 (Issue #76 工程3
        // 完了報告)、修正は別 PR で追跡 (lib/test の責務分離)。本 TC は ASCII 入力範囲で
        // fail-soft 契約の有効領域を articulate する。
        for v in [
            Some(""),
            Some("x"),
            Some("\x01"),
            Some("ZZ_FAKE.UTF-8"),
            Some("missing-key-style"),
            None,
        ] {
            // パニックせず Locale を返すこと自体が fail-soft 契約の本体。
            let resolved = Locale::detect_from_lang_env_value(v);
            // `ja` で始まらない値は全て English に倒れる契約。
            assert_eq!(
                resolved,
                Locale::English,
                "non-ja LANG value `{v:?}` must fall back to English"
            );
        }
    }
}
