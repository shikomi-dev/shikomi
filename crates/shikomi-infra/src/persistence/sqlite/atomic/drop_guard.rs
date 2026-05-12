//! `Drop` implementation for `AtomicWriteSession` — cleanup 保証（Fail Safe）。

use super::session::AtomicWriteSession;
use super::writer::AtomicWriter;

impl Drop for AtomicWriteSession {
    /// `finalize` 未呼出のまま drop された場合は `.new` を best-effort 削除（Fail Safe）。
    ///
    /// `new_path` が `None`（`finalize` が所有権を取得済）の場合は何もしない。
    ///
    /// **順序**: `conn` を先に close してからファイル削除する。
    /// Windows はオープン中のファイルハンドルを持つファイルの削除を
    /// `ERROR_ACCESS_DENIED (5)` で拒否するため、`cleanup_new` の前に
    /// `conn.take()` で `rusqlite::Connection` を drop しなければならない。
    fn drop(&mut self) {
        // Windows: conn が Some のまま remove_file すると ERROR_ACCESS_DENIED (5)。
        // take() で Connection を drop し、ファイルハンドルを解放してから削除する。
        drop(self.conn.take());
        if let Some(ref path) = self.new_path {
            AtomicWriter::cleanup_new(path);
        }
    }
}
