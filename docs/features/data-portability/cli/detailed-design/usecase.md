# 詳細設計書 — data-portability / cli / UseCase 変更

<!-- feature: data-portability / sub-feature: cli / Issue #141 -->
<!-- 配置先: docs/features/data-portability/cli/detailed-design/usecase.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親: ../basic-design.md -->
<!-- 兄弟: cli.md / presenter.md -->

## 記述ルール

疑似コード禁止。処理順序は番号付き箇条書きで表現する。型・フィールド・モジュールパスは `code` 表記で明示する。

## 変更対象ファイル

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/usecase/mod.rs` | 編集 | `pub mod portability;` を追加（1 行のみ）|
| `crates/shikomi-cli/src/usecase/portability/mod.rs` | 新規 | `export` / `import` / `error` モジュールの re-export |
| `crates/shikomi-cli/src/usecase/portability/error.rs` | 新規 | `DataPortabilityError` 型定義 |
| `crates/shikomi-cli/src/usecase/portability/export.rs` | 新規 | `export_records` 関数 + `ExportSummary` 型 |
| `crates/shikomi-cli/src/usecase/portability/import.rs` | 新規 | `import_records_sqlite` / `import_records_ipc` 関数 + `ImportSummary` 型 + `import_record_to_domain` helper |

---

## `usecase/portability/error.rs` の設計詳細

### `DataPortabilityError` 型（`thiserror::Error` 実装）

UseCase 内部の中間エラー型。`From<DataPortabilityError> for CliError` で変換される（`cli.md §From<DataPortabilityError>` 参照）。

| バリアント | フィールド | 発生条件 |
|-----------|-----------|---------|
| `VaultLocked` | なし | `SqliteVaultRepository::load()` が `ProtectionMode::Encrypted` を検出 / `IpcVaultRepository::add_record()` が `IpcErrorCode::VaultLocked` を返した |
| `OutputFileExists` | `path: PathBuf` | export 先ファイルが既に存在し `--force` 未指定 |
| `ConflictError` | `ids: Vec<String>` | `--on-conflict error` で衝突検出 |
| `DeserializationFailed` | `reason: String` | `serde_json::from_str` 失敗 |
| `ValidationFailed` | `ImportValidationError` | `ImportValidator::validate` 失敗 |
| `IoError` | `std::io::Error` | ファイル読み込み / `tempfile` 操作 / `persist` 失敗 |

`From<std::io::Error> for DataPortabilityError` を実装し I/O エラーを `IoError` に wrap する。

---

## `usecase/portability/export.rs` の設計詳細

### `ExportSummary` 型

| フィールド | 型 | 説明 |
|-----------|----|----|
| `record_count` | `usize` | export したレコード件数（vault 不存在時は 0）|
| `output_path` | `PathBuf` | 実際に書き込んだファイルパス |

### `export_records` 関数

シグネチャ: `pub fn export_records(repo: &dyn VaultRepository, args: &ExportArgs, vault_dir: &Path, now: OffsetDateTime) -> Result<ExportSummary, CliError>`

処理順序:

1. `args.output.exists() && !args.force` → `Err(DataPortabilityError::OutputFileExists { path: args.output.clone() }.into())`
2. `!repo.exists()?` → `export_records = vec![]`（vault 不存在は 0 件 export、エラーではない）
3. `repo.exists()?` の場合: `let vault = repo.load()?` を呼び出す
4. `vault.protection_mode() == ProtectionMode::Encrypted` → `Err(DataPortabilityError::VaultLocked.into())`（`EncryptionUnsupported` 経路でも同バリアントに変換する）
5. `vault.records()` で全レコードを取得する（`Vec<&Record>`）
6. 各レコードを `ExportRecord::try_from((&record, args.export_secrets))` で変換する。`ExportError::VaultLocked` → `Err(DataPortabilityError::VaultLocked.into())`
7. `vault_name = vault_dir.file_name().unwrap_or_else(|| OsStr::new("vault")).to_string_lossy().into_owned()`
8. `ExportPayload::new(export_records, vault_name, now)` でペイロードを構築する
9. `serde_json::to_string_pretty(&payload).expect("ExportPayload is always serializable")` — `Serialize` の不変条件によりパニックは実用上起こらない
10. `let parent = args.output.parent().unwrap_or_else(|| Path::new("."))`
11. Unix: `tempfile::Builder::new().permissions(Permissions::from_mode(0o600)).tempfile_in(parent).map_err(DataPortabilityError::IoError)?`  
    非 Unix（`#[cfg(not(unix))]`）: `tempfile::Builder::new().tempfile_in(parent).map_err(DataPortabilityError::IoError)?`
12. `tmp_file.write_all(json.as_bytes()).map_err(DataPortabilityError::IoError)?`
13. `tmp_file.persist(&args.output).map_err(|e| DataPortabilityError::IoError(e.error))?.into()`（`PersistError` の `error` フィールドが `io::Error`）
14. `Ok(ExportSummary { record_count: export_records.len(), output_path: args.output.clone() })`

**注意（vault 不存在の early return）**: 手順 2 で `repo.exists()?` が false の場合、手順 3〜6 をスキップして手順 7 以降（空 `ExportPayload` の生成）に進む。

---

## `usecase/portability/import.rs` の設計詳細

### `ImportSummary` 型

| フィールド | 型 | 説明 |
|-----------|----|----|
| `added` | `usize` | 新規追加したレコード件数 |
| `skipped` | `usize` | 衝突により skip したレコード件数（`--on-conflict skip` 時のみ非ゼロ）|
| `overwritten` | `usize` | 既存レコードを上書きしたレコード件数（`--on-conflict overwrite` 時のみ非ゼロ）|

### `import_records_sqlite` 関数（SQLite `--no-ipc` 経路）

シグネチャ: `pub fn import_records_sqlite(repo: &dyn VaultRepository, args: &ImportArgs, now: OffsetDateTime) -> Result<ImportSummary, CliError>`

処理順序:

1. `std::fs::read_to_string(&args.input).map_err(DataPortabilityError::IoError)?`
2. `serde_json::from_str::<ImportPayload>(&contents)` — `Err(e)` → `DataPortabilityError::DeserializationFailed { reason: e.to_string() }.into()`
3. vault の準備: `repo.exists()?` が true なら `repo.load()?`（`EncryptionUnsupported` → `DataPortabilityError::VaultLocked`）、false なら `Vault::new(VaultHeader::new_plaintext(VaultVersion::CURRENT, now)?)`
4. `let existing_ids: HashSet<String> = vault.records().iter().map(|r| r.id().to_string()).collect()`
5. `ImportValidator::validate(&payload, &existing_ids)` — `Err(err)` → `DataPortabilityError::ValidationFailed(err).into()`
6. `args.on_conflict == OnConflictArg::Error && !report.conflicting_ids.is_empty()` → `DataPortabilityError::ConflictError { ids: report.conflicting_ids }.into()`
7. 各 `record` を `&payload.records` から走査する:
   - `is_conflicting = report.conflicting_ids.contains(&record.id)`
   - `is_conflicting && on_conflict == Skip` → `skipped += 1; continue`
   - `domain_record = import_record_to_domain(record)?`（変換失敗 → `ImportDeserializationFailed`）
   - `is_conflicting && on_conflict == Overwrite` → `vault.remove_record(domain_record.id()).map_err(CliError::Domain)?` → `vault.add_record(domain_record).map_err(CliError::Domain)?` → `overwritten += 1`
   - それ以外 → `vault.add_record(domain_record).map_err(CliError::Domain)?` → `added += 1`
8. `repo.save(&vault)?`
9. `Ok(ImportSummary { added, skipped, overwritten })`

### `import_records_ipc` 関数（IPC デフォルト経路）

シグネチャ: `pub fn import_records_ipc(ipc: &IpcVaultRepository, args: &ImportArgs, now: OffsetDateTime) -> Result<ImportSummary, CliError>`

処理順序:

1. ファイル読み込み・パース: `import_records_sqlite` の手順 1〜2 と同じロジック
2. `ipc.list_summaries()?` で既存レコード ID を収集: `let existing_ids: HashSet<String> = outcome.records.iter().map(|s| s.id.to_string()).collect()`
3. バリデーション・衝突チェック: `import_records_sqlite` の手順 5〜6 と同じロジック
4. 各 `record` を `&payload.records` から走査する:
   - `is_conflicting = report.conflicting_ids.contains(&record.id)`
   - `is_conflicting && on_conflict == Skip` → `skipped += 1; continue`
   - `domain_record = import_record_to_domain(record)?`（変換失敗 → `ImportDeserializationFailed`）
   - `is_conflicting && on_conflict == Overwrite` → `ipc.remove_record(domain_record.id().clone()).map_err(|e| CliError::from(e))?` → `ipc.add_record_for_import(domain_record).map_err(|e| CliError::from(e))?` → `overwritten += 1`
   - それ以外 → `ipc.add_record_for_import(domain_record).map_err(|e| CliError::from(e))?` → `added += 1`
   - `IpcErrorCode::VaultLocked` が返った場合 → `ExportImportVaultLocked`（既存 `From<IpcErrorCode> for CliError` 後に `VaultLocked` → `ExportImportVaultLocked` への再写像を追加、または `run_import` 側で変換）
5. `Ok(ImportSummary { added, skipped, overwritten })`

**設計判断（IPC 経路の `add_record` 使用方法）**: `ipc.add_record()` は既存メソッドで label / kind / value / hotkey を受け付ける。import では `created_at` / `updated_at` の保存も必要なため、IPC プロトコルに `ImportRecord` を丸ごと送る `add_record_for_import` を別途追加するか、既存 `add_record` を流用して `created_at` / `updated_at` は `now` に置き換える（import 時の timestamp 保存を諦める）かを Sub-B 実装時に IPC プロトコル担当と調整する。タイムスタンプ保存の必要性は `feature-spec.md R1-DP-10` に「`hotkey` フィールドを復元する」とあるが `created_at` / `updated_at` の明示的な復元は要件化されていない。SQLite 直接経路でのみタイムスタンプを完全復元し、IPC 経路では `now` を使用するフォールバックを採用する（YAGNI）。

### `import_record_to_domain` helper 関数

シグネチャ: `fn import_record_to_domain(r: &ImportRecord) -> Result<Record, CliError>`

処理順序:

1. `uuid::Uuid::parse_str(&r.id)` 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid record id '{}': {e}", r.id) }`
2. `RecordId::new(uuid)` 失敗 → `CliError::ImportDeserializationFailed { reason: e.to_string() }`
3. `RecordLabel::try_new(r.label.clone())` 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid label: {e}") }`
4. payload 変換:
   - `ExportRecordPayload::Plaintext { value }` → `RecordPayload::Plaintext(SecretString::from_string(value.clone()))`
   - `ExportRecordPayload::Redacted` → `unreachable!("ImportValidator rejects Redacted payload; import_record_to_domain must not be called for redacted records")`
5. `OffsetDateTime::parse(&r.created_at, &Rfc3339)` 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid created_at '{}': {e}", r.created_at) }`
6. `OffsetDateTime::parse(&r.updated_at, &Rfc3339)` 失敗 → 同様
7. hotkey: `r.hotkey.as_deref().map(Hotkey::parse).transpose()` — parse 失敗 → `CliError::ImportDeserializationFailed { reason: format!("invalid hotkey: {e}") }`
8. **`Record::rehydrate(id, r.kind, label, payload, created_at, updated_at, hotkey)`** を呼ぶ（Sub-A 確認済み。`updated_at < created_at` の場合 `DomainError::VaultConsistencyError` → `CliError::ImportDeserializationFailed { reason: e.to_string() }`）
9. `Ok(record)`

**`unreachable!` の根拠**: `ImportValidator::validate` が `RedactedPayload` を `Err` として返すため、本関数が呼ばれた時点でリダクトペイロードは除外済みの契約が成立している。`unreachable!` は告知的プログラミングによる二重安全網。release ビルドでは通常の `panic!` として動作する（UB なし）。

---

## セキュリティ考慮（UseCase スコープ）

| 脅威 | 対策 |
|------|------|
| export ファイルの不正読取 | `0600` パーミッション設定を `tempfile::Builder` で実施（`cfg(unix)`）。Windows は ACL 委譲（`feature-spec.md §4` 通り）|
| vault ロック済みでの export（`Encrypted` ペイロード変換失敗の伝播）| 手順 4 で `ProtectionMode::Encrypted` を即時検出して early return。手順 6 の `ExportError::VaultLocked` が二重安全網 |
| `import_record_to_domain` での `unreachable!` panic | `ImportValidator` 通過済みのデータのみ本関数に到達する設計契約。panic は実装バグの早期検出（告知的プログラミング）|
| IPC 経路での Secret 値送信 | IPC `add_record` は既存 AddInput 経由でペイロードを送信する。Secret kind の平文は daemon ソケット（ローカル IPC）経由での送信になるが、既存 `shikomi add --kind secret` と同経路であり、脅威モデル上は許容済み |
