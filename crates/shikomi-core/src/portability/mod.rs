//! データポータビリティ — domain 型モジュール（Issue #135 / Sub-A）。
//!
//! `ExportRecord` / `ExportPayload` / `ImportPayload` / `ImportValidator` 等を
//! `shikomi-core` の公開 API として re-export する。
//!
//! # モジュール構成
//! - `export`: エクスポート用型（`ExportRecordPayload` / `ExportRecord` / `ExportPayload`）
//! - `import`: インポート用型（`ImportRecord` / `ImportPayload` / `ImportValidator`）
//! - `error`: domain 専用エラー型（`ImportValidationError` / `ExportError`）

pub mod error;
pub mod export;
pub mod import;

// re-export
pub use error::{ExportError, ImportValidationError};
pub use export::{ExportPayload, ExportRecord, ExportRecordPayload, EXPORT_FORMAT_VERSION};
pub use import::{
    ImportPayload, ImportRecord, ImportValidationReport, ImportValidator, ImportWarning,
};
