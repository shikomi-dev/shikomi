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
