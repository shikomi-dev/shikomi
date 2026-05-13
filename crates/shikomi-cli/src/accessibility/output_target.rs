//! `--output` 経路の自動切替 (env / 明示フラグ判定、Sub-F #44 Phase 6)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 自動切替経路 (`SHIKOMI_ACCESSIBILITY=1` env /
//!   OS スクリーンリーダー検出 / 明示フラグのいずれかで `--output screen` 既定を
//!   `Braille` に上書き、明示 `--output` フラグは常に最優先)
//!
//! Phase 6 〜 Phase 7:
//! - `SHIKOMI_ACCESSIBILITY` env (`"1"` / `"true"` / `"yes"` のいずれかで有効) 判定
//! - 明示フラグ最優先 (clap default が `Screen` の場合のみ env で上書き)
//! - **Phase 7**: OS スクリーンリーダー自動検出 (`screen_reader::is_screen_reader_active`)
//!   を統合。env / OS 検出のいずれかで `Screen → Braille` 上書き (OR 評価)。
//!
//! 不変条件:
//! - 明示 `--output {print,braille,audio}` は **常に** その値を返す (env 無視)。
//! - 既定値 (`Screen`) かつ `SHIKOMI_ACCESSIBILITY` 有効時のみ `Braille` に切替。
//! - env が未設定 / 無効値の場合は `Screen` のまま (Fail Closed の逆、UX 優先)。

use crate::accessibility::screen_reader;
use crate::cli::OutputTarget;

/// `--output` フラグの実効値を解決する。
///
/// `requested` が `Screen` (clap 既定) かつ以下のいずれかの場合に `Braille` 上書き:
/// - `SHIKOMI_ACCESSIBILITY` env が有効 (`1` / `true` / `yes`)
/// - OS スクリーンリーダーが起動中 (Phase 7、`screen_reader` モジュール経由)
///
/// 明示フラグ (`Print` / `Braille` / `Audio`) は env / 検出に関わらず最優先。
#[must_use]
pub fn resolve(requested: OutputTarget) -> OutputTarget {
    if requested != OutputTarget::Screen {
        return requested; // 明示フラグは最優先
    }
    let raw = std::env::var("SHIKOMI_ACCESSIBILITY").ok();
    if accessibility_env_enabled(raw.as_deref()) || screen_reader::is_screen_reader_active() {
        OutputTarget::Braille
    } else {
        OutputTarget::Screen
    }
}

/// `SHIKOMI_ACCESSIBILITY` env の値判定 pure 関数 (テスト容易性)。
#[must_use]
pub fn accessibility_env_enabled(value: Option<&str>) -> bool {
    matches!(
        value.map(str::to_ascii_lowercase).as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_env_enabled_recognizes_1_true_yes() {
        assert!(accessibility_env_enabled(Some("1")));
        assert!(accessibility_env_enabled(Some("true")));
        assert!(accessibility_env_enabled(Some("TRUE")));
        assert!(accessibility_env_enabled(Some("yes")));
        assert!(accessibility_env_enabled(Some("Yes")));
    }

    #[test]
    fn test_accessibility_env_enabled_rejects_0_false_empty_none() {
        assert!(!accessibility_env_enabled(Some("0")));
        assert!(!accessibility_env_enabled(Some("false")));
        assert!(!accessibility_env_enabled(Some("")));
        assert!(!accessibility_env_enabled(Some("garbage")));
        assert!(!accessibility_env_enabled(None));
    }

    /// 明示 `--output braille` は env 設定なしでも維持される (フラグ最優先)。
    #[test]
    fn test_resolve_explicit_braille_is_kept_regardless_of_env() {
        // env を直接いじらず、明示フラグの分岐だけを検証する (early return)。
        assert_eq!(resolve(OutputTarget::Braille), OutputTarget::Braille);
        assert_eq!(resolve(OutputTarget::Print), OutputTarget::Print);
        assert_eq!(resolve(OutputTarget::Audio), OutputTarget::Audio);
    }

    // 注: `resolve(Screen)` の env-driven 分岐は env mutation を含むため、
    // `accessibility_env_enabled` の pure 関数テストで網羅し、`resolve` 自身の
    // env-driven 分岐は integration test (`tests/it_accessibility_resolve.rs` 等)
    // で `Command::env` 経由の分離プロセスとして検証する設計とする
    // (`unsafe_code = "deny"` workspace 規約 + TC-CI-026 整合)。

    /// TC-F-U14 (C-39 / EC-F10、**旧 TC-F-U08 リナンバ**): `accessibility::output_target::
    /// resolve()` の **明示フラグ排他確認 + env / OS 検出による Screen → Braille 自動切替**
    /// 4 パターンを機械検証する。
    ///
    /// 設計書 §15.5 #14 の 4 パターン:
    /// (a) `SHIKOMI_ACCESSIBILITY=1` set + フラグ無し → env-driven 経路 (env 操作必要、
    ///     unit では `accessibility_env_enabled` の pure 関数で代替)
    /// (b) フラグ `--output print` set → Print そのまま返却 (early return)
    /// (c) どちらも未設定 + screen reader 非起動 → Screen そのまま (env-driven 検証で代替)
    /// (d) `SHIKOMI_ACCESSIBILITY=1` + `--output audio` 併用 → **明示フラグ最優先で Audio**
    ///
    /// **§15.17.2 §A 実装事実への追従**: `resolve(Screen)` の env-driven 経路は env mutation
    /// を含むため `unsafe_code = "deny"` workspace 規約に違反せずに unit から書けない。本 TC
    /// は (b)(d) **明示フラグ最優先**の 3 variant 経路 + (a)(c) 相当を `accessibility_env_enabled`
    /// pure 関数で代替し、env-driven `resolve(Screen)` 自身は `tests/it_accessibility_resolve.rs`
    /// (integration、`Command::env` 分離プロセス) で別途検証する設計を articulate する。
    ///
    /// 配置先: `crates/shikomi-cli/src/accessibility/output_target.rs::tests`
    /// (issue-76-verification.md §15.17.1 推奨配置と一致)。**旧 TC-F-U08 リナンバ**経緯:
    /// Issue #75 で TC-F-U08 = `windows_pipe_name_from_dir` 純関数性に固定されたため、
    /// 本 TC は U14 にリナンバ済 (§15.14b 履歴 articulate)。
    #[test]
    fn tc_f_u14_resolve_explicit_flag_takes_precedence_over_env_for_three_variants() {
        // (b/d) 明示フラグは env / OS 検出無視で**最優先**。
        // 早期 return 経路のため env mutation を含まず unit から決定的に検証可能。
        assert_eq!(
            resolve(OutputTarget::Print),
            OutputTarget::Print,
            "(b) 明示 --output print は env 無視で Print を返す"
        );
        assert_eq!(
            resolve(OutputTarget::Braille),
            OutputTarget::Braille,
            "明示 --output braille は env 無視で Braille を返す"
        );
        assert_eq!(
            resolve(OutputTarget::Audio),
            OutputTarget::Audio,
            "(d) 明示 --output audio は env 無視で Audio を返す (フラグ最優先)"
        );

        // (a/c) `Screen` 既定時の env-driven 切替は env mutation を含むため、
        //       `accessibility_env_enabled` pure 関数で OR 評価ロジックを代替検証する。
        // (a) `SHIKOMI_ACCESSIBILITY=1` 系 (`1` / `true` / `yes` 大文字含む) → enabled。
        for v in ["1", "true", "TRUE", "yes", "Yes"] {
            assert!(
                accessibility_env_enabled(Some(v)),
                "(a) SHIKOMI_ACCESSIBILITY={v:?} は enabled として認識されるべき"
            );
        }
        // (c) 未設定 / 偽値 → disabled (Screen のまま、screen_reader 非起動前提)。
        for v in [None, Some(""), Some("0"), Some("false"), Some("garbage")] {
            assert!(
                !accessibility_env_enabled(v),
                "(c) SHIKOMI_ACCESSIBILITY={v:?} は disabled として認識されるべき"
            );
        }
    }
}
