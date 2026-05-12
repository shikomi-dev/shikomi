# 基本設計書 — data-portability / cli（モジュール契約）

<!-- feature: data-portability / sub-feature: cli / Issue #141 -->
<!-- 配置先: docs/features/data-portability/cli/basic-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature モジュール契約）-->
<!-- 親: ../feature-spec.md -->

## §モジュール契約（機能要件）

### REQ-DP-007: `ExportArgs` / `ImportArgs` / `OnConflictArg` — CLI 引数定義

| 項目 | 内容 |
|------|------|
| 入力 | ユーザーが入力した CLI フラグ・オプション（clap `#[derive(Args)]`）|
| 処理 | `Subcommand::Export(ExportArgs)` / `Subcommand::Import(ImportArgs)` を `cli.rs` の `Subcommand` enum に追加。`clap::Parser` 派生で自動検証する。`--on-conflict` は `OnConflictArg` を `clap::ValueEnum` で定義し、`skip` / `overwrite` / `error`（既定）の 3 値を受け付ける |
| 出力 | `ExportArgs` / `ImportArgs` / `OnConflictArg` の各型（`Debug` 実装）|
| エラー時 | clap が自動的に usage エラーを生成して exit 2 する（clap 標準動作）|
| 設計原則 | KISS（clap の標準機能のみ使用）/ Fail Fast（clap が引数バリデーションを早期実行）|

**`ExportArgs` フィールド**:

| フィールド | clap 型 | 説明 |
|-----------|---------|------|
| `output` | `PathBuf`（`--output <FILE>`、必須）| export 先ファイルパス |
| `export_secrets` | `bool`（`--export-secrets`、フラグ）| 指定時に Secret kind のペイロードを平文で export する（既定: リダクト）|
| `force` | `bool`（`--force`、フラグ）| 出力先ファイルが既に存在する場合に上書きする |

**`ImportArgs` フィールド**:

| フィールド | clap 型 | 説明 |
|-----------|---------|------|
| `input` | `PathBuf`（`--input <FILE>`、必須）| import 元ファイルパス |
| `on_conflict` | `OnConflictArg`（`--on-conflict <STRATEGY>`、既定: `error`）| 衝突戦略 |

**`OnConflictArg` バリアント**:

| バリアント | clap 値 | 動作 |
|-----------|---------|------|
| `Error` | `error`（既定）| 衝突 ID が存在した場合即座に `MSG-CLI-142` で exit 1 |
| `Skip` | `skip` | 衝突 ID のレコードをスキップして残りを追加 |
| `Overwrite` | `overwrite` | 衝突 ID の既存レコードを import ファイルの値で置換 |

**設計判断**: `--on-conflict` のデフォルト `error` を採用する。上書き・スキップは**明示的な意図表明**を要求すべきであり、誤操作による既存データの消失・混入を防ぐ（Fail Fast）。`feature-spec.md R1-DP-06` 準拠。

---

### REQ-DP-008: `export_records` — エクスポート UseCase

| 項目 | 内容 |
|------|------|
| 入力 | `repo: &SqliteVaultRepository`（SQLite 直接アクセス、後述）・`args: &ExportArgs`・`vault_dir: &Path`・`now: OffsetDateTime` |
| 処理 | (1) `args.output` が既存ファイルかつ `args.force == false` → `Err(ExportOutputFileExists)`。(2) `repo.exists() == false` → 0 件 export（エラーではない。空 vault の export は正常操作）。(3) `repo.load()` でレコード取得——`ProtectionMode::Encrypted` かつ `VEK` 不在（vault ロック済み）の場合は `ExportImportVaultLocked`。(4) 全レコードを `ExportRecord::try_from((&record, args.export_secrets))?` でシリアライズ可能型に変換（`ExportError::VaultLocked` → `ExportImportVaultLocked`）。(5) `ExportPayload::new(records, vault_name, now)` でペイロード構築。(6) `serde_json::to_string_pretty` で JSON 文字列化。(7) `tempfile::Builder` で出力先と同一ディレクトリに一時ファイルを作成し、Unix 系では `0600` パーミッションを設定。(8) JSON 書き込み後、`NamedTempFile::persist(output_path)` で atomic rename。|
| 出力 | `Ok(ExportSummary { record_count: usize, output_path: PathBuf })` |
| エラー時 | vault ロック済み → `Err(ExportImportVaultLocked)` / ファイル既存（`--force` なし）→ `Err(ExportOutputFileExists)` / vault 不存在 → `Ok(ExportSummary { record_count: 0, ... })`（エラーなし）/ I/O 失敗 → `Err(Persistence(...))` |
| 設計原則 | Fail Fast（ファイル存在チェックを先行）/ アトミック性（`tempfile` + `persist` による rename）/ Tell, Don't Ask（`ExportPayload` が自身を JSON 化できる）|

**`--no-ipc` 経路の設計判断（ペテルギウス指摘 2 対応）**:

Export は **常に SQLite 直接アクセス（`SqliteVaultRepository`）を使用する**。IPC 経路（`IpcVaultRepository`）は `list_summaries()` が返すレコードのペイロードを空文字列（`RecordPayload::Plaintext("")`）で代替しており（daemon 設計 §`VaultRepository` trait 非実装の理由）、実際の平文値を含まない。そのため、`lib.rs::run_export` は `RepositoryHandle` のバリアントに関わらず `vault_dir` から `SqliteVaultRepository` を構築して UseCase に渡す。`--no-ipc` を明示しない場合も動作上の違いはなく、`tracing::warn` でログに記録する。`SHIKOMI_VAULT_DIR` env または `--vault-dir` で vault の場所を解決する（既存 `io::paths::resolve_os_default_vault_dir()` に委譲）。

**`vault.db` 不存在時の設計判断（ペテルギウス指摘 3 対応）**:

`repo.exists() == false` の場合は 0 件の `ExportPayload` を出力する。エラーにしない理由: 「vault が空の状態で export する」ことは有効な操作であり（import 側で `ImportWarning::EmptyImport` が対応）、エラーにすることは YAGNI。`feature-spec.md` はこの場合の exit code を指定していない——0 件 export の成功扱いを採用する。

**`vault_name` の決定**: vault_dir の basename を `vault_name` として `ExportPayload` に含める。`repo.load()` が `CliError::EncryptionUnsupported` を返す場合（Phase 1 平文 vault 非対応経路）も `ExportImportVaultLocked` に変換する（暗号化 vault がロック済みで読めないという意味で同値、`feature-spec.md R1-DP-03`）。

---

### REQ-DP-009: `import_records` — インポート UseCase

Import は IPC 経路（`IpcVaultRepository`）と SQLite 直接経路（`SqliteVaultRepository`）の**両方をサポートする**（`feature-spec.md R1-DP-08`）。両経路で入力バリデーション・衝突検出ロジックは共通。書き込みの実装のみ経路で異なる。

**SQLite 経路（`--no-ipc`）**:

| 項目 | 内容 |
|------|------|
| 入力 | `repo: &dyn VaultRepository`・`args: &ImportArgs`・`now: OffsetDateTime` |
| 処理 | (1) `args.input` ファイル読み込み。(2) `serde_json::from_str::<ImportPayload>` でパース。(3) `repo.load()` または未作成なら空 Vault を準備。(4) vault の全レコード ID を `HashSet<String>` に収集。(5) `ImportValidator::validate(&payload, &existing_ids)` — 失敗時は `ImportValidationFailed`。(6) `on_conflict == Error && !conflicting_ids.is_empty()` → `ImportConflict`。(7) 各 `ImportRecord` を `Record::rehydrate(...)` で domain 型に変換し、衝突戦略を適用して vault に追加・更新。(8) `repo.save(&vault)` で永続化。|
| 出力 | `Ok(ImportSummary { added, skipped, overwritten })` |
| エラー時 | JSON パース失敗 → `ImportDeserializationFailed` / バリデーション失敗 → `ImportValidationFailed` / 衝突（error 戦略）→ `ImportConflict` / I/O 失敗 → `Persistence(...)` |

**IPC 経路（デフォルト、`--no-ipc` 未指定）**:

| 項目 | 内容 |
|------|------|
| 入力 | `ipc: &IpcVaultRepository`・`args: &ImportArgs`・`now: OffsetDateTime` |
| 処理 | (1)(2) は SQLite 経路と同じ（ファイル読み込み・パース）。(3) `ipc.list_summaries()` で既存レコード ID を収集（daemon が vault の読み取り権限を保持）。(4)(5)(6) バリデーション・衝突チェックは SQLite 経路と同じ。(7) 衝突戦略適用: overwrite は `ipc.remove_record(id)` → `ipc.add_record(...)`、skip は `continue`、新規は `ipc.add_record(...)`。(8) `ipc.add_record` が `IpcErrorCode::VaultLocked` を返した場合 → `ExportImportVaultLocked`。|
| 出力 | `Ok(ImportSummary { added, skipped, overwritten })` |
| エラー時 | vault ロック済み → `ExportImportVaultLocked` / JSON パース失敗 → `ImportDeserializationFailed` / 衝突（error 戦略）→ `ImportConflict` |

**設計原則**: Fail Fast（JSON パース → バリデーション → 衝突チェックの順で早期検出）/ 単一責務（衝突戦略の適用は UseCase 責務、`ImportValidator` は衝突 ID の検出のみ）

**`ImportRecord` → `Record` 変換（ペテルギウス指摘 1 対応）**:

`Record::rehydrate` を使用する。**Sub-A 実装確認済み**（`shikomi-core/src/vault/record/aggregate.rs` L78）: `Record::rehydrate(id, kind, label, payload, created_at: OffsetDateTime, updated_at: OffsetDateTime, hotkey: Option<Hotkey>) -> Result<Self, DomainError>` が既存のため **domain 追加 API は不要**。`updated_at < created_at` の場合は `DomainError::VaultConsistencyError(InvalidUpdatedAt)` を返す。import ファイルが不正なタイムスタンプを持つ場合は `ImportDeserializationFailed { reason }` に変換する。UseCase 内 private な `fn import_record_to_domain(r: &ImportRecord) -> Result<Record, CliError>` で変換を実施する。変換失敗（UUID 解析失敗 / RFC 3339 パース失敗 / ラベル不正 / タイムスタンプ順序違反）は全て `ImportDeserializationFailed { reason }` として返す。

---

### REQ-DP-010: `DataPortabilityError` — UseCase 内部エラー型 + `CliError` 追加バリアント

| 項目 | 内容 |
|------|------|
| 入力 | UseCase 内で発生したエラー条件 |
| 処理 | `DataPortabilityError` を `shikomi-cli/src/usecase/portability/error.rs` に定義する。`From<DataPortabilityError> for CliError` を実装し、UseCase が返す `Result<_, CliError>` に統合する |
| 出力 | `CliError` の新バリアント（下表参照）|
| エラー時 | 該当なし |
| 設計原則 | 依存関係の方向（`DataPortabilityError` → `CliError` へ変換。`shikomi-core` は I/O エラーを持ち込まない）/ Tell, Don't Ask（各エラーが自分のメッセージ ID を知る）|

**`CliError` への追加バリアント**（全て `ExitCode::UserError` = exit 1）:

| バリアント | MSG ID | 発生条件 |
|-----------|--------|---------|
| `ExportImportVaultLocked` | MSG-CLI-140 | export / import 実行時に vault がロック済み（`EncryptionUnsupported` 含む）|
| `ExportOutputFileExists { path: PathBuf }` | MSG-CLI-141 | export 先ファイルが既に存在し `--force` 未指定 |
| `ImportConflict { ids: Vec<String> }` | MSG-CLI-142 | `--on-conflict error` で 1 件以上の衝突が発生 |
| `ImportDeserializationFailed { reason: String }` | MSG-CLI-143 | JSON パース失敗（`format_version` 不一致は `ImportValidationFailed` 経由）|
| `ImportValidationFailed(ImportValidationError)` | MSG-CLI-143 または MSG-CLI-144 | `ImportValidator::validate` 失敗（バリアントによって MSG を切替）|

**設計判断**: `ExportImportVaultLocked` を既存 `CliError::VaultLocked`（exit 3、Sub-F SSoT）と**別バリアント**にする。理由: `feature-spec.md` の UC-DP-001/002 は vault ロックを exit 1（ユーザー入力エラー）と定義しており、Sub-F の vault encryption 操作中のロック（exit 3）とはコンテキストが異なる。ユーザーの操作意図（「export する前にロック解除が必要」= 手順エラー）に対応するため exit 1 を採用する。

**`ImportValidationFailed` の MSG 切替ルール**（`render_error` で dispatch）:

| `ImportValidationError` バリアント | 対応 MSG |
|----------------------------------|---------|
| `UnknownFormatVersion { found }` | MSG-CLI-143 |
| `DuplicateIdInFile { id }` | MSG-CLI-143 |
| `RedactedPayload { id }` | MSG-CLI-144 |

---

### REQ-DP-011: Presenter — MSG-CLI-140〜145 成功・警告・エラー文面

| 項目 | 内容 |
|------|------|
| 入力 | `ExportSummary` / `ImportSummary` / `CliError` + `Locale` |
| 処理 | pure function として `String` を返す。stderr / stdout への書き出しは `lib.rs::run()` の責務 |
| 出力 | English 単行または JapaneseEn 2 行形式 |
| エラー時 | 該当なし（presenter は純粋関数）|
| 設計原則 | 既存 `success.rs` / `error.rs` のパターンを踏襲（DRY）|

**成功メッセージ**（`presenter/success.rs` に追加）:

`render_exported(summary: &ExportSummary, locale: Locale) -> String`:
```
exported {N} record(s) to {path}
（JapaneseEn: {N} 件のレコードを {path} に export しました）
```

`render_imported(summary: &ImportSummary, locale: Locale) -> String`:
```
imported {added} record(s) (skipped {skipped}, overwritten {overwritten})
（JapaneseEn: {added} 件を追加しました（スキップ: {skipped} 件、上書き: {overwritten} 件））
```

**警告メッセージ**（`presenter/success.rs` に追加、`lib.rs::run()` が stderr に出力）:

`render_export_secrets_warning(locale: Locale) -> String`（MSG-CLI-145、`--quiet` でも抑止不可）:
```
warning: --export-secrets is set; secret values will be written to the export file in plaintext
warning: store the export file securely and delete it when no longer needed
（JapaneseEn: 2 行追加）
```

---

## §MSG-CLI-140〜145 確定文面

文面の SSoT は本セクション。`presenter/error.rs` / `presenter/success.rs` の実装はここを参照して厳密に従うこと。

### MSG-CLI-140（ExportImportVaultLocked）— exit 1

```
error: vault is locked; unlock the vault before running export or import
error: vault がロックされています。export / import の前に vault のロックを解除してください
hint: run `shikomi vault unlock` first
hint: 先に `shikomi vault unlock` を実行してください
```

### MSG-CLI-141（ExportOutputFileExists）— exit 1

```
error: export output file already exists: {path}
error: export 先ファイルが既に存在します: {path}
hint: pass --force to overwrite, or choose a different --output path
hint: 上書きする場合は --force を指定するか、別の --output パスを指定してください
```

### MSG-CLI-142（ImportConflict）— exit 1

`ids` が 4 件を超える場合は先頭 4 件 + `... (N more)` と省略する（端末の視認性）。

```
error: import conflict: {N} record(s) already exist in vault (ids: {id1}, {id2}, ...)
error: import 衝突: {N} 件のレコードが vault に既に存在します（ID: {id1}, {id2}, ...）
hint: use --on-conflict skip to skip conflicting records, or --on-conflict overwrite to replace them
hint: --on-conflict skip で衝突レコードをスキップするか、--on-conflict overwrite で上書きしてください
```

### MSG-CLI-143（ImportDeserializationFailed / ImportValidationFailed 一部）— exit 1

`{reason}` には `ImportDeserializationFailed.reason`、または `ImportValidationError` の `Display` 出力を使用する。

```
error: failed to parse import file: {reason}
error: import ファイルの解析に失敗しました: {reason}
hint: verify the file is a valid shikomi export (format_version must be 1)
hint: ファイルが有効な shikomi export ファイルであることを確認してください（format_version は 1 である必要があります）
```

### MSG-CLI-144（ImportValidationFailed / RedactedPayload）— exit 1

`{id}` には `ImportValidationError::RedactedPayload { id }` の ID を使用する。

```
error: cannot import record {id}: payload is redacted
error: レコード {id} を import できません: ペイロードがリダクトされています
hint: re-export the source vault with --export-secrets, then import the new file
hint: ソース vault を --export-secrets 付きで再 export し、新しいファイルを import してください
```

### MSG-CLI-145（--export-secrets 警告）— stderr 出力・exit 0・`--quiet` 抑止不可

```
warning: --export-secrets is set; secret values will be written to the export file in plaintext
warning: store the export file securely and delete it when no longer needed
（JapaneseEn 追加行）
warning: --export-secrets が指定されています。Secret の値が平文でエクスポートファイルに書き込まれます
warning: エクスポートファイルを安全に保管し、不要になったら削除してください
```

**設計判断**: `--quiet` でも MSG-CLI-145 を抑止しない。`feature-spec.md R1-DP-02` および `§4 非機能要件` の「`--export-secrets` 実行時は `MSG-CLI-145` を stderr に必ず出力する（`--quiet` でも抑止不可）」に準拠。`lib.rs::run()` で `--export-secrets == true` を検知した場合、UseCase 呼び出しの**前に**必ず stderr に出力する。UseCase は警告の出力責務を持たない（純粋性を保つ）。

---

## §モジュール配置

| クレート | パス | 変更種別 | 内容 |
|---------|------|---------|------|
| `shikomi-cli` | `src/cli.rs` | 編集 | `Subcommand::Export(ExportArgs)` / `Subcommand::Import(ImportArgs)` / `ExportArgs` / `ImportArgs` / `OnConflictArg` を追加 |
| `shikomi-cli` | `src/error.rs` | 編集 | `CliError` 新バリアント 5 種追加 / `ExitCode::from(&CliError)` の match arm 追加 / `From<DataPortabilityError> for CliError` 実装 |
| `shikomi-cli` | `src/usecase/portability/mod.rs` | 新規 | `export` / `import` / `error` モジュールの re-export |
| `shikomi-cli` | `src/usecase/portability/error.rs` | 新規 | `DataPortabilityError` 型定義（UseCase 内部中間エラー）|
| `shikomi-cli` | `src/usecase/portability/export.rs` | 新規 | `export_records` 関数 + `ExportSummary` 型 |
| `shikomi-cli` | `src/usecase/portability/import.rs` | 新規 | `import_records_sqlite` / `import_records_ipc` 関数 + `ImportSummary` 型 + `import_record_to_domain` helper |
| `shikomi-cli` | `src/usecase/mod.rs` | 編集 | `pub mod portability;` を追加 |
| `shikomi-cli` | `src/presenter/success.rs` | 編集 | `render_exported` / `render_imported` / `render_export_secrets_warning` を追加 |
| `shikomi-cli` | `src/presenter/error.rs` | 編集 | `lines_for` に 5 種の新 `CliError` バリアントの match arm を追加。`render_error` の dispatch 追加（`ImportValidationFailed(RedactedPayload)` → MSG-CLI-144 専用 helper）|
| `shikomi-cli` | `src/lib.rs` | 編集 | `Subcommand::Export` / `Subcommand::Import` の match arm 追加。MSG-CLI-145 の stderr 出力ロジック追加 |

**変更不要ファイル**:

| ファイル | 理由 |
|---------|------|
| `crates/shikomi-cli/Cargo.toml` | `tempfile = { workspace = true }` は既存依存（line 71）。追加依存なし |
| `crates/shikomi-core/` 以下全ファイル | CLI UseCase / Presenter / CLI 引数は `shikomi-cli` のみに閉じる。`shikomi-core` は Sub-A で完成済み |

---

## §ユーザー向けメッセージ一覧（Sub-B スコープ）

| ID | 種別 | 出力先 | 発生条件 | 終了コード |
|----|------|------|---------|----------|
| MSG-CLI-140 | error | stderr | vault ロック済みで export / import 不可 | 1 |
| MSG-CLI-141 | error | stderr | export 先ファイルが既に存在（`--force` 未指定）| 1 |
| MSG-CLI-142 | error | stderr | `--on-conflict error` で衝突発生 | 1 |
| MSG-CLI-143 | error | stderr | JSON パース失敗 / フォーマットバージョン不一致 / ファイル内重複 ID | 1 |
| MSG-CLI-144 | error | stderr | `{"kind":"redacted"}` payload レコードの import 試行 | 1 |
| MSG-CLI-145 | warning | stderr | `--export-secrets` 実行時の平文 export 警告（`--quiet` でも抑止不可）| 0（処理継続）|

文面の確定は本設計書 §MSG-CLI-140〜145 確定文面。

---

## §テスト戦略（テスト設計 Issue で詳細化）

| テストレベル | 観点 |
|-------------|------|
| UT | `export_records` — vault ロック済み検出 / ファイル既存 + `--force` なし / 正常 export ペイロード構造 |
| UT | `import_records` — JSON パース失敗 / Redacted payload → MSG-CLI-144 / 衝突 error 戦略 / skip 戦略 / overwrite 戦略 |
| UT | `render_export_secrets_warning` — `--quiet` に関わらず出力されること |
| UT | `render_exported` / `render_imported` — English / JapaneseEn 両 locale |
| UT | `From<CliError> for ExitCode` — 新バリアント 5 種が全て exit 1 にマッピングされること（SSoT matrix 拡張）|
| IT | `export_records` → `import_records` round-trip — export したペイロードを import すると同じレコードが vault に入ること（`AC-DP-07`）|
| IT | `--export-secrets` なし export → import で MSG-CLI-144（`AC-DP-08`）|
| IT | `--on-conflict skip` が衝突レコードをスキップして残りを追加すること（`AC-DP-09`）|
| IT | 同一ファイルを 2 回 import → 2 回目全件衝突で MSG-CLI-142（`AC-DP-10`）|

---

## §依存関係・前提条件

| 依存先 | 理由 |
|--------|------|
| `shikomi-core::portability` モジュール（Sub-A #140 完成済み）| `ExportRecord` / `ExportPayload` / `ImportPayload` / `ImportValidator` / `ImportValidationError` |
| `shikomi-infra::persistence::VaultRepository`（実装済み）| `repo.load()` / `repo.save()` / `repo.exists()` |
| `serde_json`（`shikomi-cli/Cargo.toml` に既存）| JSON 文字列化 / パース |
| `tempfile = { workspace = true }`（`shikomi-cli/Cargo.toml` 71 行目に既存）| atomic write（追加依存なし）|
| `time::format_description::well_known::Rfc3339`（`shikomi-cli/Cargo.toml` に既存）| `created_at` / `updated_at` の RFC 3339 パース |

---

## §セキュリティ考慮

| 脅威 | 対策 |
|------|------|
| `--export-secrets` による誤操作全漏洩 | `MSG-CLI-145` を stderr に必ず出力（`--quiet` 抑止不可）。UseCase 呼び出し前に `lib.rs::run()` が出力する |
| export ファイルの不正読取 | export ファイルを `0600`（owner read/write のみ）で作成する。`tempfile::Builder::new().permissions(Permissions::from_mode(0o600)).tempfile_in(parent)?` で書き込み前にパーミッションを設定する（Unix 系）。`threat-model.md §7.5` 参照 |
| vault ロック済みでの export（平文漏洩の試み）| `export_records` が `RecordPayload::Encrypted` を持つレコードを変換する段階で `ExportError::VaultLocked` を検出し、`ExportImportVaultLocked` で早期失敗（Fail Fast）|
| 改ざんされた import ファイルによるデータ汚染 | `ImportValidator` が `format_version` / 重複 ID / Redacted payload を早期検出（Sub-A domain 層の責務）。UseCase は validator の結果が `Ok` の場合のみ import を進める |
| 部分書き込みによる vault 破損 | `tempfile::NamedTempFile::persist(path)` による atomic rename で import 中クラッシュ時の部分書き込みを防ぐ（`feature-spec.md R1-DP-09`）|
