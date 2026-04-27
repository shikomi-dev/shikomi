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

    /// rename retry 試行を監査ログに記録する（Issue #65、`AtomicWriter` の Win retry 補強）。
    ///
    /// 設計根拠:
    /// - `docs/features/vault-persistence/basic-design/security.md`
    ///   §atomic write の二次防衛線 §retry 監査ログ
    /// - 同 §監査ログ規約 §rename retry 発火 / §rename retry 全敗
    ///
    /// `outcome` の意味と発行レベル:
    ///
    /// | `outcome`     | レベル  | 発行タイミング                                |
    /// |---------------|--------|---------------------------------------------|
    /// | `"pending"`   | `warn` | 各 retry 試行直前（sleep + 再 rename の前）  |
    /// | `"succeeded"` | `warn` | retry の rename 成功直後                     |
    /// | `"exhausted"` | `error`| 5 回全敗で `AtomicWriteFailed` 返却直前      |
    ///
    /// シグネチャは `&'static str` / `u32` / `i32` / `u64` / `&'static str` のみで秘密値を含まない
    /// （§秘密値マスクの型保証 §防衛線 と整合）。daemon 側 subscriber は本イベント頻度から DoS 兆候を
    /// 検知し OWASP A09 連携で上位通報する（別 Issue 範疇、本 crate は emit 側責務のみ）。
    ///
    /// 本関数の実呼出は `cfg(windows)` rename retry 経由のみだが、API としては全プラットフォームで
    /// 公開する（テスト・将来の他経路再利用を想定）。非 Windows ビルドの dead_code 警告を抑制する。
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn retry_event(
        stage: &'static str,
        attempt: u32,
        raw_os_error: i32,
        elapsed_ms: u64,
        outcome: &'static str,
    ) {
        if outcome == "exhausted" {
            tracing::error!(
                stage,
                attempt,
                raw_os_error,
                elapsed_ms,
                outcome,
                "persistence: rename retry exhausted"
            );
        } else {
            tracing::warn!(
                stage,
                attempt,
                raw_os_error,
                elapsed_ms,
                outcome,
                "persistence: rename retry event"
            );
        }
    }
}
