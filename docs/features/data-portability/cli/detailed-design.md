# 詳細設計書 — data-portability / cli

<!-- feature: data-portability / sub-feature: cli / Issue #141 -->
<!-- 配置先: docs/features/data-portability/cli/detailed-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 兄弟: ./basic-design.md -->

## 記述ルール

疑似コード禁止。処理順序は番号付き箇条書きで表現する。型・フィールド・モジュールパスは `code` 表記で明示する。

## 変更対象ファイル一覧

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/cli.rs` | 編集 | `Subcommand::Export(ExportArgs)` / `Subcommand::Import(ImportArgs)` / `ExportArgs` / `ImportArgs` / `OnConflictArg` を追加 |
| `crates/shikomi-cli/src/error.rs` | 編集 | `CliError` 新バリアント 5 種 + `ExitCode` match arm + `From<DataPortabilityError> for CliError` |
| `crates/shikomi-cli/src/usecase/mod.rs` | 編集 | `pub mod portability;` を追加 |
| `crates/shikomi-cli/src/usecase/portability/mod.rs` | 新規 | `export` / `import` / `error` モジュールの re-export |
| `crates/shikomi-cli/src/usecase/portability/error.rs` | 新規 | `DataPortabilityError` 型定義 |
| `crates/shikomi-cli/src/usecase/portability/export.rs` | 新規 | `export_records` + `ExportSummary` |
| `crates/shikomi-cli/src/usecase/portability/import.rs` | 新規 | `import_records` + `ImportSummary` + `import_record_to_domain` |
| `crates/shikomi-cli/src/presenter/success.rs` | 編集 | `render_exported` / `render_imported` / `render_export_secrets_warning` を追加 |
| `crates/shikomi-cli/src/presenter/error.rs` | 編集 | `lines_for` に新 5 バリアントの arm 追加 + `render_error` に MSG-CLI-144 dispatch 追加 |
| `crates/shikomi-cli/src/lib.rs` | 編集 | `Subcommand::Export` / `Subcommand::Import` の match arm + MSG-CLI-145 stderr 出力 |

変更不要ファイル:

| ファイル | 理由 |
|---------|------|
| `crates/shikomi-cli/Cargo.toml` | `tempfile = { workspace = true }` は既存依存（line 71）。追加依存なし |
| `crates/shikomi-core/` 以下全ファイル | Sub-A（Issue #140）で完成済み。CLI 層は `shikomi-core::portability` を参照するのみ |

---

## `crates/shikomi-cli/src/cli.rs` の変更詳細

### 追加: `Subcommand` バリアント

`Subcommand` enum の末尾（`Daemon` バリアントの後）に以下 2 バリアントを追加する:

```
/// vault のレコードを JSON ファイルにエクスポートする（Issue #141）。
///
/// 設計根拠: docs/features/data-portability/cli/basic-design.md §REQ-DP-007
#[command(about = "Export vault records to a JSON file")]
Export(ExportArgs),

/// JSON ファイルから vault にレコードをインポートする（Issue #141）。
///
/// 設計根拠: docs/features/data-portability/cli/basic-design.md §REQ-DP-007
#[command(about = "Import records from a JSON export file into the vault")]
Import(ImportArgs),
```

### 追加: `ExportArgs` 型

```
/// `shikomi export` の引数。
#[derive(Args, Debug)]
pub struct ExportArgs {
    /// export 先ファイルパス（必須）。
    #[arg(long, value_name = "FILE")]
    pub output: PathBuf,

    /// Secret kind のペイロードを平文で export する。
    /// 未指定時は `{"kind":"redacted"}` でリダクトされる（既定）。
    /// 実行時は stderr に MSG-CLI-145 が必ず出力される（--quiet でも抑止不可）。
    #[arg(long)]
    pub export_secrets: bool,

    /// 出力先ファイルが既に存在する場合に上書きする。
    #[arg(long)]
    pub force: bool,
}
```

### 追加: `ImportArgs` 型

```
/// `shikomi import` の引数。
#[derive(Args, Debug)]
pub struct ImportArgs {
    /// import 元ファイルパス（必須）。
    #[arg(long, value_name = "FILE")]
    pub input: PathBuf,

    /// ID 衝突時の戦略（既定: error）。
    #[arg(long, value_enum, default_value = "error")]
    pub on_conflict: OnConflictArg,
}
```

### 追加: `OnConflictArg` 型

```
/// `shikomi import --on-conflict` の値（feature-spec.md R1-DP-06）。
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
pub enum OnConflictArg {
    /// 衝突した場合は即座にエラーで終了する（既定）。
    Error,
    /// 衝突した ID のレコードをスキップして残りを追加する。
    Skip,
    /// 衝突した ID の既存レコードを import ファイルの値で置換する。
    Overwrite,
}
```

---

## `crates/shikomi-cli/src/error.rs` の変更詳細

### 追加: `CliError` バリアント（`#[non_exhaustive]` enum の末尾に追加）

コメント区切りを追加してグループ化する:

```
// ---------------- Issue #141: data-portability export / import ----------------

/// export / import 実行時に vault がロック済み（`EncryptionUnsupported` 含む）。
/// feature-spec.md UC-DP-001 代替フロー B / UC-DP-002（MSG-CLI-140、exit 1）。
#[error("vault is locked; unlock the vault before running export or import")]
ExportImportVaultLocked,

/// export 先ファイルが既に存在し `--force` 未指定（MSG-CLI-141、exit 1）。
#[error("export output file already exists: {path}")]
ExportOutputFileExists {
    path: PathBuf,
},

/// `--on-conflict error` で 1 件以上の衝突が発生（MSG-CLI-142、exit 1）。
/// `ids` は衝突した RecordId の文字列表現（表示用に最大 4 件 + 省略）。
#[error("import conflict: {} record(s) already exist in vault", ids.len())]
ImportConflict {
    ids: Vec<String>,
},

/// JSON パース失敗（MSG-CLI-143、exit 1）。
#[error("failed to parse import file: {reason}")]
ImportDeserializationFailed {
    reason: String,
},

/// `ImportValidator::validate` 失敗（MSG-CLI-143 または MSG-CLI-144、exit 1）。
/// バリアント内容によって `render_error` が MSG を切り替える。
#[error("import validation failed: {0}")]
ImportValidationFailed(shikomi_core::portability::ImportValidationError),
```

### 追加: `ExitCode::from(&CliError)` の match arm

`ExitCode::UserError` の arm に以下を追加する（`HotkeyParseError { .. }` の後）:

```
| CliError::ExportImportVaultLocked
| CliError::ExportOutputFileExists { .. }
| CliError::ImportConflict { .. }
| CliError::ImportDeserializationFailed { .. }
| CliError::ImportValidationFailed(_) => Self::UserError,
```

### 追加: `DataPortabilityError` → `CliError` 変換（`From` 実装）

`error.rs` の末尾（既存の `From<PersistenceError>` の後）に追加:

```
impl From<crate::usecase::portability::error::DataPortabilityError> for CliError {
    fn from(e: crate::usecase::portability::error::DataPortabilityError) -> Self {
        use crate::usecase::portability::error::DataPortabilityError as Dp;
        match e {
            Dp::VaultLocked => Self::ExportImportVaultLocked,
            Dp::OutputFileExists { path } => Self::ExportOutputFileExists { path },
            Dp::ConflictError { ids } => Self::ImportConflict { ids },
            Dp::DeserializationFailed { reason } => Self::ImportDeserializationFailed { reason },
            Dp::ValidationFailed(err) => Self::ImportValidationFailed(err),
            Dp::IoError(io_err) => Self::Persistence(
                shikomi_infra::persistence::PersistenceError::Internal {
                    reason: io_err.to_string(),
                },
            ),
        }
    }
}
```

---

## `crates/shikomi-cli/src/usecase/portability/error.rs` の設計詳細

### `DataPortabilityError` 型

UseCase 内部の中間エラー型。`From` 実装で `CliError` に変換される。

| バリアント | フィールド | 発生条件 |
|-----------|-----------|---------|
| `VaultLocked` | なし | `repo.load()` が `CliError::VaultLocked` / `CliError::EncryptionUnsupported` を返した場合 |
| `OutputFileExists` | `path: PathBuf` | export 先ファイルが既に存在し `--force` 未指定 |
| `ConflictError` | `ids: Vec<String>` | `--on-conflict error` で衝突検出 |
| `DeserializationFailed` | `reason: String` | `serde_json::from_str` 失敗 |
| `ValidationFailed` | `ImportValidationError` | `ImportValidator::validate` 失敗 |
| `IoError` | `std::io::Error` | ファイル読み込み / `tempfile` 操作 / `persist` 失敗 |

- `std::error::Error` / `Display` / `thiserror::Error` を実装する
- `From<std::io::Error> for DataPortabilityError` を実装し、I/O エラーを `IoError` に wrap する

---

## `crates/shikomi-cli/src/usecase/portability/export.rs` の設計詳細

### `ExportSummary` 型

| フィールド | 型 | 説明 |
|-----------|----|----|
| `record_count` | `usize` | export したレコード件数 |
| `output_path` | `PathBuf` | 実際に書き込んだファイルパス（正規化済み）|

### `export_records` 関数

シグネチャ: `pub fn export_records(repo: &dyn VaultRepository, args: &ExportArgs, vault_dir: &Path, now: OffsetDateTime) -> Result<ExportSummary, CliError>`

処理順序:

1. `args.output` が既存ファイル（`args.output.exists()`）かつ `args.force == false` → `Err(DataPortabilityError::OutputFileExists { path: args.output.clone() }.into())`
2. `repo.load()` を呼び出す。`CliError::VaultLocked` または `CliError::EncryptionUnsupported` が返った場合は `DataPortabilityError::VaultLocked.into()` に変換して返す。`repo.exists()` が `false` の場合も vault が空として扱う（空 export を許容）
3. `vault.records()` で全レコードを取得する（`Vec<&Record>`）
4. 各レコードを `ExportRecord::try_from((&record, args.export_secrets))` で変換する。`ExportError::VaultLocked` が返った場合は `DataPortabilityError::VaultLocked.into()` を返す
5. `vault_name = vault_dir.file_name().unwrap_or_else(|| std::ffi::OsStr::new("vault")).to_string_lossy().into_owned()`
6. `ExportPayload::new(export_records, vault_name, now)` でペイロードを構築する
7. `serde_json::to_string_pretty(&payload)` で JSON 文字列化する（`ExportPayload` の型が `Serialize` を保証しているため `unwrap` は許容される。panic を避けるため `expect("ExportPayload is always serializable")` を使う）
8. `let parent = args.output.parent().unwrap_or_else(|| Path::new("."));`
9. Unix 系: `tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o600)).tempfile_in(parent).map_err(DataPortabilityError::IoError)?`。非 Unix 系（Windows）: `tempfile::Builder::new().tempfile_in(parent).map_err(DataPortabilityError::IoError)?`（`cfg(unix)` で分岐）
10. `tmp_file.write_all(json_bytes).map_err(DataPortabilityError::IoError)?`
11. `tmp_file.persist(&args.output).map_err(|e| DataPortabilityError::IoError(e.error))?`（`persist` の `Err` は `PersistError { error: io::Error, file: NamedTempFile }` を持つ）
12. `Ok(ExportSummary { record_count: export_records.len(), output_path: args.output.clone() })`

**設計判断（`repo.exists() == false` の扱い）**: vault が存在しない場合、空の `ExportPayload`（`records: []`）を export する。エラーにしない。理由: 「空の vault を export する」ことは有効な操作であり（`ImportWarning::EmptyImport` に対応）、エラーにすることは YAGNI。

**設計判断（`cfg(unix)` 分岐）**: Windows では `fs::Permissions::from_mode` が使えない（`std::os::unix::fs::PermissionsExt` が Unix 限定）。Windows での `tempfile` のパーミッション設定はファイルシステム側の ACL に委ねる（`feature-spec.md §4` は "Unix 系" と明記）。

---

## `crates/shikomi-cli/src/usecase/portability/import.rs` の設計詳細

### `ImportSummary` 型

| フィールド | 型 | 説明 |
|-----------|----|----|
| `added` | `usize` | 新規追加したレコード件数 |
| `skipped` | `usize` | 衝突により skip したレコード件数（`--on-conflict skip` 時のみ非ゼロ）|
| `overwritten` | `usize` | 既存レコードを上書きしたレコード件数（`--on-conflict overwrite` 時のみ非ゼロ）|

### `import_records` 関数

シグネチャ: `pub fn import_records(repo: &dyn VaultRepository, args: &ImportArgs, now: OffsetDateTime) -> Result<ImportSummary, CliError>`

処理順序:

1. `std::fs::read_to_string(&args.input).map_err(DataPortabilityError::IoError)?` でファイル内容を読み込む
2. `serde_json::from_str::<shikomi_core::portability::ImportPayload>(&contents)` でパース。`Err(e)` → `DataPortabilityError::DeserializationFailed { reason: e.to_string() }.into()`
3. vault を準備する: `repo.load()` が成功すれば既存 vault を使用、`repo.exists() == false` なら空の `Vault::new(VaultHeader::new_plaintext(...))` を作成する
4. `let existing_ids: std::collections::HashSet<String> = vault.records().iter().map(|r| r.id().to_string()).collect()`
5. `ImportValidator::validate(&payload, &existing_ids)` を呼び出す。`Err(err)` → `DataPortabilityError::ValidationFailed(err).into()`
6. `args.on_conflict == OnConflictArg::Error && !report.conflicting_ids.is_empty()` → `DataPortabilityError::ConflictError { ids: report.conflicting_ids.clone() }.into()`
7. `let mut added = 0; let mut skipped = 0; let mut overwritten = 0;`
8. `for import_record in &payload.records` を走査する:
   - `let is_conflicting = report.conflicting_ids.contains(&import_record.id);`
   - `is_conflicting && args.on_conflict == OnConflictArg::Skip` → `skipped += 1; continue`
   - `let record = import_record_to_domain(import_record, now)?` で domain `Record` に変換する
   - `is_conflicting && args.on_conflict == OnConflictArg::Overwrite` → `vault.remove_record(record.id()).map_err(CliError::Domain)?; vault.add_record(record).map_err(CliError::Domain)?; overwritten += 1`
   - それ以外（新規追加）→ `vault.add_record(record).map_err(CliError::Domain)?; added += 1`
9. `repo.save(&vault)?`
10. `Ok(ImportSummary { added, skipped, overwritten })`

### `import_record_to_domain` helper 関数

シグネチャ: `fn import_record_to_domain(r: &shikomi_core::portability::ImportRecord, now: OffsetDateTime) -> Result<Record, CliError>`

処理順序:

1. `uuid::Uuid::parse_str(&r.id)` → parse 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid record id '{}': {}", r.id, e) }`
2. `RecordId::new(uuid)` → `Err(e)` → `CliError::ImportDeserializationFailed { reason: e.to_string() }`
3. `RecordLabel::try_new(r.label.clone())` → `Err(e)` → `CliError::ImportDeserializationFailed { reason: format!("invalid label: {e}") }`
4. payload を変換する:
   - `ExportRecordPayload::Plaintext { value }` → `RecordPayload::Plaintext(shikomi_core::SecretString::from_string(value.clone()))`
   - `ExportRecordPayload::Redacted` → `unreachable!("ImportValidator rejects Redacted payload before import_record_to_domain is called")`
5. `OffsetDateTime::parse(&r.created_at, &time::format_description::well_known::Rfc3339)` → parse 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid created_at '{}': {}", r.created_at, e) }`
6. `OffsetDateTime::parse(&r.updated_at, &time::format_description::well_known::Rfc3339)` → parse 失敗 → 同様
7. hotkey を変換する: `r.hotkey.as_deref().map(Hotkey::parse).transpose()` → parse 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid hotkey '{}': {}", s, e) }`
8. `Record` を `created_at` / `updated_at` を含む形で構築する。`Record::new_with_timestamps(id, r.kind, label, payload, created_at, updated_at)` が存在しない場合は domain に追加コンストラクタを要求する。hotkey は `vault.assign_hotkey(&id, hotkey)` で別途設定する（import.rs 内でループ後にまとめて適用する）
9. `Ok(record)`

**設計判断（`unreachable!` の使用）**: `ImportValidator::validate` が `RedactedPayload` を `Err` として返すため、本関数が呼ばれた時点でリダクトペイロードは除外済みのはず。`unreachable!` は開発時のパニックで実装上の契約違反を検出する（告知的プログラミング）。release ビルドで到達した場合は UB ではなくパニックになる（`unreachable!` は `#[cold]` + panic）。

---

## `crates/shikomi-cli/src/presenter/success.rs` の変更詳細

### 追加: `render_exported`

```
pub fn render_exported(record_count: usize, output_path: &Path, locale: Locale) -> String {
    let path_str = output_path.display();
    let mut out = format!("exported {record_count} record(s) to {path_str}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("{record_count} 件のレコードを {path_str} に export しました\n"));
    }
    out
}
```

### 追加: `render_imported`

```
pub fn render_imported(added: usize, skipped: usize, overwritten: usize, locale: Locale) -> String {
    let mut out = format!("imported {added} record(s) (skipped {skipped}, overwritten {overwritten})\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!(
            "{added} 件を追加しました（スキップ: {skipped} 件、上書き: {overwritten} 件）\n"
        ));
    }
    out
}
```

### 追加: `render_export_secrets_warning`（MSG-CLI-145）

`--quiet` でも抑止不可の stderr 警告。呼び出し元（`lib.rs::run_export`）が `args.export_secrets == true` の場合にのみ呼び出す。

```
pub fn render_export_secrets_warning(locale: Locale) -> String {
    let mut out = String::from(
        "warning: --export-secrets is set; secret values will be written to the export file in plaintext\n",
    );
    out.push_str("warning: store the export file securely and delete it when no longer needed\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("warning: --export-secrets が指定されています。Secret の値が平文でエクスポートファイルに書き込まれます\n");
        out.push_str("warning: エクスポートファイルを安全に保管し、不要になったら削除してください\n");
    }
    out
}
```

---

## `crates/shikomi-cli/src/presenter/error.rs` の変更詳細

### 追加: `render_error` の dispatch（MSG-CLI-144 専用分岐）

`render_error` 関数の `match` に `ImportValidationFailed` の特殊ケースを追加する:

```
CliError::ImportValidationFailed(
    shikomi_core::portability::ImportValidationError::RedactedPayload { id }
) => render_import_validation_redacted(id, locale),
```

`render_import_validation_redacted` は `render_error` 内の private helper として定義する（MSG-CLI-144）。`UnknownFormatVersion` / `DuplicateIdInFile` は `lines_for` の fallback（MSG-CLI-143）で処理する。

### 追加: `lines_for` の match arm（新バリアント 5 種）

`lines_for` 末尾の `unreachable` パターンの直前に以下を追加する（コンパイル時網羅を保証するため wildcard `_` を使わない方針に従う）:

```
// ---- Issue #141: data-portability ----
CliError::ExportImportVaultLocked => lit(
    "vault is locked; unlock the vault before running export or import",
    "vault がロックされています。export / import の前に vault のロックを解除してください",
    "run `shikomi vault unlock` first",
    "先に `shikomi vault unlock` を実行してください",
),
CliError::ExportOutputFileExists { path } => (
    format!("export output file already exists: {}", path.display()),
    format!("export 先ファイルが既に存在します: {}", path.display()),
    "pass --force to overwrite, or choose a different --output path".to_owned(),
    "上書きする場合は --force を指定するか、別の --output パスを指定してください".to_owned(),
),
CliError::ImportConflict { ids } => {
    let display = format_conflict_ids(ids);
    let n = ids.len();
    (
        format!("import conflict: {n} record(s) already exist in vault (ids: {display})"),
        format!("import 衝突: {n} 件のレコードが vault に既に存在します（ID: {display}）"),
        "use --on-conflict skip to skip conflicting records, or --on-conflict overwrite to replace them".to_owned(),
        "--on-conflict skip で衝突レコードをスキップするか、--on-conflict overwrite で上書きしてください".to_owned(),
    )
},
CliError::ImportDeserializationFailed { reason } => (
    format!("failed to parse import file: {reason}"),
    format!("import ファイルの解析に失敗しました: {reason}"),
    "verify the file is a valid shikomi export (format_version must be 1)".to_owned(),
    "ファイルが有効な shikomi export ファイルであることを確認してください（format_version は 1 である必要があります）".to_owned(),
),
CliError::ImportValidationFailed(err) => (
    format!("failed to parse import file: {err}"),
    format!("import ファイルの解析に失敗しました: {err}"),
    "verify the file is a valid shikomi export (format_version must be 1)".to_owned(),
    "ファイルが有効な shikomi export ファイルであることを確認してください（format_version は 1 である必要があります）".to_owned(),
),
```

### 追加: `format_conflict_ids` helper 関数

4 件を超える `ids` は先頭 4 件 + `... (N more)` と省略する:

```
fn format_conflict_ids(ids: &[String]) -> String {
    const MAX_DISPLAY: usize = 4;
    if ids.len() <= MAX_DISPLAY {
        ids.join(", ")
    } else {
        let head = ids[..MAX_DISPLAY].join(", ");
        format!("{head}, ... ({} more)", ids.len() - MAX_DISPLAY)
    }
}
```

### 追加: `render_import_validation_redacted` private helper（MSG-CLI-144）

```
fn render_import_validation_redacted(id: &str, locale: Locale) -> String {
    let mut out = format!("error: cannot import record {id}: payload is redacted\n");
    if matches!(locale, Locale::JapaneseEn) {
        let _ = writeln!(out, "error: レコード {id} を import できません: ペイロードがリダクトされています");
    }
    out.push_str("hint: re-export the source vault with --export-secrets, then import the new file\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("hint: ソース vault を --export-secrets 付きで再 export し、新しいファイルを import してください\n");
    }
    out
}
```

---

## `crates/shikomi-cli/src/lib.rs` の変更詳細

### 追加: `Subcommand::Export` / `Subcommand::Import` の match arm

`lib.rs::run()` の `match cli.subcommand` に以下を追加する:

```
Subcommand::Export(args) => run_export(&args, repo, cli.quiet, vault_dir, locale),
Subcommand::Import(args) => run_import(&args, repo, cli.quiet, vault_dir, locale),
```

### 追加: `run_export` 関数

```
fn run_export(
    args: &ExportArgs,
    repo: &dyn VaultRepository,
    quiet: bool,
    vault_dir: &Path,
    locale: Locale,
) -> Result<(), CliError> {
    // MSG-CLI-145: --export-secrets 警告は --quiet でも抑止不可（feature-spec.md R1-DP-02）。
    if args.export_secrets {
        eprintln!("{}", presenter::success::render_export_secrets_warning(locale));
    }
    let now = OffsetDateTime::now_utc();
    let summary = usecase::portability::export::export_records(repo, args, vault_dir, now)?;
    if !quiet {
        print!("{}", presenter::success::render_exported(summary.record_count, &summary.output_path, locale));
    }
    Ok(())
}
```

**設計判断（`eprintln!` で直接出力）**: MSG-CLI-145 は `eprintln!` で stderr に直接出力する。`print!` / `println!` は stdout に書くため誤用を防ぐためにも `eprintln!` を明示する。`--quiet` フラグの確認を `run_export` で行い、UseCase はその判定を持たない（pure）。

### 追加: `run_import` 関数

```
fn run_import(
    args: &ImportArgs,
    repo: &dyn VaultRepository,
    quiet: bool,
    vault_dir: &Path,
    locale: Locale,
) -> Result<(), CliError> {
    let now = OffsetDateTime::now_utc();
    let summary = usecase::portability::import::import_records(repo, args, now)?;
    if !quiet {
        print!("{}", presenter::success::render_imported(
            summary.added, summary.skipped, summary.overwritten, locale,
        ));
    }
    Ok(())
}
```

---

## `crates/shikomi-cli/src/usecase/mod.rs` の変更詳細

既存の `pub mod` 宣言群の末尾に 1 行追加する:

```
pub mod portability;
```

変更は 1 行のみ。

---

## セキュリティ考慮（cli スコープ）

| 脅威 | 対策 |
|------|------|
| `--export-secrets` による誤操作全漏洩 | `run_export` が `args.export_secrets == true` の場合に UseCase 呼び出し前に `eprintln!` で MSG-CLI-145 を stderr に出力。`quiet` フラグを無視する経路が `lib.rs` 側に明示される。テストで `--quiet --export-secrets` 組み合わせ時も警告が出ることを確認する |
| export ファイルの不正読取（0600 設定漏れ）| `export.rs` の `tempfile::Builder` に `permissions(Permissions::from_mode(0o600))` を必ず設定する（`cfg(unix)` 条件付き）。非 Unix 環境でも `tempfile + persist` の atomic rename は有効 |
| vault ロック済みでの export（`Encrypted` ペイロード変換失敗の伝播）| `ExportRecord::try_from` が `Err(ExportError::VaultLocked)` を返す経路が `export_records` でキャッチされ、`ExportImportVaultLocked` として早期失敗する。`VaultLocked` 状態のレコードが JSON ファイルに部分的に書き込まれることはない（tempfile + persist が atomic であるため）|
| Redacted ペイロードの import によるデータ不整合 | `ImportValidator::validate` が `RedactedPayload` を検出して `Err(ValidationFailed)` を返す。`import_record_to_domain` の `unreachable!` が二重安全網となる |
| 過大な import ファイルによる DoS | `std::fs::read_to_string` はファイル全体をメモリに読み込む。MVP スコープではサイズ制限は設けない。`threat-model.md §7.5` に将来のストリーミング読み取り推奨を記載済み |
