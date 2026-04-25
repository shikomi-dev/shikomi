//! `shikomi_cli` — CLI 内部公開 API（lib target）。
//!
//! 本 crate は `[lib] + [[bin]]` の 2 ターゲット構成。lib の公開項目は全て
//! `#[doc(hidden)]` で `cargo doc` から隠し、外部契約化しない（`publish = false`）。
//! 結合テスト（`tests/`）からは通常通り参照可能（`#[doc(hidden)]` は可視性を制限しない）。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/public-api.md
//! §前提: crate 構成、composition-root.md §処理順序

#[doc(hidden)]
pub mod cli;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod input;
#[doc(hidden)]
pub mod io;
#[doc(hidden)]
pub mod presenter;
#[doc(hidden)]
pub mod usecase;
#[doc(hidden)]
pub mod view;

pub use error::{CliError, ExitCode};

use std::io::Write;
use std::sync::OnceLock;

use shikomi_infra::persistence::SqliteVaultRepository;
use time::OffsetDateTime;

use cli::{AddArgs, CliArgs, EditArgs, RemoveArgs, Subcommand};
use input::{AddInput, ConfirmedRemoveInput, EditInput};
use io::ipc_vault_repository::IpcVaultRepository;
use presenter::Locale;
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use shikomi_infra::persistence::VaultRepository;

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
// run() — コンポジションルート
// -------------------------------------------------------------------

/// CLI 全体のエントリ関数。
///
/// 処理順序（詳細設計 `composition-root.md §処理順序`）:
/// 1. panic hook 登録
/// 2. Locale 決定 + `LOCALE_CACHE` 格納
/// 3. clap パース（失敗時は clap エラー扱い）
/// 4. `tracing_subscriber` 初期化
/// 5. Repository 構築（`--vault-dir` > env (clap attribute) > OS デフォルト）
/// 6. サブコマンド分岐 → `run_*` 関数 → `Result<(), CliError>`
/// 7. `Err` は `render_error` で stderr 出力 + `ExitCode::from(&err)` 写像
#[must_use]
pub fn run() -> ExitCode {
    std::panic::set_hook(Box::new(panic_hook));

    let locale = Locale::detect_from_env();
    // set は初回のみ成功。テスト中の再入は無視してよい。
    let _ = LOCALE_CACHE.set(locale);

    let args = match CliArgs::try_parse() {
        Ok(a) => a,
        Err(e) => return handle_clap_error(e),
    };

    init_tracing(args.verbose);

    let quiet = args.quiet;

    if args.ipc {
        // `--ipc` は現状 `list` のみ対応（Phase 2 移行 PR で add/edit/remove の透過化を完成）。
        // ここで早期 reject することで、IpcVaultRepository が `VaultRepository` trait を
        // 偽実装する必要がなくなる（旧実装の「嘘の Plaintext(empty) 注入」「嘘 ID 出荷」「常に true な exists()」
        // を構造的に消滅させる）。
        if !matches!(args.subcommand, Subcommand::List) {
            return emit_error_and_exit(
                &CliError::UsageError(
                    "--ipc currently supports only the `list` subcommand; \
                     for add/edit/remove, omit --ipc to use direct vault file access"
                        .to_owned(),
                ),
                locale,
            );
        }
        if !quiet {
            eprintln!("warning: --ipc is an opt-in preview; only `list` is currently supported");
        }
        run_list_via_ipc(locale, quiet)
    } else {
        run_with_sqlite_repo(&args, locale, quiet)
    }
}

fn run_with_sqlite_repo(args: &CliArgs, locale: Locale, quiet: bool) -> ExitCode {
    let repo = match build_repo(args.vault_dir.as_deref()) {
        Ok(r) => r,
        Err(err) => return emit_error_and_exit(&err, locale),
    };
    let vault_dir = repo.paths().dir().to_path_buf();
    let result = match &args.subcommand {
        Subcommand::List => run_list(&repo, &vault_dir, locale, quiet),
        Subcommand::Add(a) => run_add(&repo, a, &vault_dir, locale, quiet),
        Subcommand::Edit(a) => run_edit(&repo, a, &vault_dir, locale, quiet),
        Subcommand::Remove(a) => run_remove(&repo, a, &vault_dir, locale, quiet),
    };
    match result {
        Ok(()) => ExitCode::Success,
        Err(err) => emit_error_and_exit(&err, locale),
    }
}

/// `--ipc list` 専用エントリ。`IpcVaultRepository::list_summaries` で `RecordSummary` 列を
/// 取得し、`Vault` 集約を経由せずに直接 `RecordView` に射影する。
fn run_list_via_ipc(locale: Locale, quiet: bool) -> ExitCode {
    let socket_path = match IpcVaultRepository::default_socket_path() {
        Ok(p) => p,
        Err(err) => return emit_error_and_exit(&CliError::from(err), locale),
    };
    let client = match IpcVaultRepository::connect(&socket_path) {
        Ok(c) => c,
        Err(err) => return emit_error_and_exit(&CliError::from(err), locale),
    };
    let summaries = match client.list_summaries() {
        Ok(s) => s,
        Err(err) => return emit_error_and_exit(&CliError::from(err), locale),
    };
    let views = usecase::list::summaries_to_views(&summaries);
    if !quiet {
        let rendered = presenter::list::render_list(&views, locale);
        print_stdout(&rendered);
    }
    ExitCode::Success
}

// -------------------------------------------------------------------
// 補助関数
// -------------------------------------------------------------------

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

/// `--vault-dir` フラグ（clap attribute で env `SHIKOMI_VAULT_DIR` も吸収済み）があれば
/// その path で、なければ OS デフォルトで Repository を構築する。
fn build_repo(vault_dir: Option<&std::path::Path>) -> Result<SqliteVaultRepository, CliError> {
    let path = match vault_dir {
        Some(p) => p.to_path_buf(),
        None => io::paths::resolve_os_default_vault_dir()?,
    };
    SqliteVaultRepository::from_directory(&path).map_err(CliError::from)
}

fn emit_error_and_exit(err: &CliError, locale: Locale) -> ExitCode {
    let rendered = presenter::error::render_error(err, locale);
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(rendered.as_bytes());
    ExitCode::from(err)
}

// -------------------------------------------------------------------
// サブコマンドごとの dispatcher
// -------------------------------------------------------------------

fn run_list(
    repo: &dyn VaultRepository,
    vault_dir: &std::path::Path,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let views = usecase::list::list_records(repo, vault_dir)?;
    if !quiet {
        let rendered = presenter::list::render_list(&views, locale);
        print_stdout(&rendered);
    }
    Ok(())
}

fn run_add(
    repo: &dyn VaultRepository,
    args: &AddArgs,
    vault_dir: &std::path::Path,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    // 値指定フラグの相互排他検証（Fail Fast）
    let value = resolve_secret_value(args.value.as_deref(), args.stdin, args.kind.into())?;

    // shell 履歴警告: `--kind secret && --value` 指定時
    if matches!(args.kind, cli::KindArg::Secret) && args.value.is_some() {
        let warning = presenter::warning::render_shell_history_warning(locale);
        eprint_stderr(&warning);
    }

    let label = RecordLabel::try_new(args.label.clone()).map_err(CliError::InvalidLabel)?;

    let input = AddInput {
        kind: args.kind.into(),
        label,
        value,
    };

    let initially_existed = repo.exists().map_err(CliError::from)?;

    let now = OffsetDateTime::now_utc();
    let id = usecase::add::add_record(repo, input, now)?;

    if !quiet {
        if !initially_existed {
            let init_msg = presenter::success::render_initialized_vault(vault_dir, locale);
            print_stdout(&init_msg);
        }
        let added = presenter::success::render_added(&id, locale);
        print_stdout(&added);
    }
    Ok(())
}

fn run_edit(
    repo: &dyn VaultRepository,
    args: &EditArgs,
    vault_dir: &std::path::Path,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    // 「最低 1 つの更新フラグ必須」検証
    if args.label.is_none() && args.value.is_none() && !args.stdin {
        return Err(CliError::UsageError(
            "at least one of --label/--value/--stdin is required".to_owned(),
        ));
    }

    let id = RecordId::try_from_str(&args.id).map_err(CliError::InvalidId)?;

    let label = match args.label.as_ref() {
        Some(s) => Some(RecordLabel::try_new(s.clone()).map_err(CliError::InvalidLabel)?),
        None => None,
    };

    // 既存 kind を事前 load で取得（value 入力時の非エコー判定 + shell 履歴警告に使う）。
    // 設計書 composition-root.md §run_edit L59-62: 「値取得は run_add と同様（kind に応じて
    // `read_password` / `read_line`）」「警告判定は load 後の既存 kind と照合するかは実装時判断」
    // ここでは load 2 回を許容して既存 kind を参照する（run_remove と同方針、ペテルギウス指摘
    // ①への対応）。load 失敗は Fail Fast（`?` でユーザに伝搬）。
    let needs_value_input = args.value.is_some() || args.stdin;
    let existing_kind = if needs_value_input {
        if !repo.exists()? {
            return Err(CliError::VaultNotInitialized(vault_dir.to_path_buf()));
        }
        let vault = repo.load()?;
        if vault.protection_mode() == shikomi_core::ProtectionMode::Encrypted {
            return Err(CliError::EncryptionUnsupported);
        }
        Some(
            vault
                .find_record(&id)
                .map(shikomi_core::Record::kind)
                .ok_or_else(|| CliError::RecordNotFound(id.clone()))?,
        )
    } else {
        None
    };

    let value = if needs_value_input {
        // 既存が Secret ならエコーしない、Text なら通常 readline。
        // `--value` と `--stdin` の相互排他は resolve_secret_value 側で検出。
        let kind_for_input = existing_kind.unwrap_or(RecordKind::Text);
        Some(resolve_secret_value(
            args.value.as_deref(),
            args.stdin,
            kind_for_input,
        )?)
    } else {
        None
    };

    // shell 履歴警告: 既存 kind が Secret で `--value` 直接指定（= shell 履歴残留リスク）なら警告
    if matches!(existing_kind, Some(RecordKind::Secret)) && args.value.is_some() {
        let warning = presenter::warning::render_shell_history_warning(locale);
        eprint_stderr(&warning);
    }

    let input = EditInput {
        id: id.clone(),
        label,
        value,
    };

    let now = OffsetDateTime::now_utc();
    let id = usecase::edit::edit_record(repo, input, now, vault_dir)?;

    if !quiet {
        let rendered = presenter::success::render_updated(&id, locale);
        print_stdout(&rendered);
    }
    Ok(())
}

fn run_remove(
    repo: &dyn VaultRepository,
    args: &RemoveArgs,
    vault_dir: &std::path::Path,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let id = RecordId::try_from_str(&args.id).map_err(CliError::InvalidId)?;

    let confirmed = if args.yes {
        true
    } else if io::terminal::is_stdin_tty() {
        // プロンプト表示用の label を取得するために load して find_record。
        // load 失敗は Fail Fast で return（`.ok()` で握り潰さない — ペテルギウス指摘②）。
        // これによりユーザは「load に失敗したのに y を押してしまう」欺き構造を踏まない。
        if !repo.exists()? {
            return Err(CliError::VaultNotInitialized(vault_dir.to_path_buf()));
        }
        let vault = repo.load()?;
        if vault.protection_mode() == shikomi_core::ProtectionMode::Encrypted {
            return Err(CliError::EncryptionUnsupported);
        }
        let label = vault
            .find_record(&id)
            .map(|r| r.label().as_str().to_owned());
        let prompt = presenter::prompt::render_remove_prompt(&id, label.as_deref(), locale);
        match io::terminal::read_line(&prompt) {
            Ok(line) => matches!(line.trim(), "y" | "Y"),
            Err(e) => {
                return Err(CliError::Persistence(
                    shikomi_infra::persistence::PersistenceError::Io {
                        path: std::path::PathBuf::from("<stdin>"),
                        source: e,
                    },
                ));
            }
        }
    } else {
        return Err(CliError::NonInteractiveRemove);
    };

    if !confirmed {
        if !quiet {
            print_stdout(&presenter::success::render_cancelled(locale));
        }
        return Ok(());
    }

    let input = ConfirmedRemoveInput::new(id);
    let id = usecase::remove::remove_record(repo, input, vault_dir)?;

    if !quiet {
        print_stdout(&presenter::success::render_removed(&id, locale));
    }
    Ok(())
}

// -------------------------------------------------------------------
// 値取得ヘルパ
// -------------------------------------------------------------------

/// `--value` / `--stdin` の 4 パターンを評価して `SecretString` を得る。
fn resolve_secret_value(
    value: Option<&str>,
    stdin: bool,
    kind: RecordKind,
) -> Result<SecretString, CliError> {
    match (value, stdin) {
        (Some(_), true) => Err(CliError::UsageError(
            "--value and --stdin cannot be used together".to_owned(),
        )),
        (None, false) => Err(CliError::UsageError(
            "either --value or --stdin is required".to_owned(),
        )),
        (Some(v), false) => Ok(SecretString::from_string(v.to_owned())),
        (None, true) => read_value_from_stdin(kind),
    }
}

fn read_value_from_stdin(kind: RecordKind) -> Result<SecretString, CliError> {
    let buf = if matches!(kind, RecordKind::Secret) && io::terminal::is_stdin_tty() {
        // 非エコー入力（Secret + TTY）
        io::terminal::read_password("value: ").map_err(|e| {
            CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
                path: std::path::PathBuf::from("<stdin>"),
                source: e,
            })
        })?
    } else {
        // 通常 readline。末尾 \n / \r を trim。
        let line = io::terminal::read_line("").map_err(|e| {
            CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
                path: std::path::PathBuf::from("<stdin>"),
                source: e,
            })
        })?;
        SecretString::from_string(line)
    };
    Ok(buf)
}

// -------------------------------------------------------------------
// I/O 薄ラッパ
// -------------------------------------------------------------------

fn print_stdout(s: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(s.as_bytes());
}

fn eprint_stderr(s: &str) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(s.as_bytes());
}
