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

use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
use time::OffsetDateTime;

use cli::{AddArgs, CliArgs, EditArgs, RemoveArgs, Subcommand};
use input::{AddInput, ConfirmedRemoveInput, EditInput};
use io::ipc_vault_repository::IpcVaultRepository;
use presenter::Locale;
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};

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

    let locale = Locale::detect_from_env();
    // set は初回のみ成功。テスト中の再入は無視してよい。
    let _ = LOCALE_CACHE.set(locale);

    let args = match CliArgs::try_parse() {
        Ok(a) => a,
        Err(e) => return handle_clap_error(e),
    };

    init_tracing(args.verbose);

    let quiet = args.quiet;

    let handle = match build_handle(&args, locale, quiet) {
        Ok(h) => h,
        Err(err) => return emit_error_and_exit(&err, locale),
    };

    let result = match &args.subcommand {
        Subcommand::List => run_list(&handle, locale, quiet),
        Subcommand::Add(a) => run_add(&handle, a, locale, quiet),
        Subcommand::Edit(a) => run_edit(&handle, a, locale, quiet),
        Subcommand::Remove(a) => run_remove(&handle, a, locale, quiet),
    };

    match result {
        Ok(()) => ExitCode::Success,
        Err(err) => emit_error_and_exit(&err, locale),
    }
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
// サブコマンド dispatcher（各関数は `&RepositoryHandle` を受領し
// 内部で `match handle` の 2 アーム分岐）
// -------------------------------------------------------------------

fn run_list(handle: &RepositoryHandle, locale: Locale, quiet: bool) -> Result<(), CliError> {
    let views = match handle {
        RepositoryHandle::Sqlite(repo) => {
            let vault_dir = repo.paths().dir().to_path_buf();
            usecase::list::list_records(repo, &vault_dir)?
        }
        RepositoryHandle::Ipc(ipc) => {
            let summaries = ipc.list_summaries()?;
            usecase::list::summaries_to_views(&summaries)
        }
    };
    if !quiet {
        let rendered = presenter::list::render_list(&views, locale);
        print_stdout(&rendered);
    }
    Ok(())
}

fn run_add(
    handle: &RepositoryHandle,
    args: &AddArgs,
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

    let now = OffsetDateTime::now_utc();

    let id = match handle {
        RepositoryHandle::Sqlite(repo) => {
            let vault_dir = repo.paths().dir().to_path_buf();
            // 初期化メッセージ判定は Sqlite 経路でのみ意味を持つ（IPC 経路では daemon が
            // vault の存在を保証する前提、composition-root.md §run_add ステップ 6）。
            let initially_existed = repo.exists().map_err(CliError::from)?;
            let id = usecase::add::add_record(repo, input, now)?;
            if !quiet && !initially_existed {
                let init_msg = presenter::success::render_initialized_vault(&vault_dir, locale);
                print_stdout(&init_msg);
            }
            id
        }
        RepositoryHandle::Ipc(ipc) => {
            // id は **daemon 側で生成**され `IpcResponse::Added { id }` でそのまま返る。
            // CLI 側で `Uuid::*` を呼ばない契約（CI grep TC-CI-029）。
            ipc.add_record(input.kind, input.label, input.value, now)?
        }
    };

    if !quiet {
        let added = presenter::success::render_added(&id, locale);
        print_stdout(&added);
    }
    Ok(())
}

fn run_edit(
    handle: &RepositoryHandle,
    args: &EditArgs,
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

    let needs_value_input = args.value.is_some() || args.stdin;

    // 既存 kind は **Sqlite 経路かつ value 入力時のみ** load 経由で取得する。
    // IPC 経路では事前 load を行わず `existing_kind = None` のままにする
    // （daemon round-trip のコストとレース回避）。後段で fail-secure に Secret
    // 扱いへ寄せる（composition-root.md §run_edit IPC 経路の方針 B）。
    let existing_kind = match handle {
        RepositoryHandle::Sqlite(repo) if needs_value_input => {
            let vault_dir = repo.paths().dir().to_path_buf();
            if !repo.exists()? {
                return Err(CliError::VaultNotInitialized(vault_dir));
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
        }
        _ => None,
    };

    // value 入力 kind の決定:
    // - 既存 kind が判明（Sqlite + load 成功）→ それを使う
    // - IPC 経路で既存 kind 不明 → **fail-secure で `Secret` 強制**。TTY からの value
    //   入力は `read_password`（非エコー）経路を通る。既存が Text であっても画面
    //   エコーが出ないだけで機能上は等価、Secret であれば想定通りの保護が成立する
    //   （`composition-root.md §run_edit IPC 経路の方針 B`）。
    // - Sqlite で needs_value_input == false → 入力しないので `Text` dummy で十分
    //   （`resolve_secret_value` は `--value` 経路で kind を参照しない）。
    let kind_for_input = match (existing_kind, handle) {
        (Some(k), _) => k,
        (None, RepositoryHandle::Ipc(_)) => RecordKind::Secret,
        (None, RepositoryHandle::Sqlite(_)) => RecordKind::Text,
    };

    let value = if needs_value_input {
        Some(resolve_secret_value(
            args.value.as_deref(),
            args.stdin,
            kind_for_input,
        )?)
    } else {
        None
    };

    // shell 履歴警告: 入力 kind が Secret として扱われる経路で `--value` 直接指定
    // （= shell 履歴残留リスク）なら警告。IPC 経路の fail-secure（kind 不明 →
    // Secret 想定）でも同様に警告を出すことで、Sqlite 経路と挙動を揃える（DRY、
    // セキュリティ機能を経路依存にしない）。
    if matches!(kind_for_input, RecordKind::Secret) && args.value.is_some() {
        let warning = presenter::warning::render_shell_history_warning(locale);
        eprint_stderr(&warning);
    }

    let input = EditInput {
        id: id.clone(),
        label,
        value,
    };

    let now = OffsetDateTime::now_utc();

    let new_id = match handle {
        RepositoryHandle::Sqlite(repo) => {
            let vault_dir = repo.paths().dir().to_path_buf();
            usecase::edit::edit_record(repo, input, now, &vault_dir)?
        }
        RepositoryHandle::Ipc(ipc) => ipc.edit_record(input.id, input.label, input.value, now)?,
    };

    if !quiet {
        let rendered = presenter::success::render_updated(&new_id, locale);
        print_stdout(&rendered);
    }
    Ok(())
}

fn run_remove(
    handle: &RepositoryHandle,
    args: &RemoveArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let id = RecordId::try_from_str(&args.id).map_err(CliError::InvalidId)?;

    // 確認プロンプト前段で **`--yes` の有無に関わらず** label を取得して id 存在確認を
    // 行う（composition-root.md §確認プロンプトの label 表示と id 存在確認）。
    // 非存在時は `RemoveRecord` リクエストを発行する前に Fail Fast で early return。
    let label = lookup_label_for_remove(handle, &id)?;

    let confirmed = if args.yes {
        true
    } else if io::terminal::is_stdin_tty() {
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
    let removed_id = match handle {
        RepositoryHandle::Sqlite(repo) => {
            let vault_dir = repo.paths().dir().to_path_buf();
            usecase::remove::remove_record(repo, input, &vault_dir)?
        }
        RepositoryHandle::Ipc(ipc) => ipc.remove_record(input.id().clone())?,
    };

    if !quiet {
        print_stdout(&presenter::success::render_removed(&removed_id, locale));
    }
    Ok(())
}

/// `run_remove` の確認プロンプト用 label 取得 + id 存在確認。
///
/// - `Sqlite` 経路: `repo.load()` → `find_record(&id)` → `label`
/// - `Ipc` 経路: `ipc.list_summaries()` → `iter().find(|s| s.id == id)` → `s.label`
///
/// 両経路ともに **id 非存在は `CliError::RecordNotFound(id)` で early return**。
/// これにより `--yes` / 非 `--yes` で Fail Fast 経路が完全一致する
/// （composition-root.md §確認プロンプトの label 表示と id 存在確認）。
fn lookup_label_for_remove(
    handle: &RepositoryHandle,
    id: &RecordId,
) -> Result<Option<String>, CliError> {
    match handle {
        RepositoryHandle::Sqlite(repo) => {
            let vault_dir = repo.paths().dir().to_path_buf();
            if !repo.exists()? {
                return Err(CliError::VaultNotInitialized(vault_dir));
            }
            let vault = repo.load()?;
            if vault.protection_mode() == shikomi_core::ProtectionMode::Encrypted {
                return Err(CliError::EncryptionUnsupported);
            }
            let label = vault
                .find_record(id)
                .map(|r| r.label().as_str().to_owned())
                .ok_or_else(|| CliError::RecordNotFound(id.clone()))?;
            Ok(Some(label))
        }
        RepositoryHandle::Ipc(ipc) => {
            let summaries = ipc.list_summaries()?;
            let label = summaries
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.label.as_str().to_owned())
                .ok_or_else(|| CliError::RecordNotFound(id.clone()))?;
            Ok(Some(label))
        }
    }
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
