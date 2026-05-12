//! `AtomicWriter` — アトミック書き込みの補助ユーティリティ（ZST 名前空間）。

use std::path::Path;

use crate::persistence::error::PersistenceError;

#[cfg(test)]
use super::session::AtomicWriteSession;
#[cfg(test)]
use crate::persistence::paths::VaultPaths;
#[cfg(test)]
use shikomi_core::Vault;

/// アトミック書き込みの補助ユーティリティ（ZST 名前空間）。
///
/// Phase 8 リファクタで主要ロジックは `AtomicWriteSession` に移行。
/// `detect_orphan` / `cleanup_new` の名前空間として残存する。
pub(crate) struct AtomicWriter;

impl AtomicWriter {
    /// `vault.db.new` が存在する場合に孤立ファイルエラーを返す。
    ///
    /// # Errors
    ///
    /// - `.new` が存在する: `PersistenceError::OrphanNewFile`
    /// - 存在確認 IO エラー: `PersistenceError::Io`
    pub(crate) fn detect_orphan(new_path: &Path) -> Result<(), PersistenceError> {
        match new_path.try_exists() {
            Ok(true) => Err(PersistenceError::OrphanNewFile {
                path: new_path.to_path_buf(),
            }),
            Ok(false) => Ok(()),
            Err(e) => Err(PersistenceError::Io {
                path: new_path.to_path_buf(),
                source: e,
            }),
        }
    }

    /// `vault.db.new` を best-effort 削除する。
    ///
    /// 失敗時は `tracing::warn!` でログ出力し、呼出側のエラーを上書きしない（上位に伝播しない）。
    pub(crate) fn cleanup_new(new_path: &Path) {
        if let Err(e) = std::fs::remove_file(new_path) {
            tracing::warn!(
                path = %new_path.display(),
                error = %e,
                "failed to cleanup .new file (best-effort)"
            );
        }
    }

    /// `vault.db.new` に vault の内容を書き込むが fsync/rename は行わない（テスト専用）。
    ///
    /// `AtomicWriteSession::new` と同一ロジックで `.new` を書き込み、`finalize` を呼ばずに返す。
    /// atomic write の中断状態を決定的に再現するためのテストフック（AC-06 対応）。
    ///
    /// # Errors
    ///
    /// - `AtomicWriteSession::new` と同じ
    #[cfg(test)]
    pub(crate) fn write_new_only(
        paths: &VaultPaths,
        vault: &Vault,
    ) -> Result<(), PersistenceError> {
        let session = AtomicWriteSession::new(paths, vault)?;
        // conn を close して SQLite ハンドルを解放し、new_path を None にして
        // Drop が cleanup_new を呼ばないようにすることで .new を残す。
        session.close_without_rename();
        Ok(())
    }
}
