//! 永続化レイヤー。
//!
//! `VaultRepository` trait と `SqliteVaultRepository` 実装を提供する。

use shikomi_core::Vault;

use crate::persistence::error::PersistenceError;

// -------------------------------------------------------------------
// サブモジュール
// -------------------------------------------------------------------

pub(crate) mod audit;
pub(crate) mod error;
pub(crate) mod lock;
pub(crate) mod paths;
pub(crate) mod permission;
pub(crate) mod repository;
pub(crate) mod sqlite;

// -------------------------------------------------------------------
// 公開 re-export
// -------------------------------------------------------------------

pub use error::{AtomicWriteStage, CorruptedReason, PersistenceError, VaultDirReason};
pub use paths::VaultPaths;
pub use repository::SqliteVaultRepository;

// -------------------------------------------------------------------
// 定数
// -------------------------------------------------------------------

/// 暗号化 vault 永続化の追跡 Issue 番号。未登録のため `None`。
pub(crate) const TRACKING_ISSUE_ENCRYPTED_VAULT: Option<u32> = None;

// -------------------------------------------------------------------
// VaultRepository trait
// -------------------------------------------------------------------

/// vault の読み書き操作を抽象化する trait。
pub trait VaultRepository {
    /// vault を読み込んで返す。
    ///
    /// # Errors
    ///
    /// - vault.db が存在しない: `PersistenceError::Io`
    /// - 破損データ: `PersistenceError::Corrupted`
    /// - ロック取得失敗: `PersistenceError::Locked`
    /// - その他 IO/SQLite エラー
    fn load(&self) -> Result<Vault, PersistenceError>;

    /// vault を保存する。
    ///
    /// # Errors
    ///
    /// - ディレクトリ作成失敗: `PersistenceError::Io`
    /// - ロック取得失敗: `PersistenceError::Locked`
    /// - 孤立 `.new` ファイル: `PersistenceError::OrphanNewFile`
    /// - 書き込み失敗: `PersistenceError::Sqlite` / `PersistenceError::AtomicWriteFailed`
    fn save(&self, vault: &Vault) -> Result<(), PersistenceError>;

    /// vault.db が存在するかどうかを返す。
    ///
    /// # Errors
    ///
    /// - 存在確認 IO エラー: `PersistenceError::Io`
    fn exists(&self) -> Result<bool, PersistenceError>;
}
