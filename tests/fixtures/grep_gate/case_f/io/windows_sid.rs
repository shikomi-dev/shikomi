// fixture for TC-CI-026-f (control file) — 許可リスト登録の本物 path
// この path は許可リストに登録されるため、実 unsafe があっても除外される。
// 比較対象として `windows_sid.rs.bypass/evil.rs` の bypass 試行を sentinel 化する。
//
// 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-f
// 関連: Bug-CI-031（マユリ工程4 発見、服部・ペテルギウス工程5 致命指摘）

#![allow(unsafe_code)]

pub fn fake_legitimate_unsafe() {
    unsafe {
        // 許可リスト登録 path での実 unsafe（FFI 模倣）
    }
}
