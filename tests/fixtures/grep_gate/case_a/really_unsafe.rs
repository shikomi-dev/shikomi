// fixture for TC-CI-026-a — 許可リスト外ファイルに実 unsafe ブロックが存在
// 期待: audit_unsafe_blocks が FAIL を返し、stderr に該当 file:line:content を出力
//
// 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-a
// (rename: realy_unsafe.rs → really_unsafe.rs、ペガサス工程5 typo 指摘解消)

pub fn deref_raw(p: *const u8) -> u8 {
    unsafe {
        *p
    }
}
