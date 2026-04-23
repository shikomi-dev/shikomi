//! ドメイン型と `SQLite` 行のマッピング。
//!
//! `Mapping` はドメイン型 → `SQLite` パラメータ、`SQLite` 行 → ドメイン型の変換を提供する。

mod header;
mod params;
mod record;

pub(crate) use params::{HeaderParams, RecordParams};

// -------------------------------------------------------------------
// Mapping
// -------------------------------------------------------------------

/// ドメイン型と `SQLite` 行のマッピングを提供するゼロサイズ型。
pub(crate) struct Mapping;

#[cfg(test)]
mod tests;
