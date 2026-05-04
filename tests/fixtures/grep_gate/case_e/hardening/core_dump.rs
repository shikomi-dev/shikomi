// fixture for TC-CI-026-e (ii) — 許可リスト登録ファイル `hardening/core_dump.rs` 模倣
// C-41 core dump 抑制 (Sub-F Phase 5) で `libc::prctl(PR_SET_DUMPABLE, 0)` /
// `libc::setrlimit(RLIMIT_CORE, 0)` の FFI 呼出に必要な最小 unsafe。
// Issue #75 で許可リスト追加された経緯（cli-subcommands.md §C-41）の回帰防止。
// 期待: PASS（exit 0）
//
// 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-e

#![allow(unsafe_code)]

pub fn fake_set_dumpable() -> i32 {
    let rc = unsafe { 0i32 };
    rc
}

pub fn fake_setrlimit() -> i32 {
    let rc = unsafe { 0i32 };
    rc
}
