// fixture for TC-CI-026-b — doc コメント内の `unsafe { ... }` 解説文字列のみ
// 実 unsafe ブロックは存在しない。Issue #75 (PR #82) で
// crates/shikomi-cli/src/io/ipc_vault_repository.rs:725 に混入した実例の最小再現。
// 期待: コメント行除外パイプにより grep にヒットせず PASS（exit 0）
//
// 設計根拠: docs/features/dev-workflow/test-design.md §TC-CI-026-b

/// `unsafe { std::env::remove_var(...) }` は Rust 2024 edition の env 操作 unsafe 化
/// 規約に従う（test スコープ内のみ、`serial_test` で他スレッドと直列化済）。
pub fn safe_only() -> i32 {
    0
}
