// fixture for TC-CI-026-f — Bug-CI-031 path 偽装 silent bypass の sentinel
//
// 攻撃シナリオ: 許可リスト entry 文字列 (`case_f/io/windows_sid.rs`) を path に
// substring として含むサブディレクトリ (`windows_sid.rs.bypass/evil.rs`) を作り、
// 内部に実 unsafe ブロックを書く。grep -vF substring 一致では `windows_sid.rs`
// が含まれるため除外され、silent bypass が成立してしまう（マユリ工程4 実機確認）。
//
// 期待: awk -F: '$1 != "<allowlist>"' の path 完全一致除外により検出される。
//       evil.rs:N:    unsafe { ... } が stderr に出力され FAIL（exit 1）。
//
// 攻撃面 articulate:
// - path 偽装 `*.rs.bypass/`: 本 fixture の経路
// - 拡張子偽装 `*.rsx`: `--include='*.rs'` で grep にヒットしないため fail-closed
// - content 注入: 許可リスト文字列をコメント混入しても、grep 出力 $1 はファイルの
//   実 path のみ抽出されるため $1 完全一致でブロック
//
// 設計根拠:
// - docs/features/dev-workflow/test-design.md §TC-CI-026-f
// - docs/features/dev-workflow/detailed-design/scripts.md §検出契約 §許可リスト
// - 服部工程5 致命指摘: substring 一致 → path 完全一致への構造的修正
// - ペテルギウス工程5 致命指摘1: 設計書 line 33「ファイル粒度」articulate との整合回復

pub fn deref_raw(p: *const u8) -> u8 {
    unsafe {
        *p
    }
}
