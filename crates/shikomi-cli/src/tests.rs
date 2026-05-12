//! lib.rs の結合テスト（`#[cfg(test)]` モジュール）。
//!
//! lib.rs から分離した理由:
//! - ペガサス 500 行ルール（lib.rs が 510 行超過）
//! - TC-CI-026 unsafe ブロック監査: lib.rs 内の `unsafe { set_var }` が
//!   `audit_unsafe_blocks` に誤検出されるのを防ぐため、テストファイルを
//!   `src/tests.rs` に移動して allowlist に追加する。
//!
//! 設計根拠: docs/features/cli-vault-commands/test-design/unit.md §TC-UT-154〜159

use super::*;

// --- TC-UT-153~155, TC-UT-159: build_handle / Issue #126 ---

/// TC-UT-154 (REQ-DDM-002 / AC-DDM-02): `no_ipc=true` → `RepositoryHandle::Sqlite(_)`
#[test]
fn tc_ut_154_build_handle_no_ipc_true_returns_sqlite() {
    use crate::cli::CliArgs;
    use crate::presenter::Locale;
    use clap::Parser;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    // Unix: 0o700
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(dir.path()).unwrap().permissions();
        p.set_mode(0o700);
        std::fs::set_permissions(dir.path(), p).unwrap();
    }
    let args = CliArgs::parse_from([
        "shikomi",
        "--no-ipc",
        "--vault-dir",
        dir.path().to_str().unwrap(),
        "list",
    ]);
    let result = build_handle(&args, Locale::English, false);
    assert!(
        result.is_ok(),
        "build_handle --no-ipc should succeed, got: {:?}",
        result.err()
    );
    assert!(
        matches!(result.unwrap(), RepositoryHandle::Sqlite(_)),
        "should return Sqlite handle"
    );
}

/// TC-UT-155 (REQ-DDM-002 / AC-DDM-03): daemon 未起動 + `no_ipc=false` → `CliError::DaemonNotRunning`
#[cfg(unix)]
#[test]
#[allow(unsafe_code)]
fn tc_ut_155_build_handle_ipc_no_daemon_returns_daemon_not_running() {
    use crate::cli::CliArgs;
    use crate::error::CliError;
    use crate::presenter::Locale;
    use clap::Parser;
    use tempfile::TempDir;

    // XDG_RUNTIME_DIR を空の tempdir に向けることでソケット不在を保証
    let xdg_dir = TempDir::new().unwrap();
    // Create shikomi subdir (no socket)
    std::fs::create_dir_all(xdg_dir.path().join("shikomi")).unwrap();

    // env 上書きが必要（std::env::set_var はテスト並列実行で unsafe だが serial_test 等で管理）
    let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();
    // SAFETY: test-only env manipulation, XDG_RUNTIME_DIR → socket path resolution only
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", xdg_dir.path());
    }

    let args = CliArgs::parse_from(["shikomi", "list"]);
    let result = build_handle(&args, Locale::English, false);

    if let Some(old) = old_xdg {
        // SAFETY: restore original value
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", old);
        }
    } else {
        // SAFETY: restore original state (no XDG_RUNTIME_DIR)
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }

    match result {
        Err(CliError::DaemonNotRunning(_)) => {}
        Err(e) => panic!("expected DaemonNotRunning, got: {e:?}"),
        Ok(_) => panic!("expected Err, got Ok"),
    }
}

/// TC-UT-159 (REQ-DDM-005 / AC-DDM-06): `args.no_ipc` が lib.rs の非テスト部分に
/// 正確に 2 件の実行コード参照を持つ。
/// ① build_handle 内 IPC/SQLite 分岐 / ② vault dispatch の MSG-CLI-052 出力判定
///
/// NOTE: `include_str!` はテストコード自身も含むため、`#[cfg(test)]` ブロック前の
/// 非コメント行に絞ってカウントする（フォールスポジティブ回避）。
/// `include_str!("lib.rs")` は本ファイル（tests.rs）から見た相対パスで src/lib.rs を
/// 参照する（`include_str!` はコンパイル時にソースファイルのディレクトリを起点にする）。
#[test]
fn tc_ut_159_no_ipc_referenced_in_lib_rs() {
    let src = include_str!("lib.rs");

    // #[cfg(test)] 以降のテストブロックを除外する
    let non_test_src: String = {
        let mut in_test = false;
        src.lines()
            .filter(|line| {
                let trimmed = line.trim();
                if trimmed == "#[cfg(test)]" {
                    in_test = true;
                }
                !in_test
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 実行コード行のみ（コメント行を除く）で args.no_ipc の参照数を確認
    let code_refs = non_test_src
        .lines()
        .filter(|l| {
            let trimmed = l.trim_start();
            !trimmed.starts_with("///") && !trimmed.starts_with("//") && l.contains("args.no_ipc")
        })
        .count();
    assert_eq!(
        code_refs, 2,
        "args.no_ipc should appear in exactly 2 non-comment, non-test lines in lib.rs \
         (① build_handle branch / ② vault MSG-CLI-052 dispatch), got {code_refs}"
    );
}
