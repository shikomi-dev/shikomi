//! 永続化レイヤーのエラー型。
//!
//! `PersistenceError` と付随する理由列挙型を定義する。

use std::path::PathBuf;

use thiserror::Error;

// -------------------------------------------------------------------
// 付随 Reason 列挙
// -------------------------------------------------------------------

/// `PersistenceError::Corrupted` の詳細理由。
#[derive(Debug)]
#[non_exhaustive]
pub enum CorruptedReason {
    /// `vault_header` テーブルに行が存在しない。
    MissingVaultHeader,
    /// 未知の保護モード文字列が格納されている。
    UnknownProtectionMode {
        /// DB に格納されていた生の文字列。
        raw: String,
    },
    /// 行の組み合わせが無効（複数行ヘッダ等）。
    InvalidRowCombination {
        /// 詳細説明。
        detail: String,
    },
    /// RFC3339 パース失敗。
    InvalidRfc3339 {
        /// 失敗したカラム名。
        column: &'static str,
        /// DB に格納されていた生の文字列。
        raw: String,
    },
    /// UUID 文字列パース失敗。
    InvalidUuidString {
        /// DB に格納されていた生の文字列。
        raw: String,
    },
    /// ペイロードバリアントとデータが一致しない。
    PayloadVariantMismatch {
        /// 期待するバリアント。
        expected: &'static str,
        /// 実際に見つかったバリアント。
        got: &'static str,
    },
    /// NULL 制約違反（必須カラムが NULL）。
    NullViolation {
        /// NULL だったカラム名。
        column: &'static str,
    },
}

impl std::fmt::Display for CorruptedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVaultHeader => write!(f, "vault_header row not found"),
            Self::UnknownProtectionMode { raw } => {
                write!(f, "unknown protection mode: {raw:?}")
            }
            Self::InvalidRowCombination { detail } => {
                write!(f, "invalid row combination: {detail}")
            }
            Self::InvalidRfc3339 { column, raw } => {
                write!(f, "column {column:?} contains invalid RFC3339: {raw:?}")
            }
            Self::InvalidUuidString { raw } => {
                write!(f, "invalid UUID string: {raw:?}")
            }
            Self::PayloadVariantMismatch { expected, got } => {
                write!(
                    f,
                    "payload variant mismatch: expected {expected:?}, got {got:?}"
                )
            }
            Self::NullViolation { column } => {
                write!(f, "required column {column:?} is NULL")
            }
        }
    }
}

/// `PersistenceError::InvalidVaultDir` の詳細理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VaultDirReason {
    /// パスが絶対パスでない。
    #[error("path is not absolute")]
    NotAbsolute,
    /// パスに `..` コンポーネントが含まれる。
    #[error("path contains '..' traversal component")]
    PathTraversal,
    /// パスがシンボリックリンクである。
    #[error("vault dir must not be a symlink")]
    SymlinkNotAllowed,
    /// `fs::canonicalize` が失敗した。
    #[error("failed to canonicalize vault dir path: {source}")]
    Canonicalize {
        /// 元の IO エラー。
        #[source]
        source: std::io::Error,
    },
    /// 保護されたシステム領域のプレフィックスと一致した。
    #[error("vault dir is inside protected system area: {prefix}")]
    ProtectedSystemArea {
        /// 一致したプレフィックス。
        prefix: &'static str,
    },
    /// 既存パスがディレクトリではない。
    #[error("vault dir path exists but is not a directory")]
    NotADirectory,
}

/// `PersistenceError::AtomicWriteFailed` のステージ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicWriteStage {
    /// `.new` ファイル作成準備中。
    PrepareNew,
    /// `.new` への `SQLite` 書き込み中。
    WriteTemp,
    /// `.new` の fsync 中。
    FsyncTemp,
    /// 親ディレクトリの fsync 中。
    FsyncDir,
    /// `.new` → `vault.db` リネーム中。
    Rename,
    /// 孤立 `.new` ファイルのクリーンアップ中。
    CleanupOrphan,
}

impl std::fmt::Display for AtomicWriteStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrepareNew => write!(f, "prepare-new"),
            Self::WriteTemp => write!(f, "write-temp"),
            Self::FsyncTemp => write!(f, "fsync-temp"),
            Self::FsyncDir => write!(f, "fsync-dir"),
            Self::Rename => write!(f, "rename"),
            Self::CleanupOrphan => write!(f, "cleanup-orphan"),
        }
    }
}

// -------------------------------------------------------------------
// PersistenceError
// -------------------------------------------------------------------

/// 永続化レイヤーが返す統一エラー型。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PersistenceError {
    /// ファイル IO エラー。
    #[error("IO error on {path}: {source}")]
    Io {
        /// 操作対象のパス。
        path: PathBuf,
        /// 元の IO エラー。
        #[source]
        source: std::io::Error,
    },

    /// `SQLite` エラー。
    #[error("SQLite error: {source}")]
    Sqlite {
        /// 元の rusqlite エラー。
        #[source]
        source: rusqlite::Error,
    },

    /// DB の内容が破損している（スキーマ正常だが値が不正）。
    #[error("corrupted data in table {table:?}: {reason}")]
    Corrupted {
        /// 問題のあるテーブル名。
        table: &'static str,
        /// 問題のある行の主キー（分かる場合）。
        row_key: Option<String>,
        /// 破損の詳細理由。
        reason: CorruptedReason,
        /// 起因したドメインエラー（あれば）。
        #[source]
        source: Option<shikomi_core::DomainError>,
    },

    /// ファイルのパーミッションが期待値と異なる。
    #[error("invalid permission on {path}: expected {expected}, actual {actual}")]
    InvalidPermission {
        /// 対象パス。
        path: PathBuf,
        /// 期待するパーミッション文字列。
        expected: &'static str,
        /// 実際のパーミッション文字列。
        actual: String,
    },

    /// vault ディレクトリのパスが不正。
    #[error("invalid vault dir {path}: {reason}")]
    InvalidVaultDir {
        /// 不正なパス。
        path: PathBuf,
        /// 不正の詳細理由。
        reason: VaultDirReason,
    },

    /// 孤立した `.new` ファイルが存在する（前回の書き込みが途中で中断された可能性）。
    #[error("orphan new file found at {path}; remove it manually or re-run")]
    OrphanNewFile {
        /// 孤立ファイルのパス。
        path: PathBuf,
    },

    /// アトミック書き込みの特定ステージで失敗した。
    #[error("atomic write failed at stage {stage}: {source}")]
    AtomicWriteFailed {
        /// 失敗したステージ。
        stage: AtomicWriteStage,
        /// 元の IO エラー。
        #[source]
        source: std::io::Error,
    },

    /// `SQLite` スキーマの `application_id` または `user_version` が期待値と異なる。
    #[error(
        "schema mismatch: expected application_id={expected_application_id:#010x}, \
         found={found_application_id:#010x}; \
         expected user_version in [{expected_version_min},{expected_version_max}], \
         found={found_user_version}"
    )]
    SchemaMismatch {
        /// 期待する `application_id`。
        expected_application_id: u32,
        /// 実際の `application_id`。
        found_application_id: u32,
        /// 期待する `user_version` の最小値。
        expected_version_min: u32,
        /// 期待する `user_version` の最大値。
        expected_version_max: u32,
        /// 実際の `user_version`。
        found_user_version: u32,
    },

    /// 未実装機能（追跡 Issue あり）。
    #[error("unsupported feature: {feature} (tracking issue: {tracking})",
        tracking = tracking_issue.map_or(
            "(not yet filed)".to_string(),
            |n| format!("#{n}")
        )
    )]
    UnsupportedYet {
        /// 未実装機能の名称。
        feature: &'static str,
        /// 追跡 Issue 番号（未登録の場合は `None`）。
        tracking_issue: Option<u32>,
    },

    /// vault ディレクトリを解決できなかった（環境変数もホームディレクトリも取得不可）。
    #[error("cannot resolve vault directory: set SHIKOMI_VAULT_DIR or ensure a home directory is available")]
    CannotResolveVaultDir,

    /// vault.db ロックファイルが他プロセスに保持されている。
    #[error("vault is locked at {path}{holder}",
        holder = holder_hint.map_or(String::new(), |pid| format!(" (held by pid {pid})"))
    )]
    Locked {
        /// ロックファイルのパス。
        path: PathBuf,
        /// ロック保持プロセスの PID ヒント（分かる場合）。
        holder_hint: Option<u32>,
    },
}

// -------------------------------------------------------------------
// From impls
// -------------------------------------------------------------------

impl From<rusqlite::Error> for PersistenceError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite { source }
    }
}

impl From<shikomi_core::DomainError> for PersistenceError {
    fn from(e: shikomi_core::DomainError) -> Self {
        Self::Corrupted {
            table: "unknown",
            row_key: None,
            reason: CorruptedReason::InvalidRowCombination {
                detail: e.to_string(),
            },
            source: Some(e),
        }
    }
}
