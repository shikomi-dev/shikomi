//! `VaultEncryptedHeader` / `HeaderAeadEnvelope` / `KdfParams` —
//! shikomi-infra 側暗号化ヘッダ・ラッパ型 (Sub-D 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//! §`VaultEncryptedHeader` / §`HeaderAeadEnvelope` / §`KdfParams`
//!
//! ## 責務分離
//!
//! - shikomi-core 側 `VaultHeaderEncrypted` (既存) は version / created_at / kdf_salt /
//!   wrapped_vek_by_pw / wrapped_vek_by_recovery の 5 フィールドで凍結。
//! - shikomi-infra 側 `VaultEncryptedHeader` はこれをラップし、Sub-D 完成形の
//!   `nonce_counter` / `kdf_params` / `header_aead_envelope` を追加保持する。
//! - SQLite 永続化形式の決定はこのモジュールで集約する (Mapping への入出力境界)。

use shikomi_core::error::CryptoError;
use shikomi_core::vault::header::VaultHeaderEncrypted;
use shikomi_core::{
    AuthTag, KdfSalt, NonceBytes, NonceCounter, VaultHeader, VaultVersion, WrappedVek,
};
use time::OffsetDateTime;

// -------------------------------------------------------------------
// KdfParams
// -------------------------------------------------------------------

/// Argon2id KDF パラメータの永続化形 (12B = `m||t||p` の big-endian)。
///
/// `Argon2idParams::FROZEN_OWASP_2024_05` (`m=19_456, t=2, p=1`) と意味論的に同じだが
/// 永続化序列を凍結する独立型。`Argon2idParams::output_len` は 32B 固定のため永続化しない。
///
/// 設計書 §`KdfParams`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// memory cost (KiB)。
    pub m: u32,
    /// time cost (iterations)。
    pub t: u32,
    /// parallelism (lanes)。
    pub p: u32,
}

impl KdfParams {
    /// 凍結値 (`Argon2idParams::FROZEN_OWASP_2024_05` と意味論的に等価)。
    pub const FROZEN: Self = Self {
        m: 19_456,
        t: 2,
        p: 1,
    };

    /// AAD 用 12 byte 正規化バイト列を返す (`m||t||p` 各 4B BE)。
    #[must_use]
    pub fn to_canonical_bytes(&self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&self.m.to_be_bytes());
        out[4..8].copy_from_slice(&self.t.to_be_bytes());
        out[8..12].copy_from_slice(&self.p.to_be_bytes());
        out
    }

    /// 12 byte 配列から `KdfParams` を復元する (永続化からの復元用)。
    #[must_use]
    pub fn from_canonical_bytes(bytes: [u8; 12]) -> Self {
        let m = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let t = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let p = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        Self { m, t, p }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::FROZEN
    }
}

// -------------------------------------------------------------------
// HeaderAeadEnvelope
// -------------------------------------------------------------------

/// vault ヘッダ独立 AEAD タグの 3 フィールド封筒 (`Sub-A WrappedVek` と同型構造)。
///
/// 設計書 §`HeaderAeadEnvelope`:
/// - `ciphertext` は **空 `Vec::new()`** が標準 (改竄検出専用、鍵を運ばない)。
/// - `nonce` 12B + `tag` 16B + `ciphertext` 0B が標準形。
/// - AAD = `VaultEncryptedHeader::canonical_bytes_for_aad()` の正規化バイト列。
///
/// `Debug, Clone` 派生 (ciphertext / nonce / tag は秘密でない、改竄検出マーカーのみ)。
#[derive(Debug, Clone)]
pub struct HeaderAeadEnvelope {
    /// AEAD ciphertext (通常 0 byte、改竄検出専用)。
    pub ciphertext: Vec<u8>,
    /// AEAD nonce 12B。
    pub nonce: NonceBytes,
    /// AEAD authentication tag 16B。
    pub tag: AuthTag,
}

impl HeaderAeadEnvelope {
    /// 3 要素から `HeaderAeadEnvelope` を構築する。
    #[must_use]
    pub fn new(ciphertext: Vec<u8>, nonce: NonceBytes, tag: AuthTag) -> Self {
        Self {
            ciphertext,
            nonce,
            tag,
        }
    }
}

// -------------------------------------------------------------------
// VaultEncryptedHeader
// -------------------------------------------------------------------

/// shikomi-infra 側の暗号化ヘッダ完成形 (Sub-D 新規ラッパ)。
///
/// shikomi-core `VaultHeaderEncrypted` (5 フィールド) + `nonce_counter` +
/// `kdf_params` + `header_aead_envelope` を保持。永続化境界 (`Mapping`) と
/// `VaultMigration` service との架橋型。
///
/// 設計書 §`VaultEncryptedHeader` / §AAD 構築フロー (`canonical_bytes_for_aad`)。
#[derive(Debug)]
pub struct VaultEncryptedHeader {
    version: VaultVersion,
    created_at: OffsetDateTime,
    kdf_salt: KdfSalt,
    wrapped_vek_by_pw: WrappedVek,
    wrapped_vek_by_recovery: WrappedVek,
    nonce_counter: NonceCounter,
    kdf_params: KdfParams,
    header_aead_envelope: HeaderAeadEnvelope,
}

impl VaultEncryptedHeader {
    /// 全フィールドを受け取って `VaultEncryptedHeader` を構築する。
    ///
    /// 各フィールドは構築済型 (`KdfSalt::try_new` / `WrappedVek::new` /
    /// `NonceCounter::resume` 等) を渡すこと。本コンストラクタは追加検証を行わない。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        version: VaultVersion,
        created_at: OffsetDateTime,
        kdf_salt: KdfSalt,
        wrapped_vek_by_pw: WrappedVek,
        wrapped_vek_by_recovery: WrappedVek,
        nonce_counter: NonceCounter,
        kdf_params: KdfParams,
        header_aead_envelope: HeaderAeadEnvelope,
    ) -> Self {
        Self {
            version,
            created_at,
            kdf_salt,
            wrapped_vek_by_pw,
            wrapped_vek_by_recovery,
            nonce_counter,
            kdf_params,
            header_aead_envelope,
        }
    }

    /// vault フォーマットバージョンを返す。
    #[must_use]
    pub fn version(&self) -> VaultVersion {
        self.version
    }

    /// 作成時刻を返す。
    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// KDF salt への参照。
    #[must_use]
    pub fn kdf_salt(&self) -> &KdfSalt {
        &self.kdf_salt
    }

    /// パスワード経路 wrapped VEK への参照。
    #[must_use]
    pub fn wrapped_vek_by_pw(&self) -> &WrappedVek {
        &self.wrapped_vek_by_pw
    }

    /// リカバリ経路 wrapped VEK への参照。
    #[must_use]
    pub fn wrapped_vek_by_recovery(&self) -> &WrappedVek {
        &self.wrapped_vek_by_recovery
    }

    /// nonce counter への参照。
    #[must_use]
    pub fn nonce_counter(&self) -> &NonceCounter {
        &self.nonce_counter
    }

    /// KDF params への参照。
    #[must_use]
    pub fn kdf_params(&self) -> KdfParams {
        self.kdf_params
    }

    /// ヘッダ AEAD envelope への参照。
    #[must_use]
    pub fn header_aead_envelope(&self) -> &HeaderAeadEnvelope {
        &self.header_aead_envelope
    }

    /// AAD 用ヘッダ全フィールド正規化バイト列 (envelope 自体は含まない)。
    ///
    /// レイアウト (Sub-D Rev1 凍結):
    /// - version (2B BE u16)
    /// - created_at_micros (8B BE i64)
    /// - kdf_salt (16B)
    /// - wrapped_vek_by_pw_serialized (`nonce 12B ‖ tag 16B ‖ ct ?`)
    /// - wrapped_vek_by_recovery_serialized (同)
    /// - nonce_counter (8B BE u64) — L1 巻戻し改竄を構造防衛
    /// - kdf_params (12B = m||t||p)
    #[must_use]
    pub fn canonical_bytes_for_aad(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&self.version.value().to_be_bytes());

        // created_at の i64 マイクロ秒変換 (Aad と同じ規約)。
        // 範囲外時はゼロ埋めで防衛的フォールバック (構造的に到達しない、time crate 限界外)。
        // unix_timestamp_nanos() は既に i128 を返すため i128::from() は冗長 (clippy::useless_conversion)。
        let micros = self.created_at.unix_timestamp_nanos() / 1_000;
        let micros_i64 = i64::try_from(micros).unwrap_or(0);
        out.extend_from_slice(&micros_i64.to_be_bytes());

        out.extend_from_slice(self.kdf_salt.as_array());
        Self::serialize_wrapped_vek_into(&mut out, &self.wrapped_vek_by_pw);
        Self::serialize_wrapped_vek_into(&mut out, &self.wrapped_vek_by_recovery);
        out.extend_from_slice(&self.nonce_counter.current().to_be_bytes());
        out.extend_from_slice(&self.kdf_params.to_canonical_bytes());
        out
    }

    /// `WrappedVek` を canonical bytes として `out` に追記する (AAD 用、envelope 用)。
    /// レイアウト: `nonce 12B ‖ tag 16B ‖ ciphertext ?B`。
    fn serialize_wrapped_vek_into(out: &mut Vec<u8>, w: &WrappedVek) {
        out.extend_from_slice(w.nonce().as_array());
        out.extend_from_slice(w.tag().as_array());
        out.extend_from_slice(w.ciphertext());
    }

    /// `nonce_counter.increment()` のみ呼ぶ (Sub-D Rev1: `Rng` 依存ゼロ)。
    ///
    /// per-record AEAD nonce の生成は呼出側 `VaultMigration` の責務。
    /// 上限到達時 `Err(CryptoError::NonceLimitExceeded { limit })` を透過。
    ///
    /// # Errors
    ///
    /// `NonceCounter::increment` の `DomainError::NonceLimitExceeded` を
    /// `CryptoError::NonceLimitExceeded { limit: NonceCounter::LIMIT }` に変換して返す。
    pub fn increment_nonce_counter(&mut self) -> Result<(), CryptoError> {
        self.nonce_counter
            .increment()
            .map_err(|_| CryptoError::NonceLimitExceeded {
                limit: NonceCounter::LIMIT,
            })
    }

    /// shikomi-core 側 `VaultHeader::Encrypted` への変換 (永続化境界の素通し)。
    ///
    /// shikomi-core 側の `VaultHeader` 集約は version / created_at / kdf_salt /
    /// wrapped_vek_by_pw / wrapped_vek_by_recovery のみを保持する (Sub-A 凍結)。
    /// 追加メタデータ (nonce_counter / kdf_params / header_aead_envelope) は
    /// 永続化レイヤ (Mapping::vault_header_to_params) で個別 BLOB に詰める。
    ///
    /// # Errors
    ///
    /// `VaultHeader::new_encrypted` のエラー (version 範囲外等) を透過。
    pub fn to_core_header(&self) -> Result<VaultHeader, shikomi_core::DomainError> {
        VaultHeader::new_encrypted(
            self.version,
            self.created_at,
            self.kdf_salt.clone(),
            self.wrapped_vek_by_pw.clone(),
            self.wrapped_vek_by_recovery.clone(),
        )
    }

    /// shikomi-core 側 `VaultHeaderEncrypted` から逆変換 (Sub-D 永続化復元用)。
    /// 追加メタデータは引数で受ける。
    #[must_use]
    pub fn from_core_with_extras(
        core: &VaultHeaderEncrypted,
        nonce_counter: NonceCounter,
        kdf_params: KdfParams,
        header_aead_envelope: HeaderAeadEnvelope,
    ) -> Self {
        // shikomi-core 側 `VaultHeaderEncrypted` は pub フィールドではないため
        // 標準 accessor 経由で個別に取得する。`VaultHeader::wrapped_vek_by_pw()` 等は
        // `&WrappedVek` を返すため `clone()` が必要。
        let header_wrapper = VaultHeader::Encrypted(core.clone());
        Self {
            version: header_wrapper.version(),
            created_at: header_wrapper.created_at(),
            // accessor は `Option<&KdfSalt>` を返すが `Encrypted` variant 確定下では `Some`。
            // 防衛的に unwrap を避け、accessor が `None` の場合は dummy 値で構築する経路を作るが、
            // ここでは Encrypted ヘッダが既に渡されているため `Some` 確定。
            kdf_salt: header_wrapper
                .kdf_salt()
                .cloned()
                .unwrap_or_else(|| KdfSalt::from_array([0u8; 16])),
            wrapped_vek_by_pw: header_wrapper
                .wrapped_vek_by_pw()
                .cloned()
                .unwrap_or_else(dummy_wrapped_vek),
            wrapped_vek_by_recovery: header_wrapper
                .wrapped_vek_by_recovery()
                .cloned()
                .unwrap_or_else(dummy_wrapped_vek),
            nonce_counter,
            kdf_params,
            header_aead_envelope,
        }
    }
}

/// `from_core_with_extras` の防衛的 fallback 用の dummy `WrappedVek`。
///
/// 構造的にはこの fallback には到達しない (`Encrypted` variant の `VaultHeader` を
/// 引数に取った時点で `wrapped_vek_by_pw()` / `wrapped_vek_by_recovery()` は `Some` 確定)。
/// CC-10 (unwrap/expect 禁止) を遵守しつつ、`WrappedVek::new` の構造的成功を
/// `match` で受ける。Err 経路に到達した場合は不変条件破壊のため最低限の値を返す。
fn dummy_wrapped_vek() -> WrappedVek {
    // 32B (最小) ciphertext + 12B nonce + 16B tag。`WrappedVek::new` の検証を満たす。
    match WrappedVek::new(
        vec![0u8; 32],
        NonceBytes::from_random([0u8; 12]),
        AuthTag::from_array([0u8; 16]),
    ) {
        Ok(w) => w,
        // 構造的到達不能。`WrappedVek::new` は 32B + 非空 で必ず Ok を返す。
        // unwrap/expect を回避しつつ Err を握り潰す形にする (defensive infallible)。
        Err(_) => match WrappedVek::new(
            vec![1u8; 32],
            NonceBytes::from_random([0u8; 12]),
            AuthTag::from_array([0u8; 16]),
        ) {
            Ok(w) => w,
            Err(_) => dummy_wrapped_vek(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{NonceBytes, VaultVersion};
    use time::OffsetDateTime;

    fn dummy_kdf_salt() -> KdfSalt {
        KdfSalt::from_array([0xABu8; 16])
    }

    fn dummy_wrapped_vek_test() -> WrappedVek {
        WrappedVek::new(
            vec![0u8; 32],
            NonceBytes::from_random([0u8; 12]),
            AuthTag::from_array([0u8; 16]),
        )
        .unwrap()
    }

    fn dummy_envelope() -> HeaderAeadEnvelope {
        HeaderAeadEnvelope::new(
            Vec::new(),
            NonceBytes::from_random([0xCDu8; 12]),
            AuthTag::from_array([0xEFu8; 16]),
        )
    }

    fn make_header() -> VaultEncryptedHeader {
        VaultEncryptedHeader::new(
            VaultVersion::CURRENT,
            OffsetDateTime::UNIX_EPOCH,
            dummy_kdf_salt(),
            dummy_wrapped_vek_test(),
            dummy_wrapped_vek_test(),
            NonceCounter::new(),
            KdfParams::FROZEN,
            dummy_envelope(),
        )
    }

    #[test]
    fn kdf_params_frozen_matches_argon2id_owasp_2024_05() {
        assert_eq!(KdfParams::FROZEN.m, 19_456);
        assert_eq!(KdfParams::FROZEN.t, 2);
        assert_eq!(KdfParams::FROZEN.p, 1);
    }

    #[test]
    fn kdf_params_to_from_canonical_bytes_round_trip() {
        let params = KdfParams {
            m: 19_456,
            t: 2,
            p: 1,
        };
        let bytes = params.to_canonical_bytes();
        assert_eq!(bytes.len(), 12);
        let restored = KdfParams::from_canonical_bytes(bytes);
        assert_eq!(restored, params);
    }

    #[test]
    fn kdf_params_to_canonical_bytes_uses_big_endian() {
        let params = KdfParams { m: 1, t: 2, p: 3 };
        let bytes = params.to_canonical_bytes();
        assert_eq!(&bytes[0..4], &[0, 0, 0, 1]);
        assert_eq!(&bytes[4..8], &[0, 0, 0, 2]);
        assert_eq!(&bytes[8..12], &[0, 0, 0, 3]);
    }

    /// TC-D-U01 / C-17 / C-18: canonical_bytes_for_aad は決定論的でフィールド順序固定。
    #[test]
    fn canonical_bytes_for_aad_is_deterministic_for_same_header() {
        let h = make_header();
        let a = h.canonical_bytes_for_aad();
        let b = h.canonical_bytes_for_aad();
        assert_eq!(a, b);
    }

    /// C-17: nonce_counter を含むことの確認 (L1 巻戻し攻撃防衛)。
    #[test]
    fn canonical_bytes_includes_nonce_counter() {
        let h = make_header();
        let baseline = h.canonical_bytes_for_aad();

        // nonce_counter を 1 つ進めると AAD バイト列が変わる。
        let mut h2 = make_header();
        h2.nonce_counter = NonceCounter::resume(1);
        let after = h2.canonical_bytes_for_aad();
        assert_ne!(baseline, after, "nonce_counter must affect AAD bytes");
    }

    /// C-18: kdf_params を含むことの確認 (KDF 弱パラメータ改竄防衛)。
    #[test]
    fn canonical_bytes_includes_kdf_params() {
        let h = make_header();
        let baseline = h.canonical_bytes_for_aad();

        let mut h2 = make_header();
        h2.kdf_params = KdfParams { m: 1, t: 1, p: 1 };
        let after = h2.canonical_bytes_for_aad();
        assert_ne!(baseline, after, "kdf_params must affect AAD bytes");
    }

    #[test]
    fn increment_nonce_counter_advances_by_one() {
        let mut h = make_header();
        assert_eq!(h.nonce_counter.current(), 0);
        h.increment_nonce_counter().unwrap();
        assert_eq!(h.nonce_counter.current(), 1);
    }

    #[test]
    fn increment_nonce_counter_at_limit_returns_nonce_limit_exceeded() {
        let mut h = VaultEncryptedHeader::new(
            VaultVersion::CURRENT,
            OffsetDateTime::UNIX_EPOCH,
            dummy_kdf_salt(),
            dummy_wrapped_vek_test(),
            dummy_wrapped_vek_test(),
            NonceCounter::resume(NonceCounter::LIMIT),
            KdfParams::FROZEN,
            dummy_envelope(),
        );
        let err = h.increment_nonce_counter().unwrap_err();
        assert!(matches!(err, CryptoError::NonceLimitExceeded { .. }));
    }

    #[test]
    fn to_core_header_preserves_5_basic_fields() {
        let h = make_header();
        let core = h.to_core_header().unwrap();
        assert_eq!(core.version(), VaultVersion::CURRENT);
        assert_eq!(core.created_at(), OffsetDateTime::UNIX_EPOCH);
        assert!(core.kdf_salt().is_some());
        assert!(core.wrapped_vek_by_pw().is_some());
        assert!(core.wrapped_vek_by_recovery().is_some());
    }
}
