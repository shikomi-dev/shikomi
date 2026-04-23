//! レコードおよび関連型（`RecordKind` / `RecordLabel` / `RecordPayload`）。

mod aggregate;
mod kind;
mod label;
mod payload;

pub use aggregate::Record;
pub use kind::RecordKind;
pub use label::RecordLabel;
pub use payload::{RecordPayload, RecordPayloadEncrypted};

#[cfg(test)]
mod tests;
