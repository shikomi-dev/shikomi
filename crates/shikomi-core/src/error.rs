//! ドメインエラー型。
//!
//! 全モジュールが返す `DomainError` と、各バリアントの詳細理由を列挙する。
//! 暗号特化エラーは `CryptoError` に集約し、`DomainError::Crypto` で内包する。

use thiserror::Error;

// -------------------------------------------------------------------
// DomainError
// -------------------------------------------------------------------

/// shikomi-core ドメイン層が返す統一エラー型。
///
/// 各バリアントは「開発者向けエラー文面（MSG-DEV-001〜008）」を
/// `Display` として持つ。CLI/GUI 層で i18n 写像を行うこと。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DomainError {
    /// MSG-DEV-001: 未知の保護モード文字列。
    #[error("unknown protection mode: {0}")]
    InvalidProtectionMode(String),

    /// MSG-DEV-002: 非対応の vault バージョン。
    #[error("unsupported vault version: {0}")]
    UnsupportedVaultVersion(u16),

    /// MSG-DEV-008: vault ヘッダの不整合。
    #[error("invalid vault header: {0}")]
    InvalidVaultHeader(#[from] InvalidVaultHeaderReason),

    /// MSG-DEV-006: 不正なレコード ID。
    #[error("invalid record id: {0}")]
    InvalidRecordId(InvalidRecordIdReason),

    /// MSG-DEV-003: 不正なレコードラベル。
    #[error("invalid record label: {0}")]
    InvalidRecordLabel(InvalidRecordLabelReason),

    /// MSG-DEV-007: 不正なレコードペイロード。
    #[error("invalid record payload: {0}")]
    InvalidRecordPayload(InvalidRecordPayloadReason),

    /// MSG-DEV-004: vault とレコードのモード不整合、または状態遷移違反。
    /// 詳細文言は内包する `VaultConsistencyReason` の `Display` に委譲する。
    #[error("{0}")]
    VaultConsistencyError(VaultConsistencyReason),

    /// MSG-DEV-005: `NonceCounter` が上限 (`1u64 << 32`) に達した。
    /// rekey が必要 (NIST SP 800-38D §8.3 random nonce birthday bound)。
    ///
    /// Sub-A 凍結文言。Sub-0 で `NonceLimitExceeded` に統一。
    #[error("nonce counter limit exceeded; rekey required")]
    NonceLimitExceeded,

    /// 暗号系エラーを内包する透過バリアント。
    ///
    /// 暗号操作 (KDF / AEAD / 強度ゲート) の失敗は `CryptoError` 側で詳細化し、
    /// `DomainError` はその通過点として動作する。
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

// -------------------------------------------------------------------
// 付随 Reason 列挙
// -------------------------------------------------------------------

/// `DomainError::InvalidVaultHeader` の詳細理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidVaultHeaderReason {
    /// KDF ソルトのバイト長が期待値と異なる。
    #[error("kdf_salt length mismatch: expected {expected}, got {got}")]
    KdfSaltLength { expected: usize, got: usize },

    /// `WrappedVek` の `ciphertext` が空（0 バイト）。
    #[error("wrapped_vek ciphertext is empty")]
    WrappedVekEmpty,

    /// `WrappedVek` の `ciphertext` が最小長未満（VEK 32B が最小）。
    #[error("wrapped_vek ciphertext is too short")]
    WrappedVekTooShort,

    /// `AuthTag` のバイト長が 16 ではない（AES-GCM 認証タグ仕様）。
    #[error("auth_tag length mismatch: expected 16, got {got}")]
    AuthTagLength { got: usize },
}

/// `DomainError::InvalidRecordId` の詳細理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidRecordIdReason {
    /// UUID のバージョンが v7 ではない。
    #[error("expected UUIDv7, got version {actual}")]
    WrongVersion { actual: u8 },

    /// nil UUID（全ゼロ）は `RecordId` に使用できない。
    #[error("nil UUID is not allowed as RecordId")]
    NilUuid,

    /// 文字列の UUID パース失敗。
    #[error("failed to parse UUID: {0}")]
    ParseError(String),
}

/// `DomainError::InvalidRecordLabel` の詳細理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidRecordLabelReason {
    /// 空文字列。
    #[error("label must not be empty")]
    Empty,

    /// 禁止制御文字を含む（U+0000〜U+001F の \t/\n/\r 以外、または U+007F）。
    #[error("label contains a forbidden control character at byte position {position}")]
    ControlChar { position: usize },

    /// grapheme cluster 数が 255 を超える。
    #[error("label is too long: {grapheme_count} graphemes (max 255)")]
    TooLong { grapheme_count: usize },
}

/// `DomainError::InvalidRecordPayload` の詳細理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidRecordPayloadReason {
    /// nonce のバイト長が 12 ではない。
    #[error("nonce length mismatch: expected {expected}, got {got}")]
    NonceLength { expected: usize, got: usize },

    /// ciphertext が空（0 バイト）。
    #[error("ciphertext must not be empty")]
    CipherTextEmpty,

    /// AAD に必要なフィールドが欠けている。
    #[error("aad missing field: {0}")]
    AadMissingField(String),

    /// AAD の `record_created_at` を i64 マイクロ秒に変換したとき範囲外。
    #[error("aad timestamp is out of i64 microseconds range")]
    AadTimestampOutOfRange,
}

/// `DomainError::VaultConsistencyError` の詳細理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VaultConsistencyReason {
    /// vault の保護モードとレコードペイロードのモードが一致しない。
    #[error("vault is in {vault_mode:?} mode but record payload is {record_mode:?}")]
    ModeMismatch {
        vault_mode: crate::vault::ProtectionMode,
        record_mode: crate::vault::ProtectionMode,
    },

    /// 同一 `RecordId` が既に vault に存在する。
    #[error("duplicate record id: {0:?}")]
    DuplicateId(crate::vault::RecordId),

    /// 平文 vault に対して rekey を実行しようとした。
    #[error("rekey is not applicable to a plaintext vault")]
    RekeyInPlaintextMode,

    /// rekey 中の部分再暗号化失敗（呼び出し元でトランザクションをロールバックすること）。
    #[error("rekey partially failed; vault is in inconsistent state")]
    RekeyPartialFailure,

    /// 指定された `RecordId` が vault に存在しない。
    #[error("record not found: {0:?}")]
    RecordNotFound(crate::vault::RecordId),

    /// `updated_at` が `created_at` より前の時刻になる更新は拒否。
    #[error("updated_at must not precede created_at")]
    InvalidUpdatedAt,
}

// -------------------------------------------------------------------
// CryptoError
// -------------------------------------------------------------------

/// 暗号操作（KDF / AEAD / 強度ゲート / Verified 構築）に特化したエラー。
///
/// `DomainError::Crypto(CryptoError)` で `DomainError` に内包される。
/// 詳細は `docs/features/vault-encryption/detailed-design/errors-and-contracts.md` を参照。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// `MasterPassword::new` が `PasswordStrengthGate::validate` で拒否された。
    /// 内包する `WeakPasswordFeedback` をそのまま MSG-S08 に渡す（Fail Kindly）。
    #[error("weak password rejected by strength gate")]
    WeakPassword(crate::crypto::password::WeakPasswordFeedback),

    /// AEAD 復号タグ検証失敗（vault.db 改竄の可能性）。MSG-S10 経路。
    #[error("AEAD authentication tag verification failed")]
    AeadTagMismatch,

    /// `NonceCounter::increment` が上限到達。`rekey` 強制（NIST SP 800-38D §8.3）。MSG-S11 経路。
    #[error("nonce counter exceeded {limit}; rekey required")]
    NonceLimitExceeded {
        /// 上限値 (`1u64 << 32`)。
        limit: u64,
    },

    /// KDF 計算失敗（Argon2id / PBKDF2 / HKDF）。Sub-B が source エラーを内包する。
    #[error("KDF computation failed: {kind:?}")]
    KdfFailed {
        /// 失敗した KDF の種別。
        kind: KdfErrorKind,
        /// 元エラー（Sub-B 実装が `Box<dyn>` 化して内包）。
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// `Plaintext` を `Verified<_>` 経由なしで構築しようとした runtime 検出。
    /// 通常は `pub(in crate::crypto::verified)` 可視性でコンパイル時に防がれる。
    #[error("Plaintext requires Verified<_> wrapper")]
    VerifyRequired,
}

/// `CryptoError::KdfFailed` の KDF 種別を表す。Sub-B で source エラーを差別化する用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KdfErrorKind {
    /// Argon2id (Master Password 由来 KEK).
    Argon2id,
    /// PBKDF2-HMAC-SHA512 (BIP-39 mnemonic → seed).
    Pbkdf2,
    /// HKDF-SHA256 (PBKDF2 seed → KEK_recovery).
    Hkdf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::protection_mode::ProtectionMode;

    #[test]
    fn test_display_invalid_protection_mode_contains_keyword() {
        let err = DomainError::InvalidProtectionMode("x".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("unknown protection mode"), "got: {msg}");
    }

    #[test]
    fn test_display_unsupported_vault_version_contains_version_number() {
        let err = DomainError::UnsupportedVaultVersion(99);
        let msg = format!("{err}");
        assert!(
            msg.contains("unsupported vault version") && msg.contains("99"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_display_invalid_record_label_contains_keyword() {
        let err = DomainError::InvalidRecordLabel(InvalidRecordLabelReason::Empty);
        let msg = format!("{err}");
        assert!(msg.contains("invalid record label"), "got: {msg}");
    }

    #[test]
    fn test_display_vault_consistency_mode_mismatch_contains_keywords() {
        let err = DomainError::VaultConsistencyError(VaultConsistencyReason::ModeMismatch {
            vault_mode: ProtectionMode::Plaintext,
            record_mode: ProtectionMode::Encrypted,
        });
        let msg = format!("{err}");
        assert!(msg.contains("vault") && msg.contains("mode"), "got: {msg}");
    }

    #[test]
    fn test_display_vault_consistency_duplicate_id_does_not_contain_mode_mismatch() {
        use crate::vault::RecordId;
        use uuid::Uuid;
        let id = RecordId::new(Uuid::now_v7()).unwrap();
        let err = DomainError::VaultConsistencyError(VaultConsistencyReason::DuplicateId(id));
        let msg = format!("{err}");
        assert!(
            !msg.contains("mode mismatch"),
            "DuplicateId should not contain 'mode mismatch', got: {msg}"
        );
        assert!(msg.contains("duplicate"), "got: {msg}");
    }

    #[test]
    fn test_display_nonce_limit_exceeded_contains_keyword() {
        let err = DomainError::NonceLimitExceeded;
        let msg = format!("{err}");
        assert!(msg.contains("nonce counter limit exceeded"), "got: {msg}");
    }

    #[test]
    fn test_display_invalid_record_id_contains_keyword() {
        let err = DomainError::InvalidRecordId(InvalidRecordIdReason::NilUuid);
        let msg = format!("{err}");
        assert!(msg.contains("invalid record id"), "got: {msg}");
    }

    #[test]
    fn test_display_invalid_record_payload_contains_keyword() {
        let err = DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::CipherTextEmpty);
        let msg = format!("{err}");
        assert!(msg.contains("invalid record payload"), "got: {msg}");
    }

    #[test]
    fn test_display_invalid_vault_header_contains_keyword() {
        let err = DomainError::InvalidVaultHeader(InvalidVaultHeaderReason::KdfSaltLength {
            expected: 16,
            got: 15,
        });
        let msg = format!("{err}");
        assert!(msg.contains("invalid vault header"), "got: {msg}");
    }

    #[test]
    fn test_crypto_error_aead_tag_mismatch_displays_known_phrase() {
        let err = CryptoError::AeadTagMismatch;
        let msg = format!("{err}");
        assert!(msg.contains("AEAD"), "got: {msg}");
    }

    #[test]
    fn test_crypto_error_nonce_limit_exceeded_includes_limit_value() {
        let err = CryptoError::NonceLimitExceeded { limit: 1u64 << 32 };
        let msg = format!("{err}");
        assert!(msg.contains(&(1u64 << 32).to_string()), "got: {msg}");
    }

    #[test]
    fn test_crypto_error_into_domain_error_via_from() {
        let crypto = CryptoError::AeadTagMismatch;
        let domain: DomainError = crypto.into();
        assert!(matches!(domain, DomainError::Crypto(_)));
    }
}
