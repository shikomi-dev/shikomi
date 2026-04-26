//! clap 派生型の置き場。
//!
//! `run()` の肥大化を避けるため、`#[derive(Parser)]` 構造体群を本モジュールに集約。
//! コマンド分岐は `lib.rs::run()` 側の `match` に残す。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/clap-config.md

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand as ClapSubcommand, ValueEnum};
use shikomi_core::RecordKind;

/// vault ディレクトリを上書きする環境変数名。clap の `env` attribute から参照する。
pub const ENV_VAR_VAULT_DIR: &str = "SHIKOMI_VAULT_DIR";

// -------------------------------------------------------------------
// CliArgs
// -------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "shikomi",
    version,
    about = "shikomi — a local encrypted credential vault (CLI Phase 1: plaintext only)",
    long_about = None,
)]
pub struct CliArgs {
    /// vault ディレクトリ上書き。env `SHIKOMI_VAULT_DIR` も自動吸収する（clap `env` attribute、
    /// 真実源の二重化防止のためアプリ層では env を読まない）。
    #[arg(long = "vault-dir", global = true, env = ENV_VAR_VAULT_DIR, value_name = "PATH")]
    pub vault_dir: Option<PathBuf>,

    /// 成功出力を抑止する（stderr は通常通り）。
    #[arg(long, short, global = true)]
    pub quiet: bool,

    /// tracing を debug レベルへ上げる。
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Use the running shikomi-daemon over IPC instead of opening the vault file directly.
    /// Currently supported only with the `list` subcommand; requires shikomi-daemon to be running.
    // NOTE: 内部メモ — daemon-ipc feature (Issue #26) で追加。`add`/`edit`/`remove` の IPC 経路は
    // Phase 2 移行 PR で完成させるため、本フラグは現状 `list` 限定の opt-in。
    // ユーザ向けには上記 doc comment のみ露出する（`--help` 内部用語汚染を避けるため）。
    #[arg(long, global = true)]
    pub ipc: bool,

    #[command(subcommand)]
    pub subcommand: Subcommand,
}

impl CliArgs {
    /// `clap::Parser::try_parse` の薄ラッパ（run 側の import 削減）。
    ///
    /// # Errors
    /// clap が返す任意の `clap::Error` を透過する。
    pub fn try_parse() -> Result<Self, clap::Error> {
        <Self as Parser>::try_parse()
    }
}

// -------------------------------------------------------------------
// Subcommand
// -------------------------------------------------------------------

#[derive(ClapSubcommand, Debug)]
pub enum Subcommand {
    /// vault 内のレコード一覧を表示する。
    #[command(about = "List all records")]
    List,
    /// 新しいレコードを追加する。
    #[command(about = "Add a new record")]
    Add(AddArgs),
    /// 既存レコードを編集する。
    #[command(about = "Edit an existing record")]
    Edit(EditArgs),
    /// レコードを削除する。
    #[command(about = "Remove a record", visible_alias = "rm")]
    Remove(RemoveArgs),
    /// vault 暗号化管理サブコマンド群（Sub-F #44、F-F1〜F-F7）。
    ///
    /// 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
    /// §設計判断 vault サブコマンドのグループ化（案 B 採用）
    #[command(
        subcommand,
        about = "Vault encryption management commands (encrypt/decrypt/unlock/lock/change-password/rekey/rotate-recovery)"
    )]
    Vault(VaultSubcommand),
}

// -------------------------------------------------------------------
// VaultSubcommand（Sub-F #44）
// -------------------------------------------------------------------

/// `shikomi vault {subcommand}` の 7 サブコマンド group（cli-subcommands.md §F-F1〜F-F7）。
///
/// `recovery-show` は廃止し、24 語の出力経路は `encrypt/rekey/rotate-recovery` の
/// `--output {screen,print,braille,audio}` フラグに統合（Rev1 ペガサス致命指摘 ① 解消）。
#[derive(ClapSubcommand, Debug)]
pub enum VaultSubcommand {
    /// 平文 vault → 暗号化 vault 初回マイグレーション (F-F1)。
    /// 完了時に新生成 recovery 24 語を `--output` 経路で表示する。
    #[command(about = "Encrypt the vault (F-F1) and disclose recovery 24 words once")]
    Encrypt(EncryptArgs),
    /// 暗号化 vault → 平文 vault 戻し (F-F2)。確認入力 `DECRYPT` 必須。
    #[command(about = "Decrypt the vault back to plaintext (F-F2, requires DECRYPT confirmation)")]
    Decrypt,
    /// 暗号化 vault のロック解除 (F-F3、password 経路 / `--recovery` 経路)。
    #[command(about = "Unlock the vault (F-F3, password or --recovery path)")]
    Unlock(UnlockArgs),
    /// 暗号化 vault を明示ロック (F-F4)。VEK 即 zeroize。
    #[command(about = "Lock the vault explicitly (F-F4, VEK zeroize)")]
    Lock,
    /// マスターパスワード変更 (F-F5、O(1)、VEK 不変)。
    #[command(about = "Change master password (F-F5, O(1) wrap update)")]
    ChangePassword,
    /// VEK 入替 + recovery 24 語ローテーション atomic 実行 (F-F6)。
    /// 新 24 語を `--output` 経路で表示する。
    #[command(about = "Rekey VEK and rotate recovery words (F-F6)")]
    Rekey(OutputArgs),
    /// recovery 24 語のみローテーション atomic 実行 (F-F7)。
    /// 新 24 語を `--output` 経路で表示する。
    #[command(about = "Rotate recovery words only (F-F7)")]
    RotateRecovery(OutputArgs),
}

// -------------------------------------------------------------------
// VaultSubcommand 子型（EncryptArgs / UnlockArgs / OutputArgs / OutputTarget）
// -------------------------------------------------------------------

/// `vault encrypt` の引数（F-F1）。
#[derive(Args, Debug)]
pub struct EncryptArgs {
    /// 強度ゲート緩和の明示同意フラグ (REQ-S08、`MasterPassword::new` 警告経路)。
    #[arg(long)]
    pub accept_limits: bool,

    /// 24 語の出力経路 (Sub-F §アクセシビリティ代替経路 MSG-S18、C-39)。
    #[arg(long, value_enum, default_value = "screen")]
    pub output: OutputTarget,
}

/// `vault unlock` の引数（F-F3）。
#[derive(Args, Debug)]
pub struct UnlockArgs {
    /// recovery 24 語経路で unlock する (`--recovery`)。
    /// 未指定時はパスワード経路。
    #[arg(long)]
    pub recovery: bool,
}

/// `vault rekey` / `rotate-recovery` の共通引数（24 語出力経路）。
#[derive(Args, Debug)]
pub struct OutputArgs {
    /// 24 語の出力経路 (Sub-F §アクセシビリティ代替経路 MSG-S18、C-39)。
    #[arg(long, value_enum, default_value = "screen")]
    pub output: OutputTarget,
}

/// 24 語の出力先（C-39 排他指定 + アクセシビリティ自動切替）。
///
/// 自動切替経路 (`SHIKOMI_ACCESSIBILITY=1` env / OS スクリーンリーダー検出) は
/// `accessibility::output_target::resolve` に集約され、`Screen` 既定時のみ
/// `Braille` に上書きされる。明示 `--output` フラグは常に最優先。
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
pub enum OutputTarget {
    /// 端末標準出力（既定）。`NO_COLOR` / 非 TTY 時はカラー無効化。
    Screen,
    /// ハイコントラスト PDF 出力。手書き PDF 1.4、黒地白文字 Helvetica 18pt、
    /// `umask(0o077)` 内部適用でリダイレクト先 0600 相当。
    Print,
    /// BRF（Braille Ready Format）テキスト出力。北米 ASCII Braille (Grade 1
    /// fallback + 一部 UEB single-letter wordsign)。
    Braille,
    /// OS TTS への直接パイプ出力（macOS `say` / Linux `espeak` /
    /// Windows `SAPI`）、中間ファイルなし `Stdio::piped()` のみ + env allowlist。
    Audio,
}

// -------------------------------------------------------------------
// AddArgs
// -------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct AddArgs {
    /// レコード種別（`text` / `secret`）。
    #[arg(long, value_enum)]
    pub kind: KindArg,

    /// レコードラベル。
    #[arg(long, value_name = "STRING")]
    pub label: String,

    /// 値を直接指定する（secret の場合は shell 履歴に残るため `--stdin` 推奨）。
    /// `--stdin` と併用不可。
    #[arg(long, value_name = "STRING")]
    pub value: Option<String>,

    /// 値を stdin から読み取る。secret 入力時は TTY なら非エコー読取。
    #[arg(long)]
    pub stdin: bool,
}

// -------------------------------------------------------------------
// EditArgs
// -------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct EditArgs {
    /// 対象レコード ID（UUIDv7 全長 36 文字）。
    #[arg(long, value_name = "UUID")]
    pub id: String,

    /// 新しいラベル（任意）。
    #[arg(long, value_name = "STRING")]
    pub label: Option<String>,

    /// 新しい値（任意）。
    #[arg(long, value_name = "STRING")]
    pub value: Option<String>,

    /// 値を stdin から読み取る。`--value` と併用不可。
    #[arg(long)]
    pub stdin: bool,
    // NOTE: `--kind` フィールドは定義しない（Phase 1 スコープ外、requirements.md REQ-CLI-003）。
}

// -------------------------------------------------------------------
// RemoveArgs
// -------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct RemoveArgs {
    /// 対象レコード ID（UUIDv7 全長 36 文字）。
    #[arg(long, value_name = "UUID")]
    pub id: String,

    /// 対話確認をスキップする。非 TTY 環境では必須（`CliError::NonInteractiveRemove` 回避）。
    #[arg(long, short = 'y')]
    pub yes: bool,
}

// -------------------------------------------------------------------
// KindArg
// -------------------------------------------------------------------

#[derive(ValueEnum, Clone, Copy, Debug)]
#[value(rename_all = "snake_case")]
pub enum KindArg {
    /// テキスト（URL / メモ等、機密度が低い）
    Text,
    /// シークレット（パスワード / 鍵等、機密度が高い）
    Secret,
}

impl From<KindArg> for RecordKind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::Text => RecordKind::Text,
            KindArg::Secret => RecordKind::Secret,
        }
    }
}

// -------------------------------------------------------------------
// テスト
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_arg_text_maps_to_record_kind_text() {
        assert!(matches!(RecordKind::from(KindArg::Text), RecordKind::Text));
    }

    #[test]
    fn test_kind_arg_secret_maps_to_record_kind_secret() {
        assert!(matches!(
            RecordKind::from(KindArg::Secret),
            RecordKind::Secret
        ));
    }

    #[test]
    fn test_cli_args_parses_list_subcommand() {
        let args = CliArgs::try_parse_from(["shikomi", "list"]).unwrap();
        assert!(matches!(args.subcommand, Subcommand::List));
    }

    #[test]
    fn test_cli_args_parses_add_subcommand_with_kind_label_value() {
        let args = CliArgs::try_parse_from([
            "shikomi", "add", "--kind", "text", "--label", "l", "--value", "v",
        ])
        .unwrap();
        assert!(matches!(args.subcommand, Subcommand::Add(_)));
    }

    #[test]
    fn test_cli_args_remove_alias_rm_accepted() {
        let args = CliArgs::try_parse_from([
            "shikomi",
            "rm",
            "--id",
            "01234567-0123-7000-8000-0123456789ab",
        ])
        .unwrap();
        assert!(matches!(args.subcommand, Subcommand::Remove(_)));
    }

    // ---------------------------------------------------------------
    // Sub-F (#44) Phase 2: VaultSubcommand clap 派生型の最小受理確認
    // 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
    // §clap 派生型構造（Subcommand 拡張）
    // ---------------------------------------------------------------

    #[test]
    fn test_cli_args_parses_vault_encrypt_with_default_output() {
        let args = CliArgs::try_parse_from(["shikomi", "vault", "encrypt"]).unwrap();
        match args.subcommand {
            Subcommand::Vault(VaultSubcommand::Encrypt(a)) => {
                assert_eq!(a.output, OutputTarget::Screen);
                assert!(!a.accept_limits);
            }
            other => panic!("expected Vault(Encrypt(_)), got {other:?}"),
        }
    }

    #[test]
    fn test_cli_args_parses_vault_encrypt_with_accept_limits_and_braille() {
        let args = CliArgs::try_parse_from([
            "shikomi",
            "vault",
            "encrypt",
            "--accept-limits",
            "--output",
            "braille",
        ])
        .unwrap();
        match args.subcommand {
            Subcommand::Vault(VaultSubcommand::Encrypt(a)) => {
                assert!(a.accept_limits);
                assert_eq!(a.output, OutputTarget::Braille);
            }
            other => panic!("expected Vault(Encrypt(_)), got {other:?}"),
        }
    }

    #[test]
    fn test_cli_args_parses_vault_decrypt_lock_change_password() {
        for sub in ["decrypt", "lock", "change-password"] {
            let args = CliArgs::try_parse_from(["shikomi", "vault", sub]).unwrap();
            assert!(matches!(args.subcommand, Subcommand::Vault(_)));
        }
    }

    #[test]
    fn test_cli_args_parses_vault_unlock_recovery_flag() {
        let args = CliArgs::try_parse_from(["shikomi", "vault", "unlock", "--recovery"]).unwrap();
        match args.subcommand {
            Subcommand::Vault(VaultSubcommand::Unlock(a)) => assert!(a.recovery),
            other => panic!("expected Vault(Unlock(_)), got {other:?}"),
        }
    }

    #[test]
    fn test_cli_args_parses_vault_rekey_with_print_output() {
        let args =
            CliArgs::try_parse_from(["shikomi", "vault", "rekey", "--output", "print"]).unwrap();
        match args.subcommand {
            Subcommand::Vault(VaultSubcommand::Rekey(a)) => {
                assert_eq!(a.output, OutputTarget::Print);
            }
            other => panic!("expected Vault(Rekey(_)), got {other:?}"),
        }
    }

    #[test]
    fn test_cli_args_rejects_password_flag_on_vault_unlock() {
        // C-38 / 服部指摘: パスワードを CLI 引数として受け付けない契約。
        // `--password` は **clap 派生型に定義しない**ため不明引数として拒否される。
        let result = CliArgs::try_parse_from(["shikomi", "vault", "unlock", "--password", "x"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_args_edit_kind_flag_is_unknown_arg() {
        // Phase 1 スコープ外のため `edit --kind` は clap のエラーになる
        let result = CliArgs::try_parse_from([
            "shikomi",
            "edit",
            "--id",
            "01234567-0123-7000-8000-0123456789ab",
            "--kind",
            "text",
        ]);
        assert!(result.is_err());
    }
}
