//! Windows ACL 経由の出力ファイル所有者限定 (Sub-F #44 Phase 7)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §セキュリティ設計 §`--output print` / `--output braille` の一時ファイル /
//!   リダイレクト対策 (Rust では `nix::sys::stat::umask` / Windows は ACL 経由)
//!
//! 設計判断 (Phase 7):
//! - Unix では `accessibility::umask::with_secure_umask` が umask(0o077) で
//!   リダイレクト先 0600 相当を保証している。本モジュールは **Windows 限定の
//!   ACL 経路** で同等の所有者限定を実現する責務を持つ。
//! - Windows: ユーザの `> recovery.pdf` リダイレクトは shell が `CreateFile` で
//!   作成するため、shikomi-cli 側からファイルパスが見えない (stdout 経由のため)。
//!   そのため Phase 7 では **「リダイレクト後にユーザに ACL 設定を案内する」
//!   MSG 出力** を提供する形に留める (本実装スタブ + ガイダンス文言)。
//! - 将来 minor で **`shikomi vault rekey --output print --to-file <path>`**
//!   フラグを追加した場合、本モジュールで `windows_sys::Win32::Security` の
//!   `SetSecurityInfo` 経由で DACL を設定する経路を本実装する。
//!
//! 不変条件:
//! - Unix では完全 noop (`with_secure_umask` 経由で既に保護済み)。
//! - Windows でも現状はガイダンス文言のみ返す。実 ACL 設定は将来 minor。

/// Windows のリダイレクト先ファイルに対するユーザ案内文言を返す (英 / 日)。
/// Unix では空文字を返す (umask で既に保護済みのため不要)。
#[must_use]
pub fn redirect_acl_hint() -> &'static str {
    redirect_acl_hint_impl()
}

#[cfg(windows)]
fn redirect_acl_hint_impl() -> &'static str {
    "hint: on Windows, the file created by '> recovery.pdf' uses default ACL.\n\
     hint: tighten access by running: icacls recovery.pdf /inheritance:r /grant:r \"%USERNAME%:F\"\n"
}

#[cfg(not(windows))]
fn redirect_acl_hint_impl() -> &'static str {
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    /// shape: 関数シグネチャが `&'static str` を返す (no allocation, テスト容易)。
    #[test]
    fn test_redirect_acl_hint_signature() {
        let _: fn() -> &'static str = redirect_acl_hint;
    }

    /// Unix では空文字 (umask で既に保護済み)。
    #[cfg(unix)]
    #[test]
    fn test_redirect_acl_hint_empty_on_unix() {
        assert_eq!(redirect_acl_hint(), "");
    }

    /// Windows ではガイダンス文言が含まれる。
    #[cfg(windows)]
    #[test]
    fn test_redirect_acl_hint_contains_icacls_on_windows() {
        let hint = redirect_acl_hint();
        assert!(hint.contains("icacls"), "expected icacls hint, got: {hint}");
    }
}
