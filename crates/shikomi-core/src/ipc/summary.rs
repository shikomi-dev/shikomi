//! `ListRecords` 応答の投影型（機密値非含有）。

use serde::{Deserialize, Serialize};

use crate::vault::id::RecordId;
use crate::vault::record::{Record, RecordKind, RecordLabel};

/// List 応答に含めるプレビュー長（先頭 char 数）。
const PREVIEW_MAX_CHARS: usize = 40;

// -------------------------------------------------------------------
// RecordSummary
// -------------------------------------------------------------------

/// `ListRecords` 応答に含むレコード summary（機密値非含有）。
///
/// - `value_preview`: Text の場合は `Record::text_preview(40)` の結果、Secret は `None`
/// - `value_masked`: Secret は `true`、Text は `false`
///
/// 設計根拠: docs/features/daemon-ipc/detailed-design/protocol-types.md §`RecordSummary`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecordSummary {
    /// レコード ID。
    pub id: RecordId,
    /// レコード種別。
    pub kind: RecordKind,
    /// ラベル。
    pub label: RecordLabel,
    /// プレビュー（Text のみ Some）。
    pub value_preview: Option<String>,
    /// マスク表示ヒント（Secret は true）。
    pub value_masked: bool,
}

impl RecordSummary {
    /// `Record` から投影 summary を構築する。
    ///
    /// `Record::text_preview` は内部で平文取り出しを行うが、その呼出は core 内に閉じる。
    #[must_use]
    pub fn from_record(record: &Record) -> Self {
        let kind = record.kind();
        let (value_preview, value_masked) = match kind {
            RecordKind::Text => (record.text_preview(PREVIEW_MAX_CHARS), false),
            RecordKind::Secret => (None, true),
        };
        Self {
            id: record.id().clone(),
            kind,
            label: record.label().clone(),
            value_preview,
            value_masked,
        }
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use crate::secret::SecretString;
    use crate::vault::record::{Record, RecordPayload};
    use uuid::Uuid;

    use super::*;

    fn fixed_now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn new_record(kind: RecordKind, value: &str) -> Record {
        let id = RecordId::new(Uuid::now_v7()).unwrap();
        let label = RecordLabel::try_new("test-label".to_owned()).unwrap();
        let payload = RecordPayload::Plaintext(SecretString::from_string(value.to_owned()));
        Record::new(id, kind, label, payload, fixed_now())
    }

    #[test]
    fn test_from_record_text_kind_returns_preview_and_unmasked() {
        let record = new_record(RecordKind::Text, "https://example.com");
        let s = RecordSummary::from_record(&record);
        assert_eq!(s.kind, RecordKind::Text);
        assert!(!s.value_masked);
        assert_eq!(s.value_preview.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn test_from_record_secret_kind_returns_none_and_masked() {
        let record = new_record(RecordKind::Secret, "topsecret");
        let s = RecordSummary::from_record(&record);
        assert_eq!(s.kind, RecordKind::Secret);
        assert!(s.value_masked);
        assert!(s.value_preview.is_none());
    }
}
