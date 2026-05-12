# 基本設計書 — data-portability / domain（モジュール契約）

<!-- feature: data-portability / sub-feature: domain / Issue #140 -->
<!-- 配置先: docs/features/data-portability/domain/basic-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature モジュール契約）-->
<!-- 親: ../feature-spec.md -->

## §モジュール契約（機能要件）

### REQ-DP-001: `ExportRecordPayload` — ペイロードリダクション表現

| 項目 | 内容 |
|------|------|
| 入力 | `RecordPayload` + `kind: RecordKind` + `include_secrets: bool` |
| 処理 | (1) `RecordPayload::Encrypted` の場合 → 即座に `Err(ExportError::VaultLocked)` を返す（Fail Fast）。(2) `include_secrets == false` かつ `kind == RecordKind::Secret` の場合 → `Ok(ExportRecordPayload::Redacted)` を返す。(3) それ以外 → `expose_secret()` を呼び出して `Ok(ExportRecordPayload::Plaintext { value })` を返す（`expose_secret` の呼び出しはこの分岐にのみ閉じる）|
| 出力 | `Result<ExportRecordPayload, ExportError>` — `ExportRecordPayload` は `Plaintext { value: String }` / `Redacted` の 2 バリアント |
| エラー時 | `RecordPayload::Encrypted` → `Err(ExportError::VaultLocked)` |
| 設計原則 | Tell, Don't Ask（`expose_secret` の呼び出しをこの型変換に閉じ込める）/ Fail Fast（`Encrypted` ペイロードを即時エラーにし、不整合な中間状態を作らない）|

**`ExportRecordPayload` バリアント一覧（`Locked` バリアントは存在しない）**:

| バリアント | JSON | 説明 |
|-----------|------|------|
| `Plaintext { value: String }` | `{ "kind": "plaintext", "value": "<text>" }` | 平文ペイロード |
| `Redacted` | `{ "kind": "redacted" }` | Secret kind のリダクト表現 |

`payload_redacted: bool` フラットフィールドではなく tagged union を採用する。理由: `"[REDACTED]"` 文字列リテラルと平文 `"[REDACTED]"` の衝突を構造的に排除するため。`Locked` バリアントは設けない——`Encrypted` ペイロードは `from_record` で即時 `Err` にするため、この型に到達しない。

**`ExportError` 型**: `from_record` の戻り値 `Err` に使う。`shikomi-core` の `portability/error.rs` に定義する（`ImportValidationError` と同居）。バリアント: `VaultLocked`。

### REQ-DP-002: `ExportRecord` — エクスポートレコード値オブジェクト

| 項目 | 内容 |
|------|------|
| 入力 | `Record` + `include_secrets: bool` |
| 処理 | `Record` の各フィールドを JSON シリアライズ可能な値オブジェクトに変換する。(1) `id` → `String`（UUID v7 の文字列表現）/ (2) `kind` → `RecordKind`（既存 serde 実装を流用）/ (3) `label` → `String` / (4) `payload` → `ExportRecordPayload::from_record(&payload, kind, include_secrets)?`（`ExportError::VaultLocked` を伝播）/ (5) `created_at` / `updated_at` → RFC 3339 文字列（マイクロ秒精度）/ (6) `hotkey` → `Option<String>`（`Hotkey::as_str()` の正規化文字列、`None` の場合は `null`）|
| 出力 | `Result<ExportRecord, ExportError>`（全フィールドに `Serialize` / `Deserialize` 実装）|
| エラー時 | `RecordPayload::Encrypted` を持つレコード → `Err(ExportError::VaultLocked)` を伝播 |
| 設計原則 | DDD 値オブジェクト（不変・同一性なし）/ Composition（`ExportRecordPayload` に変換を委譲）|

### REQ-DP-003: `ExportPayload` — エクスポートファイル全体

| 項目 | 内容 |
|------|------|
| 入力 | `records: Vec<ExportRecord>` + `vault_name: String` + `exported_at: OffsetDateTime` |
| 処理 | エクスポートファイルのルート JSON オブジェクトを構築する。`format_version: 1`（定数）を必ず含める |
| 出力 | `ExportPayload`（`Serialize` / `Deserialize` 実装）|
| エラー時 | 該当なし |
| 設計原則 | 拡張性（`format_version` で将来のスキーマ変更に対応）|

**JSON スキーマ（`format_version: 1`）**:

| フィールド | 型 | 説明 |
|-----------|----|----|
| `format_version` | `u32` | 常に `1`（MVP）|
| `exported_at` | `String` | RFC 3339（例: `"2026-05-12T09:00:00.000000Z"`）|
| `vault_name` | `String` | vault ディレクトリの basename（識別用メタデータ）|
| `records` | `Array<ExportRecord>` | 全レコード |

各 `ExportRecord` のフィールド:

| フィールド | 型 | 説明 |
|-----------|----|------|
| `id` | `String` | UUID v7 文字列 |
| `kind` | `"text"` / `"secret"` | `RecordKind` の snake_case serde 表現 |
| `label` | `String` | レコードラベル |
| `payload` | `Object` | `ExportRecordPayload` の tagged union |
| `created_at` | `String` | RFC 3339 マイクロ秒精度 |
| `updated_at` | `String` | RFC 3339 マイクロ秒精度 |
| `hotkey` | `String` / `null` | ホットキー正規化文字列（`"alt+ctrl+1"` 形式）または `null`。文字列形式の SSoT は `daemon-hotkey-clipboard/domain/basic-design.md §文字列表現`（`Hotkey::as_str()` の正規化形式: 修飾キーをアルファベット順に並べた `+` 区切り文字列）。この形式は `format_version: 1` で凍結。将来の形式変更は `format_version` バンプを要する |

### REQ-DP-004: `ImportPayload` / `ImportRecord` — インポート入力バリデーション前型

| 項目 | 内容 |
|------|------|
| 入力 | ファイルから読み込んだ JSON 文字列（`serde_json::from_str`）|
| 処理 | `ExportPayload` と同じスキーマで `Deserialize` する。`ImportRecord` は `ExportRecord` の type alias（`type ImportRecord = ExportRecord`）とし、同一フィールド定義を 2 箇所で管理しない（DRY）。`ImportPayload` は `ExportPayload` の alias とせず独立型とする理由: import 固有のバリデーション責務（`ImportValidator::validate()`）を保有するため |
| 出力 | `ImportPayload { format_version, exported_at, vault_name, records: Vec<ImportRecord> }`（`ImportRecord = ExportRecord` の alias）|
| エラー時 | JSON パース失敗 → `DataPortabilityError::DeserializationFailed { reason }` |
| 設計原則 | Fail Fast（JSON 構造不正は即時失敗）|

### REQ-DP-005: `ImportValidator` — バリデーション責務

| 項目 | 内容 |
|------|------|
| 入力 | `ImportPayload` + `existing_ids: HashSet<RecordId>`（現在の vault の全 ID）|
| 処理 | 以下を順次検証する。(1) `format_version` ≤ `CURRENT_FORMAT_VERSION(=1)` であること（未知のバージョンは拒否）/ (2) `records` が空でないこと（空 import は warning 扱い、エラーではない）/ (3) import ファイル内の `id` 重複がないこと / (4) `payload.kind == "redacted"` のレコードが存在しないこと / (5) `existing_ids` との衝突検出（ID 一覧を返す、戦略適用は呼び出し側）|
| 出力 | `ImportValidationReport { conflicting_ids: Vec<RecordId>, warnings: Vec<ImportWarning> }` |
| エラー時 | `DataPortabilityError::ValidationFailed(ImportValidationError)` — `ImportValidationError` は以下のバリアントを持つ: `UnknownFormatVersion { found: u32 }` / `DuplicateIdInFile { id: String }` / `RedactedPayload { id: String }` |
| 設計原則 | Fail Fast（バリデーション失敗は早期に検出して呼び出し側に返す）/ 単一責務（戦略適用は UseCase 側の責務）|

### REQ-DP-006: `DataPortabilityError` — エラー型階層

| バリアント | 発生条件 |
|-----------|---------|
| `VaultLocked` | 暗号化 vault がロック済みの状態で export / import が呼ばれた |
| `OutputFileExists { path: PathBuf }` | export 先ファイルが既に存在し `--force` 未指定 |
| `DeserializationFailed { reason: String }` | JSON パース失敗 |
| `ValidationFailed(ImportValidationError)` | import バリデーション失敗 |
| `IoError(std::io::Error)` | ファイル I/O エラー |
| `ConflictError { ids: Vec<RecordId> }` | `--on-conflict error` で衝突が発生 |

**設計判断**: `DataPortabilityError` は `shikomi-core` ではなく `shikomi-cli` の `usecase/` 層に定義する。domain 型（`ExportRecord` / `ImportPayload` 等）は `shikomi-core` に置くが、error は CLI UseCase の責務であり、`shikomi-core` に I/O エラーを持ち込まない。

## モジュール配置

| クレート | パス | 内容 |
|---------|------|------|
| `shikomi-core` | `src/portability/mod.rs` | `ExportRecordPayload` / `ExportRecord` / `ExportPayload` / `ImportPayload` / `ImportRecord` / `ImportValidator` / `ImportValidationError` |
| `shikomi-core` | `src/portability/export.rs` | `ExportRecord` / `ExportPayload` 型定義 + `From<(&Record, bool)>` 変換実装 |
| `shikomi-core` | `src/portability/import.rs` | `ImportPayload` / `ImportRecord` / `ImportValidator` 型定義 |
| `shikomi-core` | `src/portability/error.rs` | `ImportValidationError` 型定義 |

**設計判断**: `portability` モジュールを `shikomi-core` に新設する。`Record` は `shikomi-core` に存在し、export は `Record` への変換を伴う。`shikomi-cli` の `usecase/` は `portability` モジュールを利用するだけで良い。`serde_json` は `shikomi-core` の `Cargo.toml` に既に存在するため追加依存なし。

## ユーザー向けメッセージ一覧（Sub-A スコープ）

Sub-A では UI を持たない。メッセージ ID の予約のみ行う。

| ID | 表示条件 | 終了コード |
|----|---------|---------|
| MSG-CLI-140 | vault がロック済みで export / import 不可 | 1 |
| MSG-CLI-141 | export 先ファイルが既に存在（`--force` 未指定）| 1 |
| MSG-CLI-142 | `--on-conflict error` で衝突発生 | 1 |
| MSG-CLI-143 | JSON パース失敗 / フォーマットバージョン不一致 | 1 |
| MSG-CLI-144 | `{"kind":"redacted"}` payload レコードの import 試行 | 1 |
| MSG-CLI-145 | `--export-secrets` 実行時 Secret 平文 export 警告 | 0（stderr 出力のみ、処理は続行）|

文面の確定は Sub-B（Issue #141）の `cli/basic-design.md §ユーザー向けメッセージ` で行う。

## テスト戦略（テスト設計 Issue で詳細化）

| テストレベル | 観点 |
|-------------|------|
| UT | `ExportRecordPayload::from_record` — Secret kind がリダクトされること / Text kind が平文で返ること |
| UT | `ExportRecord::from` — 全フィールドが正しく変換されること |
| UT | `ImportValidator::validate` — UnknownFormatVersion / DuplicateIdInFile / RedactedPayload の各エラーが正しく返ること |
| UT | `ExportPayload` → JSON → `ImportPayload` のラウンドトリップ（`serde` roundtrip）|
| IT | 該当なし — domain 型はファイル I/O を持たない |

## 依存関係・前提条件

| 依存先 | 理由 |
|--------|------|
| `shikomi-core` の `Record` / `RecordKind` / `RecordPayload`（実装済み）| `ExportRecord::from` の変換元 |
| `serde` / `serde_json`（`shikomi-core` の `Cargo.toml` に既存）| JSON シリアライゼーション。追加依存なし |
| `daemon-hotkey-clipboard` feature の `Hotkey::as_str()`（`shikomi-core::vault::record::hotkey::Hotkey`）| hotkey フィールドの文字列化（SSoT: `daemon-hotkey-clipboard/domain/basic-design.md §文字列表現`）。`daemon-hotkey-clipboard` が未完了の場合、`hotkey` フィールドは常に `null` で export する（フォールバック）|

## セキュリティ考慮

| 脅威 | 対策 |
|------|------|
| Secret kind の平文漏洩 | `from_record` が `Result` を返し、`Encrypted` ペイロードは即時 `Err(ExportError::VaultLocked)` にする（Fail Fast）。`expose_secret` の呼び出しはこの関数にのみ閉じる |
| `[REDACTED]` 文字列リテラルとの混同 | tagged union（`{"kind":"redacted"}`）を採用し、sentinel 文字列の衝突を構造的に排除する |
| export ファイルの不正読取 | export ファイルは `0600`（owner read/write のみ）で作成する。`tempfile::Builder::new().permissions(0o600)` で書き込み前にパーミッションを設定する。vault.db と同等の保護水準を確保する（`threat-model.md §7.5` 参照）|
| `--export-secrets` による誤操作全漏洩 | `MSG-CLI-145` を stderr に必ず出力する（`--quiet` でも抑止不可）。ユーザーが意図を確認できる最終ゲート |
| import ファイルへの不正データ注入 | `ImportValidator` が `format_version` / 重複 ID / Redacted payload を検出して早期失敗させる |
| 改ざんされた import ファイル | 完全性検証（HMAC 等）は YAGNI（MVP スコープ外）。export ファイルへの署名は将来拡張 |
