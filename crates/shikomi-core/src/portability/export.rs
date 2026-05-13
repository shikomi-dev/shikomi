//! エクスポート用ドメイン型。
//!
//! `ExportRecordPayload`: Secret kind のリダクション表現（tagged union）。
//! `ExportRecord`: 1 レコードの JSON シリアライズ可能な値オブジェクト。
//! `ExportPayload`: エクスポートファイル全体のルート JSON オブジェクト。
//!
//! # expose_secret 呼び出し経路
//! `ExportRecordPayload::from_record` がこのモジュール内で唯一の `expose_secret` 呼び出し箇所。
//! `shikomi-cli` 側から `expose_secret` を直接呼ばせないための集約点
//! （`cli-vault-commands/basic-design/security.md §expose_secret 経路監査` 準拠）。

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{Record, RecordKind, RecordPayload};

use super::error::ExportError;

// -------------------------------------------------------------------
// EXPORT_FORMAT_VERSION
// -------------------------------------------------------------------

/// エクスポートファイルのフォーマットバージョン定数。
///
/// `format_version: 1` で凍結（`basic-design.md §REQ-DP-002`）。
/// 将来の形式変更はこの定数をバンプし、`ImportValidator` の検証を更新する。
pub const EXPORT_FORMAT_VERSION: u32 = 1;

// -------------------------------------------------------------------
// ExportRecordPayload
// -------------------------------------------------------------------

/// レコードペイロードのエクスポート表現（tagged union）。
///
/// `payload_redacted: bool` フラットフィールドではなく tagged union を採用する理由:
/// `"[REDACTED]"` 文字列リテラルと平文値の衝突を構造的に排除するため。
///
/// `Locked` バリアントは存在しない。`RecordPayload::Encrypted` は `from_record` で
/// 即時 `Err(ExportError::VaultLocked)` にするため、この型に到達しない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportRecordPayload {
    /// 平文ペイロード。JSON: `{ "kind": "plaintext", "value": "..." }`
    Plaintext {
        /// 平文値。
        value: String,
    },
    /// Secret kind のリダクト表現。JSON: `{ "kind": "redacted" }`
    Redacted,
}

impl ExportRecordPayload {
    /// `RecordPayload` から `ExportRecordPayload` へ変換する。
    ///
    /// # 処理順序（設計書 `basic-design.md §REQ-DP-001`）
    /// 1. `payload` が `RecordPayload::Encrypted` → 即座に `Err(ExportError::VaultLocked)`（Fail Fast）
    /// 2. `kind == RecordKind::Secret` かつ `include_secrets == false` → `Ok(Redacted)`
    /// 3. 上記以外 → `expose_secret()` を呼び出し `Ok(Plaintext { value })`
    ///
    /// # expose_secret 呼び出し集約
    /// `expose_secret` の呼び出しはこの関数にのみ閉じる。
    ///
    /// # Errors
    /// `payload` が `RecordPayload::Encrypted` の場合 `ExportError::VaultLocked`。
    pub fn from_record(
        payload: &RecordPayload,
        kind: RecordKind,
        include_secrets: bool,
    ) -> Result<Self, ExportError> {
        // Fail Fast: Encrypted ペイロードは即時エラー（release ビルドでも動作）
        let plaintext_secret = match payload {
            RecordPayload::Encrypted(_) => return Err(ExportError::VaultLocked),
            RecordPayload::Plaintext(s) => s,
        };

        if kind == RecordKind::Secret && !include_secrets {
            return Ok(Self::Redacted);
        }

        // expose_secret の呼び出しはここにのみ閉じる
        Ok(Self::Plaintext {
            value: plaintext_secret.expose_secret().to_owned(),
        })
    }
}

// -------------------------------------------------------------------
// ExportRecord
// -------------------------------------------------------------------

/// 1 レコードのエクスポート表現（値オブジェクト）。
///
/// 全フィールドに `Serialize` / `Deserialize` を実装する。
/// `Deserialize` は `ImportRecord = type alias of ExportRecord` での roundtrip に使用する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRecord {
    /// UUID v7 文字列。
    pub id: String,
    /// レコード種別（`"text"` / `"secret"`）。
    pub kind: RecordKind,
    /// ラベル文字列。
    pub label: String,
    /// ペイロード（tagged union）。
    pub payload: ExportRecordPayload,
    /// 作成時刻（RFC 3339 マイクロ秒精度）。
    pub created_at: String,
    /// 最終更新時刻（RFC 3339 マイクロ秒精度）。
    pub updated_at: String,
    /// ホットキー正規化文字列（`"alt+ctrl+1"` 形式）または `null`。
    ///
    /// 文字列形式の SSoT は `daemon-hotkey-clipboard/domain/basic-design.md §文字列表現`。
    /// `format_version: 1` で凍結。
    pub hotkey: Option<String>,
}

impl TryFrom<(&Record, bool)> for ExportRecord {
    type Error = ExportError;

    /// `Record` から `ExportRecord` へ変換する。
    ///
    /// `bool` は `include_secrets`（`true` = Secret kind も平文で export）。
    ///
    /// `From` ではなく `TryFrom` を使う理由:
    /// `ExportRecordPayload::from_record` が `Result` を返すため。
    ///
    /// # Errors
    /// `record.payload()` が `RecordPayload::Encrypted` の場合 `ExportError::VaultLocked`。
    fn try_from((record, include_secrets): (&Record, bool)) -> Result<Self, Self::Error> {
        let payload =
            ExportRecordPayload::from_record(record.payload(), record.kind(), include_secrets)?;

        let created_at = record
            .created_at()
            .format(&Rfc3339)
            .expect("OffsetDateTime should always be formattable as RFC 3339");
        let updated_at = record
            .updated_at()
            .format(&Rfc3339)
            .expect("OffsetDateTime should always be formattable as RFC 3339");

        Ok(Self {
            id: record.id().to_string(),
            kind: record.kind(),
            label: record.label().as_str().to_owned(),
            payload,
            created_at,
            updated_at,
            hotkey: record.hotkey().map(|h| h.as_str().to_owned()),
        })
    }
}

// -------------------------------------------------------------------
// ExportPayload
// -------------------------------------------------------------------

/// エクスポートファイル全体のルート JSON オブジェクト。
///
/// `format_version: 1` を必ず含める（`basic-design.md §REQ-DP-003`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPayload {
    /// フォーマットバージョン（常に `EXPORT_FORMAT_VERSION = 1`）。
    pub format_version: u32,
    /// エクスポート実行時刻（RFC 3339）。
    pub exported_at: String,
    /// vault ディレクトリの basename（識別用メタデータ）。
    pub vault_name: String,
    /// 全エクスポートレコード。
    pub records: Vec<ExportRecord>,
}

impl ExportPayload {
    /// `ExportPayload` を構築する。
    ///
    /// `format_version` は `EXPORT_FORMAT_VERSION` で固定する。
    #[must_use]
    pub fn new(records: Vec<ExportRecord>, vault_name: String, now: OffsetDateTime) -> Self {
        let exported_at = now
            .format(&Rfc3339)
            .expect("OffsetDateTime should always be formattable as RFC 3339");
        Self {
            format_version: EXPORT_FORMAT_VERSION,
            exported_at,
            vault_name,
            records,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::SecretString;
    use crate::{Aad, CipherText, NonceBytes, RecordId, RecordPayloadEncrypted, VaultVersion};
    use time::OffsetDateTime;

    fn make_plaintext_payload(s: &str) -> RecordPayload {
        RecordPayload::Plaintext(SecretString::from_string(s.to_owned()))
    }

    fn make_encrypted_payload() -> RecordPayload {
        let nonce = NonceBytes::try_new(&[0u8; 12]).unwrap();
        let ct = CipherText::try_new(vec![0u8; 16].into_boxed_slice()).unwrap();
        let id = RecordId::new(uuid::Uuid::now_v7()).unwrap();
        let aad = Aad::new(id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap();
        RecordPayload::Encrypted(RecordPayloadEncrypted::new(nonce, ct, aad).unwrap())
    }

    // --- TC-UT-177: Secret kind + include_secrets=false → Redacted ---
    #[test]
    fn tc_ut_177_secret_kind_without_include_secrets_returns_redacted() {
        let payload = make_plaintext_payload("my-password");
        let result = ExportRecordPayload::from_record(&payload, RecordKind::Secret, false);
        assert_eq!(result, Ok(ExportRecordPayload::Redacted));
    }

    // --- TC-UT-178: Secret kind + include_secrets=true → Plaintext ---
    #[test]
    fn tc_ut_178_secret_kind_with_include_secrets_returns_plaintext() {
        let payload = make_plaintext_payload("my-password");
        let result = ExportRecordPayload::from_record(&payload, RecordKind::Secret, true);
        assert_eq!(
            result,
            Ok(ExportRecordPayload::Plaintext {
                value: "my-password".to_owned()
            })
        );
    }

    // --- TC-UT-179: Text kind + include_secrets=false → Plaintext ---
    #[test]
    fn tc_ut_179_text_kind_without_include_secrets_returns_plaintext() {
        let payload = make_plaintext_payload("hello");
        let result = ExportRecordPayload::from_record(&payload, RecordKind::Text, false);
        assert_eq!(
            result,
            Ok(ExportRecordPayload::Plaintext {
                value: "hello".to_owned()
            })
        );
    }

    // --- TC-UT-195: Encrypted ペイロード → Err(VaultLocked) （Fail Fast）---
    #[test]
    fn tc_ut_195_encrypted_payload_returns_vault_locked() {
        let payload = make_encrypted_payload();
        let result = ExportRecordPayload::from_record(&payload, RecordKind::Secret, false);
        assert_eq!(result, Err(ExportError::VaultLocked));
    }

    // --- TC-UT-195 Text kind でも Encrypted は即時エラー ---
    #[test]
    fn tc_ut_195b_encrypted_text_payload_also_returns_vault_locked() {
        let payload = make_encrypted_payload();
        let result = ExportRecordPayload::from_record(&payload, RecordKind::Text, true);
        assert_eq!(result, Err(ExportError::VaultLocked));
    }
}
