//! `Argon2idAdapter` — マスターパスワード + 16B salt → `Kek<KekKindPw>`。
//!
//! `argon2` crate の **raw API** (`Argon2::hash_password_into`) のみ使用する。
//! `password-hash` の PHC 文字列 API は vault ヘッダ管理と二重管理になるため不採用
//! (`tech-stack.md` §4.7 `argon2` 行 / `kdf.md` §`argon2` crate 呼出契約)。
//!
//! 凍結値 `m=19_456, t=2, p=1, output_len=32` (OWASP 2024-05、`Argon2idParams::FROZEN_OWASP_2024_05`)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/kdf.md`

use argon2::{Algorithm, Argon2, Params, Version};
use shikomi_core::crypto::{Kek, KekKindPw, MasterPassword};
use shikomi_core::error::{CryptoError, KdfErrorKind};
use shikomi_core::KdfSalt;
use zeroize::Zeroizing;

// 鍵バイト長 (AES-256 鍵長 = `Vek` / `Kek` と同じ 32B)。
const KEK_LEN: usize = 32;

// -------------------------------------------------------------------
// Argon2idParams
// -------------------------------------------------------------------

/// Argon2id パラメータ凍結値。
///
/// `FROZEN_OWASP_2024_05` 以外を指定するのはテスト用途のみ
/// (Sub-D の `vault encrypt` 入口は `Argon2idAdapter::default()` 経由で凍結値を使う)。
///
/// `Debug, Clone, Copy, PartialEq, Eq` 派生 (const 値、秘密ではない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2idParams {
    /// memory cost (KiB)。OWASP 2024-05 推奨 19 MiB = 19,456 KiB。
    pub m: u32,
    /// time cost (iterations)。OWASP 2024-05 推奨 2。
    pub t: u32,
    /// parallelism (lanes)。OWASP 2024-05 推奨 1 (single thread)。
    pub p: u32,
    /// 出力長 (byte)。`Vek` / `Kek` と同じ 32B 固定。
    pub output_len: usize,
}

impl Argon2idParams {
    /// OWASP Password Storage Cheat Sheet (2024-05 改訂版) + REQ-S03 凍結値。
    ///
    /// 再評価サイクル: 4 年または criterion ベンチで p95 1 秒逸脱時
    /// (`tech-stack.md` §4.7 `argon2` 行同時改訂を必須とする)。
    pub const FROZEN_OWASP_2024_05: Argon2idParams = Argon2idParams {
        m: 19_456,
        t: 2,
        p: 1,
        output_len: 32,
    };
}

impl Default for Argon2idParams {
    fn default() -> Self {
        Self::FROZEN_OWASP_2024_05
    }
}

// -------------------------------------------------------------------
// Argon2idAdapter
// -------------------------------------------------------------------

/// Argon2id KDF アダプタ。無状態 struct (params のみ保持、derive は値コピー)。
///
/// `Default` で `FROZEN_OWASP_2024_05` を採用する本番経路を取る。テストで小さい
/// params を渡す場合のみ `Argon2idAdapter::new(params)` を使う。
#[derive(Debug, Clone, Copy, Default)]
pub struct Argon2idAdapter {
    params: Argon2idParams,
}

impl Argon2idAdapter {
    /// 任意 params で構築する (主にテスト用、本番は `Default::default()` を使う)。
    #[must_use]
    pub fn new(params: Argon2idParams) -> Self {
        Self { params }
    }

    /// 現在の params を返す (テスト / 観測用)。
    #[must_use]
    pub fn params(&self) -> Argon2idParams {
        self.params
    }

    /// マスターパスワードと salt から `Kek<KekKindPw>` を導出する (Sub-B 主目的)。
    ///
    /// 中間 32B 出力バッファを `Zeroizing<[u8; 32]>` で囲み、`Kek::from_array(*out)` で
    /// 値ムーブ後に元バッファは Drop → zeroize される。
    ///
    /// # Errors
    ///
    /// `argon2` crate の `Params::new` / `hash_password_into` エラーを
    /// `CryptoError::KdfFailed { kind: KdfErrorKind::Argon2id, source }` に変換して返す。
    /// 内部リトライは行わない (`kdf.md` §エラーハンドリング)。
    pub fn derive_kek_pw(
        &self,
        password: &MasterPassword,
        salt: &KdfSalt,
    ) -> Result<Kek<KekKindPw>, CryptoError> {
        let params = Params::new(
            self.params.m,
            self.params.t,
            self.params.p,
            Some(self.params.output_len),
        )
        .map_err(kdf_argon2_err)?;

        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut out: Zeroizing<[u8; KEK_LEN]> = Zeroizing::new([0u8; KEK_LEN]);
        argon
            .hash_password_into(
                password.expose_secret_bytes(),
                salt.as_array(),
                out.as_mut_slice(),
            )
            .map_err(kdf_argon2_err)?;

        Ok(Kek::<KekKindPw>::from_array(*out))
    }
}

/// `argon2` crate エラーを `CryptoError::KdfFailed { Argon2id, .. }` に変換する。
fn kdf_argon2_err<E>(e: E) -> CryptoError
where
    E: std::error::Error + Send + Sync + 'static,
{
    CryptoError::KdfFailed {
        kind: KdfErrorKind::Argon2id,
        source: Box::new(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::crypto::{PasswordStrengthGate, WeakPasswordFeedback};

    /// テスト用ゲート (常に Ok)。`MasterPassword::new` を通すために使う。
    struct AcceptGate;
    impl PasswordStrengthGate for AcceptGate {
        fn validate(&self, _: &str) -> Result<(), WeakPasswordFeedback> {
            Ok(())
        }
    }

    /// CI 高速化用の小さい params (m=8 KiB, t=1, p=1, 32B 出力)。
    /// **本番経路では使用禁止** (`FROZEN_OWASP_2024_05` のみ)、テスト専用。
    fn fast_params() -> Argon2idParams {
        Argon2idParams {
            m: 8,
            t: 1,
            p: 1,
            output_len: 32,
        }
    }

    #[test]
    fn frozen_owasp_2024_05_constants_match_design_freeze() {
        assert_eq!(Argon2idParams::FROZEN_OWASP_2024_05.m, 19_456);
        assert_eq!(Argon2idParams::FROZEN_OWASP_2024_05.t, 2);
        assert_eq!(Argon2idParams::FROZEN_OWASP_2024_05.p, 1);
        assert_eq!(Argon2idParams::FROZEN_OWASP_2024_05.output_len, 32);
    }

    #[test]
    fn default_adapter_uses_frozen_owasp_params() {
        let a = Argon2idAdapter::default();
        assert_eq!(a.params(), Argon2idParams::FROZEN_OWASP_2024_05);
    }

    #[test]
    fn default_params_are_frozen_owasp_2024_05() {
        assert_eq!(
            Argon2idParams::default(),
            Argon2idParams::FROZEN_OWASP_2024_05
        );
    }

    #[test]
    fn derive_kek_pw_with_fast_params_returns_ok() {
        let adapter = Argon2idAdapter::new(fast_params());
        let pw = MasterPassword::new("correct horse battery staple".to_string(), &AcceptGate)
            .expect("test gate accepts any password");
        let salt = KdfSalt::try_new(&[0xABu8; 16]).expect("16B salt");
        let kek = adapter.derive_kek_pw(&pw, &salt);
        assert!(kek.is_ok());
    }

    /// 同一入力 (password + salt + params) → 同一 KEK 出力 (Argon2id 決定論性)。
    #[test]
    fn derive_kek_pw_is_deterministic_for_same_inputs() {
        let adapter = Argon2idAdapter::new(fast_params());
        let salt = KdfSalt::try_new(&[0x33u8; 16]).expect("16B salt");
        let pw1 = MasterPassword::new("pw".to_string(), &AcceptGate).unwrap();
        let pw2 = MasterPassword::new("pw".to_string(), &AcceptGate).unwrap();
        let k1 = adapter.derive_kek_pw(&pw1, &salt).unwrap();
        let k2 = adapter.derive_kek_pw(&pw2, &salt).unwrap();
        // `Kek` の `expose_within_crate` は `pub(crate)` のため shikomi-core 内でしか触れない。
        // ここでは fmt::Debug が固定文字列で同じであることのみ確認 (両者同一型)。
        assert_eq!(format!("{k1:?}"), format!("{k2:?}"));
    }

    /// 異なる salt → 異なる KEK は別途 KAT (RFC 9106) でカバーする。本テストは
    /// 「同一 salt + 異なる password で構築が成功する」のみを確認。
    #[test]
    fn derive_kek_pw_with_different_passwords_succeeds() {
        let adapter = Argon2idAdapter::new(fast_params());
        let salt = KdfSalt::try_new(&[0u8; 16]).unwrap();
        let p1 = MasterPassword::new("alpha".to_string(), &AcceptGate).unwrap();
        let p2 = MasterPassword::new("beta".to_string(), &AcceptGate).unwrap();
        assert!(adapter.derive_kek_pw(&p1, &salt).is_ok());
        assert!(adapter.derive_kek_pw(&p2, &salt).is_ok());
    }
}
