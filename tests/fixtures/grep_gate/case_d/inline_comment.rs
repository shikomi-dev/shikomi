// fixture for TC-CI-026-d — 行内コメント形式の実 unsafe コード
// `unsafe { ... } // SAFETY: ...` は実コードに unsafe ブロックがある以上 grep は
// 正しくヒットしなければならない。コメント行除外パイプを「行内コメントまで除外」
// に過剰一般化した実装ミスを sentinel として早期検出する。
// 期待: FAIL（exit 1、stderr に file:line:content）
//
// 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-d

pub fn read_raw(p: *const u8) -> u8 {
    unsafe { *p } // SAFETY: caller guarantees p is valid for reads of u8
}
