# 詳細設計書 — data-portability / cli / Presenter・lib.rs 変更

<!-- feature: data-portability / sub-feature: cli / Issue #141 -->
<!-- 配置先: docs/features/data-portability/cli/detailed-design/presenter.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親: ../basic-design.md -->
<!-- 兄弟: cli.md / usecase.md -->

## 記述ルール

疑似コード禁止。処理順序は番号付き箇条書きで表現する。型・フィールド・モジュールパスは `code` 表記で明示する。

## 変更対象ファイル

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/presenter/success.rs` | 編集 | `render_exported` / `render_imported` / `render_export_secrets_warning` を追加 |
| `crates/shikomi-cli/src/presenter/error.rs` | 編集 | `render_error` に MSG-CLI-144 dispatch 追加 + `lines_for` に新 6 バリアントの arm 追加 + `format_conflict_ids` helper 追加 |
| `crates/shikomi-cli/src/lib.rs` | 編集 | `Subcommand::Export` / `Subcommand::Import` の match arm + `run_export` / `run_import` dispatcher 追加 |

---

## `crates/shikomi-cli/src/presenter/success.rs` の変更詳細

既存の Sub-B (#127) autostart メッセージ群の後に以下を追加する。

### 追加: `render_exported`

シグネチャ: `pub fn render_exported(record_count: usize, output_path: &Path, locale: Locale) -> String`

- English 行: `"exported {record_count} record(s) to {path_str}\n"`（`path_str = output_path.display()`）
- JapaneseEn 追加行: `"{record_count} 件のレコードを {path_str} に export しました\n"`

### 追加: `render_imported`

シグネチャ: `pub fn render_imported(added: usize, skipped: usize, overwritten: usize, locale: Locale) -> String`

- English 行: `"imported {added} record(s) (skipped {skipped}, overwritten {overwritten})\n"`
- JapaneseEn 追加行: `"{added} 件を追加しました（スキップ: {skipped} 件、上書き: {overwritten} 件）\n"`

### 追加: `render_export_secrets_warning`（MSG-CLI-145）

シグネチャ: `pub fn render_export_secrets_warning(locale: Locale) -> String`

`--quiet` でも抑止不可の stderr 警告。`lib.rs::run_export` が `args.export_secrets == true` の場合にのみ呼び出す。

出力行（`basic-design.md §MSG-CLI-145` の確定文面を厳密に使用する）:

- English 行 1: `"warning: --export-secrets is set; secret values will be written to the export file in plaintext\n"`
- English 行 2: `"warning: store the export file securely and delete it when no longer needed\n"`
- JapaneseEn 追加行 1: `"warning: --export-secrets が指定されています。Secret の値が平文でエクスポートファイルに書き込まれます\n"`
- JapaneseEn 追加行 2: `"warning: エクスポートファイルを安全に保管し、不要になったら削除してください\n"`

---

## `crates/shikomi-cli/src/presenter/error.rs` の変更詳細

### 変更: `render_error` の dispatch（MSG-CLI-144 専用分岐）

`render_error` 関数の `match err` に以下の arm を追加する（`DaemonNotRunning` / `ProtocolVersionMismatch` の専用 arm の後）:

- `CliError::ImportValidationFailed(ImportValidationError::RedactedPayload { id })` → `render_import_validation_redacted(id, locale)` を呼び出す

`UnknownFormatVersion` / `DuplicateIdInFile` は `lines_for` の fallback（MSG-CLI-143）で処理するため、この arm では `RedactedPayload` のみを dispatch する。

### 追加: `lines_for` の match arm（新バリアント 6 種）

`lines_for` 末尾の `unreachable` sentinel の直前に以下を追加する（wildcard `_` は使わない、コンパイル時網羅性を維持）。各 arm は `(error_en, error_ja, hint_en, hint_ja)` のタプルを返す:

**`ExportImportVaultLocked`**（MSG-CLI-140）:
- error EN: `"vault is locked; unlock the vault before running export or import"`
- error JA: `"vault がロックされています。export / import の前に vault のロックを解除してください"`
- hint EN: `"run \`shikomi vault unlock\` first"`
- hint JA: `"先に \`shikomi vault unlock\` を実行してください"`

**`ExportOutputFileExists { path }`**（MSG-CLI-141）:
- error EN: `format!("export output file already exists: {}", path.display())`
- error JA: `format!("export 先ファイルが既に存在します: {}", path.display())`
- hint EN: `"pass --force to overwrite, or choose a different --output path"`
- hint JA: `"上書きする場合は --force を指定するか、別の --output パスを指定してください"`

**`ImportConflict { ids }`**（MSG-CLI-142）—  `format_conflict_ids(ids)` ヘルパを使用:
- error EN: `format!("import conflict: {} record(s) already exist in vault (ids: {display})", ids.len())`
- error JA: `format!("import 衝突: {} 件のレコードが vault に既に存在します（ID: {display}）", ids.len())`
- hint EN: `"use --on-conflict skip to skip conflicting records, or --on-conflict overwrite to replace them"`
- hint JA: `"--on-conflict skip で衝突レコードをスキップするか、--on-conflict overwrite で上書きしてください"`

**`ImportDeserializationFailed { reason }`**（MSG-CLI-143）:
- error EN: `format!("failed to parse import file: {reason}")`
- error JA: `format!("import ファイルの解析に失敗しました: {reason}")`
- hint EN: `"verify the file is a valid shikomi export (format_version must be 1)"`
- hint JA: `"ファイルが有効な shikomi export ファイルであることを確認してください（format_version は 1 である必要があります）"`

**`ImportValidationFailed(err)`**（MSG-CLI-143、`RedactedPayload` は `render_error` で事前 dispatch 済みのため非到達だが網羅性のため記述）:
- error EN: `format!("failed to parse import file: {err}")`
- error JA: `format!("import ファイルの解析に失敗しました: {err}")`
- hint EN/JA: `ImportDeserializationFailed` と同文言

**`ImportVaultBusy`**（MSG-CLI-146）:
- error EN: `"vault is in use by shikomi-daemon; import aborted after 2 seconds"`
- error JA: `"vault が shikomi-daemon に使用されています。2 秒待機後に import を中断しました"`
- hint EN: `"stop shikomi-daemon, then retry (to disable autostart: shikomi daemon uninstall)"`
- hint JA: `"shikomi-daemon を停止してから再実行してください（自動起動の無効化: shikomi daemon uninstall）"`

**`DaemonNotRunning` / `ProtocolVersionMismatch`** の sentinel arm: `lines_for` の契約上、これらは `render_error` の専用 helper で処理されるため `lines_for` には到達しない。`debug_assert!(false, "...")` を保持する（既存パターン）。

### 追加: `format_conflict_ids` private helper

4 件を超える `ids` は先頭 4 件 + `... (N more)` と省略する:

- `ids.len() <= 4` → `ids.join(", ")`
- それ以外 → `format!("{head}, ... ({} more)", ids.len() - 4)` ただし `head = ids[..4].join(", ")`

### 追加: `render_import_validation_redacted` private helper（MSG-CLI-144）

`render_error` から dispatch される専用 helper。シグネチャ: `fn render_import_validation_redacted(id: &str, locale: Locale) -> String`

出力行（`basic-design.md §MSG-CLI-144` の確定文面を厳密に使用する）:

- `"error: cannot import record {id}: payload is redacted\n"`
- JapaneseEn: `"error: レコード {id} を import できません: ペイロードがリダクトされています\n"`
- `"hint: re-export the source vault with --export-secrets, then import the new file\n"`
- JapaneseEn: `"hint: ソース vault を --export-secrets 付きで再 export し、新しいファイルを import してください\n"`

---

## `crates/shikomi-cli/src/lib.rs` の変更詳細

### 変更: `Subcommand` match arm 追加

`run()` 内の `match &args.subcommand` に以下を追加する（`Subcommand::Daemon` の unreachable arm の前）:

- `Subcommand::Export(a) => run_export(a, &handle, args.vault_dir.as_deref(), quiet, locale)`
- `Subcommand::Import(a) => run_import(a, &handle, quiet, locale)`

### 追加: `run_export` 関数

処理順序:

1. `args.export_secrets == true` → `eprintln!("{}", presenter::success::render_export_secrets_warning(locale))` （`--quiet` 無視。`eprint_stderr` ではなく直接 `eprintln!` を使い stderr への強制出力を明示する）
2. vault_dir の解決: `args.vault_dir.as_deref()` が `Some(p)` なら `p` を使用、`None` なら `io::paths::resolve_os_default_vault_dir()?`
3. `RepositoryHandle` のバリアントに**関わらず**、解決した vault_dir から `SqliteVaultRepository::new(&vault_path)` を構築する（IPC 経路でも SQLite 直接アクセスを使用。理由: `IpcVaultRepository` はペイロードを返さないため。詳細: `basic-design.md §REQ-DP-008 --no-ipc 経路の設計判断`）
4. `handle` が `RepositoryHandle::Ipc(_)` かつ `args.no_ipc == false` の場合: `tracing::warn!(target: "shikomi_cli::export", "export uses direct SQLite access regardless of IPC mode")` を記録する
5. `let now = OffsetDateTime::now_utc()`
6. `usecase::portability::export::export_records(&sqlite_repo, args, &vault_dir, now)?`
7. `!quiet` の場合: `print_stdout(&presenter::success::render_exported(summary.record_count, &summary.output_path, locale))`

**注意**: `export_secrets` の警告（手順 1）は UseCase 呼び出し（手順 6）の**前**に出力する。これにより、UseCase がエラーを返した場合でも警告が表示され、ユーザーが試みた操作の記録が残る。

### 追加: `run_import` 関数

処理順序:

1. vault_dir の解決: `args.vault_dir.as_deref()` が `Some(p)` なら `p` を使用、`None` なら `io::paths::resolve_os_default_vault_dir()?`
2. `handle` が `RepositoryHandle::Ipc(_)` かつ `args.no_ipc == false` の場合: `tracing::warn!(target: "shikomi_cli::import", "import uses direct SQLite access regardless of IPC mode")` を記録する（`run_export` と同じ pattern。import も常に SQLite 直接アクセス、R1-DP-08）
3. `RepositoryHandle` のバリアントに**関わらず**、解決した vault_dir から `SqliteVaultRepository::new(&vault_path)` を構築する（理由: IPC per-record 書き込みは R1-DP-09 の atomicity 要件に非適合。`basic-design.md §REQ-DP-009` 参照）
4. `let now = OffsetDateTime::now_utc()`
5. `usecase::portability::import::import_records(&sqlite_repo, args, now)?`
6. `!quiet` の場合: `print_stdout(&presenter::success::render_imported(summary.added, summary.skipped, summary.overwritten, locale))`

---

## セキュリティ考慮（Presenter / lib.rs スコープ）

| 脅威 | 対策 |
|------|------|
| `--export-secrets` による誤操作全漏洩 | `run_export` が UseCase 呼び出し前に `eprintln!` で MSG-CLI-145 を stderr に強制出力。`quiet` フラグを確認しない経路として設計し、コードレビューで確認可能な形にする |
| MSG-CLI-145 の `--quiet` 抑止 | `run_export` で `quiet` フラグを参照せず直接 `eprintln!` を使用。将来 `quiet` フラグの分岐が追加されても MSG-CLI-145 が影響を受けないよう、コメントで `--quiet 抑止不可` を明記する |
| `format_conflict_ids` での情報過多 | 最大 4 件 + 省略表示。大量の ID を表示して端末を溢れさせない |
| import の部分書き込み（クラッシュ）| IPC per-record 書き込みを廃止し `SqliteVaultRepository::save()` による atomic write に一本化（R1-DP-09）。`run_import` も `run_export` と同様に常に SQLite 直接アクセスを使用する |
| MSG-CLI-146 hint の過剰情報漏洩 | hint に OS 固有のプロセス停止コマンドは含めない（パス情報漏洩・環境依存のリスクを排除）。`shikomi daemon uninstall` コマンドのみを案内する（KISS）|
