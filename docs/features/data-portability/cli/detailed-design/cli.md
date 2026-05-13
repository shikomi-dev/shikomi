# 詳細設計書 — data-portability / cli / cli.rs・error.rs 変更

<!-- feature: data-portability / sub-feature: cli / Issue #141 -->
<!-- 配置先: docs/features/data-portability/cli/detailed-design/cli.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親: ../basic-design.md -->
<!-- 兄弟: usecase.md / presenter.md -->

## 記述ルール

疑似コード禁止。処理順序は番号付き箇条書きで表現する。型・フィールド・モジュールパスは `code` 表記で明示する。

## 変更対象ファイル

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/cli.rs` | 編集 | `Subcommand::Export(ExportArgs)` / `Subcommand::Import(ImportArgs)` / `ExportArgs` / `ImportArgs` / `OnConflictArg` を追加 |
| `crates/shikomi-cli/src/error.rs` | 編集 | `CliError` 新バリアント 6 種 + `ExitCode` match arm + `From<DataPortabilityError> for CliError` |

---

## `crates/shikomi-cli/src/cli.rs` の変更詳細

### 追加: `Subcommand` バリアント

`Subcommand` enum の末尾（`Daemon` バリアントの後）に以下 2 バリアントを追加する:

- `Export(ExportArgs)` — about: `"Export vault records to a JSON file"` — 設計根拠: `basic-design.md §REQ-DP-007`
- `Import(ImportArgs)` — about: `"Import records from a JSON export file into the vault"`

### 追加: `ExportArgs` 型（`#[derive(Args, Debug)]`）

| フィールド | clap 属性 | 型 | 説明 |
|-----------|----------|----|------|
| `output` | `#[arg(long, value_name = "FILE")]`（必須）| `PathBuf` | export 先ファイルパス |
| `export_secrets` | `#[arg(long)]`（フラグ）| `bool` | Secret kind のペイロードを平文で export する（既定: リダクト）|
| `force` | `#[arg(long)]`（フラグ）| `bool` | 出力先ファイルが既に存在する場合に上書きする |

`--export-secrets` フラグの doc comment: 「Secret kind のペイロードを平文で export する（既定はリダクト）。実行時は stderr に MSG-CLI-145 が必ず出力される（--quiet でも抑止不可）。」

### 追加: `ImportArgs` 型（`#[derive(Args, Debug)]`）

| フィールド | clap 属性 | 型 | 説明 |
|-----------|----------|----|------|
| `input` | `#[arg(long, value_name = "FILE")]`（必須）| `PathBuf` | import 元ファイルパス |
| `on_conflict` | `#[arg(long, value_enum, default_value = "error")]` | `OnConflictArg` | 衝突戦略 |

### 追加: `OnConflictArg` 型（`#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]`）

`#[value(rename_all = "snake_case")]` を付与する。バリアント:

| バリアント | clap 値 | 動作 |
|-----------|---------|------|
| `Error` | `error`（既定）| 衝突 ID が存在した場合即座に `MSG-CLI-142` で exit 1 |
| `Skip` | `skip` | 衝突 ID のレコードをスキップして残りを追加 |
| `Overwrite` | `overwrite` | 衝突 ID の既存レコードを import ファイルの値で置換 |

---

## `crates/shikomi-cli/src/error.rs` の変更詳細

### 追加: `CliError` バリアント（`#[non_exhaustive]` enum 末尾のコメント区切り後に追加）

コメント区切り `// ---------------- Issue #141: data-portability export / import ----------------` を追加してグループ化する。

| バリアント | `#[error]` 文言 | MSG ID | exit code |
|-----------|----------------|--------|----------|
| `ExportImportVaultLocked` | `"vault is locked; unlock the vault before running export or import"` | MSG-CLI-140 | 1 |
| `ExportOutputFileExists { path: PathBuf }` | `"export output file already exists: {path}"` | MSG-CLI-141 | 1 |
| `ImportConflict { ids: Vec<String> }` | `"import conflict: {} record(s) already exist in vault"` (`.len()` 使用) | MSG-CLI-142 | 1 |
| `ImportDeserializationFailed { reason: String }` | `"failed to parse import file: {reason}"` | MSG-CLI-143 | 1 |
| `ImportValidationFailed(shikomi_core::portability::ImportValidationError)` | `"import validation failed: {0}"` | MSG-CLI-143 / MSG-CLI-144 | 1 |
| `ImportVaultBusy` | `"vault is in use by shikomi-daemon; import aborted after 2 seconds"` | MSG-CLI-146 | 1 |

### 追加: `ExitCode::from(&CliError)` の match arm

`ExitCode::UserError` の arm に以下 6 バリアントを追加する（既存 `HotkeyParseError { .. }` の後）:

- `CliError::ExportImportVaultLocked`
- `CliError::ExportOutputFileExists { .. }`
- `CliError::ImportConflict { .. }`
- `CliError::ImportDeserializationFailed { .. }`
- `CliError::ImportValidationFailed(_)`
- `CliError::ImportVaultBusy`

全て `Self::UserError`（exit 1）に写像する。

**注意**: `error.rs` に存在する `tc_f_u15_exit_code_ssot_mapping_for_all_cli_error_variants_in_one_matrix` テストの `user_error_cases` Vec にも 6 件を追加する（コンパイル時網羅性確認のため）。

### 追加: `From<DataPortabilityError> for CliError` 実装

既存の `From<PersistenceError> for CliError` の後に追加する。`DataPortabilityError` の各バリアントを対応する `CliError` バリアントに変換する:

| `DataPortabilityError` | `CliError` |
|------------------------|-----------|
| `VaultLocked` | `ExportImportVaultLocked` |
| `OutputFileExists { path }` | `ExportOutputFileExists { path }` |
| `ConflictError { ids }` | `ImportConflict { ids }` |
| `DeserializationFailed { reason }` | `ImportDeserializationFailed { reason }` |
| `ValidationFailed(err)` | `ImportValidationFailed(err)` |
| `IoError(io_err)` | `Persistence(PersistenceError::Internal { reason: io_err.to_string() })` |
| `VaultBusy` | `ImportVaultBusy` |

`DataPortabilityError` の型パスは `crate::usecase::portability::error::DataPortabilityError`。

---

## セキュリティ考慮（cli.rs / error.rs スコープ）

| 脅威 | 対策 |
|------|------|
| `CliError::Display` に Secret 値が混入 | 新バリアントの `#[error]` 文言は全て固定文言または path / reason（検証済み文字列）のみ。Secret 値は一切含まれない |
| exit code の意味論的混乱（ExportImportVaultLocked が exit 1 / 既存 VaultLocked が exit 3）| 別バリアントで明示的に分離。`tc_f_u15_exit_code_ssot_mapping` テストが網羅的に検証 |
