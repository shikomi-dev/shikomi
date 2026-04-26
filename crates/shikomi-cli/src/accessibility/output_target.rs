//! `--output` 経路の自動切替 (env / 明示フラグ判定、Sub-F #44 Phase 6)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 自動切替経路 (`SHIKOMI_ACCESSIBILITY=1` env /
//!   OS スクリーンリーダー検出 / 明示フラグのいずれかで `--output screen` 既定を
//!   `Braille` に上書き、明示 `--output` フラグは常に最優先)
//!
//! Phase 6 スコープ:
//! - `SHIKOMI_ACCESSIBILITY` env (`"1"` / `"true"` / `"yes"` のいずれかで有効) 判定
//! - 明示フラグ最優先 (clap default が `Screen` の場合のみ env で上書き)
//! - OS スクリーンリーダー自動検出 (macOS `defaults read` / Win Narrator process /
//!   Linux Orca DBus) は **Phase 7** に分離
//!
//! 不変条件:
//! - 明示 `--output {print,braille,audio}` は **常に** その値を返す (env 無視)。
//! - 既定値 (`Screen`) かつ `SHIKOMI_ACCESSIBILITY` 有効時のみ `Braille` に切替。
//! - env が未設定 / 無効値の場合は `Screen` のまま (Fail Closed の逆、UX 優先)。

use crate::cli::OutputTarget;

/// `--output` フラグの実効値を解決する。
///
/// `requested` が `Screen` (clap 既定) かつ `SHIKOMI_ACCESSIBILITY` env 有効時
/// のみ `Braille` に上書き。それ以外は `requested` をそのまま返す。
#[must_use]
pub fn resolve(requested: OutputTarget) -> OutputTarget {
    if requested != OutputTarget::Screen {
        return requested; // 明示フラグは最優先
    }
    let raw = std::env::var("SHIKOMI_ACCESSIBILITY").ok();
    if accessibility_env_enabled(raw.as_deref()) {
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
}
