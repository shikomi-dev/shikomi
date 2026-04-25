//! レコード表示名。

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::{DomainError, InvalidRecordLabelReason};

/// `RecordLabel` の grapheme cluster 最大数。
const LABEL_MAX_GRAPHEMES: usize = 255;

// -------------------------------------------------------------------
// RecordLabel
// -------------------------------------------------------------------

/// レコードの表示名。
///
/// 以下の条件を満たす文字列のみ `try_new` を通過できる（Fail Fast）：
/// - 非空
/// - 禁止制御文字なし（U+0000〜U+001F のうち `\t`/`\n`/`\r` は許可、U+007F は禁止）
/// - grapheme cluster 数 ≤ 255
///
/// IPC 経路では内部 `String` を `serialize` し、`deserialize` 時は `try_new` で再検証する。
#[derive(Debug, Clone)]
pub struct RecordLabel {
    inner: String,
}

impl Serialize for RecordLabel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.inner)
    }
}

impl<'de> Deserialize<'de> for RecordLabel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
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
