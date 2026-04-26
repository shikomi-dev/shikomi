//! 成功時の stdout メッセージ整形。
//!
//! MSG-CLI-001〜005 + Sub-F (#44) MSG-S01〜S07 / S19 / S20 経路。pure function、
//! `String` を返すのみ。

use std::path::Path;

use shikomi_core::ipc::SerializableSecretBytes;
use shikomi_core::RecordId;

use super::Locale;
use crate::cli::OutputTarget;

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

/// 非 Screen 経路（Print/Braille/Audio）の Phase 2 fallback 文言。
///
/// Phase 5 で `accessibility::{print_pdf, braille_brf, audio_tts}` が実装される
/// までの間、ユーザに「Screen 経路にフォールバック中」を明示する。
#[must_use]
pub fn render_recovery_disclosure_screen_with_fallback_notice(
    disclosure: &[SerializableSecretBytes],
    output: OutputTarget,
    locale: Locale,
) -> String {
    let mut out = render_recovery_disclosure_screen(disclosure, locale);
    out.push_str(&fallback_notice(output, locale));
    out
}

/// `vault rekey` 成功文言（MSG-S07 + 24 語表示、cache_relocked false 時は MSG-S20 連結 = Phase 4）。
#[must_use]
pub fn render_rekeyed(
    records_count: usize,
    words: &[SerializableSecretBytes],
    cache_relocked: bool,
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
    if !cache_relocked {
        out.push_str(&cache_relocked_warning(locale));
    }
    out
}

/// `vault rekey` の非 Screen 経路 fallback。
#[must_use]
pub fn render_rekeyed_with_fallback_notice(
    records_count: usize,
    words: &[SerializableSecretBytes],
    cache_relocked: bool,
    output: OutputTarget,
    locale: Locale,
) -> String {
    let mut out = render_rekeyed(records_count, words, cache_relocked, locale);
    out.push_str(&fallback_notice(output, locale));
    out
}

/// `vault rotate-recovery` 成功文言（MSG-S19、cache_relocked false 時は Phase 4 で連結）。
#[must_use]
pub fn render_recovery_rotated(
    words: &[SerializableSecretBytes],
    cache_relocked: bool,
    locale: Locale,
) -> String {
    let mut out = String::from("recovery words rotated\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("リカバリ用 24 語をローテーションしました\n");
    }
    out.push_str("new recovery words (shown only once):\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("新しい 24 語（再表示されません）:\n");
    }
    push_word_lines(&mut out, words);
    if !cache_relocked {
        out.push_str(&cache_relocked_warning(locale));
    }
    out
}

/// `vault rotate-recovery` の非 Screen 経路 fallback。
#[must_use]
pub fn render_recovery_rotated_with_fallback_notice(
    words: &[SerializableSecretBytes],
    cache_relocked: bool,
    output: OutputTarget,
    locale: Locale,
) -> String {
    let mut out = render_recovery_rotated(words, cache_relocked, locale);
    out.push_str(&fallback_notice(output, locale));
    out
}

/// 24 語を 1 語 1 行で push する（番号 1〜n、UTF-8 lossy が secret_bytes 側 helper で適用済）。
fn push_word_lines(out: &mut String, words: &[SerializableSecretBytes]) {
    for (i, w) in words.iter().enumerate() {
        let s = w.to_lossy_string_for_handler();
        out.push_str(&format!("  {:>2}. {}\n", i + 1, s));
    }
}

/// `cache_relocked: false` 時の MSG-S20 連結警告（Phase 2 minimal、Phase 4 で再構成）。
fn cache_relocked_warning(locale: Locale) -> String {
    let mut out = String::from(
        "\nwarning: rekey/rotation succeeded but the unlock cache could not be refreshed.\n",
    );
    out.push_str("hint: run `shikomi vault unlock` again before the next operation.\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "警告: 鍵情報の再キャッシュに失敗しました。次の操作前に `shikomi vault unlock` を再度実行してください。\n",
        );
    }
    out
}

/// 非 Screen 経路の Phase 2 fallback 注記。
fn fallback_notice(output: OutputTarget, locale: Locale) -> String {
    let label = match output {
        OutputTarget::Print => "print",
        OutputTarget::Braille => "braille",
        OutputTarget::Audio => "audio",
        OutputTarget::Screen => "screen",
    };
    let mut out = format!(
        "\nnote: --output {label} is not yet wired in this build (Phase 5); displayed on screen instead.\n"
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!(
            "備考: --output {label} は本ビルドで未実装のため画面に表示しています（Phase 5 で実装予定）。\n"
        ));
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
}
