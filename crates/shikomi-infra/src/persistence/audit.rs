//! 永続化操作の監査ログ。
//!
//! `Audit` はゼロサイズ型で load/save の開始・成功・失敗を `tracing` に記録する。
//! 秘密値を含まないエラー表示のみを行う。

use shikomi_core::ProtectionMode;

use super::error::PersistenceError;
use super::paths::VaultPaths;

// -------------------------------------------------------------------
// Audit
// -------------------------------------------------------------------

/// 監査ログ操作を提供するゼロサイズ型。
pub(crate) struct Audit;

impl Audit {
    /// `load` 開始を記録する。
    pub(crate) fn entry_load(paths: &VaultPaths) {
        tracing::info!(vault_dir = %paths.dir().display(), "load: entry");
    }

    /// `save` 開始を記録する。
    pub(crate) fn entry_save(paths: &VaultPaths, record_count: usize) {
        tracing::info!(
            vault_dir = %paths.dir().display(),
            record_count,
            "save: entry"
        );
    }

    /// `load` 成功を記録する。
    pub(crate) fn exit_ok_load(
        record_count: usize,
        protection_mode: ProtectionMode,
        elapsed_ms: u64,
    ) {
        tracing::info!(
            record_count,
            protection_mode = protection_mode.as_persisted_str(),
            elapsed_ms,
            "load: ok"
        );
    }

    /// `save` 成功を記録する。
    pub(crate) fn exit_ok_save(record_count: usize, bytes_written: u64, elapsed_ms: u64) {
        tracing::info!(record_count, bytes_written, elapsed_ms, "save: ok");
    }

    /// エラー終了を記録する（秘密値を含まないエラー表示のみ）。
    ///
    /// `UnsupportedYet` と `Locked` は warn レベル、その他は error レベルで記録する。
    pub(crate) fn exit_err(err: &PersistenceError, elapsed_ms: u64) {
        match err {
            PersistenceError::UnsupportedYet { .. } | PersistenceError::Locked { .. } => {
                tracing::warn!(error = %err, elapsed_ms, "persistence: exit with warning");
            }
            _ => {
                tracing::error!(error = %err, elapsed_ms, "persistence: exit with error");
            }
        }
    }
}
