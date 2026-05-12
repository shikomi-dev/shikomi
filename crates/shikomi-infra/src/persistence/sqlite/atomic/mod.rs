//! アトミック書き込みユーティリティ。
//!
//! Phase 8 リファクタ（Issue #73）: `AtomicWriter` ZST + 静的メソッド連鎖から
//! セッション型 `AtomicWriteSession { conn, new_path }` に移行。
//! `finalize(self, retry_policy)` の所有権消費でクローズ順序契約を型レベルで強制。
//!
//! - **`AtomicWriteSession`**: `.new` 書込から `conn.close()` + fsync + rename まで一連で完結。
//! - **`AtomicWriter`** (ZST): `detect_orphan` / `cleanup_new` の名前空間として残存。
//! - **`RetryPolicy`** trait: Win rename retry の振る舞いをテスト注入可能に抽象化。
//!
//! Bug-G-001 反映（2026-04-27）: Win CI ランナーで Defender / Search Indexer が
//! ハンドルを `drop` 後も `~250ms+` 保持し続けるため、retry を指数バックオフへ拡張
//! （`50ms × 2^(n-1)` ± `25ms` jitter × 最大 5 回、最悪 ~1675ms / 平均 ~1550ms）。
//!
//! Windows DACL 適用順序確定（Phase 8、`./classes.md` §3.3）: rename 前に `.new` に
//! `ensure_file` を適用し、`MoveFileExW` がソース SD を保持することで `vault.db` に引継。

mod constants;
mod retry_policy;
mod session;
mod writer;

#[cfg(test)]
mod tests;

pub(crate) use retry_policy::ExponentialBackoffRetryPolicy;
pub(crate) use session::AtomicWriteSession;
pub(crate) use writer::AtomicWriter;

#[cfg(test)]
pub(crate) use retry_policy::NoSleepRetryPolicy;
