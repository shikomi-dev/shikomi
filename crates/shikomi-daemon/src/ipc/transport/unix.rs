//! Unix Domain Socket transport ヘルパ（薄い）。
//!
//! 主処理は `server.rs` 側 `accept` ループに集約し、本ファイルは型エイリアスのみ。

#![cfg(unix)]
