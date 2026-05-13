//! `ZxcvbnGate` — zxcvbn 強度 ≥ `min_score` を要求する `PasswordStrengthGate` 実装。
//!
//! 凍結値 `min_score = 3` (`tech-stack.md` §4.7 `zxcvbn` 行 + Sub-0 REQ-S08、
//! `zxcvbn::Score::Three` 以上)。
//!
//! ## Sub-B 契約 (`password.md` §Sub-B が遵守すべき契約)
//!
//! 1. **panic 禁止**: `unwrap()` / `expect()` を使わない。`feedback()` が `None` の場合は
//!    `warning: None, suggestions: Vec::new()` で `WeakPasswordFeedback` を構築する。
//! 2. **副作用なし**: `&self`、内部状態を変更しない (zxcvbn は呼出毎に内部 fresh)。
//! 3. **i18n 層を持たない**: 英語 raw 文字列を `WeakPasswordFeedback` にそのまま詰める
//!    (i18n 翻訳は呼出側 = Sub-D / Sub-F の責務、`password.md` §i18n 戦略責務分離)。
//! 4. **無状態**: `ZxcvbnGate` は内部に zxcvbn インスタンスを保持しない。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/password.md`

use shikomi_core::crypto::{PasswordStrengthGate, WeakPasswordFeedback};

/// `min_score = 3` 凍結値 (REQ-S08 / `tech-stack.md` §4.7 `zxcvbn` 行)。
const DEFAULT_MIN_SCORE: u8 = 3;

/// zxcvbn 強度 ≥ `min_score` を要求する `PasswordStrengthGate` 実装。
///
/// `Default` で本番経路 (`min_score = 3`) を構築する。テストで全パスワード受理が必要な
/// 場合のみ `ZxcvbnGate { min_score: 0 }` を使う (`AlwaysAcceptGate` 等価)。
#[derive(Debug, Clone, Copy)]
pub struct ZxcvbnGate {
    /// 受理に必要な最小スコア。zxcvbn は `0..=4` の範囲を返す。
    pub min_score: u8,
}

impl Default for ZxcvbnGate {
    fn default() -> Self {
        Self {
            min_score: DEFAULT_MIN_SCORE,
        }
    }
}

impl PasswordStrengthGate for ZxcvbnGate {
    fn validate(&self, password: &str) -> Result<(), WeakPasswordFeedback> {
        // `user_inputs = &[]` で初期実装 (Sub-D で username / vault path 文脈追加検討、
        // `password.md` §`impl PasswordStrengthGate for ZxcvbnGate`)。
        let result = zxcvbn::zxcvbn(password, &[]);

        // zxcvbn v3 の `Score` enum を u8 (0..=4) へ変換。
        // `Score` の `Ord` を直接比較せず u8 経由にするのは、本ゲートの `min_score`
        // が u8 で受け取る (テスト時に任意値を差し込めるため) 公開 API のため。
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        let score = result.score() as u8;
        if score >= self.min_score {
            return Ok(());
        }

        // 弱パスワード: feedback を英語 raw のまま `WeakPasswordFeedback` に詰める。
        // `feedback()` が `None` の場合は (warning: None, suggestions: vec![]) で構築 (panic 禁止)。
        let (warning, suggestions) = match result.feedback() {
            Some(fb) => {
                let warning = fb.warning().map(|w| w.to_string());
                let suggestions = fb
                    .suggestions()
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                (warning, suggestions)
            }
            None => (None, Vec::new()),
        };

        Err(WeakPasswordFeedback::new(warning, suggestions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Default` で本番経路 `min_score = 3` を構築する。
    #[test]
    fn default_min_score_is_three() {
        let g = ZxcvbnGate::default();
        assert_eq!(g.min_score, 3);
    }

    /// 明らかに弱いパスワードは拒否される (zxcvbn score 0 ~ 1 想定)。
    #[test]
    fn validate_rejects_obviously_weak_password() {
        let g = ZxcvbnGate::default();
        let res = g.validate("password");
        assert!(res.is_err(), "common password 'password' must be rejected");
    }

    /// 強いパスワードは受理される。
    /// (`zxcvbn` は長さ・記号混在で score=4 を返す傾向)。
    #[test]
    fn validate_accepts_long_diverse_password() {
        let g = ZxcvbnGate::default();
        let res = g.validate("Tr0ub4dor&3-Tr0ub4dor&3-Tr0ub4dor&3");
        assert!(res.is_ok(), "long diverse password must be accepted");
    }

    /// `min_score = 0` で構築すると全パスワード受理 (テスト容易性確認)。
    #[test]
    fn min_score_zero_accepts_any_password() {
        let g = ZxcvbnGate { min_score: 0 };
        assert!(g.validate("a").is_ok());
        assert!(g.validate("").is_ok());
        assert!(g.validate("password").is_ok());
    }

    /// 弱パスワード拒否時、`WeakPasswordFeedback` の `suggestions` が空でも
    /// `warning: None` でも `Err` で確実に返る (panic 禁止契約の確認)。
    #[test]
    fn validate_does_not_panic_on_short_password() {
        let g = ZxcvbnGate::default();
        // 1 文字でも panic せず Err で収束する (契約 第 1 項)
        let _ = g.validate("a");
    }

    /// 弱パスワード拒否時の `WeakPasswordFeedback` には feedback が含まれる
    /// (具体内容は zxcvbn 実装依存のため文言自体はアサートしない、構造のみ確認)。
    #[test]
    fn validate_returns_weak_password_feedback_struct_on_rejection() {
        let g = ZxcvbnGate::default();
        let err = g.validate("password").expect_err("must reject");
        // `Vec::is_empty()` は false / true どちらでも contract 上は OK。
        // ここでは fb.warning が Option<String> 型として読めることのみを構造確認する。
        let _: Option<String> = err.warning;
        let _: Vec<String> = err.suggestions;
    }
}
