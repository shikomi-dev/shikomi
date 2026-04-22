//! ドメインエラー型。
//!
//! 全モジュールが返す `DomainError` と、各バリアントの詳細理由を列挙する。

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
    #[error("vault and record payload mode mismatch: {0}")]
    VaultConsistencyError(VaultConsistencyReason),

    /// MSG-DEV-005: `NonceCounter` が上限 (`u32::MAX`) に達した。rekey が必要。
    #[error("nonce counter exhausted; rekey required")]
    NonceOverflow,
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

    /// `WrappedVek` が空（0 バイト）。
    #[error("wrapped_vek is empty")]
    WrappedVekEmpty,

    /// `WrappedVek` が最小長未満（AES-GCM タグ 16 B 以上必要）。
    #[error("wrapped_vek is too short")]
    WrappedVekTooShort,
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
