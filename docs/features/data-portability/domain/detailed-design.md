# 詳細設計書 — data-portability / domain

<!-- feature: data-portability / sub-feature: domain / Issue #140 -->
<!-- 配置先: docs/features/data-portability/domain/detailed-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 兄弟: ./basic-design.md -->

## 記述ルール

疑似コード禁止。処理順序は番号付き箇条書きで表現する。型・フィールド・モジュールパスは \`code\` 表記で明示する。

## 変更対象ファイル一覧

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-core/src/lib.rs` | 編集 | `pub mod portability;` を追加 |
| `crates/shikomi-core/src/portability/mod.rs` | 新規 | `portability` モジュールのエクスポート（re-export）|
| `crates/shikomi-core/src/portability/export.rs` | 新規 | `ExportRecordPayload` / `ExportRecord` / `ExportPayload` |
| `crates/shikomi-core/src/portability/import.rs` | 新規 | `ImportRecord` / `ImportPayload` / `ImportValidator` |
| `crates/shikomi-core/src/portability/error.rs` | 新規 | `ImportValidationError` / `ExportError` |

変更不要ファイル:

| ファイル | 理由 |
|---------|------|
| `crates/shikomi-core/Cargo.toml` | `serde` / `serde_json` / `time(serde)` / `uuid(serde)` は既存依存に含まれる |
| `crates/shikomi-core/src/vault/` 以下全ファイル | `portability` モジュールは `vault` を参照するが、`vault` は `portability` を参照しない（依存逆転なし）|
| `crates/shikomi-cli/` 以下全ファイル | Sub-B（Issue #141）スコープ |

## `crates/shikomi-core/src/portability/export.rs` の設計詳細

### `ExportRecordPayload` 型

- `Serialize` / `Deserialize` 実装。`serde(tag = "kind", rename_all = "snake_case")` で tagged union とする
- バリアント:
  1. `Plaintext { value: String }` → JSON: `{ "kind": "plaintext", "value": "..." }`
  2. `Redacted` → JSON: `{ "kind": "redacted" }`
- `from_record(payload: &RecordPayload, kind: RecordKind, include_secrets: bool) -> Result<Self, ExportError>` 関連関数:
  1. `payload` が `RecordPayload::Encrypted` → 即座に `Err(ExportError::VaultLocked)` を返す（Fail Fast。release ビルドでも動作する）
  2. `kind == RecordKind::Secret` かつ `include_secrets == false` → `Ok(Redacted)` を返す
  3. 上記以外 → `payload` の平文値から `expose_secret()` を呼び出し `Ok(Plaintext { value })` を返す
- **`expose_secret` 呼び出し集約**: この関数が唯一の expose_secret 呼び出し箇所（`cli-vault-commands/basic-design/security.md §expose_secret 経路監査` の方針に準拠）

### `ExportRecord` 型

- 全フィールドに `Serialize` / `Deserialize` 実装
- フィールドとその変換元:

| フィールド | Rust 型 | serde 表現 | 変換元 |
|-----------|---------|-----------|--------|
| `id` | `String` | 文字列 | `record.id().to_string()` |
| `kind` | `RecordKind` | `"text"` / `"secret"` | `record.kind()` |
| `label` | `String` | 文字列 | `record.label().as_str().to_owned()` |
| `payload` | `ExportRecordPayload` | tagged union | `ExportRecordPayload::from_record(...)?`（`ExportError::VaultLocked` を伝播）|
| `created_at` | `String` | RFC 3339 | `record.created_at()` → `time::format_description::well_known::Rfc3339` |
| `updated_at` | `String` | RFC 3339 | `record.updated_at()` → `Rfc3339` |
| `hotkey` | `Option<String>` | 文字列 or null | `record.hotkey().map(|h| h.as_str().to_owned())` |

- `ExportRecord::try_from((&Record, bool)) -> Result<Self, ExportError>` を実装する（`bool` は `include_secrets`）。`From` ではなく `TryFrom` を使う理由: `from_record` が `Result` を返すため|

### `ExportPayload` 型

- 全フィールドに `Serialize` / `Deserialize` 実装
- フィールド:

| フィールド | Rust 型 | serde 表現 | 注記 |
|-----------|---------|-----------|------|
| `format_version` | `u32` | 数値 | 定数 `EXPORT_FORMAT_VERSION: u32 = 1` を使用 |
| `exported_at` | `String` | RFC 3339 | コンストラクタ引数 `OffsetDateTime` から変換 |
| `vault_name` | `String` | 文字列 | コンストラクタ引数 |
| `records` | `Vec<ExportRecord>` | 配列 | コンストラクタ引数 |

- `ExportPayload::new(records, vault_name, now)` コンストラクタを提供する

## `crates/shikomi-core/src/portability/import.rs` の設計詳細

### `ImportRecord` 型（`ExportRecord` の type alias）

- `type ImportRecord = ExportRecord` — type alias として定義する。フィールド定義を 2 箇所で管理しない（DRY / KISS）
- 設計判断: `ImportRecord` を独立した struct にする振る舞い上の差異が存在しない。`ImportPayload.records` の要素型として使うだけで、バリデーション責務は `ImportValidator` が持つ。`ExportRecord` が `Serialize + Deserialize` の両方を実装しているため、roundtrip テストもこのまま成立する
- `serde` の `deny_unknown_fields` は使用しない（将来バージョンが追加フィールドを持っても import できるよう前方互換を保つ）

### `ImportPayload` 型

- `ExportPayload` と同一フィールド定義。`Deserialize` のみ実装
- 追加責務: `ImportValidator::validate(&self, existing_ids)` を呼び出すファサード

### `ImportValidator` 型

- ステートレス（関連関数のみ）
- `validate(payload: &ImportPayload, existing_ids: &HashSet<String>) -> Result<ImportValidationReport, ImportValidationError>` の処理順序:
  1. `payload.format_version > EXPORT_FORMAT_VERSION` → `ImportValidationError::UnknownFormatVersion { found: payload.format_version }` を返す
  2. `payload.records` を走査し、`id` の重複を `HashSet` で検出 → 重複あれば `ImportValidationError::DuplicateIdInFile { id }` を返す（最初の重複 ID のみ返す。全件列挙は YAGNI）
  3. `payload.records` を走査し、`payload.kind == "redacted"` のレコードを検出 → `ImportValidationError::RedactedPayload { id }` を返す（最初の 1 件のみ）
  4. `existing_ids` との衝突 ID を収集 → `conflicting_ids` に格納
  5. 全バリデーション通過 → `ImportValidationReport { conflicting_ids, warnings }` を返す
- バリデーション順序の設計判断: フォーマットバージョン → ファイル内重複 → Redacted → 既存衝突。フォーマット不正は最優先でエラーにし、ユーザーが正しい状態から再試行できるようにする

### `ImportValidationReport` 型

| フィールド | 型 | 説明 |
|-----------|----|----|
| `conflicting_ids` | `Vec<String>` | 既存 vault と ID が衝突するレコードの ID 一覧 |
| `warnings` | `Vec<ImportWarning>` | 警告一覧（`records` が空の場合に `EmptyImport` 警告を追加）|

## `crates/shikomi-core/src/portability/error.rs` の設計詳細

### `ImportValidationError` 型

| バリアント | フィールド | 説明 |
|-----------|-----------|------|
| `UnknownFormatVersion` | `found: u32` | 未知の `format_version`（`> 1`）|
| `DuplicateIdInFile` | `id: String` | import ファイル内で ID が重複 |
| `RedactedPayload` | `id: String` | `payload.kind == "redacted"` のレコードを import 試行 |

- `std::error::Error` / `Display` を実装する
- `Display` の出力例: `"cannot import: payload is redacted for record id=<id>"` — CLI エラーメッセージの `{reason}` に展開される

## `portability/mod.rs` の re-export 設計

以下を public re-export する:

- `export::ExportRecord`
- `export::ExportRecordPayload`
- `export::ExportPayload`
- `export::EXPORT_FORMAT_VERSION`
- `import::ImportRecord`
- `import::ImportPayload`
- `import::ImportValidator`
- `import::ImportValidationReport`
- `import::ImportWarning`
- `error::ImportValidationError`

## `crates/shikomi-core/src/lib.rs` の変更詳細

- 既存の `pub mod` 宣言群の末尾に `pub mod portability;` を追加する
- 変更は 1 行のみ

## セキュリティ考慮（domain スコープ）

| 脅威 | 対策 |
|------|------|
| Secret kind の平文漏洩 | `from_record` 内で `include_secrets == false` 時に `Redacted` を返す。`expose_secret` の呼び出しはこの関数に閉じる |
| `[REDACTED]` 文字列リテラルとの混同 | tagged union（`{ "kind": "redacted" }`）を採用し、sentinel 文字列の衝突を構造的に排除する |
| import ファイルへの不正データ注入 | `ImportValidator` が `format_version` / 重複 ID / Redacted payload を検出して早期失敗させる |
| 改ざんされた import ファイル | 完全性検証（HMAC 等）は YAGNI（MVP スコープ外）。export ファイルへの署名は将来拡張 |
