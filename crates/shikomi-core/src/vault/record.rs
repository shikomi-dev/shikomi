//! レコードおよび関連型（RecordKind / `RecordLabel` / `RecordPayload`）。

use time::OffsetDateTime;
use unicode_segmentation::UnicodeSegmentation;

use crate::error::{DomainError, InvalidRecordLabelReason, InvalidRecordPayloadReason};
use crate::secret::SecretString;
use crate::vault::crypto_data::{Aad, CipherText};
use crate::vault::id::RecordId;
use crate::vault::nonce::NonceBytes;
use crate::vault::protection_mode::ProtectionMode;

/// `RecordLabel` の grapheme cluster 最大数。
const LABEL_MAX_GRAPHEMES: usize = 255;

// -------------------------------------------------------------------
// ユーティリティ
// -------------------------------------------------------------------

/// `OffsetDateTime` をマイクロ秒精度に切り捨てる（サブマイクロ秒は切り捨て）。
///
/// `Record::new` / `Record::with_updated_*` が内部で呼び出す。
/// 永続化（SQLite RFC3339）と AAD 計算のラウンドトリップを保証するため。
fn truncate_to_microsecond(dt: OffsetDateTime) -> OffsetDateTime {
    // nanosecond() は [0, 999_999_999]。% 1_000 でサブマイクロ秒 ns を取り出す。
    let sub_micro_ns = i64::from(dt.nanosecond() % 1_000);
    dt - time::Duration::nanoseconds(sub_micro_ns)
}

// -------------------------------------------------------------------
// RecordKind
// -------------------------------------------------------------------

/// レコードの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// テキストレコード（URL・メモ等、機密度が低い）。
    Text,
    /// シークレットレコード（パスワード・鍵等、機密度が高い）。
    Secret,
}

// -------------------------------------------------------------------
// RecordLabel
// -------------------------------------------------------------------

/// レコードの表示名。
///
/// 以下の条件を満たす文字列のみ `try_new` を通過できる（Fail Fast）：
/// - 非空
/// - 禁止制御文字なし（U+0000〜U+001F のうち `\t`/`\n`/`\r` は許可、U+007F は禁止）
/// - grapheme cluster 数 ≤ 255
#[derive(Debug, Clone)]
pub struct RecordLabel {
    inner: String,
}

impl RecordLabel {
    /// 文字列から `RecordLabel` を構築する。
    ///
    /// # Errors
    /// - 空文字列: `DomainError::InvalidRecordLabel(Empty)`
    /// - 禁止制御文字: `DomainError::InvalidRecordLabel(ControlChar { position })`
    /// - 256 grapheme 以上: `DomainError::InvalidRecordLabel(TooLong { grapheme_count })`
    pub fn try_new(raw: String) -> Result<Self, DomainError> {
        if raw.is_empty() {
            return Err(DomainError::InvalidRecordLabel(
                InvalidRecordLabelReason::Empty,
            ));
        }

        // 禁止制御文字チェック
        for (pos, ch) in raw.char_indices() {
            if is_forbidden_control(ch) {
                return Err(DomainError::InvalidRecordLabel(
                    InvalidRecordLabelReason::ControlChar { position: pos },
                ));
            }
        }

        // grapheme cluster 数チェック
        let grapheme_count = raw.graphemes(true).count();
        if grapheme_count > LABEL_MAX_GRAPHEMES {
            return Err(DomainError::InvalidRecordLabel(
                InvalidRecordLabelReason::TooLong { grapheme_count },
            ));
        }

        Ok(Self { inner: raw })
    }

    /// 内包する文字列への参照を返す。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

/// 禁止制御文字かどうかを判定する。
///
/// U+0000〜U+001F のうち `\t`(U+0009) / `\n`(U+000A) / `\r`(U+000D) は許可。
/// U+007F (DEL) は禁止。
fn is_forbidden_control(ch: char) -> bool {
    match ch {
        '\t' | '\n' | '\r' => false,
        c if c <= '\u{001F}' => true,
        '\u{007F}' => true,
        _ => false,
    }
}

// -------------------------------------------------------------------
// RecordPayloadEncrypted
// -------------------------------------------------------------------

/// 暗号化バリアントのペイロード内部データ。
#[derive(Debug, Clone)]
pub struct RecordPayloadEncrypted {
    nonce: NonceBytes,
    ciphertext: CipherText,
    aad: Aad,
}

impl RecordPayloadEncrypted {
    /// 暗号化ペイロードを構築する。
    ///
    /// `nonce` / `ciphertext` / `aad` はそれぞれの `try_new` / `new` で検証済みの型を渡す。
    ///
    /// # Errors
    /// 現時点では `nonce` / `ciphertext` の検証は各型の `try_new` で行うため、
    /// この関数自体は `Ok` を返す。将来の追加検証のために `Result` を維持する。
    pub fn new(nonce: NonceBytes, ciphertext: CipherText, aad: Aad) -> Result<Self, DomainError> {
        Ok(Self {
            nonce,
            ciphertext,
            aad,
        })
    }

    /// nonce への参照を返す。
    #[must_use]
    pub fn nonce(&self) -> &NonceBytes {
        &self.nonce
    }

    /// ciphertext への参照を返す。
    #[must_use]
    pub fn ciphertext(&self) -> &CipherText {
        &self.ciphertext
    }

    /// AAD への参照を返す。
    #[must_use]
    pub fn aad(&self) -> &Aad {
        &self.aad
    }
}

// -------------------------------------------------------------------
// RecordPayload
// -------------------------------------------------------------------

/// レコードのペイロード。平文と暗号化を enum バリアントで排他する。
#[derive(Debug, Clone)]
pub enum RecordPayload {
    /// 平文ペイロード。SecretString に保持する。
    Plaintext(SecretString),
    /// 暗号化ペイロード（nonce + ciphertext + AAD）。
    Encrypted(RecordPayloadEncrypted),
}

impl RecordPayload {
    /// このペイロードが対応する `ProtectionMode` を返す。
    #[must_use]
    pub fn variant_mode(&self) -> ProtectionMode {
        match self {
            Self::Plaintext(_) => ProtectionMode::Plaintext,
            Self::Encrypted(_) => ProtectionMode::Encrypted,
        }
    }
}

// -------------------------------------------------------------------
// Record
// -------------------------------------------------------------------

/// vault 内のレコードエンティティ。
///
/// `Record::new` に渡す引数は全て検証済み型のみ受け付けるため、
/// 構築自体は失敗しない（Fail Fast は各引数の型構築時に行われる）。
#[derive(Debug, Clone)]
pub struct Record {
    id: RecordId,
    kind: RecordKind,
    label: RecordLabel,
    payload: RecordPayload,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl Record {
    /// レコードを構築する。
    ///
    /// `created_at = updated_at = now`（マイクロ秒精度に切り捨て済み）。
    #[must_use]
    pub fn new(
        id: RecordId,
        kind: RecordKind,
        label: RecordLabel,
        payload: RecordPayload,
        now: OffsetDateTime,
    ) -> Self {
        let ts = truncate_to_microsecond(now);
        Self {
            id,
            kind,
            label,
            payload,
            created_at: ts,
            updated_at: ts,
        }
    }

    /// レコード ID への参照を返す。
    #[must_use]
    pub fn id(&self) -> &RecordId {
        &self.id
    }

    /// レコード種別を返す。
    #[must_use]
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// ラベルへの参照を返す。
    #[must_use]
    pub fn label(&self) -> &RecordLabel {
        &self.label
    }

    /// ペイロードへの参照を返す。
    #[must_use]
    pub fn payload(&self) -> &RecordPayload {
        &self.payload
    }

    /// 作成時刻を返す（マイクロ秒精度）。
    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// 最終更新時刻を返す（マイクロ秒精度）。
    #[must_use]
    pub fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }

    /// ラベルを更新した新しい `Record` を返す（self を消費）。
    ///
    /// # Errors
    /// `now < self.created_at` の場合 `DomainError::VaultConsistencyError(InvalidUpdatedAt)` を返す。
    pub fn with_updated_label(
        mut self,
        label: RecordLabel,
        now: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let ts = truncate_to_microsecond(now);
        if ts < self.created_at {
            return Err(DomainError::VaultConsistencyError(
                crate::error::VaultConsistencyReason::InvalidUpdatedAt,
            ));
        }
        self.label = label;
        self.updated_at = ts;
        Ok(self)
    }

    /// 永続化層からレコードを復元する（rehydration コンストラクタ）。
    ///
    /// `Record::new` と異なり `created_at` / `updated_at` を引数から直接設定する。
    /// サブマイクロ秒成分はマイクロ秒精度に切り捨てる。
    /// 副作用なし・ビジネスロジックなし。時刻順序の検証のみ行う。
    ///
    /// # Errors
    /// `updated_at < created_at` の場合 `DomainError::VaultConsistencyError(InvalidUpdatedAt)`
    pub fn rehydrate(
        id: RecordId,
        kind: RecordKind,
        label: RecordLabel,
        payload: RecordPayload,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let created_at = truncate_to_microsecond(created_at);
        let updated_at = truncate_to_microsecond(updated_at);
        if updated_at < created_at {
            return Err(DomainError::VaultConsistencyError(
                crate::error::VaultConsistencyReason::InvalidUpdatedAt,
            ));
        }
        Ok(Self {
            id,
            kind,
            label,
            payload,
            created_at,
            updated_at,
        })
    }

    /// ペイロードを更新した新しい `Record` を返す（self を消費）。
    ///
    /// 内部で `updated_at` をマイクロ秒精度に切り捨てる。
    ///
    /// # Errors
    /// `now < self.created_at` の場合 `DomainError::VaultConsistencyError(InvalidUpdatedAt)` を返す。
    pub fn with_updated_payload(
        mut self,
        payload: RecordPayload,
        now: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let ts = truncate_to_microsecond(now);
        if ts < self.created_at {
            return Err(DomainError::VaultConsistencyError(
                crate::error::VaultConsistencyReason::InvalidUpdatedAt,
            ));
        }
        self.payload = payload;
        self.updated_at = ts;
        Ok(self)
    }
}

// 使われていない import を suppres するための re-export
#[allow(unused_imports)]
use InvalidRecordPayloadReason as _;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{DomainError, InvalidRecordLabelReason, InvalidRecordPayloadReason};
    use crate::secret::SecretString;
    use crate::vault::crypto_data::{Aad, CipherText};
    use crate::vault::id::RecordId;
    use crate::vault::nonce::NonceBytes;
    use crate::vault::version::VaultVersion;
    use time::OffsetDateTime;

    fn make_id() -> RecordId {
        RecordId::new(uuid::Uuid::now_v7()).unwrap()
    }

    fn make_aad() -> Aad {
        let id = make_id();
        Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap()
    }

    // --- TC-U05: RecordLabel ---

    #[test]
    fn test_record_label_try_new_one_char_ok() {
        assert!(RecordLabel::try_new("A".to_string()).is_ok());
    }

    #[test]
    fn test_record_label_try_new_255_graphemes_ok() {
        let s = "あ".repeat(255);
        assert!(RecordLabel::try_new(s).is_ok());
    }

    #[test]
    fn test_record_label_try_new_256_graphemes_returns_too_long() {
        let s = "あ".repeat(256);
        let err = RecordLabel::try_new(s).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordLabel(InvalidRecordLabelReason::TooLong {
                grapheme_count: 256
            })
        ));
    }

    #[test]
    fn test_record_label_try_new_empty_returns_empty_error() {
        let err = RecordLabel::try_new("".to_string()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordLabel(InvalidRecordLabelReason::Empty)
        ));
    }

    #[test]
    fn test_record_label_try_new_nul_char_returns_control_char_error() {
        let err = RecordLabel::try_new("\x00A".to_string()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { position: 0 })
        ));
    }

    #[test]
    fn test_record_label_try_new_us1f_returns_control_char_error() {
        let err = RecordLabel::try_new("\x1FA".to_string()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
        ));
    }

    #[test]
    fn test_record_label_try_new_del_returns_control_char_error() {
        let err = RecordLabel::try_new("\x7FA".to_string()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
        ));
    }

    #[test]
    fn test_record_label_try_new_tab_is_allowed() {
        assert!(RecordLabel::try_new("A\tB".to_string()).is_ok());
    }

    #[test]
    fn test_record_label_try_new_newline_is_allowed() {
        assert!(RecordLabel::try_new("A\nB".to_string()).is_ok());
    }

    #[test]
    fn test_record_label_try_new_carriage_return_is_allowed() {
        assert!(RecordLabel::try_new("A\rB".to_string()).is_ok());
    }

    #[test]
    fn test_record_label_as_str_returns_stored_string() {
        let label = RecordLabel::try_new("Test".to_string()).unwrap();
        assert_eq!(label.as_str(), "Test");
    }

    // --- TC-U06: RecordPayload ---

    #[test]
    fn test_record_payload_plaintext_holds_secret_string() {
        let payload = RecordPayload::Plaintext(SecretString::from_string("s".to_string()));
        assert!(matches!(payload, RecordPayload::Plaintext(_)));
        assert_eq!(payload.variant_mode(), ProtectionMode::Plaintext);
    }

    #[test]
    fn test_record_payload_encrypted_new_with_valid_args_ok() {
        let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
        let cipher = CipherText::try_new(vec![1u8; 32].into_boxed_slice()).unwrap();
        let aad = make_aad();
        assert!(RecordPayloadEncrypted::new(nonce, cipher, aad).is_ok());
    }

    #[test]
    fn test_nonce_bytes_try_new_with_11_bytes_returns_nonce_length_error() {
        // RecordPayloadEncrypted requires NonceBytes; NonceBytes rejects 11-byte input
        let err = NonceBytes::try_new(&[0u8; 11]).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::NonceLength {
                expected: 12,
                got: 11
            })
        ));
    }

    #[test]
    fn test_cipher_text_try_new_with_empty_bytes_returns_cipher_text_empty_error() {
        let err = CipherText::try_new(vec![].into_boxed_slice()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidRecordPayload(InvalidRecordPayloadReason::CipherTextEmpty)
        ));
    }

    #[test]
    fn test_record_payload_plaintext_variant_mode_is_plaintext() {
        let payload = RecordPayload::Plaintext(SecretString::from_string("s".to_string()));
        assert_eq!(payload.variant_mode(), ProtectionMode::Plaintext);
    }

    #[test]
    fn test_record_payload_encrypted_variant_mode_is_encrypted() {
        let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
        let cipher = CipherText::try_new(vec![1u8; 32].into_boxed_slice()).unwrap();
        let aad = make_aad();
        let enc = RecordPayloadEncrypted::new(nonce, cipher, aad).unwrap();
        let payload = RecordPayload::Encrypted(enc);
        assert_eq!(payload.variant_mode(), ProtectionMode::Encrypted);
    }

    // --- TC-U12: Record timestamp subsecond rounding ---

    #[test]
    fn test_record_new_truncates_created_at_to_microseconds() {
        // 123_456_789 ns = 0.123456789s → 789ns sub-µs component should be truncated
        let now = OffsetDateTime::from_unix_timestamp_nanos(123_456_789).unwrap();
        let record = Record::new(
            make_id(),
            RecordKind::Text,
            RecordLabel::try_new("label".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
            now,
        );
        assert_eq!(
            record.created_at().nanosecond() % 1_000,
            0,
            "789ns sub-µs should have been truncated to 000ns"
        );
    }

    #[test]
    fn test_record_new_updated_at_equals_created_at_and_is_microsecond_precision() {
        let now = OffsetDateTime::from_unix_timestamp_nanos(123_456_789).unwrap();
        let record = Record::new(
            make_id(),
            RecordKind::Text,
            RecordLabel::try_new("label".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
            now,
        );
        assert_eq!(record.updated_at().nanosecond() % 1_000, 0);
        assert_eq!(record.updated_at(), record.created_at());
    }

    // --- TC-U13: Record::rehydrate — updated_at < created_at → InvalidUpdatedAt ---

    #[test]
    fn test_record_rehydrate_updated_at_before_created_at_returns_invalid_updated_at() {
        let created_at = OffsetDateTime::from_unix_timestamp(1_000_000).unwrap();
        let updated_at = OffsetDateTime::from_unix_timestamp(999_999).unwrap(); // 1秒前
        let result = Record::rehydrate(
            make_id(),
            RecordKind::Secret,
            RecordLabel::try_new("label".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
            created_at,
            updated_at,
        );
        assert!(
            matches!(
                result,
                Err(DomainError::VaultConsistencyError(
                    crate::error::VaultConsistencyReason::InvalidUpdatedAt
                ))
            ),
            "updated_at < created_at は InvalidUpdatedAt を期待したが: {result:?}"
        );
    }

    // --- TC-U14: Record::rehydrate — サブマイクロ秒成分切り捨て ---

    #[test]
    fn test_record_rehydrate_truncates_subsecond_to_microseconds() {
        // 789 ns のサブマイクロ秒成分を持つタイムスタンプ
        let created_at =
            OffsetDateTime::from_unix_timestamp_nanos(1_000_000_000_123_456_789).unwrap();
        let updated_at =
            OffsetDateTime::from_unix_timestamp_nanos(1_000_000_001_987_654_321).unwrap();
        let record = Record::rehydrate(
            make_id(),
            RecordKind::Secret,
            RecordLabel::try_new("label".to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string("value".to_string())),
            created_at,
            updated_at,
        )
        .expect("rehydrate が失敗した");

        assert_eq!(
            record.created_at().nanosecond() % 1_000,
            0,
            "created_at のサブマイクロ秒成分が切り捨てられていない"
        );
        assert_eq!(
            record.updated_at().nanosecond() % 1_000,
            0,
            "updated_at のサブマイクロ秒成分が切り捨てられていない"
        );
        assert!(
            record.updated_at() >= record.created_at(),
            "切り捨て後も updated_at >= created_at を保持すべき"
        );
    }
}
