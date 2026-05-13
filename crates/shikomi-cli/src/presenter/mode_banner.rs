//! 保護モードバナー (`shikomi list` 先頭行、Sub-F #44 Phase 3、REQ-S16 / C-37)。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §`ProtectionModeBanner` enum
//!
//! 本 presenter は **副作用なし**の pure function。`color_enabled` は呼出側
//! (`lib::run_list`) が `NO_COLOR` 環境変数 / 非 TTY / `--quiet` を判定してから
//! 渡す責務 (presenter は env を読まない、テスト再現性維持)。
//!
//! 文字列は `shikomi_core::ipc::ProtectionModeBanner::label()` の凍結文字列を使う
//! (SSoT、CLI 側で再定義禁止、`#[non_exhaustive]` cross-crate 防御)。文字単独で
//! 「平文 / Locked / Unlocked / Unknown」が判別可能なため、色覚多様性下でも
//! 安全に意味を伝える (REQ-S16 二重符号化)。

use shikomi_core::ipc::ProtectionModeBanner;

// -------------------------------------------------------------------
// ANSI escape sequences (16-color SGR、broad terminal compatibility)
// -------------------------------------------------------------------

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_CYAN_DIM: &str = "\x1b[2;36m"; // 灰色 (cyan dim) — Plaintext
const ANSI_YELLOW: &str = "\x1b[33m"; // 橙色 — EncryptedLocked
const ANSI_GREEN: &str = "\x1b[32m"; // 緑色 — EncryptedUnlocked
const ANSI_RED: &str = "\x1b[31m"; // 赤色 — Unknown (Fail-Secure)

/// 保護モードバナー文字列を返す (末尾改行付き)。
///
/// `color_enabled == false` 時は ANSI escape を一切含まないプレーン文字列を返す
/// (`NO_COLOR` 規約準拠、`https://no-color.org`)。
///
/// # 不変条件
///
/// - 呼出側が `protection_mode == Unknown` を検出して exit 3 で fail-fast する
///   責務 (REQ-S16)。本 presenter は表示文字列のみを返し、終了コードへは関与しない
///   (Single Responsibility)。
/// - `#[non_exhaustive]` cross-crate 防御: 将来 variant 追加時も
///   `ProtectionModeBanner::label()` の `_` arm が `[unknown]` に倒すため、本関数も
///   fail-secure に追従する (Sub-D Rev3 凍結方針継承)。
#[must_use]
pub fn display(protection_mode: ProtectionModeBanner, color_enabled: bool) -> String {
    let label = protection_mode.label();
    if !color_enabled {
        return format!("{label}\n");
    }
    let color = ansi_color_for(protection_mode);
    format!("{color}{label}{ANSI_RESET}\n")
}

/// 各 variant に紐づく ANSI 16-color SGR 開始シーケンスを返す。
///
/// 設計書 §`ProtectionModeBanner` enum の凍結マッピング:
///
/// | variant | ANSI カラー |
/// |---|---|
/// | `Plaintext` | 灰色 (cyan dim) |
/// | `EncryptedLocked` | 橙色 (yellow) |
/// | `EncryptedUnlocked` | 緑色 (green) |
/// | `Unknown` | 赤色 (red) |
fn ansi_color_for(protection_mode: ProtectionModeBanner) -> &'static str {
    match protection_mode {
        ProtectionModeBanner::Plaintext => ANSI_CYAN_DIM,
        ProtectionModeBanner::EncryptedLocked => ANSI_YELLOW,
        ProtectionModeBanner::EncryptedUnlocked => ANSI_GREEN,
        // `Unknown` + cross-crate `#[non_exhaustive]` 防御 (Sub-D Rev3 凍結継承):
        // `Unknown` を明示的に赤、将来追加されうる variant も同じく赤 (Fail-Secure)
        // に倒す。同一 body の `Unknown => ANSI_RED` 冗長 arm は `_` に集約して
        // `clippy::match_same_arms` を解消する (`label()` 側の `_ => "[unknown]"`
        // 経路と意味的にも一致)。
        _ => ANSI_RED,
    }
}

// -------------------------------------------------------------------
// tests — pure function なので env を経由しない
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_plaintext_with_color_disabled_is_plain_label() {
        let out = display(ProtectionModeBanner::Plaintext, false);
        assert_eq!(out, "[plaintext]\n");
        assert!(
            !out.contains("\x1b["),
            "color-disabled output must not contain ANSI escapes"
        );
    }

    #[test]
    fn display_encrypted_locked_with_color_disabled_is_plain_label() {
        let out = display(ProtectionModeBanner::EncryptedLocked, false);
        assert_eq!(out, "[encrypted, locked]\n");
    }

    #[test]
    fn display_encrypted_unlocked_with_color_disabled_is_plain_label() {
        let out = display(ProtectionModeBanner::EncryptedUnlocked, false);
        assert_eq!(out, "[encrypted, unlocked]\n");
    }

    #[test]
    fn display_unknown_with_color_disabled_is_plain_label() {
        let out = display(ProtectionModeBanner::Unknown, false);
        assert_eq!(out, "[unknown]\n");
    }

    #[test]
    fn display_plaintext_with_color_wraps_in_cyan_dim() {
        let out = display(ProtectionModeBanner::Plaintext, true);
        assert!(out.starts_with(ANSI_CYAN_DIM));
        assert!(out.contains("[plaintext]"));
        assert!(out.ends_with(&format!("{ANSI_RESET}\n")));
    }

    #[test]
    fn display_encrypted_locked_with_color_wraps_in_yellow() {
        let out = display(ProtectionModeBanner::EncryptedLocked, true);
        assert!(out.starts_with(ANSI_YELLOW));
        assert!(out.contains("[encrypted, locked]"));
    }

    #[test]
    fn display_encrypted_unlocked_with_color_wraps_in_green() {
        let out = display(ProtectionModeBanner::EncryptedUnlocked, true);
        assert!(out.starts_with(ANSI_GREEN));
        assert!(out.contains("[encrypted, unlocked]"));
    }

    #[test]
    fn display_unknown_with_color_wraps_in_red() {
        let out = display(ProtectionModeBanner::Unknown, true);
        assert!(out.starts_with(ANSI_RED));
        assert!(out.contains("[unknown]"));
    }

    /// 色覚多様性下でも label 文字列単独で意味が伝わる二重符号化の保証。
    /// 4 variant それぞれの label は人間可読かつ排他的に異なる。
    #[test]
    fn labels_are_pairwise_distinct_for_dual_encoding() {
        let labels = [
            ProtectionModeBanner::Plaintext.label(),
            ProtectionModeBanner::EncryptedLocked.label(),
            ProtectionModeBanner::EncryptedUnlocked.label(),
            ProtectionModeBanner::Unknown.label(),
        ];
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                assert_ne!(
                    labels[i], labels[j],
                    "labels must be pairwise distinct (REQ-S16 dual encoding)"
                );
            }
        }
    }

    // ---------------------------------------------------------------
    // Issue #76 (#74-B): TC-F-U05 / TC-F-U07
    // 設計根拠: docs/features/vault-encryption/test-design/sub-f-cli-subcommands/
    //          {index.md §15.5, issue-76-verification.md §15.17.1}
    // ---------------------------------------------------------------

    /// TC-F-U05 (EC-F9): `mode_banner::display(ProtectionModeBanner)` が **4 variant
    /// 全て**で正規 label 文字列を返し、`color_enabled = true` 時は ANSI カラー
    /// シーケンス付き、`false` 時は剥離されたプレーン文字列を返す **NO_COLOR 切替**
    /// の機械検証。
    ///
    /// 設計書 §15.5 #5 の (a) 4 variant × (b) NO_COLOR 切替を 1 関数で網羅検証する。
    /// 既存の variant 単独検証 (`display_*_with_color_*`) と直交し、本 TC は variant
    /// × color の **2 次元マトリクス**を一括 articulate する SSoT。
    ///
    /// 配置先: `crates/shikomi-cli/src/presenter/mode_banner.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置と一致)。
    #[test]
    fn tc_f_u05_display_renders_four_variants_with_no_color_toggle() {
        let cases = [
            (
                ProtectionModeBanner::Plaintext,
                "[plaintext]",
                ANSI_CYAN_DIM,
            ),
            (
                ProtectionModeBanner::EncryptedLocked,
                "[encrypted, locked]",
                ANSI_YELLOW,
            ),
            (
                ProtectionModeBanner::EncryptedUnlocked,
                "[encrypted, unlocked]",
                ANSI_GREEN,
            ),
            (ProtectionModeBanner::Unknown, "[unknown]", ANSI_RED),
        ];

        for (variant, expected_label, expected_color) in cases {
            // (a) NO_COLOR 相当 (color_enabled = false): プレーン文字列のみ。
            let plain = display(variant, false);
            assert_eq!(
                plain,
                format!("{expected_label}\n"),
                "{variant:?}: color_enabled = false で ANSI escape が含まれてはならない"
            );
            assert!(
                !plain.contains("\x1b["),
                "{variant:?}: NO_COLOR 経路で ANSI escape は除去されるべき"
            );

            // (b) color_enabled = true: ANSI カラー → label → reset の順。
            let colored = display(variant, true);
            assert!(
                colored.starts_with(expected_color),
                "{variant:?}: ANSI start sequence mismatch, got {colored:?}"
            );
            assert!(
                colored.contains(expected_label),
                "{variant:?}: label 文字列が含まれていない"
            );
            assert!(
                colored.ends_with(&format!("{ANSI_RESET}\n")),
                "{variant:?}: reset + 改行で終端すべき"
            );
        }
    }

    /// TC-F-U07 (C-37 / EC-F9): `match` 式が `ProtectionModeBanner` の 4 variant 全て
    /// を網羅し、かつ **`#[non_exhaustive]` cross-crate 防御的 `_` arm を許容**して
    /// fail-secure に倒すパターンの正当性を機械検証する (Sub-E TC-E-S01 同型、Rev1
    /// ペテルギウス致命1 解消)。
    ///
    /// 設計書 §15.5 #7 の cross-crate match パターン:
    /// `Plaintext => "p", EncryptedLocked => "el", EncryptedUnlocked => "eu",
    /// Unknown => "u", _ => "fail-secure"` を本 enum に対して書き、4 variant 全てが
    /// 期待 tag を返し、`_` arm が defensive fail-secure として正当に許容されること
    /// (将来 variant 追加時にも fail-secure に倒れる) を確認する。
    ///
    /// `#[non_exhaustive]` 対応は本 enum 自体ではなく `shikomi_core::ipc::ProtectionModeBanner`
    /// 側で凍結済 (cross-crate context)。本 unit test は CLI 側で `_` arm を書いた
    /// match 式が `cargo check` 警告ゼロで通ること + 4 variant 全網羅マッピングを
    /// 担保する。
    ///
    /// 配置先: `crates/shikomi-cli/src/presenter/mode_banner.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置と一致)。
    #[test]
    fn tc_f_u07_protection_mode_banner_match_with_defensive_underscore_arm_compiles() {
        fn tag_for(banner: ProtectionModeBanner) -> &'static str {
            // Sub-E TC-E-S01 同型: 4 variant 明示 + `_` defensive fail-secure arm。
            // `#[non_exhaustive]` cross-crate 防御 (将来追加 variant も "fail-secure" に倒す)。
            match banner {
                ProtectionModeBanner::Plaintext => "p",
                ProtectionModeBanner::EncryptedLocked => "el",
                ProtectionModeBanner::EncryptedUnlocked => "eu",
                ProtectionModeBanner::Unknown => "u",
                // 防御的 `_` arm: cross-crate `#[non_exhaustive]` の将来 variant 追加への
                // 備え (clippy::wildcard_in_or_patterns 不適用)。
                _ => "fail-secure",
            }
        }

        // 4 variant 全網羅: 明示マッピングが正しく適用される。
        assert_eq!(tag_for(ProtectionModeBanner::Plaintext), "p");
        assert_eq!(tag_for(ProtectionModeBanner::EncryptedLocked), "el");
        assert_eq!(tag_for(ProtectionModeBanner::EncryptedUnlocked), "eu");
        assert_eq!(tag_for(ProtectionModeBanner::Unknown), "u");

        // 4 tag が pairwise distinct であること (二重符号化の補助、`labels_are_pairwise_distinct_for_dual_encoding`
        // と直交)。
        let tags: std::collections::BTreeSet<&str> = [
            tag_for(ProtectionModeBanner::Plaintext),
            tag_for(ProtectionModeBanner::EncryptedLocked),
            tag_for(ProtectionModeBanner::EncryptedUnlocked),
            tag_for(ProtectionModeBanner::Unknown),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            tags.len(),
            4,
            "4 variant の tag は pairwise distinct であるべき"
        );
    }
}
