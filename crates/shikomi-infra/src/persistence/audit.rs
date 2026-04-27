//! 永続化操作の監査ログ。
//!
//! `Audit` はゼロサイズ型で load/save の開始・成功・失敗を `tracing` に記録する。
//! 秘密値を含まないエラー表示のみを行う。

use shikomi_core::ProtectionMode;

use super::error::PersistenceError;
use super::paths::VaultPaths;

// -------------------------------------------------------------------
// RetryOutcome (Issue #65、Tell-Don't-Ask 化)
// -------------------------------------------------------------------

/// rename retry 監査イベントの結末を型レベルで表現する列挙（`Audit::retry_event` の `outcome` 引数）。
///
/// 設計根拠:
/// - `docs/features/vault-persistence/basic-design/security.md`
///   §atomic write の二次防衛線 §retry 監査ログ
///
/// 当初は `outcome: &'static str` で `"pending" / "succeeded" / "exhausted"` を渡す API
/// だったが、文字列 switch のタイポ即バグの罠を構造的に塞ぐため列挙化した
/// （ペテルギウス再レビュー指摘 §Tell-Don't-Ask）。さらにペテルギウス工程5 再レビュー指摘で
/// 「設計書 `data.md` §RetryOutcome / `security.md` §retry監査ログ が `Display` 契約を約束
/// しているのに実装は `as_str()` メソッドだった」整合違反を解消するため、`Display` を実装し
/// `as_str()` を撤去した（Tell, Don't Ask: 値自身がフォーマットを知る）。
///
/// `tracing` には `outcome = %outcome` の `%` 表記で渡す:
/// `"pending" / "succeeded" / "exhausted"` の wire format を bit-exact 維持し、
/// 既存テスト (`integration_windows_retry.rs` `logs_contain(r#"outcome=\"pending\""#)` 等)
/// と互換である。
///
/// 秘密値非含有: 全 variant が unit variant で値を持たないため、
/// `§監査ログ規約 §秘密値マスクの型保証` は維持される。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum RetryOutcome {
    /// 各 retry 試行直前（sleep + 再 rename の前）。`warn` レベルで emit。
    Pending,
    /// retry の rename 成功直後。`warn` レベルで emit。
    Succeeded,
    /// `MAX_RETRIES` 回全敗で `AtomicWriteFailed` 返却直前。`error` レベルで emit。
    /// daemon 側 subscriber が DoS 兆候として OWASP A09 連携で上位通報する起点。
    Exhausted,
}

impl std::fmt::Display for RetryOutcome {
    /// tracing `outcome = %outcome` 経由で出力する wire format。
    ///
    /// 設計書 `detailed-design/data.md` §`RetryOutcome` で約束した `Display` 契約。
    /// `logs_contain` テスト (`integration_windows_retry.rs`) の文字列マッチ互換のため
    /// `"pending" / "succeeded" / "exhausted"` 完全固定（変更時は subscriber / テスト両方を破壊する
    /// SSoT 違反として却下対象）。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Exhausted => "exhausted",
        };
        f.write_str(s)
    }
}

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
    /// `outcome` は `RetryOutcome` 列挙で型レベルに昇格済（タイポ即バグ防止、Tell-Don't-Ask）。
    /// 発行レベルは `match outcome` で網羅判定し、新 variant 追加時に compile error で気付ける構造。
    ///
    /// | `outcome`              | レベル  | 発行タイミング                                |
    /// |------------------------|--------|---------------------------------------------|
    /// | `RetryOutcome::Pending`   | `warn` | 各 retry 試行直前（sleep + 再 rename の前）  |
    /// | `RetryOutcome::Succeeded` | `warn` | retry の rename 成功直後                     |
    /// | `RetryOutcome::Exhausted` | `error`| 5 回全敗で `AtomicWriteFailed` 返却直前      |
    ///
    /// シグネチャは `&'static str` / `u32` / `i32` / `u64` / `RetryOutcome` のみで秘密値を含まない
    /// （§秘密値マスクの型保証 §防衛線 と整合、`RetryOutcome` も unit variant のみで値非保持）。
    /// daemon 側 subscriber は本イベント頻度から DoS 兆候を検知し OWASP A09 連携で上位通報する
    /// （別 Issue 範疇、本 crate は emit 側責務のみ）。
    ///
    /// tracing 出力の `outcome="..."` 文字列は `RetryOutcome` の `Display` 実装経由で
    /// `"pending" / "succeeded" / "exhausted"` を維持（`integration_windows_retry.rs` の
    /// `logs_contain` アサーション互換、`detailed-design/data.md` §RetryOutcome SSoT）。
    ///
    /// 本関数の実呼出は `cfg(windows)` rename retry 経由のみだが、API としては全プラットフォームで
    /// 公開する（テスト・将来の他経路再利用を想定）。非 Windows ビルドの dead_code 警告を抑制する。
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn retry_event(
        stage: &'static str,
        attempt: u32,
        raw_os_error: i32,
        elapsed_ms: u64,
        outcome: RetryOutcome,
    ) {
        match outcome {
            RetryOutcome::Exhausted => {
                tracing::error!(
                    stage,
                    attempt,
                    raw_os_error,
                    elapsed_ms,
                    outcome = %outcome,
                    "persistence: rename retry exhausted"
                );
            }
            RetryOutcome::Pending | RetryOutcome::Succeeded => {
                tracing::warn!(
                    stage,
                    attempt,
                    raw_os_error,
                    elapsed_ms,
                    outcome = %outcome,
                    "persistence: rename retry event"
                );
            }
        }
    }
}
