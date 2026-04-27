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
                SerializableSecretBytes::from_secret_string(SecretString::from_string(format!(
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

    /// TC-F-U12 (EC-F12 / C-19): 24 語表示経路で **`SerializableSecretBytes` の lossy_string
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
                SerializableSecretBytes::from_secret_string(SecretString::from_string(format!(
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
                    SerializableSecretBytes::from_secret_string(SecretString::from_string(format!(
                        "scope{i:02}"
                    )))
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
