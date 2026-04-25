//! 鍵階層型 — `Vek` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>`。
//!
//! - `Vek`: Vault Encryption Key 本体 (32 byte 固定)。
//! - `Kek<Kind>`: Key Encryption Key。phantom-typed で `KekKindPw` / `KekKindRecovery` を区別する。
//!   外部 crate での `KekKind` 実装は Sealed trait で禁止 (誤用混入の構造封鎖)。
//!
//! いずれも `secrecy::SecretBox<Zeroizing<[u8; 32]>>` を内包し、`Drop` 時に 32B を zeroize する。
//! `Clone` / `Copy` / `Display` / `serde::Serialize` / `PartialEq` / `Eq` は **意図的に未実装**
//! (契約 C-2/C-4/C-5)。`Debug` は `[REDACTED VEK]` / `[REDACTED KEK<Pw>]` /
//! `[REDACTED KEK<Recovery>]` の固定文字列を返す (契約 C-3)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/crypto-types.md`

use core::fmt;
use core::marker::PhantomData;

use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

/// 鍵バイト長 (AES-256 鍵長 = 32 byte)。
pub(crate) const KEY_LEN: usize = 32;

// -------------------------------------------------------------------
// Sealed trait — 外部 crate での KekKind 実装禁止
// -------------------------------------------------------------------

mod sealed {
    /// 外部 crate からの実装を禁止する Sealed trait。
    ///
    /// 鍵用途の追加は必ず `shikomi-core` 側で `KekKind` の実装を追加する形にする
    /// (Sub-A 設計改訂を経由)。
    pub trait Sealed {}
}

/// `Kek<Kind>` の用途マーカー trait。Sealed のため外部 crate からは実装できない。
pub trait KekKind: 'static + sealed::Sealed {}

/// マスターパスワード由来 KEK のマーカー (Argon2id 出力)。
pub struct KekKindPw;

/// リカバリ・ニーモニック由来 KEK のマーカー (PBKDF2-HMAC-SHA512 + HKDF-SHA256 出力)。
pub struct KekKindRecovery;

impl sealed::Sealed for KekKindPw {}
impl KekKind for KekKindPw {}
impl sealed::Sealed for KekKindRecovery {}
impl KekKind for KekKindRecovery {}

// -------------------------------------------------------------------
// Vek
// -------------------------------------------------------------------

/// Vault Encryption Key (32 byte AES-256 鍵)。
///
/// 内部表現は `SecretBox<Zeroizing<[u8; 32]>>`。`Drop` で 32B を確定的に zeroize する。
/// CSPRNG 由来の `[u8; 32]` を `from_array` で受け取って構築する (no-I/O 制約)。
///
/// # Forbidden traits (compile-time enforced)
///
/// `Clone`, `Copy`, `Display`, `serde::Serialize`, `serde::Deserialize`, `PartialEq`, `Eq` は
/// **意図的に未実装**。下記は `compile_fail` doc test で防御する。
///
/// ```compile_fail
/// use shikomi_core::crypto::Vek;
/// let v = Vek::from_array([0u8; 32]);
/// let _ = v.clone();
/// ```
///
/// ```compile_fail
/// use shikomi_core::crypto::Vek;
/// let v = Vek::from_array([0u8; 32]);
/// let _ = format!("{}", v);
/// ```
pub struct Vek {
    inner: SecretBox<Zeroizing<[u8; KEY_LEN]>>,
}

impl Vek {
    /// 32 byte の鍵バイト列から `Vek` を構築する。
    ///
    /// `bytes` は本関数のスコープ抜けで Rust の所有権ルールにより消去対象になる。
    /// 呼出側は CSPRNG から取得直後に本関数へ移動し、ローカル変数に長く保持しないこと。
    #[must_use]
    pub fn from_array(bytes: [u8; KEY_LEN]) -> Self {
        Self {
            inner: SecretBox::new(Box::new(Zeroizing::new(bytes))),
        }
    }

    /// crate 内部からのみ生バイト参照を取り出す。
    ///
    /// `pub(crate)` 可視性。`shikomi-infra` 等の外部 crate からは呼出不可
    /// (= 外部 crate に VEK 生バイトを渡す API は存在しない)。
    pub(crate) fn expose_within_crate(&self) -> &[u8; KEY_LEN] {
        self.inner.expose_secret().as_ref()
    }
}

impl fmt::Debug for Vek {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED VEK]")
    }
}

// -------------------------------------------------------------------
// Kek<Kind>
// -------------------------------------------------------------------

/// Key Encryption Key (32 byte)。phantom-typed で `KekKindPw` / `KekKindRecovery` を区別する。
///
/// `Kek<KekKindPw>` は Argon2id 由来の KEK、`Kek<KekKindRecovery>` は PBKDF2+HKDF 由来の KEK。
/// 同一バイト長だが用途が異なるため、関数シグネチャの取り違えをコンパイルエラーで弾く。
///
/// `Drop` で 32B を zeroize。`Clone` / `Display` / `Serialize` / `PartialEq` / `Eq` は未実装。
///
/// ```compile_fail
/// use shikomi_core::crypto::{Kek, KekKindPw, KekKindRecovery};
/// fn want_pw(_: &Kek<KekKindPw>) {}
/// let k = Kek::<KekKindRecovery>::from_array([0u8; 32]);
/// want_pw(&k); // 型不一致でコンパイルエラー (契約 C-6)
/// ```
pub struct Kek<Kind: KekKind> {
    inner: SecretBox<Zeroizing<[u8; KEY_LEN]>>,
    _kind: PhantomData<fn() -> Kind>,
}

impl<Kind: KekKind> Kek<Kind> {
    /// 32 byte の鍵バイト列から `Kek<Kind>` を構築する。
    #[must_use]
    pub fn from_array(bytes: [u8; KEY_LEN]) -> Self {
        Self {
            inner: SecretBox::new(Box::new(Zeroizing::new(bytes))),
            _kind: PhantomData,
        }
    }

    /// crate 内部からのみ生バイト参照を取り出す。
    pub(crate) fn expose_within_crate(&self) -> &[u8; KEY_LEN] {
        self.inner.expose_secret().as_ref()
    }
}

impl fmt::Debug for Kek<KekKindPw> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED KEK<Pw>]")
    }
}

impl fmt::Debug for Kek<KekKindRecovery> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED KEK<Recovery>]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Vek
    // -----------------------------------------------------------------

    #[test]
    fn vek_from_array_constructs_without_panic() {
        let _ = Vek::from_array([0u8; KEY_LEN]);
    }

    #[test]
    fn vek_debug_returns_redacted_fixed_string() {
        let v = Vek::from_array([0xAAu8; KEY_LEN]);
        let s = format!("{v:?}");
        assert_eq!(s, "[REDACTED VEK]");
    }

    #[test]
    fn vek_debug_does_not_expose_secret_bytes() {
        let v = Vek::from_array([0xAAu8; KEY_LEN]);
        let s = format!("{v:?}");
        assert!(!s.contains("AA"), "Debug must not expose secret bytes: {s}");
    }

    #[test]
    fn vek_expose_within_crate_returns_original_bytes() {
        let bytes = [0x42u8; KEY_LEN];
        let v = Vek::from_array(bytes);
        assert_eq!(v.expose_within_crate(), &bytes);
    }

    // -----------------------------------------------------------------
    // Kek<KekKindPw>
    // -----------------------------------------------------------------

    #[test]
    fn kek_pw_debug_returns_pw_marker() {
        let k = Kek::<KekKindPw>::from_array([0u8; KEY_LEN]);
        assert_eq!(format!("{k:?}"), "[REDACTED KEK<Pw>]");
    }

    #[test]
    fn kek_recovery_debug_returns_recovery_marker() {
        let k = Kek::<KekKindRecovery>::from_array([0u8; KEY_LEN]);
        assert_eq!(format!("{k:?}"), "[REDACTED KEK<Recovery>]");
    }

    #[test]
    fn kek_pw_expose_within_crate_returns_original_bytes() {
        let bytes = [0x33u8; KEY_LEN];
        let k = Kek::<KekKindPw>::from_array(bytes);
        assert_eq!(k.expose_within_crate(), &bytes);
    }

    #[test]
    fn kek_recovery_expose_within_crate_returns_original_bytes() {
        let bytes = [0x77u8; KEY_LEN];
        let k = Kek::<KekKindRecovery>::from_array(bytes);
        assert_eq!(k.expose_within_crate(), &bytes);
    }
}
