//! データポータビリティ UseCase 群（Issue #141 Sub-B）。
//!
//! - `error`: UseCase 内部中間エラー型 `DataPortabilityError`
//! - `export`: `export_records` + `ExportSummary`
//! - `import`: `import_records` + `ImportSummary`
//!
//! 設計根拠: docs/features/data-portability/cli/detailed-design/usecase.md

pub mod error;
pub mod export;
pub mod import;
