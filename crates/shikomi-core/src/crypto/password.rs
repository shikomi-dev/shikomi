//! パスワード認証境界 — `MasterPassword` / `PasswordStrengthGate` trait / `WeakPasswordFeedback`。
//!
//! `MasterPassword::new` は `&dyn PasswordStrengthGate` の通過を必須とする (契約 C-8)。
//! 強度ゲートの具体実装 (zxcvbn) は Sub-D の `shikomi-infra` 側に置く (Clean Architecture)。
//! 本モジュールは trait シグネチャと `WeakPasswordFeedback` 構造体のみ提供する。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/password.md`

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CryptoError;
use crate::secret::SecretBytes;

// -------------------------------------------------------------------
// MasterPassword
// -------------------------------------------------------------------

/// ユーザ入力のマスターパスワードを保持する型。
///
/// 構築時に `PasswordStrengthGate::validate` を必ず通過する (契約 C-8)。
/// 内部は `SecretBytes` ベースで `Drop` 時に zeroize される。
///
/// # Forbidden traits
///
/// `Clone` / `Copy` / `Display` / `serde::Serialize` / `PartialEq` / `Eq` は未実装。
///
/// ```compile_fail
/// use shikomi_core::crypto::MasterPassword;
/// // Display 未実装 — println! でフォーマット指定子 {} は通らない
/// fn check(p: &MasterPassword) { let _ = format!("{}", p); }
/// ```
pub struct MasterPassword {
    inner: SecretBytes,
}

impl MasterPassword {
    /// 強度ゲートを通過した文字列から `MasterPassword` を構築する (契約 C-8)。
    ///
    /// # Errors
    ///
    /// `gate.validate(&s)` が `Err(WeakPasswordFeedback)` を返した場合は
    /// `CryptoError::WeakPassword(feedback)` を返し、`MasterPassword` は構築されない。
    /// 入力 `s` は本関数内で `into_bytes()` 経由で消費されるため、呼出側は別途
    /// 保持していたコピーを `zeroize` で消す責務を負う (Sub-D 境界で明示)。
    pub fn new(s: String, gate: &dyn PasswordStrengthGate) -> Result<Self, CryptoError> {
        gate.validate(&s)
            .map_err(|fb| CryptoError::WeakPassword(Box::new(fb)))?;
        let bytes = s.into_bytes();
        Ok(Self {
            inner: SecretBytes::from_vec(bytes),
        })
    }

    /// 生バイト列を取り出す (Sub-B `Argon2idAdapter::derive_kek_pw` 入力専用)。
    ///
    /// 可視性: `pub` (`shikomi-infra::crypto::kdf` への正規入力経路)。
    ///
    /// 可視性ポリシーの差別化 (Sub-B Rev2 で凍結、`detailed-design/password.md`
    /// §`MasterPassword` 参照):
    /// - `Vek` / `Kek<_>` / `HeaderAeadKey::expose_within_crate` は **`pub(crate)`**
    ///   (鍵バイトは外部 crate に渡さない、`shikomi-core::crypto` 内部閉じ)
    /// - `MasterPassword::expose_secret_bytes` / `RecoveryMnemonic::expose_words`
    ///   は **`pub`** (KDF 入力として `shikomi-infra` アダプタへの正規経路として開放)
    ///
    /// **呼び出して良いのは `shikomi-infra::crypto::kdf` モジュールのみ**。
    /// CLI / GUI / daemon の bin crate から本メソッドを呼ぶことは禁止
    /// (CI grep による静的検出対象、`detailed-design/kdf.md` §`Argon2idAdapter` 参照)。
    #[must_use]
    pub fn expose_secret_bytes(&self) -> &[u8] {
        self.inner.expose_secret()
    }
}

impl fmt::Debug for MasterPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED MASTER PASSWORD]")
    }
}

// -------------------------------------------------------------------
// PasswordStrengthGate trait
// -------------------------------------------------------------------

/// パスワード強度判定 trait (dyn-safe)。
///
/// shikomi-core では trait シグネチャのみ定義する。実装 (zxcvbn 強度 ≥ 3 等) は
/// Sub-D の `shikomi-infra::crypto::ZxcvbnGate` 担当 (Clean Architecture)。
///
/// # 実装が遵守すべき契約 (Sub-D 設計書 §`PasswordStrengthGate` trait 参照)
///
/// 1. 強度判定の合否しか trait シグネチャに表れない (内部スコア値・辞書は外に漏らさない)。
/// 2. `validate` は決して panic しない (`WeakPasswordFeedback` または `Ok(())` に収束)。
/// 3. 副作用なし (`&self`, I/O なし)。
pub trait PasswordStrengthGate {
    /// パスワード文字列の強度を判定する。弱い場合は `WeakPasswordFeedback` を返す。
    ///
    /// # Errors
    ///
    /// 強度不足の場合は `WeakPasswordFeedback` (`warning` + `suggestions`) を返す。
    fn validate(&self, password: &str) -> Result<(), WeakPasswordFeedback>;
}

// -------------------------------------------------------------------
// WeakPasswordFeedback
// -------------------------------------------------------------------

/// 弱パスワード検出時に MSG-S08 (Fail Kindly) で利用する構造データ。
///
/// `warning` / `suggestions` は zxcvbn の `feedback` をそのまま英語 raw のまま運ぶ。
/// **i18n 翻訳は呼出側 (Sub-D / Sub-F) の責務** (設計書 §i18n 戦略責務分離)。
///
/// `warning` が `None` の場合に「無音でユーザに渡す」実装は契約として禁止。
/// Sub-D は `suggestions` 先頭文 / 強度スコア値 / 既定文の代替警告文を提示する責務を負う
/// (設計書 §`warning=None` 時の代替警告文契約)。
///
/// `Serialize` / `Deserialize` は IPC 経由で daemon→CLI/GUI に渡すために実装する
/// (鍵バイト列とは異なり警告文自体は秘密でない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeakPasswordFeedback {
    /// zxcvbn の `feedback.warning` (主要な警告文)。`None` の場合あり。
    pub warning: Option<String>,
    /// zxcvbn の `feedback.suggestions` (改善提案リスト)。空ベクタも有効。
    pub suggestions: Vec<String>,
}

impl WeakPasswordFeedback {
    /// 警告と改善提案を指定して構築する。
    #[must_use]
    pub fn new(warning: Option<String>, suggestions: Vec<String>) -> Self {
        Self {
            warning,
            suggestions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 常に Ok を返すテスト用ゲート。
    struct AlwaysAcceptGate;
    impl PasswordStrengthGate for AlwaysAcceptGate {
        fn validate(&self, _password: &str) -> Result<(), WeakPasswordFeedback> {
            Ok(())
        }
    }

    /// 常に Err を返すテスト用ゲート。
    struct AlwaysRejectGate;
    impl PasswordStrengthGate for AlwaysRejectGate {
        fn validate(&self, _password: &str) -> Result<(), WeakPasswordFeedback> {
            Err(WeakPasswordFeedback::new(
                Some("test reject".to_string()),
                vec!["use longer".to_string()],
            ))
        }
    }

    // -----------------------------------------------------------------
    // MasterPassword (C-8)
    // -----------------------------------------------------------------

    #[test]
    fn master_password_new_accepts_when_gate_returns_ok() {
        let p = MasterPassword::new("anything".to_string(), &AlwaysAcceptGate).unwrap();
        assert_eq!(p.expose_secret_bytes(), b"anything");
    }

    #[test]
    fn master_password_new_rejects_when_gate_returns_err_with_weak_password_feedback() {
        let err = MasterPassword::new("weak".to_string(), &AlwaysRejectGate).unwrap_err();
        match err {
            CryptoError::WeakPassword(fb) => {
                assert_eq!(fb.warning.as_deref(), Some("test reject"));
                assert_eq!(fb.suggestions, vec!["use longer".to_string()]);
            }
            other => panic!("expected WeakPassword, got {other:?}"),
        }
    }

    #[test]
    fn master_password_new_returns_boxed_feedback_to_keep_crypto_error_small() {
        // CryptoError::WeakPassword は Box<WeakPasswordFeedback> を保持する。
        // この型形状そのものをコンパイル時に固定する (回帰防止)。
        let err = MasterPassword::new("x".to_string(), &AlwaysRejectGate).unwrap_err();
        if let CryptoError::WeakPassword(boxed) = err {
            let _: Box<WeakPasswordFeedback> = boxed;
        } else {
            panic!("expected WeakPassword variant");
        }
    }

    #[test]
    fn master_password_new_invokes_gate_dynamically_via_dyn_dispatch() {
        let gate: &dyn PasswordStrengthGate = &AlwaysAcceptGate;
        assert!(MasterPassword::new("ok".to_string(), gate).is_ok());
    }

    #[test]
    fn master_password_debug_returns_fixed_redacted_string() {
        let p = MasterPassword::new("secret-pw".to_string(), &AlwaysAcceptGate).unwrap();
        let s = format!("{p:?}");
        assert_eq!(s, "[REDACTED MASTER PASSWORD]");
        assert!(!s.contains("secret-pw"));
    }

    // -----------------------------------------------------------------
    // WeakPasswordFeedback
    // -----------------------------------------------------------------

    #[test]
    fn weak_password_feedback_supports_warning_none_for_sub_d_fallback_contract() {
        // warning=None の入力を許容する (Sub-D が fallback 警告文を提示する責務)
        let fb = WeakPasswordFeedback::new(None, vec!["add uppercase".to_string()]);
        assert!(fb.warning.is_none());
        assert_eq!(fb.suggestions.len(), 1);
    }

    #[test]
    fn weak_password_feedback_supports_empty_suggestions() {
        let fb = WeakPasswordFeedback::new(Some("too short".to_string()), Vec::new());
        assert!(fb.suggestions.is_empty());
    }

    /// `Serialize` / `Deserialize` derive の存在を型レベルで担保する。
    /// (実際の round-trip 検証は Sub-E の IPC 結合テストで実施)
    #[test]
    fn weak_password_feedback_implements_serialize_and_deserialize_traits() {
        fn assert_serde<T: serde::Serialize + serde::de::DeserializeOwned>() {}
        assert_serde::<WeakPasswordFeedback>();
    }
}
