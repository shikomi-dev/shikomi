# 詳細設計書 — clap-config（clap attribute 詳細 / エラー扱い）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/detailed-design/clap-config.md -->
<!-- 兄弟: ./index.md, ./data-structures.md, ./public-api.md, ./composition-root.md, ./infra-changes.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。clap の attribute は**属性名と意味**のみ示し、具体的なマクロ展開は書かない。

## `CliArgs` 構造体の attribute

- `#[derive(Parser, Debug)]`
- `#[command(name = "shikomi", version, about = "...", long_about = None)]`
- フィールド:
  - `vault_dir: Option<PathBuf>` — `#[arg(long = "vault-dir", global = true, env = "SHIKOMI_VAULT_DIR", value_name = "PATH")]`
    - **`env` attribute で env 吸収を完結させる**（真実源の二重化防止、ペテルギウス指摘 ④）
    - `global = true` で全サブコマンドから共用可
    - アプリ層で `std::env::var("SHIKOMI_VAULT_DIR")` を**追加で呼ばない**
  - `quiet: bool` — `#[arg(long, short, global = true)]`
    - 成功出力抑止（エラー出力は通常通り）
  - `verbose: bool` — `#[arg(long, short, global = true)]`
    - `tracing` を `debug` に上げる（`info` デフォルト）
  - `ipc: bool` — `#[arg(long = "ipc", global = true)]` （**Issue #26 / `daemon-ipc` feature で追加**）
    - daemon 経由経路（Phase 2）に切替えるオプトインフラグ
    - 既定（`false`） → 既存の `SqliteVaultRepository::from_directory(...)` 直結経路
    - `true` → `IpcVaultRepository::connect(&default_socket_path()?)` 経由で daemon に接続
    - daemon 未起動時は `CliError::DaemonNotRunning` で Fail Fast（終了コード 1、`MSG-CLI-110`）
    - 詳細: `docs/features/daemon-ipc/detailed-design/composition-root.md §shikomi_cli::run の編集`
  - `subcommand: Subcommand` — `#[command(subcommand)]`

## `Subcommand` enum の attribute

- `#[derive(Subcommand, Debug)]`
- バリアント:
  - `List` — サブコマンドなしフィールド
  - `Add(AddArgs)` — `#[command(about = "Add a new record")]`
  - `Edit(EditArgs)` — `#[command(about = "Edit an existing record")]`
  - `Remove(RemoveArgs)` — `#[command(about = "Remove a record", visible_alias = "rm")]`

**`List` の `about`**: `#[command(about = "List all records")]` を `List` バリアントに付ける。

## `AddArgs` 構造体の attribute

- `#[derive(Args, Debug)]`
- フィールド:
  - `kind: KindArg` — `#[arg(long, value_enum)]`
  - `label: String` — `#[arg(long, value_name = "STRING")]`
  - `value: Option<String>` — `#[arg(long, value_name = "STRING")]`
  - `stdin: bool` — `#[arg(long)]`

**`--value` と `--stdin` の衝突検出**: clap の `conflicts_with` attribute は**使わない**。clap の自動衝突メッセージは i18n 対応が煩雑で、`CliError::UsageError` 経由の統一的処理と競合する。`run()` 側で `(args.value.is_some(), args.stdin)` の 4 パターンを明示評価し、両真なら `CliError::UsageError("--value and --stdin are mutually exclusive")` を返す。

## `EditArgs` 構造体の attribute

- `#[derive(Args, Debug)]`
- フィールド:
  - `id: String` — `#[arg(long, value_name = "UUID")]`
  - `label: Option<String>` — `#[arg(long, value_name = "STRING")]`
  - `value: Option<String>` — `#[arg(long, value_name = "STRING")]`
  - `stdin: bool` — `#[arg(long)]`
  - **`--kind` フィールドは定義しない**（Phase 1 スコープ外、`requirements.md` REQ-CLI-003 注記）。ユーザが `shikomi edit --kind secret` と打った場合、clap は「unknown flag」で自動エラー終了（終了コード 1 に揃える処理は後述）

**「最低 1 つの更新フラグ必須」のチェック**: clap の `arg_required_else_help` や `required_unless_present` では複数フラグの「少なくとも 1 つ」表現が煩雑。`run()` 側で `(args.label.is_some() || args.value.is_some() || args.stdin)` を評価し、全 false なら `CliError::UsageError("at least one of --label/--value/--stdin is required")` を返す。

## `RemoveArgs` 構造体の attribute

- `#[derive(Args, Debug)]`
- フィールド:
  - `id: String` — `#[arg(long, value_name = "UUID")]`
  - `yes: bool` — `#[arg(long, short = 'y')]`

## `KindArg` enum の attribute

- `#[derive(ValueEnum, Clone, Debug)]`
- `#[value(rename_all = "snake_case")]`
- バリアント:
  - `Text` — `#[value(name = "text")]`
  - `Secret` — `#[value(name = "secret")]`

## `KindArg` → `RecordKind` の From 実装

- `impl From<KindArg> for RecordKind`
  - `KindArg::Text → RecordKind::Text`
  - `KindArg::Secret → RecordKind::Secret`
- CLI 層のみに閉じる写像。`shikomi-core` は `clap` に依存しない

## clap エラー扱い

clap が返す `clap::Error`（不正フラグ / ヘルプ要求 / バージョン要求 / usage error）は `run()` で以下のように処理する:

| `ErrorKind` | 処理 | 終了コード |
|------------|------|----------|
| `DisplayHelp` / `DisplayHelpOnMissingArgumentOrSubcommand` | clap の自動出力をそのまま stdout に流し、`ExitCode::Success`（0） | 0 |
| `DisplayVersion` | 同上、stdout に version 表示、`ExitCode::Success` | 0 |
| `InvalidValue` / `UnknownArgument` / `InvalidSubcommand` / `MissingRequiredArgument` / `NoEquals` / `TooManyValues` / `TooFewValues` / `MissingSubcommand` / `DisplayHelpOnMissingArgument` / その他 usage 系 | clap のエラーメッセージを **stderr** に流し、**終了コードを 1** に揃える（clap デフォルト 2 を上書き） | 1 |
| `Io` / `Format` | 同上、stderr に流して終了コード 2（システムエラー扱い） | 2 |

**実装手順**:

1. `run()` 先頭で `let args = match CliArgs::try_parse() { Ok(a) => a, Err(e) => return handle_clap_error(e) };` の形
2. `handle_clap_error(e: clap::Error) -> ExitCode` は上記テーブルに従って分岐
3. `e.print()` もしくは `e.to_string()` でメッセージを取得
4. stdout / stderr を `ErrorKind` で分岐

## テスト観点

- `--help` / `--version` が終了コード 0（`assert_cmd` で検証）
- 未知サブコマンド → 終了コード 1 かつ stderr にメッセージ
- `--value` と `--stdin` 両指定 → 終了コード 1 + `MSG-CLI-100`
- `edit` に `--kind` 指定 → clap の「unknown argument」エラー → 終了コード 1（本 feature で `--kind` を定義しないことの検証）
- `add --kind text --label L`（値指定なし） → 終了コード 1 + usage error
- `--ipc` 未指定（既定）→ `args.ipc == false` で SQLite 直結経路（`daemon-ipc` feature 追加分の検証）
- `--ipc` 指定 → `args.ipc == true` で `IpcVaultRepository::connect` 経路（daemon 起動状態の E2E テスト、`docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md` 参照）
