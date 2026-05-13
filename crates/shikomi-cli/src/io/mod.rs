//! TTY / path 系の副作用を持つ IO ヘルパ。
//!
//! UseCase 層からは呼ばれず、`run()` が薄く wrap して呼び出す。

pub mod ipc_client;
pub mod ipc_vault_repository;
pub mod paths;
pub mod terminal;

#[cfg(windows)]
pub mod windows_sid;
