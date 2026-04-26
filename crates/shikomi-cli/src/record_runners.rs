//! `shikomi {list,add,edit,remove}` レコード操作系の dispatcher 群。
//!
//! 工程5 ペガサス指摘 (lib.rs 858行 → 500 行ルール超過) 解消のため、`lib.rs` から
//! レコード CRUD 系 dispatcher と入力ヘルパを本ファイルに切り出した。`vault`
//! サブコマンド経路 (`run_vault`) と関心事を分離し、`lib.rs` をコンポジション
//! ルート + IPC handshake に集約する設計に整理。
//!
//! 設計根拠:
//! - docs/features/cli-vault-commands/detailed-design/composition-root.md
//!   §処理順序 / §`RepositoryHandle` enum
//! - docs/features/daemon-ipc/test-design/unit.md §2.17 / §3.10 ①

use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use shikomi_infra::persistence::VaultRepository;
use time::OffsetDateTime;

use crate::cli::{self, AddArgs, EditArgs, RemoveArgs};
use crate::error::CliError;
use crate::input::{AddInput, ConfirmedRemoveInput, EditInput};
use crate::io;
use crate::presenter::{self, Locale};
use crate::usecase;
use crate::{eprint_stderr, print_stdout, RepositoryHandle, RepositoryHandleDiscriminant};

// -------------------------------------------------------------------
// pure helpers (Issue #33 / Phase 1.5-ε)
// -------------------------------------------------------------------

/// `RepositoryHandle` を判別タグへ落とす射影。
pub(crate) fn discriminant(handle: &RepositoryHandle) -> RepositoryHandleDiscriminant {
    match handle {
        RepositoryHandle::Sqlite(_) => RepositoryHandleDiscriminant::Sqlite,
        RepositoryHandle::Ipc(_) => RepositoryHandleDiscriminant::Ipc,
    }
}

/// `run_edit` の値入力 kind を決定する純粋関数 (詳細は元 lib.rs ヘッダコメント参照)。
pub(crate) fn decide_kind_for_input(
    existing_kind: Option<RecordKind>,
    handle: RepositoryHandleDiscriminant,
) -> RecordKind {
    match (existing_kind, handle) {
        (_, RepositoryHandleDiscriminant::Ipc) => RecordKind::Secret,
        (Some(k), RepositoryHandleDiscriminant::Sqlite) => k,
        (None, RepositoryHandleDiscriminant::Sqlite) => RecordKind::Text,
    }
}

// -------------------------------------------------------------------
// run_list / run_add / run_edit / run_remove (lib.rs から移管)
// -------------------------------------------------------------------

pub(crate) fn run_list(
    handle: &RepositoryHandle,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
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

    if matches!(
        protection_mode,
        shikomi_core::ipc::ProtectionModeBanner::Unknown
    ) {
        return Err(CliError::ProtectionModeUnknown);
    }

    if !quiet {
        let color_enabled = is_color_enabled();
        let rendered = presenter::list::render_list(&views, protection_mode, color_enabled, locale);
        print_stdout(&rendered);
    }
    Ok(())
}

fn is_color_enabled() -> bool {
    use is_terminal::IsTerminal;
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

pub(crate) fn run_add(
    handle: &RepositoryHandle,
    args: &AddArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let value = resolve_secret_value(args.value.as_deref(), args.stdin, args.kind.into())?;

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
            let initially_existed = repo.exists().map_err(CliError::from)?;
            let id = usecase::add::add_record(repo, input, now)?;
            if !quiet && !initially_existed {
                let init_msg = presenter::success::render_initialized_vault(&vault_dir, locale);
                print_stdout(&init_msg);
            }
            id
        }
        RepositoryHandle::Ipc(ipc) => ipc.add_record(input.kind, input.label, input.value, now)?,
    };

    if !quiet {
        let added = presenter::success::render_added(&id, locale);
        print_stdout(&added);
    }
    Ok(())
}

pub(crate) fn run_edit(
    handle: &RepositoryHandle,
    args: &EditArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
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

pub(crate) fn run_remove(
    handle: &RepositoryHandle,
    args: &RemoveArgs,
    locale: Locale,
    quiet: bool,
) -> Result<(), CliError> {
    let id = RecordId::try_from_str(&args.id).map_err(CliError::InvalidId)?;

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
        io::terminal::read_password("value: ").map_err(|e| {
            CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
                path: std::path::PathBuf::from("<stdin>"),
                source: e,
            })
        })?
    } else {
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
// tests (Issue #33 / Phase 1.5-ε、lib.rs から移管)
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{decide_kind_for_input, discriminant};
    use crate::{RepositoryHandle, RepositoryHandleDiscriminant};
    use shikomi_core::RecordKind;
    use shikomi_infra::persistence::SqliteVaultRepository;

    #[test]
    fn tc_ut_130_existing_text_with_sqlite_returns_text() {
        let kind =
            decide_kind_for_input(Some(RecordKind::Text), RepositoryHandleDiscriminant::Sqlite);
        assert_eq!(kind, RecordKind::Text);
    }

    #[test]
    fn tc_ut_131_existing_secret_with_sqlite_returns_secret() {
        let kind = decide_kind_for_input(
            Some(RecordKind::Secret),
            RepositoryHandleDiscriminant::Sqlite,
        );
        assert_eq!(kind, RecordKind::Secret);
    }

    #[test]
    fn tc_ut_132_unknown_kind_with_ipc_returns_secret_fail_secure() {
        let kind = decide_kind_for_input(None, RepositoryHandleDiscriminant::Ipc);
        assert_eq!(
            kind,
            RecordKind::Secret,
            "fail-secure: IPC 経路で kind 不明時は Secret に強制されるべき (方針 B)"
        );
    }

    #[test]
    fn tc_ut_133_unknown_kind_with_sqlite_returns_text_dummy() {
        let kind = decide_kind_for_input(None, RepositoryHandleDiscriminant::Sqlite);
        assert_eq!(kind, RecordKind::Text);
    }

    #[test]
    fn tc_ut_134_ipc_arm_never_returns_text_invariant() {
        let inputs: [Option<RecordKind>; 3] =
            [None, Some(RecordKind::Text), Some(RecordKind::Secret)];

        for existing in inputs {
            let result = decide_kind_for_input(existing, RepositoryHandleDiscriminant::Ipc);
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

    #[test]
    fn discriminant_maps_sqlite_handle_to_sqlite_tag() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let repo = SqliteVaultRepository::from_directory(tmp.path()).expect("repo");
        let handle = RepositoryHandle::Sqlite(repo);
        assert_eq!(discriminant(&handle), RepositoryHandleDiscriminant::Sqlite);
    }
}
