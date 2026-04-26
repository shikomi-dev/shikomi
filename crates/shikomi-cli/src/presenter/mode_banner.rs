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
        ProtectionModeBanner::Unknown => ANSI_RED,
        // `#[non_exhaustive]` cross-crate 防御 (Sub-D Rev3 凍結継承):
        // 将来 variant 追加時は赤 (Unknown 同等) に倒す Fail-Secure。
        #[allow(unreachable_patterns)]
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
}
