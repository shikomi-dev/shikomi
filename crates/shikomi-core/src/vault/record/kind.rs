//! レコード種別。

use serde::{Deserialize, Serialize};

// -------------------------------------------------------------------
// RecordKind
// -------------------------------------------------------------------

/// レコードの種別。
///
/// IPC 経路では `serde(rename_all = "snake_case")` で `"text"` / `"secret"` 表現。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// テキストレコード（URL・メモ等、機密度が低い）。
    Text,
    /// シークレットレコード（パスワード・鍵等、機密度が高い）。
    Secret,
}
