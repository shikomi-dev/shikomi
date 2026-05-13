//! アトミック書き込みモジュールの内部定数。

/// `SQLite` サイドカーファイル名のサフィックス。
pub(super) const SQLITE_SIDECAR_SUFFIXES: &[&str] = &["-journal", "-wal", "-shm"];
