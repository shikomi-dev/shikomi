//! OS 自動起動管理モジュール（Sub-B Issue #127）。
//!
//! 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/backend-trait.md

use std::path::PathBuf;

// -------------------------------------------------------------------
// AutostartBackend trait
// -------------------------------------------------------------------

/// OS 自動起動バックエンドの統一インターフェース。
///
/// 各 OS バックエンド（launchd / systemd / XDG / Task Scheduler）が実装する。
/// 設計根拠: backend-trait.md §AutostartBackend trait 型シグネチャ
pub trait AutostartBackend {
    /// バックエンド識別名（tracing::info! の `backend=` フィールドで使用）。
    fn name(&self) -> &'static str;

    /// OS 自動起動に daemon を登録する。冪等（登録済みなら再登録せず Ok を返す）。
    ///
    /// # Errors
    /// 登録に失敗した場合 `AutostartError` を返す。
    fn install(&self) -> Result<(), AutostartError>;

    /// OS 自動起動登録を解除する。冪等（未登録なら Ok を返す）。
    ///
    /// # Errors
    /// 解除に失敗した場合 `AutostartError` を返す。
    fn uninstall(&self) -> Result<(), AutostartError>;

    /// 自動起動登録状態を返す。probe 失敗時は `false`（Fail Safe）。
    fn is_registered(&self) -> bool;

    /// install 成功時に stdout へ追記する OS 固有の hint（None なら追記なし）。
    fn install_hint(&self) -> Option<String> {
        None
    }
}

// -------------------------------------------------------------------
// AutostartError
// -------------------------------------------------------------------

/// autostart 操作のエラー型。
///
/// 設計根拠: backend-trait.md §AutostartError 型定義
#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    /// 外部コマンド実行失敗。
    #[error("command failed: `{cmd}`: {stderr_excerpt}")]
    CommandFailed {
        cmd: String,
        /// stderr の最初の 80 文字のみ（secret 非含有 / security.md §脅威モデル「stderr 情報漏洩」）
        stderr_excerpt: String,
    },

    /// I/O エラー。
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// 未サポートプラットフォーム。
    #[error("unsupported: {reason}")]
    Unsupported { reason: String },
}

// -------------------------------------------------------------------
// resolve_daemon_path
// -------------------------------------------------------------------

/// `exe_dir` を直接受け取って `shikomi-daemon` パスを解決する内部ヘルパー。
///
/// `current_exe()` に依存しないため unit test でパス注入が可能。
/// `resolve_daemon_path()` の実装本体。
///
/// 設計根拠: test-design/unit.md §TC-UT-164〜165 実装メモ
///
/// # Errors
/// `shikomi-daemon` バイナリが存在しない場合 `AutostartError::IoError(NotFound)` を返す。
pub(crate) fn resolve_daemon_path_from(
    exe_dir: &std::path::Path,
) -> Result<PathBuf, AutostartError> {
    let daemon_name = if cfg!(target_os = "windows") {
        "shikomi-daemon.exe"
    } else {
        "shikomi-daemon"
    };
    let daemon_path = exe_dir.join(daemon_name);
    // Fail Fast: バイナリ不在なら即時失敗（backend-trait.md §resolve_daemon_path() Step 6）
    if !daemon_path.exists() {
        return Err(AutostartError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "shikomi-daemon binary not found",
        )));
    }
    Ok(daemon_path)
}

/// `shikomi-daemon` バイナリの絶対パスを解決する共通ヘルパー。
///
/// autostart 登録ファイルに書き込む起動パスを返す。
///
/// **Fail Fast**: バイナリが現時点で存在しない場合は即座に
/// `AutostartError::IoError(NotFound)` を返す。
/// 悪意あるバイナリ置換攻撃への対策として `canonicalize()` 後に `exists()` を確認する
/// （設計根拠: security.md §脅威モデル「resolve_daemon_path が悪意あるバイナリを解決」）。
///
/// 設計根拠: backend-trait.md §resolve_daemon_path() 共通ヘルパー
///
/// # Errors
/// - 実行ファイルのディレクトリ解決に失敗した場合 `AutostartError::IoError` を返す。
/// - `shikomi-daemon` バイナリが存在しない場合 `AutostartError::IoError(NotFound)` を返す。
pub fn resolve_daemon_path() -> Result<PathBuf, AutostartError> {
    let exe = std::env::current_exe()?;
    // canonicalize でシンボリックリンクを解決（シンボリックリンク攻撃防止 / security.md §脅威モデル）
    let real = exe.canonicalize()?;
    let dir = real.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine binary directory",
        )
    })?;
    resolve_daemon_path_from(dir)
}

// -------------------------------------------------------------------
// unit tests — TC-UT-164〜165
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    /// TC-UT-164: daemon バイナリ存在時に `Ok(PathBuf)` を返すこと
    ///
    /// 対応要件: REQ-DDM-010〜017
    /// 受入基準: AC-DDM-07
    /// 設計根拠: test-design/unit.md §5.2 TC-UT-164
    #[test]
    fn tc_ut_164_resolve_daemon_path_from_returns_ok_when_binary_exists() {
        let dir = tempfile::tempdir().expect("tempdir 作成失敗");
        let daemon_name = if cfg!(target_os = "windows") {
            "shikomi-daemon.exe"
        } else {
            "shikomi-daemon"
        };
        std::fs::write(dir.path().join(daemon_name), b"").expect("ダミーバイナリ書き込み失敗");

        let result = resolve_daemon_path_from(dir.path());
        assert!(result.is_ok(), "daemon 存在時は Ok を返すべき: {result:?}");
        assert!(result.unwrap().exists(), "返却パスが実際に存在すること");
    }

    /// TC-UT-165: daemon バイナリ不在時に `AutostartError::IoError(NotFound)` を返すこと
    ///
    /// 対応要件: REQ-DDM-010〜017
    /// 受入基準: AC-DDM-07
    /// 設計根拠: test-design/unit.md §5.2 TC-UT-165
    #[test]
    fn tc_ut_165_resolve_daemon_path_from_returns_not_found_when_binary_absent() {
        let dir = tempfile::tempdir().expect("tempdir 作成失敗");
        // shikomi-daemon を作成しない（空ディレクトリのまま）

        let result = resolve_daemon_path_from(dir.path());
        assert!(
            matches!(&result, Err(AutostartError::IoError(e)) if e.kind() == io::ErrorKind::NotFound),
            "daemon 不在時は AutostartError::IoError(NotFound) を返すべき: {result:?}"
        );
    }
}

// -------------------------------------------------------------------
// detect
// -------------------------------------------------------------------

/// OS を判定して適切な AutostartBackend 実装を返す（コンパイル時 #[cfg] 分岐）。
///
/// 設計根拠: backend-trait.md §detect() 関数定義
#[must_use]
pub fn detect() -> Box<dyn AutostartBackend> {
    _detect_impl()
}

#[cfg(target_os = "macos")]
fn _detect_impl() -> Box<dyn AutostartBackend> {
    Box::new(launchd::LaunchdBackend)
}

#[cfg(target_os = "windows")]
fn _detect_impl() -> Box<dyn AutostartBackend> {
    Box::new(windows::WindowsTaskSchedulerBackend)
}

#[cfg(target_os = "linux")]
fn _detect_impl() -> Box<dyn AutostartBackend> {
    if systemd::SystemdBackend::is_available() {
        Box::new(systemd::SystemdBackend)
    } else {
        Box::new(xdg::XdgAutostartBackend)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn _detect_impl() -> Box<dyn AutostartBackend> {
    Box::new(UnsupportedBackend)
}

// -------------------------------------------------------------------
// UnsupportedBackend（FreeBSD 等）
// -------------------------------------------------------------------

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
struct UnsupportedBackend;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl AutostartBackend for UnsupportedBackend {
    fn name(&self) -> &'static str {
        "UnsupportedBackend"
    }

    fn install(&self) -> Result<(), AutostartError> {
        Err(AutostartError::Unsupported {
            reason: "platform not supported".to_owned(),
        })
    }

    fn uninstall(&self) -> Result<(), AutostartError> {
        Err(AutostartError::Unsupported {
            reason: "platform not supported".to_owned(),
        })
    }

    fn is_registered(&self) -> bool {
        false
    }
}

// -------------------------------------------------------------------
// モジュール宣言
// -------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub mod launchd;

#[cfg(target_os = "linux")]
pub mod systemd;

#[cfg(target_os = "linux")]
pub mod xdg;

#[cfg(target_os = "windows")]
pub mod windows;

// -------------------------------------------------------------------
// stderr_excerpt ヘルパー（コマンド失敗時の stderr を 80 文字に切り詰める）
// -------------------------------------------------------------------

/// stderr バイト列を lossy UTF-8 変換して先頭 80 文字に切り詰める。
///
/// マルチバイト境界を `char_indices` で考慮する。
pub(super) fn truncate_stderr(stderr: &[u8]) -> String {
    let s = String::from_utf8_lossy(stderr);
    let end = s.char_indices().nth(80).map(|(i, _)| i).unwrap_or(s.len());
    s[..end].to_owned()
}
