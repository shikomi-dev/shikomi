//! vault ファイルのパス管理。
//!
//! `VaultPaths` は vault ディレクトリの検証と各ファイルパスの導出を担う。

use std::path::{Component, Path, PathBuf};

use super::error::{PersistenceError, VaultDirReason};

// -------------------------------------------------------------------
// vault dir 解決ヘルパ（crate 内部のみ）
// -------------------------------------------------------------------

/// `SHIKOMI_VAULT_DIR` 環境変数 → OS データディレクトリ直下の `shikomi/` の順で
/// vault ディレクトリパスを解決する。
///
/// `SqliteVaultRepository::new()` の後方互換ラッパ用。新規の CLI / GUI コードは
/// 本関数ではなく `SqliteVaultRepository::from_directory(&Path)` を直接使うこと
/// （env 解決の真実源は clap attribute 側に寄せる、
/// `docs/features/cli-vault-commands/detailed-design/infra-changes.md §変更 2`）。
///
/// # Errors
/// `dirs::data_dir()` が `None` かつ `SHIKOMI_VAULT_DIR` 未設定の場合
/// `PersistenceError::CannotResolveVaultDir` を返す。
pub(crate) fn resolve_os_default_or_env() -> Result<PathBuf, PersistenceError> {
    if let Ok(val) = std::env::var(ENV_VAR_VAULT_DIR) {
        Ok(PathBuf::from(val))
    } else {
        dirs::data_dir()
            .ok_or(PersistenceError::CannotResolveVaultDir)
            .map(|base| base.join(APP_SUBDIR_NAME))
    }
}

// -------------------------------------------------------------------
// 定数
// -------------------------------------------------------------------

/// vault DB ファイル名。
pub const VAULT_DB_FILENAME: &str = "vault.db";

/// アトミック書き込み中間ファイル名。
pub const VAULT_DB_NEW_FILENAME: &str = "vault.db.new";

/// ロックファイル名。
pub const VAULT_DB_LOCK_FILENAME: &str = "vault.db.lock";

/// vault ディレクトリを上書きする環境変数名。
pub const ENV_VAR_VAULT_DIR: &str = "SHIKOMI_VAULT_DIR";

/// XDG data dir 以下のアプリサブディレクトリ名。
pub const APP_SUBDIR_NAME: &str = "shikomi";

/// アクセスを禁止するシステム領域のプレフィックス一覧（Unix）。
#[cfg(unix)]
const PROTECTED_PATH_PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/etc", "/boot", "/root"];

/// アクセスを禁止するシステム領域のプレフィックス一覧（Windows）。
#[cfg(windows)]
const PROTECTED_PATH_PREFIXES: &[&str] = &[
    r"C:\Windows",
    r"C:\Program Files",
    r"C:\Program Files (x86)",
];

/// 非 unix/windows 環境ではプレフィックス一覧を空にする。
#[cfg(not(any(unix, windows)))]
const PROTECTED_PATH_PREFIXES: &[&str] = &[];

// -------------------------------------------------------------------
// VaultPaths
// -------------------------------------------------------------------

/// vault ディレクトリとその配下のファイルパスをまとめて管理する型。
///
/// `new` は 7 ステップの検証を行い、問題があれば [`PersistenceError`] を返す。
pub struct VaultPaths {
    dir: PathBuf,
    vault_db: PathBuf,
    vault_db_new: PathBuf,
    vault_db_lock: PathBuf,
}

impl VaultPaths {
    /// `dir` を検証し、`VaultPaths` を構築する。
    ///
    /// # Errors
    ///
    /// - `dir` が絶対パスでない: `InvalidVaultDir { reason: NotAbsolute }`
    /// - `dir` に `..` が含まれる: `InvalidVaultDir { reason: PathTraversal }`
    /// - `dir` がシンボリックリンク: `InvalidVaultDir { reason: SymlinkNotAllowed }`
    /// - `dir` が既存かつディレクトリでない: `InvalidVaultDir { reason: NotADirectory }`
    /// - canonicalize 失敗: `InvalidVaultDir { reason: Canonicalize { source } }`
    /// - 保護システム領域: `InvalidVaultDir { reason: ProtectedSystemArea { prefix } }`
    pub fn new(dir: PathBuf) -> Result<Self, PersistenceError> {
        // Step 1: 絶対パスチェック
        if !dir.is_absolute() {
            return Err(PersistenceError::InvalidVaultDir {
                path: dir,
                reason: VaultDirReason::NotAbsolute,
            });
        }

        // Step 2: `..` コンポーネントチェック
        if dir.components().any(|c| c == Component::ParentDir) {
            return Err(PersistenceError::InvalidVaultDir {
                path: dir,
                reason: VaultDirReason::PathTraversal,
            });
        }

        // Step 3: symlink_metadata でパスの存在・種別を調べる
        match std::fs::symlink_metadata(&dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // パスが存在しない（新規作成予定）→ シンボリックリンク/ディレクトリチェック不要
                // Step 5 (非存在パス向け): 保護されたシステム領域チェック（canonicalize 不要の raw パスで評価）
                for &prefix in PROTECTED_PATH_PREFIXES {
                    if dir.starts_with(prefix) {
                        return Err(PersistenceError::InvalidVaultDir {
                            path: dir,
                            reason: VaultDirReason::ProtectedSystemArea { prefix },
                        });
                    }
                }
                // Step 7: 子パスを導出して返す
                return Ok(Self::derive(dir));
            }
            Err(e) => {
                return Err(PersistenceError::Io {
                    path: dir,
                    source: e,
                });
            }
            Ok(meta) => {
                if meta.is_symlink() {
                    return Err(PersistenceError::InvalidVaultDir {
                        path: dir,
                        reason: VaultDirReason::SymlinkNotAllowed,
                    });
                }
                if !meta.is_dir() {
                    return Err(PersistenceError::InvalidVaultDir {
                        path: dir,
                        reason: VaultDirReason::NotADirectory,
                    });
                }
            }
        }

        // Step 4: canonicalize（パスが存在するので安全に呼び出せる）
        let canonical =
            std::fs::canonicalize(&dir).map_err(|source| PersistenceError::InvalidVaultDir {
                path: dir.clone(),
                reason: VaultDirReason::Canonicalize { source },
            })?;

        // Step 5: 保護されたシステム領域チェック
        for &prefix in PROTECTED_PATH_PREFIXES {
            if canonical.starts_with(prefix) {
                return Err(PersistenceError::InvalidVaultDir {
                    path: canonical,
                    reason: VaultDirReason::ProtectedSystemArea { prefix },
                });
            }
        }

        // Step 6: canonical path を使って子パスを導出
        Ok(Self::derive(canonical))
    }

    /// 検証をスキップして `VaultPaths` を構築する（テスト・内部用途のみ）。
    #[cfg(test)]
    pub(crate) fn new_unchecked(dir: PathBuf) -> Self {
        Self::derive(dir)
    }

    /// 子パスを導出する共通ロジック。
    fn derive(dir: PathBuf) -> Self {
        let vault_db = dir.join(VAULT_DB_FILENAME);
        let vault_db_new = dir.join(VAULT_DB_NEW_FILENAME);
        let vault_db_lock = dir.join(VAULT_DB_LOCK_FILENAME);
        Self {
            dir,
            vault_db,
            vault_db_new,
            vault_db_lock,
        }
    }

    /// vault ディレクトリへの参照を返す。
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `vault.db` のパスへの参照を返す。
    #[must_use]
    pub fn vault_db(&self) -> &Path {
        &self.vault_db
    }

    /// `vault.db.new` のパスへの参照を返す。
    #[must_use]
    pub fn vault_db_new(&self) -> &Path {
        &self.vault_db_new
    }

    /// `vault.db.lock` のパスへの参照を返す。
    #[must_use]
    pub fn vault_db_lock(&self) -> &Path {
        &self.vault_db_lock
    }
}

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --- TC-U01: VaultPaths::new — 正常パスのパス導出 ---

    #[test]
    fn tc_u01_vault_paths_new_valid_dir() {
        let dir = TempDir::new().unwrap();
        let paths = VaultPaths::new(dir.path().to_path_buf()).unwrap();

        assert_eq!(paths.dir(), dir.path().canonicalize().unwrap());
        assert_eq!(
            paths.vault_db(),
            dir.path().canonicalize().unwrap().join("vault.db")
        );
        assert_eq!(
            paths.vault_db_new(),
            dir.path().canonicalize().unwrap().join("vault.db.new")
        );
        assert_eq!(
            paths.vault_db_lock(),
            dir.path().canonicalize().unwrap().join("vault.db.lock")
        );
    }

    // --- TC-U13: VaultPaths::new — 相対パス → NotAbsolute ---

    #[test]
    fn tc_u13_relative_path_not_absolute() {
        let result = VaultPaths::new(PathBuf::from("relative/path"));
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidVaultDir {
                    ref path,
                    reason: VaultDirReason::NotAbsolute
                }) if path.as_path() == Path::new("relative/path")
            ),
            "NotAbsolute を期待したが Err={:?}",
            result.as_ref().err()
        );
    }

    // --- TC-U14: VaultPaths::new — `..` 含むパス → PathTraversal ---

    /// TC-U14 (Unix) — Unix 絶対パスに `..` が含まれる → PathTraversal
    #[cfg(unix)]
    #[test]
    fn tc_u14_path_traversal_rejected() {
        let result = VaultPaths::new(PathBuf::from("/tmp/shikomi/../../etc/passwd"));
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidVaultDir {
                    reason: VaultDirReason::PathTraversal,
                    ..
                })
            ),
            "PathTraversal を期待したが Err={:?}",
            result.as_ref().err()
        );
    }

    /// TC-U14 (Windows) — Windows 絶対パスに `..` が含まれる → PathTraversal
    ///
    /// Unix パス `/tmp/shikomi/../../etc/passwd` は Windows では絶対パスと判定されないため
    /// `NotAbsolute` になる。代わりに `C:\foo\bar\..\..\etc\passwd` を使う。
    #[cfg(windows)]
    #[test]
    fn tc_u14_path_traversal_rejected() {
        let result = VaultPaths::new(PathBuf::from(r"C:\foo\bar\..\..\etc\passwd"));
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidVaultDir {
                    reason: VaultDirReason::PathTraversal,
                    ..
                })
            ),
            "PathTraversal を期待したが Err={:?}",
            result.as_ref().err()
        );
    }

    // --- TC-U15: VaultPaths::new — シンボリックリンク → SymlinkNotAllowed（Unix）---

    #[cfg(unix)]
    #[test]
    fn tc_u15_symlink_not_allowed() {
        let dir = TempDir::new().unwrap();
        let real_dir = dir.path().join("real");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_path = dir.path().join("link");
        std::os::unix::fs::symlink(&real_dir, &symlink_path).unwrap();

        let result = VaultPaths::new(symlink_path.clone());
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidVaultDir {
                    reason: VaultDirReason::SymlinkNotAllowed,
                    ..
                })
            ),
            "SymlinkNotAllowed を期待したが Err={:?}",
            result.as_ref().err()
        );
    }

    // --- TC-U16: VaultPaths::new — 保護領域 → ProtectedSystemArea（Unix）---

    #[cfg(unix)]
    #[test]
    fn tc_u16_protected_system_area() {
        let result = VaultPaths::new(PathBuf::from("/etc/shikomi_test_dir"));
        assert!(
            matches!(
                result,
                Err(PersistenceError::InvalidVaultDir {
                    reason: VaultDirReason::ProtectedSystemArea { prefix: "/etc" },
                    ..
                })
            ),
            "ProtectedSystemArea を期待したが Err={:?}",
            result.as_ref().err()
        );
    }
}
