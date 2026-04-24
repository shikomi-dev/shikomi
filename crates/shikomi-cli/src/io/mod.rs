//! TTY / path 系の副作用を持つ IO ヘルパ。
//!
//! UseCase 層からは呼ばれず、`run()` が薄く wrap して呼び出す。

pub mod paths;
pub mod terminal;
