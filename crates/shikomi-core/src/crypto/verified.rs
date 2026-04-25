//! Fail-Secure 型 — `Plaintext` / `Verified<T>` / `verify_aead_decrypt` / `CryptoOutcome<T>`。
//!
//! - `Plaintext`: AEAD 復号後の平文を保持する型。コンストラクタは
//!   `pub(in crate::crypto::verified)` 限定 (Rev1 で `pub(crate)` から絞り込み)。
//!   `Verified::new_from_aead_decrypt` を実装する**同一モジュール内**からのみ構築可。
//! - `Verified<T>`: AEAD 検証済みマーカー (caller-asserted)。
//!   `Verified::new_from_aead_decrypt` は `pub(crate)` のため shikomi-infra から構築不可。
//! - `verify_aead_decrypt(closure)`: shikomi-infra の AES-GCM 復号クロージャを包む
//!   呼出側主張マーカー関数。
//! - `CryptoOutcome<T>`: 失敗バリアント先頭 + `Verified` 末尾の網羅 match を強制する列挙。
//!
//! 構造的封鎖の三段防御 (`detailed-design/nonce-and-aead.md` §`verify_aead_decrypt`):
//! 1. **型レベル**: `Verified::new_from_aead_decrypt` は `pub(crate)` で外部 crate 構築不可。
//! 2. **モジュール内**: `Plaintext::new_within_module` は `pub(in crate::crypto::verified)` で
//!    `Verified::new_from_aead_decrypt` と同一モジュール内のみ構築可。
//! 3. **呼出側契約**: `verify_aead_decrypt(|| ...)` クロージャ内で AEAD 検証実行を契約宣言。
//!    Sub-C PR レビューで「クロージャ内が aes-gcm の検証 API を呼んでいるか」を必須確認。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/nonce-and-aead.md`

use core::fmt;

use crate::crypto::password::WeakPasswordFeedback;
use crate::error::KdfErrorKind;
use crate::secret::SecretBytes;

// -------------------------------------------------------------------
// Plaintext
// -------------------------------------------------------------------

/// AEAD 復号後の平文。`Verified<Plaintext>` 経由でのみ取り出される。
///
/// コンストラクタ `new_within_module` の可視性は `pub(in crate::crypto::verified)`
/// — `Verified::new_from_aead_decrypt` を実装する**同一モジュール内**のみで構築可。
///
/// `expose_secret` は `pub` (クリップボード投入等で外部 crate read アクセス必要)、
/// ただし**新規構築不可**。
///
/// # Forbidden traits
///
/// `Clone` / `Copy` / `Display` / `serde::Serialize` / `PartialEq` / `Eq` 未実装。
///
/// ```compile_fail
/// use shikomi_core::crypto::Plaintext;
/// // pub(in crate::crypto::verified) のため外部 crate からは呼べない
/// let _ = Plaintext::new_within_module(b"fake".to_vec());
/// ```
pub struct Plaintext {
    inner: SecretBytes,
}

impl Plaintext {
    /// `pub(in crate::crypto::verified)` 限定。`Verified::new_from_aead_decrypt` 経路のみで使う。
    ///
    /// 外部 crate および shikomi-core 内の他モジュール (`crypto::key` 等) からも呼出不可。
    /// この絞り込みにより「未検証 ciphertext を `Plaintext` として扱う」事故を構造封鎖する。
    pub(in crate::crypto::verified) fn new_within_module(bytes: Vec<u8>) -> Self {
        Self {
            inner: SecretBytes::from_vec(bytes),
        }
    }

    /// 平文バイト列への参照を返す。読み取りは外部 crate から許可。
    #[must_use]
    pub fn expose_secret(&self) -> &[u8] {
        self.inner.expose_secret()
    }
}

impl fmt::Debug for Plaintext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED PLAINTEXT]")
    }
}

// -------------------------------------------------------------------
// Verified<T>
// -------------------------------------------------------------------

/// AEAD 検証済み (caller-asserted) マーカー。`Verified<Plaintext>` が主用途。
///
/// コンストラクタ `new_from_aead_decrypt` は `pub(crate)` のため、
/// **shikomi-infra を含む外部 crate からは直接構築不可**。
/// shikomi-infra は `verify_aead_decrypt(|| ...)` 経由でのみ `Verified<T>` を得る。
///
/// # Forbidden traits
///
/// `Clone` / `Copy` / `Display` / `serde::Serialize` / `PartialEq` / `Eq` 未実装。
/// `Clone` 禁止により AEAD 検証マーカーの複製攻撃面拡大を防ぐ。
///
/// ```compile_fail
/// use shikomi_core::crypto::{Plaintext, Verified};
/// // 外部 crate からは pub(crate) コンストラクタを呼べない (契約 C-7)
/// let _ = Verified::new_from_aead_decrypt(Plaintext::new_within_module(vec![]));
/// ```
pub struct Verified<T> {
    inner: T,
}

impl<T> Verified<T> {
    /// `pub(crate)` 限定。AEAD 復号成功直後にのみ呼ばれる契約。
    ///
    /// 外部 crate (shikomi-infra 含む) からは構築不可。
    pub(crate) fn new_from_aead_decrypt(inner: T) -> Self {
        Self { inner }
    }

    /// 内包する T を所有権付きで取り出す (検証済み平文を消費して投入する経路)。
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// 内包する T への参照を返す (検証済み平文を非消費で読む経路)。
    pub fn as_inner(&self) -> &T {
        &self.inner
    }
}

impl<T: fmt::Debug> fmt::Debug for Verified<T> {
    /// 内部 T の Debug 出力に委譲する。
    /// `T = Plaintext` の場合は `Verified<Plaintext> { inner: [REDACTED PLAINTEXT] }` となり、
    /// `Plaintext::Debug` の固定文字列で秘密が漏れない。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Verified")
            .field("inner", &self.inner)
            .finish()
    }
}

// -------------------------------------------------------------------
// verify_aead_decrypt — caller-asserted marker wrapper
// -------------------------------------------------------------------

/// shikomi-infra の AES-GCM 復号クロージャを包む呼出側主張マーカー関数。
///
/// クロージャ実行が `Ok(T)` を返したとき `Verified<T>` で包んで返す。
/// shikomi-infra (Sub-C) は AES-GCM 復号 + GMAC タグ検証を本クロージャ内で実装し、
/// 検証失敗時は `Err(E)` を返すことで `verify_aead_decrypt` 全体が `Err(E)` となる。
///
/// **本関数の保証範囲は型レベルではなく契約レベル** (詳細は設計書
/// §`verify_aead_decrypt` ラッパ関数の契約)。Sub-C 実装が AEAD 検証を skip した場合、
/// 型システムでは検出できない。三段防御の三段目 (Sub-C PR レビュー) で構造的に検出する責務。
///
/// # Errors
///
/// クロージャが返した `Err(E)` をそのまま伝播する。
pub fn verify_aead_decrypt<F, T, E>(decrypt_fn: F) -> Result<Verified<T>, E>
where
    F: FnOnce() -> Result<T, E>,
{
    decrypt_fn().map(Verified::new_from_aead_decrypt)
}

// -------------------------------------------------------------------
// CryptoOutcome<T>
// -------------------------------------------------------------------

/// 暗号操作の結果列挙。失敗バリアント先頭 + `Verified` 末尾で網羅 match を強制する。
///
/// 設計書 §設計意図:
/// - `match` の第一アームに `Verified` を置く実装は PR レビューで却下する (Boy Scout Rule)。
/// - `#[non_exhaustive]` で将来バリアント追加時の漏れを `match` 警告で検出。
#[derive(Debug)]
#[non_exhaustive]
pub enum CryptoOutcome<T> {
    /// AEAD 認証タグ検証失敗 (vault.db 改竄の可能性)。MSG-S10 経路。
    TagMismatch,
    /// `NonceCounter::increment` 上限到達。MSG-S11 経路。
    NonceLimit,
    /// KDF 計算失敗 (Argon2id / PBKDF2 / HKDF)。MSG-S09 KDF カテゴリ。
    KdfFailed(KdfErrorKind),
    /// `MasterPassword::new` 構築失敗。MSG-S08 (Fail Kindly)。
    /// `Box` で `CryptoOutcome` enum 全体のサイズを抑える (詳細は `CryptoError::WeakPassword` 参照)。
    WeakPassword(Box<WeakPasswordFeedback>),
    /// 正常系: AEAD 検証成功 + 平文取得 (`T = Plaintext` が主用途)。
    Verified(Verified<T>),
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Plaintext
    // -----------------------------------------------------------------

    #[test]
    fn plaintext_debug_returns_fixed_redacted_string() {
        let p = Plaintext::new_within_module(b"secret-bytes".to_vec());
        let s = format!("{p:?}");
        assert_eq!(s, "[REDACTED PLAINTEXT]");
        assert!(!s.contains("secret-bytes"));
    }

    #[test]
    fn plaintext_expose_secret_returns_original_bytes() {
        let p = Plaintext::new_within_module(vec![1u8, 2, 3, 4]);
        assert_eq!(p.expose_secret(), &[1u8, 2, 3, 4]);
    }

    // -----------------------------------------------------------------
    // Verified<T> (C-7 構築可視性)
    // -----------------------------------------------------------------

    #[test]
    fn verified_into_inner_returns_inner_value() {
        let v = Verified::new_from_aead_decrypt(42u32);
        assert_eq!(v.into_inner(), 42u32);
    }

    #[test]
    fn verified_as_inner_returns_reference_without_consuming() {
        let v = Verified::new_from_aead_decrypt("abc".to_string());
        assert_eq!(v.as_inner(), "abc");
        // as_inner は &self なので消費せず再アクセス可能
        assert_eq!(v.into_inner(), "abc".to_string());
    }

    #[test]
    fn verified_plaintext_debug_propagates_redacted_marker_from_inner() {
        let p = Plaintext::new_within_module(b"secret".to_vec());
        let v = Verified::new_from_aead_decrypt(p);
        let s = format!("{v:?}");
        assert!(s.contains("[REDACTED PLAINTEXT]"), "got: {s}");
        assert!(!s.contains("secret"), "got: {s}");
    }

    // -----------------------------------------------------------------
    // verify_aead_decrypt (caller-asserted marker)
    // -----------------------------------------------------------------

    #[test]
    fn verify_aead_decrypt_wraps_ok_value_into_verified() {
        let result: Result<Verified<u32>, ()> = verify_aead_decrypt(|| Ok(7));
        let v = result.unwrap();
        assert_eq!(v.into_inner(), 7);
    }

    #[test]
    fn verify_aead_decrypt_propagates_err_from_closure() {
        let result: Result<Verified<u32>, &'static str> = verify_aead_decrypt(|| Err("boom"));
        assert_eq!(result.unwrap_err(), "boom");
    }

    // -----------------------------------------------------------------
    // CryptoOutcome<T>
    // -----------------------------------------------------------------

    #[test]
    fn crypto_outcome_supports_all_documented_variants() {
        // 5 バリアント全て構築可能であること (実装抜け回帰防止)
        let _ = CryptoOutcome::<Plaintext>::TagMismatch;
        let _ = CryptoOutcome::<Plaintext>::NonceLimit;
        let _ = CryptoOutcome::<Plaintext>::KdfFailed(KdfErrorKind::Argon2id);
        let _ = CryptoOutcome::<Plaintext>::WeakPassword(Box::new(WeakPasswordFeedback::new(
            None,
            vec![],
        )));
        let _ = CryptoOutcome::<Plaintext>::Verified(Verified::new_from_aead_decrypt(
            Plaintext::new_within_module(vec![]),
        ));
    }

    #[test]
    fn crypto_outcome_match_with_verified_first_is_legal_but_discouraged() {
        // 型システムは Verified 第一パターンを許容する (lint 強制不可)。
        // 設計上の禁止は Boy Scout PR レビューで担保 (設計書 §設計意図)。
        let outcome: CryptoOutcome<u32> =
            CryptoOutcome::Verified(Verified::new_from_aead_decrypt(1));
        // `#[non_exhaustive]` の wildcard arm は外部 crate からの呼出時に必要だが、
        // 同一 crate 内では全バリアント列挙すれば exhaustive と扱われ、`_` は unreachable。
        // ここでは Verified 第一パターンが意味的に書けてしまうことを示す例として扱う。
        let _ = match outcome {
            CryptoOutcome::Verified(_) => "ok",
            CryptoOutcome::TagMismatch => "tag",
            CryptoOutcome::NonceLimit => "nonce",
            CryptoOutcome::KdfFailed(_) => "kdf",
            CryptoOutcome::WeakPassword(_) => "weak",
        };
    }
}
