//! `shikomi_cli` — CLI 内部公開 API（lib target）。
//!
//! 本 crate は `[lib] + [[bin]]` の 2 ターゲット構成。lib の公開項目は全て
//! `#[doc(hidden)]` で `cargo doc` から隠し、外部契約化しない（`publish = false`）。
//! 結合テスト（`tests/`）からは通常通り参照可能（`#[doc(hidden)]` は可視性を制限しない）。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/public-api.md
//! §前提: crate 構成、composition-root.md §処理順序

#[doc(hidden)]
pub mod accessibility;
#[doc(hidden)]
pub mod cli;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod hardening;
#[doc(hidden)]
pub mod input;
#[doc(hidden)]
pub mod io;
#[doc(hidden)]
pub mod presenter;
// 工程5 ペガサス指摘 (lib.rs 500 行ルール超過) 解消: レコード CRUD dispatcher 群を
// `record_runners` モジュールに切り出し。lib.rs はコンポジションルート + IPC
// handshake + vault サブコマンド経路に責務を集約する。
#[doc(hidden)]
pub mod record_runners;
#[doc(hidden)]
pub mod usecase;
#[doc(hidden)]
pub mod view;

pub use error::{CliError, ExitCode};

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

use shikomi_infra::persistence::SqliteVaultRepository;

use cli::{CliArgs, Subcommand, VaultSubcommand};
use io::ipc_vault_repository::IpcVaultRepository;
use presenter::Locale;

// -------------------------------------------------------------------
// グローバル Locale キャッシュ（panic hook から参照される）
// -------------------------------------------------------------------

/// `run()` 起動時に 1 度だけ設定される Locale。
///
/// panic hook 内で `Locale` を参照するために用いる。`Locale` は `Copy + Clone` な
/// 単純列挙のため、hook 内での副作用なし参照が成立する。
///
/// 設計根拠: docs/features/cli-vault-commands/basic-design/error.md §i18n 扱い、
/// detailed-design/composition-root.md §`static LOCALE_CACHE: OnceLock<Locale>` の配置
#[doc(hidden)]
pub static LOCALE_CACHE: OnceLock<Locale> = OnceLock::new();

// -------------------------------------------------------------------
// panic hook（secret 漏洩経路の遮断）
// -------------------------------------------------------------------

/// Secret 混入リスクを避けるため、panic 情報を一切参照せず固定文言のみを
/// stderr に出力する panic hook。
///
/// - `info.payload()` / `info.message()` / `info.location()` を参照しない（契約）
/// - `tracing::*` マクロを呼ばない（契約）
/// - `Locale` は `LOCALE_CACHE` から読取（未設定なら English にフォールバック）
///
/// 設計根拠: docs/features/cli-vault-commands/basic-design/security.md
/// §panic hook と secret 漏洩経路の遮断
// MSRV 1.80 のため `PanicInfo` を使用（`PanicHookInfo` は 1.81 stable）。
// どちらも `info.payload()` / `info.message()` / `info.location()` を非参照とする契約は同じ。
#[allow(deprecated)]
fn panic_hook(_info: &std::panic::PanicInfo<'_>) {
    let locale = LOCALE_CACHE.get().copied().unwrap_or(Locale::English);
    let mut stderr = std::io::stderr().lock();
    // 固定文言（MSG-CLI-109）。payload / message / location は一切参照しない。
    let _ = writeln!(stderr, "error: internal bug");
    let _ = writeln!(
        stderr,
        "hint: please report this issue to https://github.com/shikomi-dev/shikomi/issues"
    );
    if matches!(locale, Locale::JapaneseEn) {
        let _ = writeln!(stderr, "error: 内部バグが発生しました");
        let _ = writeln!(
            stderr,
            "hint: https://github.com/shikomi-dev/shikomi/issues に報告してください"
        );
    }
}

// -------------------------------------------------------------------
// RepositoryHandle — Sqlite / IPC の Composition over Inheritance
// -------------------------------------------------------------------

/// vault 操作経路を保持する non-public 値型（Issue #30、案 D）。
///
/// `IpcVaultRepository` は `VaultRepository` trait を**実装しない**ため
/// `Box<dyn VaultRepository>` で抽象化できない。代わりに enum dispatch で経路を
/// 表現し、各 `run_*` 関数の冒頭で `match` を取って 2 アームに分岐する。
///
/// `#[non_exhaustive]` を**付けない**: CLI 内部限定で、新バリアント追加時に
/// `match` 網羅性検査が変更箇所を漏れなく列挙してくれる方が安全。
///
/// 設計根拠:
/// - docs/features/cli-vault-commands/detailed-design/composition-root.md
///   §`RepositoryHandle` enum
/// - docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md
///   §案 D（VaultRepository trait 非実装 + RepositoryHandle enum）
//
// バリアント間サイズ差（Sqlite ~96 B / Ipc ~304 B、`tokio::runtime::Runtime` 込み）は
// 本 enum を `run()` 寿命中に **唯一 1 個** しか作らない契約のため、heap 1 個分の節約に
// 価値がない。`Box<IpcVaultRepository>` で alloc を増やすより、stack 配置を維持する方が
// 設計意図（composition-root.md §`Box` 不要、stack 配置）に沿う。
#[allow(clippy::large_enum_variant)]
enum RepositoryHandle {
    /// 既定の SQLite 直接アクセス経路。
    Sqlite(SqliteVaultRepository),
    /// `--ipc` opt-in の daemon 経由経路。
    Ipc(IpcVaultRepository),
}

// -------------------------------------------------------------------
// `run_edit` fail-secure 経路の純粋関数（Issue #33 / Phase 1.5-ε）
// -------------------------------------------------------------------

/// `RepositoryHandle` の判別タグ。
///
/// `decide_kind_for_input` を **`IpcVaultRepository` 構築不要**（実 daemon spawn 不要）
/// な単体テスト可能な形に保つための補助型。本 enum は `RepositoryHandle` の各
/// バリアントが内包する重い値（`SqliteVaultRepository` / `IpcVaultRepository`）を
/// 剥がした「判別だけのタグ」であり、`#[derive(Clone, Copy, PartialEq, Eq)]` で
/// テスト時に `assert_eq!` 可能。
///
/// 設計根拠: docs/features/daemon-ipc/test-design/unit.md §3.10 ①（案 1）
///
/// 工程5 ペガサス指摘解消: `discriminant` / `decide_kind_for_input` 本体は
/// `record_runners.rs` に移管。本 enum 定義のみ lib.rs に残し pub(crate) 公開。
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryHandleDiscriminant {
    Sqlite,
    Ipc,
}

// -------------------------------------------------------------------
// run() — コンポジションルート
// -------------------------------------------------------------------

/// CLI 全体のエントリ関数。
///
/// 処理順序（詳細設計 `composition-root.md §処理順序`）:
/// 1. panic hook 登録
/// 2. Locale 決定 + `LOCALE_CACHE` 格納
/// 3. clap パース（失敗時は clap エラー扱い）
/// 4. `tracing_subscriber` 初期化
/// 5. `RepositoryHandle` 構築（`args.ipc` で `Sqlite` / `Ipc` を分岐）
/// 6. サブコマンド分岐 → `run_*` 関数 → `Result<(), CliError>`
/// 7. `Err` は `render_error` で stderr 出力 + `ExitCode::from(&err)` 写像
#[must_use]
pub fn run() -> ExitCode {
    std::panic::set_hook(Box::new(panic_hook));

    // Sub-F (#44) Phase 5 / C-41: core dump 抑制を最早期に呼び出す。失敗しても
    // 起動を止めない (Fail Kindly)、ただし warn ログで観測可能化する。
    if let Err(e) = hardening::core_dump::suppress() {
        tracing::warn!(error = %e, "core dump suppression failed; continuing without it");
    }

    let locale = Locale::detect_from_env();
    // set は初回のみ成功。テスト中の再入は無視してよい。
    let _ = LOCALE_CACHE.set(locale);

    let args = match CliArgs::try_parse() {
        Ok(a) => a,
        Err(e) => return handle_clap_error(e),
    };

    init_tracing(args.verbose);

    let quiet = args.quiet;

    // Sub-F (#44) Phase 2: vault サブコマンドは daemon IPC 経路に**強制**する。
    // V1 の `RepositoryHandle::Sqlite` 経路は vault に直接触らない契約 (Phase 2 規定、
    // cli-subcommands.md §Clean Architecture の依存方向) のため、ここで先に
    // dispatch を分岐させる。`--ipc` フラグ未指定でも vault 経路は IPC 強制。
    if let Subcommand::Vault(vault) = &args.subcommand {
        // Issue #75 Bug-F-007: `--vault-dir <DIR>` を daemon socket 解決の最優先 hint
        // として渡す（`<DIR>/shikomi.sock` 最優先 + 失敗時 default fallback、
        // `cli-subcommands.md` §Bug-F-007 SSoT）。
        let result = run_vault(vault, args.vault_dir.as_deref(), locale, quiet);
        return match result {
            Ok(()) => ExitCode::Success,
            Err(err) => emit_error_and_exit(&err, locale),
        };
    }

    let handle = match build_handle(&args, locale, quiet) {
        Ok(h) => h,
        Err(err) => return emit_error_and_exit(&err, locale),
    };

    let result = match &args.subcommand {
        Subcommand::List => record_runners::run_list(&handle, locale, quiet),
        Subcommand::Add(a) => record_runners::run_add(&handle, a, locale, quiet),
        Subcommand::Edit(a) => record_runners::run_edit(&handle, a, locale, quiet),
        Subcommand::Remove(a) => record_runners::run_remove(&handle, a, locale, quiet),
        // 上の `if let Subcommand::Vault(_)` early return で処理済（網羅性のため `_` で吸収）。
        Subcommand::Vault(_) => unreachable!("vault subcommand handled above"),
    };

    match result {
        Ok(()) => ExitCode::Success,
        Err(err) => emit_error_and_exit(&err, locale),
    }
}

// -------------------------------------------------------------------
// Sub-F (#44) Phase 2: vault サブコマンド dispatch
// -------------------------------------------------------------------

/// vault サブコマンド経路（IPC 強制）の dispatch。
///
/// daemon socket 解決 → `IpcVaultRepository::connect_with_vault_dir` → handshake (V2) →
/// 7 サブコマンドの usecase 呼出。`--ipc` フラグの有無によらず IPC 経路で動作する
/// （vault 管理は daemon の責務、Phase 2 規定）。
///
/// Issue #75 Bug-F-007: `vault_dir` が `Some(<DIR>)` の場合、`<DIR>/shikomi.sock`（Unix）
/// または `\\.\pipe\shikomi-{H}`（Windows）を socket 解決の最優先候補に渡す
/// （`cli-subcommands.md` §Bug-F-007 解消 §`--vault-dir` の意味論 SSoT）。
fn run_vault(
    vault: &VaultSubcommand,
    vault_dir: Option<&Path>,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let repo = connect_vault_ipc(vault_dir, locale, quiet)?;
    match vault {
        VaultSubcommand::Encrypt(a) => usecase::vault::encrypt::execute(&repo, a, locale, quiet),
        VaultSubcommand::Decrypt => usecase::vault::decrypt::execute(&repo, locale, quiet),
        VaultSubcommand::Unlock(a) => usecase::vault::unlock::execute(&repo, a, locale, quiet),
        VaultSubcommand::Lock => usecase::vault::lock::execute(&repo, locale, quiet),
        VaultSubcommand::ChangePassword => {
            usecase::vault::change_password::execute(&repo, locale, quiet)
        }
        VaultSubcommand::Rekey(a) => usecase::vault::rekey::execute(&repo, a, locale, quiet),
        VaultSubcommand::RotateRecovery(a) => {
            usecase::vault::rotate_recovery::execute(&repo, a, locale, quiet)
        }
    }
}

/// vault サブコマンド経路の `IpcVaultRepository` 構築（IPC 強制 + opt-in 警告省略）。
///
/// vault 管理は IPC 専用の責務領域であり、`build_handle` が出力する
/// `MSG-CLI-051` (opt-in 警告) は文脈不一致のため省略する。
///
/// Issue #75 Bug-F-007: `vault_dir` を `IpcVaultRepository::connect_with_vault_dir` に渡し、
/// `<DIR>/shikomi.sock`（Unix）または `\\.\pipe\shikomi-{H}`（Windows、`<H>` 純関数）を
/// 最優先で試行する。失敗時は default 経路（`SHIKOMI_VAULT_DIR` env / `XDG_RUNTIME_DIR` /
/// `dirs::cache_dir()` / `dirs::runtime_dir()` / Windows user-SID）にフォールバック。
fn connect_vault_ipc(
    vault_dir: Option<&Path>,
    _locale: Locale,
    _quiet: bool,
) -> Result<IpcVaultRepository, CliError> {
    let repo = IpcVaultRepository::connect_with_vault_dir(vault_dir)?;
    Ok(repo)
}

// -------------------------------------------------------------------
// 補助関数 — Repository 構築 / clap / tracing / 出力
// -------------------------------------------------------------------

/// `args.ipc` フラグから `RepositoryHandle` を構築する。
///
/// IPC 経路では `MSG-CLI-051`（opt-in 警告）を `quiet` 抑止下を除き先に出力した上で、
/// daemon に接続してハンドシェイクまで完了させる。
fn build_handle(args: &CliArgs, locale: Locale, quiet: bool) -> Result<RepositoryHandle, CliError> {
    if args.ipc {
        if !quiet {
            let notice = presenter::warning::render_ipc_opt_in_notice(locale);
            eprint_stderr(&notice);
        }
        let socket_path = IpcVaultRepository::default_socket_path()?;
        let ipc = IpcVaultRepository::connect(&socket_path)?;
        Ok(RepositoryHandle::Ipc(ipc))
    } else {
        let path = match args.vault_dir.as_deref() {
            Some(p) => p.to_path_buf(),
            None => io::paths::resolve_os_default_vault_dir()?,
        };
        let repo = SqliteVaultRepository::from_directory(&path)?;
        Ok(RepositoryHandle::Sqlite(repo))
    }
}

/// clap エラーを CLI 終了コード方針に合わせて整形する。
///
/// - `DisplayHelp` / `DisplayVersion` → stdout + exit 0
/// - その他 usage 系 → stderr + exit 1（clap デフォルトの exit 2 を上書き）
/// - `Io` / `Format` → stderr + exit 2
fn handle_clap_error(err: clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    match err.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        | ErrorKind::DisplayVersion => {
            let _ = err.print();
            ExitCode::Success
        }
        ErrorKind::Io | ErrorKind::Format => {
            let _ = err.print();
            ExitCode::SystemError
        }
        _ => {
            let _ = err.print();
            ExitCode::UserError
        }
    }
}

fn init_tracing(verbose: bool) {
    use tracing_subscriber::EnvFilter;
    let default = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!("shikomi_cli={default},shikomi_infra={default}"))
    });
    // 二重初期化はテスト等で起こり得るため、結果は握り潰す（try_init）
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn emit_error_and_exit(err: &CliError, locale: Locale) -> ExitCode {
    let rendered = presenter::error::render_error(err, locale);
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(rendered.as_bytes());
    ExitCode::from(err)
}

// -------------------------------------------------------------------
// I/O 薄ラッパ (record_runners と build_handle 両方から使うため pub)
// -------------------------------------------------------------------

#[doc(hidden)]
pub fn print_stdout(s: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(s.as_bytes());
}

#[doc(hidden)]
pub fn eprint_stderr(s: &str) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(s.as_bytes());
}
