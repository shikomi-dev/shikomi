//! `--output {print,braille}` 経路の umask 内部適用（C-A05、Sub-F #44 Phase 6）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §セキュリティ設計 §`--output print` / `--output braille` の一時ファイル /
//!   リダイレクト対策
//!   「shikomi-cli 実装が `--output {print,braille}` 経路で `umask(0o077)` を
//!    出力直前に内部適用してから stdout 書出を行う」
//!
//! 不変条件:
//! - 出力直前に **既存プロセス umask を保存 → `0o077` に設定 → 出力 → 元値に復元**
//!   する RAII パターン (`with_secure_umask`) で囲む。早期 return / panic でも復元される。
//! - これにより `> recovery.brf` のリダイレクト先ファイルは所有者限定 0600 相当で
//!   生成され、後続の他プロセス読取を OS 層で構造的に拒否する (TC-F-A05 機械検証)。
//! - Windows は ACL ベースの権限モデルで umask 概念なし。`with_secure_umask` は
//!   no-op 経路で本体クロージャをそのまま実行する (`cfg` 分岐)。
//!
//! 副作用範囲:
//! - umask はプロセス全体の global state。並列スレッドが同時に `with_secure_umask`
//!   を呼ぶとレース。Sub-F の usecase は単一スレッドの run() 経路で順次呼ばれる
//!   ため Phase 6 では並列化不要。将来並列化する場合は `Mutex` で囲む。

use crate::error::CliError;

/// 0o077 umask を一時的に適用してクロージャを実行し、終了後に umask を復元する。
///
/// クロージャ内で発生した IO エラーは `CliError::Persistence` で透過する。
/// 早期 return / panic でも RAII の `Drop` で umask 復元される。
///
/// # Errors
/// クロージャ自身が返す `CliError`、または umask 復元失敗時の OS エラー。
pub fn with_secure_umask<F, T>(body: F) -> Result<T, CliError>
where
    F: FnOnce() -> Result<T, CliError>,
{
    with_secure_umask_impl(body)
}

#[cfg(unix)]
fn with_secure_umask_impl<F, T>(body: F) -> Result<T, CliError>
where
    F: FnOnce() -> Result<T, CliError>,
{
    use nix::sys::stat::{umask, Mode};
    // 既存 umask を 0o077 に置換 + 旧値返却。`Drop` で旧値復元する RAII guard。
    let previous = umask(Mode::from_bits_truncate(0o077));
    let _guard = UmaskGuard {
        previous: Some(previous),
    };
    body()
}

#[cfg(unix)]
struct UmaskGuard {
    previous: Option<nix::sys::stat::Mode>,
}

#[cfg(unix)]
impl Drop for UmaskGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.previous.take() {
            // 復元失敗時は warn ログのみ。ここで panic すると Drop 中の二重 panic で
            // プロセスが abort するため、Fail Kindly で握り潰す (Phase 6 契約)。
            let _ = nix::sys::stat::umask(prev);
        }
    }
}

#[cfg(not(unix))]
fn with_secure_umask_impl<F, T>(body: F) -> Result<T, CliError>
where
    F: FnOnce() -> Result<T, CliError>,
{
    // Windows: umask 概念がない (ACL ベース)。Phase 7 で ACL 経由の権限制御を
    // 検討するが、Phase 6 では no-op 経路で本体をそのまま実行する。
    body()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_secure_umask_runs_body_and_returns_value() {
        let result = with_secure_umask(|| Ok::<u32, CliError>(42));
        assert!(matches!(result, Ok(42)));
    }

    #[test]
    fn test_with_secure_umask_propagates_inner_error() {
        let result = with_secure_umask(|| Err::<u32, _>(CliError::NonInteractivePassword));
        assert!(matches!(result, Err(CliError::NonInteractivePassword)));
    }

    /// Unix では umask 適用前後でプロセス umask が復元される (RAII Drop で旧値復元)。
    /// Windows では no-op で副作用なし。
    #[cfg(unix)]
    #[test]
    fn test_unix_umask_restored_after_body_completion() {
        use nix::sys::stat::{umask, Mode};
        // 既存 umask を取得 (umask 関数は新値設定 + 旧値返却なので 2 回呼んで初期値復元)。
        let initial = umask(Mode::from_bits_truncate(0o022));
        let _ = umask(initial); // すぐ戻す
        let result = with_secure_umask(|| Ok::<(), CliError>(()));
        assert!(result.is_ok());
        // body 終了後、旧値 (initial) に戻っているか確認。
        let after = umask(Mode::from_bits_truncate(0o022));
        let _ = umask(after);
        // Drop で復元されているなら after == initial。
        assert_eq!(after.bits(), initial.bits());
    }
}
