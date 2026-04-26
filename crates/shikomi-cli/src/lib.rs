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
pub mod hardening;
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

use cli::{AddArgs, CliArgs, EditArgs, RemoveArgs, Subcommand, VaultSubcommand};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepositoryHandleDiscriminant {
    Sqlite,
    Ipc,
}

/// `RepositoryHandle` を判別タグへ落とす射影。
///
/// 設計根拠: docs/features/daemon-ipc/test-design/unit.md §3.10 ①
fn discriminant(handle: &RepositoryHandle) -> RepositoryHandleDiscriminant {
    match handle {
        RepositoryHandle::Sqlite(_) => RepositoryHandleDiscriminant::Sqlite,
        RepositoryHandle::Ipc(_) => RepositoryHandleDiscriminant::Ipc,
    }
}

/// `run_edit` の値入力 kind を決定する純粋関数。
///
/// PR #32 の方針 B（`composition-root.md §run_edit IPC 経路の方針 B`）を
/// **型レベルで強制**する: **IPC 経路では `existing_kind` の如何に関わらず**
/// 戻り値は `RecordKind::Secret` に**確定**する。後段の `read_value_from_stdin(kind)`
/// は TTY 上で `read_password`（非エコー）経路を選択する——この決定論を
/// 単体テスト可能な形で封じ込める。
///
/// 決定表:
///
/// | 入力 | 戻り値 | 備考 |
/// |------|--------|------|
/// | `_, Ipc` | `RecordKind::Secret` | **fail-secure**（方針 B の核、IPC アーム不変条件） |
/// | `Some(k), Sqlite` | `k` | 既存 kind を尊重（Sqlite + load 成功） |
/// | `None, Sqlite` | `RecordKind::Text` | dummy。`needs_value_input == false` 経路でのみ到達、`resolve_secret_value` の `--value` 経路は kind 非参照 |
///
/// 副次契約（**IPC アーム不変条件**）: `handle == Ipc` の任意の `existing_kind` で
/// 戻り値が **`RecordKind::Text` を一切返さない**ことを `match` の構造で保証する。
/// 「呼出側の `existing_kind` 算出が `Sqlite` アームでしか `Some(_)` を返さない」という
/// 呼出側ロジックへの依存に頼らず、本関数単体で fail-secure を担保する
/// （Defense in Depth、ペテルギウス review b50e15d 指摘 → 案 a 対応）。
///
/// 設計根拠:
/// - docs/features/daemon-ipc/test-design/unit.md §2.17（TC-UT-130〜134）/ §3.10 ①
/// - docs/features/daemon-ipc/composition-root.md `§run_edit` IPC 経路の方針 B
fn decide_kind_for_input(
    existing_kind: Option<RecordKind>,
    handle: RepositoryHandleDiscriminant,
) -> RecordKind {
    match (existing_kind, handle) {
        // IPC アーム: existing_kind の如何によらず常に Secret 強制（fail-secure）
        (_, RepositoryHandleDiscriminant::Ipc) => RecordKind::Secret,
        // Sqlite アーム: 既存 kind を尊重、不明時は dummy（needs_value_input == false 経路）
        (Some(k), RepositoryHandleDiscriminant::Sqlite) => k,
        (None, RepositoryHandleDiscriminant::Sqlite) => RecordKind::Text,
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
        let result = run_vault(vault, locale, quiet);
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
        Subcommand::List => run_list(&handle, locale, quiet),
        Subcommand::Add(a) => run_add(&handle, a, locale, quiet),
        Subcommand::Edit(a) => run_edit(&handle, a, locale, quiet),
        Subcommand::Remove(a) => run_remove(&handle, a, locale, quiet),
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
/// daemon socket 解決 → `IpcVaultRepository::connect` → handshake (V2) →
/// 7 サブコマンドの usecase 呼出。`--ipc` フラグの有無によらず IPC 経路で動作する
/// （vault 管理は daemon の責務、Phase 2 規定）。
fn run_vault(vault: &VaultSubcommand, locale: Locale, quiet: bool) -> Result<(), CliError> {
    let repo = connect_vault_ipc(locale, quiet)?;
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
fn connect_vault_ipc(_locale: Locale, _quiet: bool) -> Result<IpcVaultRepository, CliError> {
    let socket_path = IpcVaultRepository::default_socket_path()?;
    let repo = IpcVaultRepository::connect(&socket_path)?;
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
// サブコマンド dispatcher（各関数は `&RepositoryHandle` を受領し
// 内部で `match handle` の 2 アーム分岐）
// -------------------------------------------------------------------

fn run_list(handle: &RepositoryHandle, locale: Locale, quiet: bool) -> Result<(), CliError> {
    // Sub-F (#44) Phase 3 / C-37: 両経路で `protection_mode` を必ず算出する。
    // - Sqlite 経路: 上流で Encrypted は `EncryptionUnsupported` で fail-fast 済み
    //   (`usecase::list::list_records` line 30) のため、本経路に到達した時点で
    //   保護モードは Plaintext 確定。
    // - IPC 経路: daemon が `IpcResponse::Records.protection_mode` で 4 variant の
    //   いずれかを返す。`Unknown` は REQ-S16 Fail-Secure で exit 3 fail-fast。
    let (views, protection_mode) = match handle {
        RepositoryHandle::Sqlite(repo) => {
            let vault_dir = repo.paths().dir().to_path_buf();
            let views = usecase::list::list_records(repo, &vault_dir)?;
            (views, shikomi_core::ipc::ProtectionModeBanner::Plaintext)
        }
        RepositoryHandle::Ipc(ipc) => {
            let outcome = ipc.list_summaries()?;
            let views = usecase::list::summaries_to_views(&outcome.records);
            (views, outcome.protection_mode)
        }
    };

    // REQ-S16 Fail-Secure: 保護モード判定不能なら exit 3 で fail-fast、
    // レコード一覧は一切表示しない (情報漏洩経路を構造的に遮断)。
    if matches!(
        protection_mode,
        shikomi_core::ipc::ProtectionModeBanner::Unknown
    ) {
        return Err(CliError::ProtectionModeUnknown);
    }

    if !quiet {
        // `NO_COLOR` (https://no-color.org) / 非 TTY / `--quiet` はカラー無効化。
        // `--quiet` は既に分岐済みのため、ここでは env + TTY のみ判定する。
        let color_enabled = is_color_enabled();
        let rendered = presenter::list::render_list(&views, protection_mode, color_enabled, locale);
        print_stdout(&rendered);
    }
    Ok(())
}

/// `NO_COLOR` 環境変数 / stdout が非 TTY のいずれかが真ならカラー無効化。
///
/// 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
/// §`ProtectionModeBanner` enum「`NO_COLOR` 環境変数 / 非 TTY / `--quiet` 時は
/// カラー無効化、文字のみ表示」
fn is_color_enabled() -> bool {
    use is_terminal::IsTerminal;
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
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

    // value 入力 kind の決定（Issue #33 / Phase 1.5-ε で `decide_kind_for_input` に抽出）:
    // - 既存 kind が判明（Sqlite + load 成功）→ それを使う
    // - IPC 経路で既存 kind 不明 → **fail-secure で `Secret` 強制**。TTY からの value
    //   入力は `read_password`（非エコー）経路を通る。既存が Text であっても画面
    //   エコーが出ないだけで機能上は等価、Secret であれば想定通りの保護が成立する
    //   （`composition-root.md §run_edit IPC 経路の方針 B`）。
    // - Sqlite で needs_value_input == false → 入力しないので `Text` dummy で十分
    //   （`resolve_secret_value` は `--value` 経路で kind を参照しない）。
    let kind_for_input = decide_kind_for_input(existing_kind, discriminant(handle));

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
            // Sub-F (#44) Phase 3: `list_summaries` は `ListSummariesOutcome` を返す。
            // ここでは label プレビューのみ必要なので `protection_mode` は破棄する
            // (本関数は `usecase::remove` の確認プロンプト用ヘルパであり、保護モード
            // バナーの責務は `usecase::list` 経路に閉じている)。
            let outcome = ipc.list_summaries()?;
            let label = outcome
                .records
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

// -------------------------------------------------------------------
// `#[cfg(test)] mod tests` — Issue #33 / Phase 1.5-ε
// -------------------------------------------------------------------
//
// docs/features/daemon-ipc/test-design/unit.md §2.17（TC-UT-130〜134）の実装。
// `decide_kind_for_input` は `RepositoryHandleDiscriminant` を直接受けるため、
// 実 `RepositoryHandle` 構築（= daemon spawn / SQLite ファイル作成）は不要であり、
// 完全に純粋関数 1 つの単体テストで完結する（§3.10 ① 案 1 の利点を活用）。
//
// TC-UT-135（`read_value_from_stdin` 経路選択）は §2.17 末尾で **案 Y（廃止、
// E2E のみで担保）** が許容されている。本 Issue では `is_stdin_tty` の trait
// 抽象化（`TerminalProbe`）を導入しない方針のため、TC-UT-135 は実装しない。
// 代わりに TC-E2E-017（pty 経由）で fail-secure 経路の振る舞いを実観測する。

#[cfg(test)]
mod tests {
    use super::{
        decide_kind_for_input, discriminant, RepositoryHandle, RepositoryHandleDiscriminant,
    };
    use shikomi_core::RecordKind;
    use shikomi_infra::persistence::SqliteVaultRepository;

    // ---------------------------------------------------------------
    // TC-UT-130: 既存 Text 判明 → identity（Sqlite 経路）
    // unit.md §2.17 / Issue #33
    // ---------------------------------------------------------------
    #[test]
    fn tc_ut_130_existing_text_with_sqlite_returns_text() {
        let kind =
            decide_kind_for_input(Some(RecordKind::Text), RepositoryHandleDiscriminant::Sqlite);
        assert_eq!(kind, RecordKind::Text);
    }

    // ---------------------------------------------------------------
    // TC-UT-131: 既存 Secret 判明 → identity（Sqlite 経路）
    // unit.md §2.17 / Issue #33
    // ---------------------------------------------------------------
    #[test]
    fn tc_ut_131_existing_secret_with_sqlite_returns_secret() {
        let kind = decide_kind_for_input(
            Some(RecordKind::Secret),
            RepositoryHandleDiscriminant::Sqlite,
        );
        assert_eq!(kind, RecordKind::Secret);
    }

    // ---------------------------------------------------------------
    // TC-UT-132 ★方針 B の核★: IPC 経路 + 既存 kind 不明 → fail-secure で Secret 強制
    // unit.md §2.17 / Issue #33 / composition-root.md §run_edit IPC 経路の方針 B
    //
    // この 1 行が型レベルで「IPC 経路で kind が判明しないとき必ず Secret に倒れる」
    // ことを固定する。経路選択（`read_password` 非エコー）は
    // `read_value_from_stdin` の `if matches!(kind, Secret) && is_stdin_tty()` 分岐に
    // 委譲、本 UT は kind 決定論のみ検証する。後段経路の実観測は TC-E2E-017 で担保。
    // ---------------------------------------------------------------
    #[test]
    fn tc_ut_132_unknown_kind_with_ipc_returns_secret_fail_secure() {
        let kind = decide_kind_for_input(None, RepositoryHandleDiscriminant::Ipc);
        assert_eq!(
            kind,
            RecordKind::Secret,
            "fail-secure: IPC 経路で kind 不明時は Secret に強制されるべき（方針 B）"
        );
    }

    // ---------------------------------------------------------------
    // TC-UT-133: Sqlite + needs_value_input == false の dummy 経路 → Text
    // unit.md §2.17 / Issue #33
    //
    // `resolve_secret_value` の `--value` 経路は kind を参照しないため、dummy 値
    // である Text を返しても副作用なし。実装の現状値を固定する identity 検証。
    // ---------------------------------------------------------------
    #[test]
    fn tc_ut_133_unknown_kind_with_sqlite_returns_text_dummy() {
        let kind = decide_kind_for_input(None, RepositoryHandleDiscriminant::Sqlite);
        assert_eq!(kind, RecordKind::Text);
    }

    // ---------------------------------------------------------------
    // TC-UT-134: 横串（IPC アーム不変条件） — 任意の `existing_kind` 入力 × Ipc で
    // 戻り値が `RecordKind::Text` を一切返さないこと。実装は 3 入力の単純列挙で
    // 網羅し、IPC アームに dummy Text が紛れ込まない構造保証を行う。
    // unit.md §2.17 / Issue #33（**副次契約「IPC アーム不変条件」**、`b50e15d`
    // ペテルギウス review 指摘 → 案 a 対応で 3 入力全網羅 `assert_ne!` に強化）。
    //
    // **対応する本番強化（`2798473`）**: `decide_kind_for_input` の `match` を
    // `(_, Ipc) => Secret` に再構造化し、呼出側 `run_edit::existing_kind` ロジック
    // への依存を排除。本 TC はその「呼出側非依存・本関数単体で fail-secure」契約を
    // 純粋関数の I/O だけで構造保証する（Defense in Depth）。
    // ---------------------------------------------------------------
    #[test]
    fn tc_ut_134_ipc_arm_never_returns_text_invariant() {
        // 3 入力（None / Some(Text) / Some(Secret)）を全網羅
        let inputs: [Option<RecordKind>; 3] =
            [None, Some(RecordKind::Text), Some(RecordKind::Secret)];

        for existing in inputs {
            let result = decide_kind_for_input(existing, RepositoryHandleDiscriminant::Ipc);
            // 「IPC アームでは何があっても Text を返さない」ことを 3 入力全てで構造保証。
            // 補強アサート: 同時に Secret に**確定**することも併せて固定（fail-secure 強制）。
            assert_ne!(
                result,
                RecordKind::Text,
                "IPC アーム不変条件違反: existing={existing:?} で Text が返却された"
            );
            assert_eq!(
                result,
                RecordKind::Secret,
                "IPC アームは existing の如何によらず Secret 強制であるべき: existing={existing:?}"
            );
        }
    }

    // ---------------------------------------------------------------
    // discriminant() の射影確認（Sqlite アーム、補助的）
    // unit.md §2.17 §3.10 ①
    //
    // Ipc アーム（IpcVaultRepository 構築）は実 daemon spawn を要するため、
    // discriminant の Ipc 側射影確認は TC-E2E-016/017 のラウンドトリップで間接担保。
    // 本 UT は Sqlite 側のみで型射影が成立することを確認する（純粋 path-only）。
    // ---------------------------------------------------------------
    #[test]
    fn discriminant_maps_sqlite_handle_to_sqlite_tag() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let repo = SqliteVaultRepository::from_directory(tmp.path()).expect("repo");
        let handle = RepositoryHandle::Sqlite(repo);
        assert_eq!(discriminant(&handle), RepositoryHandleDiscriminant::Sqlite);
    }
}
