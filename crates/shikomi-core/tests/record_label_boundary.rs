//! 結合テスト: `RecordLabel` 境界値（TC-I03）
//! REQ-005 / AC-03, AC-06

use shikomi_core::{DomainError, InvalidRecordLabelReason, RecordLabel};

/// TC-I03: `RecordLabel` 境界値を外部 API から検証
#[test]
fn test_record_label_boundaries_from_public_api() {
    // (a) empty -> Err(Empty)
    assert!(matches!(
        RecordLabel::try_new(String::new()).unwrap_err(),
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::Empty)
    ));

    // (b) 1 char -> Ok
    assert!(RecordLabel::try_new("A".to_string()).is_ok());

    // (c) 255 graphemes -> Ok
    let s255 = "あ".repeat(255);
    assert!(RecordLabel::try_new(s255).is_ok());

    // (d) 256 graphemes -> Err(TooLong)
    let s256 = "あ".repeat(256);
    assert!(matches!(
        RecordLabel::try_new(s256).unwrap_err(),
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::TooLong {
            grapheme_count: 256
        })
    ));

    // (e) NUL (U+0000) -> Err(ControlChar)
    assert!(matches!(
        RecordLabel::try_new("\x00X".to_string()).unwrap_err(),
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
    ));

    // (f) U+001F -> Err(ControlChar)
    assert!(matches!(
        RecordLabel::try_new("\x1FX".to_string()).unwrap_err(),
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
    ));

    // (g) DEL (U+007F) -> Err(ControlChar)
    assert!(matches!(
        RecordLabel::try_new("\x7FX".to_string()).unwrap_err(),
        DomainError::InvalidRecordLabel(InvalidRecordLabelReason::ControlChar { .. })
    ));

    // (h) TAB (\t) -> Ok (permitted)
    assert!(RecordLabel::try_new("A\tB".to_string()).is_ok());

    // (i) LF (\n) -> Ok (permitted)
    assert!(RecordLabel::try_new("A\nB".to_string()).is_ok());
}
