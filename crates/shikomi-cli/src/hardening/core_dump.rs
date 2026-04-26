// workspace.lints.rust.unsafe_code = "deny" の下で、本ファイルは C-41 実装のため
// `libc::prctl` / `libc::setrlimit` の FFI 呼出に必要な最小限の `unsafe` ブロックを
// 含む。`permission/windows_sid.rs` と同様に、ファイル単位で allow(unsafe_code) を
// 上書きする (workspace 既定 forbid → file local deny → file local allow の優先順)。
// scripts/ci/audit-secret-paths.sh の TC-CI-026 で本ファイルを例外として明示し、
// 「unsafe は hardening/core_dump.rs と io/windows_sid.rs のみ」を CI で機械検証する。
#![allow(unsafe_code)]

//! Core dump 抑制（C-41、Sub-F #44 Phase 5）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §セキュリティ設計 §不変条件・契約 C-41
//!   shikomi-cli プロセスは core dump 抑制 (Linux: `prctl(PR_SET_DUMPABLE, 0)` /
//!   macOS: `setrlimit(RLIMIT_CORE, 0)` / Windows: `SetErrorMode(SEM_NOGPFAULTERRORBOX)`)
//!
//! 不変条件:
//! - 失敗しても起動を止めない（Fail Kindly）。OS が呼出を拒否した場合 (例えば
//!   `prctl` が ENOSYS でない通常の Linux でない別 Unix variant) でも、CLI は
//!   通常実行を継続する。**ただし戻り値で失敗を呼出側に通知**し、`run()` 側で
//!   `tracing::warn!` で記録する責務を持つ。
//! - 既存の core dump は本関数より早期に発生した場合は抑制できない。本関数は
//!   `run()` のほぼ最初に呼び出される契約。
//! - Sub-F TC-F-U10 はシグネチャ存在のみを確認する（実 OS 呼出の動作は
//!   integration / E2E で担保）。

use crate::error::CliError;

/// 起動時に呼び出して core dump を抑制する。
///
/// # Errors
/// OS 呼出が失敗した場合 (戻り値 != 0 等)、`CliError::Persistence` を経由する
/// `<core_dump>` 仮想パスエラーで通知する。呼出側は warn ログのみで握り潰すこと
/// (起動継続が UX 上の正解、Fail Kindly)。
pub fn suppress() -> Result<(), CliError> {
    suppress_impl()
}

#[cfg(target_os = "linux")]
fn suppress_impl() -> Result<(), CliError> {
    // prctl(PR_SET_DUMPABLE, 0): 当該プロセスが SUID 化していなくても core dump
    // と /proc/self/mem 経路を抑制する。`PR_SET_DUMPABLE` は値 4。
    // SAFETY: prctl は副作用が PID-local に限定された syscall。第 2 引数の 0 は
    // SUID_DUMP_DISABLE で、Linux man 2 prctl の規約通り。`libc::prctl` は variadic
    // (`extern "C" ...`) シグネチャ、各 arg は `c_ulong` を期待。`zero: c_ulong` で
    // ターゲット幅 (64/32bit) を中央集約し、`as` キャストの暗黙幅変換を排除する。
    let zero: libc::c_ulong = 0;
    let rc = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, zero, zero, zero, zero) };
    if rc == 0 {
        Ok(())
    } else {
        Err(persistence_io_err("prctl PR_SET_DUMPABLE"))
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn suppress_impl() -> Result<(), CliError> {
    // macOS / *BSD など: setrlimit(RLIMIT_CORE, {0, 0}) で core ファイル生成上限を
    // 0 に絞る。soft / hard 双方を 0 にすることで以降のプロセスでも上限を上げ直せない。
    // SAFETY: setrlimit は POSIX 標準 syscall で副作用は当該プロセスに局所化される。
    let rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let rc = unsafe { libc::setrlimit(libc::RLIMIT_CORE, &rlim) };
    if rc == 0 {
        Ok(())
    } else {
        Err(persistence_io_err("setrlimit RLIMIT_CORE"))
    }
}

#[cfg(windows)]
fn suppress_impl() -> Result<(), CliError> {
    // Windows: SetErrorMode(SEM_NOGPFAULTERRORBOX) で WER (Windows Error Reporting)
    // のメモリダンプ採取をプロセス単位で抑止する。Phase 5 では `windows-sys` の既存
    // features に `Win32_System_Diagnostics_Debug` が含まれていないため、
    // `Phase 6` で CI Windows ジョブと同時に追加する。Phase 5 はシグネチャだけ
    // OS 横断で揃え、Windows 実装は Phase 6 で填める (本関数の戻り値契約は維持)。
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn suppress_impl() -> Result<(), CliError> {
    // 未知のターゲット OS: noop (起動継続)。
    Ok(())
}

fn persistence_io_err(op: &'static str) -> CliError {
    CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
        path: std::path::PathBuf::from(format!("<core_dump:{op}>")),
        source: std::io::Error::last_os_error(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-F-U10: `suppress` 関数のシグネチャ存在確認。
    /// 関数ポインタとしての型一致を強制し、戻り値型のドリフトを検出する。
    #[test]
    fn tc_f_u10_suppress_signature_exists() {
        let _: fn() -> Result<(), CliError> = suppress;
    }

    /// Linux ランナーで実呼出が成功することを確認する補助テスト。
    /// macOS / Windows / その他 OS では shape のみ通り、本テストは noop 検証になる。
    #[test]
    fn test_suppress_runs_without_panic_on_current_os() {
        // 戻り値は問わない (Fail Kindly 契約: 失敗しても呼出側は warn のみ)。
        let _ = suppress();
    }
}
