// fixture for TC-CI-026-e (i) — 許可リスト登録ファイル `io/windows_sid.rs` 模倣
// Windows Win32 Security FFI の最小 unsafe（実コード相当）。許可リストに登録
// された path として audit_unsafe_blocks の grep -vF 除外で検査結果から外れる。
// 期待: PASS（exit 0）
//
// 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-e

#![allow(unsafe_code)]

pub fn fake_get_last_error() -> u32 {
    let last_err = unsafe { 0u32 };
    last_err
}

pub fn fake_open_token() {
    unsafe {
        // FFI 模倣
    }
}
