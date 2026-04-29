//! fixture for TC-CI-026-c — コメント形式 5 パターン網羅
//! 実 unsafe ブロックは存在しない。「行頭から最初の非空白文字が `//` で始まる行」
//! 一律除外契約が doc/通常/モジュール doc/先頭空白あり/空白なし 全てを吸収する
//! ことを保証。
//!
//! 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-c
//
// パターン 1: モジュール doc コメント（`//!`、行頭から）
//! unsafe { module_doc_pretend() }
//
// パターン 2: doc コメント（`///`、行頭から）
/// unsafe { doc_pretend() }
//
// パターン 3: 通常コメント（`//`、行頭から、空白なし）
//unsafe{tight_no_space()}
//
// パターン 4: 通常コメント（`//`、先頭空白あり）
    // unsafe { indented_with_space() }
//
// パターン 5: 通常コメント（`//`、先頭タブあり）
	// unsafe { indented_with_tab() }

pub fn safe_only() -> i32 {
    42
}
