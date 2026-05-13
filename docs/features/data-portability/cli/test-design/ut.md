# ユニットテスト設計 — data-portability / cli

<!-- feature: data-portability / sub-feature: cli / Issue #141 / Issue #146 -->
<!-- 配置先: docs/features/data-portability/cli/test-design/ut.md -->
<!-- 親: ../test-design.md（インデックス）-->
<!-- Vモデル対応: 階層 3（detailed-design/ → ユニットテスト）-->

## 5.3 ユニットテスト — Presenter / ExitCode / Helper（REQ-DP-010 / REQ-DP-011）

配置:
- `render_exported` / `render_imported` / `render_export_secrets_warning`: `crates/shikomi-cli/src/presenter/success.rs` `#[cfg(test)] mod tests`
- `render_error` / `format_conflict_ids`: `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)] mod tests`
- `ExitCode::from(&CliError)` 新バリアント / `From<DataPortabilityError> for CliError`: `crates/shikomi-cli/src/error.rs` `#[cfg(test)] mod tests`

---

#### TC-UT-201: `render_exported` — English locale に件数・パスが含まれる

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-201 |
| 対応要件 | REQ-DP-011 |
| 対応受入基準 | AC-DP-06 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `render_exported(3, Path::new("/tmp/out.json"), Locale::English)` を呼ぶ |
| 期待結果 | 戻り値文字列に `"exported 3 record(s)"` が含まれ、`"/tmp/out.json"` が含まれる |

---

#### TC-UT-202: `render_exported` — JapaneseEn locale に日本語文が含まれる

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-202 |
| 対応要件 | REQ-DP-011 |
| 対応受入基準 | AC-DP-06 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `render_exported(3, Path::new("/tmp/out.json"), Locale::JapaneseEn)` を呼ぶ |
| 期待結果 | 戻り値文字列に `"export しました"` が含まれる |

---

#### TC-UT-203: `render_imported` — added / skipped / overwritten の各カウンタが反映される

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-203 |
| 対応要件 | REQ-DP-011 |
| 対応受入基準 | AC-DP-07 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `render_imported(2, 1, 3, Locale::English)` を呼ぶ |
| 期待結果 | 戻り値文字列に `"imported 2 record(s)"` / `"skipped 1"` / `"overwritten 3"` が全て含まれる |

---

#### TC-UT-204: `render_export_secrets_warning` — 出力に `"warning: --export-secrets"` が含まれる

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-204 |
| 対応要件 | REQ-DP-011 |
| 対応受入基準 | —（R1-DP-02 警告の文面保証）|
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `render_export_secrets_warning(Locale::English)` を呼ぶ |
| 期待結果 | 戻り値文字列に `"warning: --export-secrets is set"` が含まれ、さらに `"store the export file securely"` が含まれる（MSG-CLI-145 の両行）|

---

#### TC-UT-205: `ExitCode::from(&CliError)` — 新バリアント 5 種が全て `UserError`（exit 1）

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-205 |
| 対応要件 | REQ-DP-010 |
| 対応受入基準 | —（設計内部保証。exit code SSoT matrix 拡張）|
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `CliError::ExportImportVaultLocked` / `CliError::ExportOutputFileExists { path: .. }` / `CliError::ImportConflict { ids: vec![] }` / `CliError::ImportDeserializationFailed { reason: "r".into() }` / `CliError::ImportValidationFailed(ImportValidationError::UnknownFormatVersion { found: 999 })` の各バリアントに対し `ExitCode::from(&e)` を呼ぶ |
| 期待結果 | 全 5 バリアントで `ExitCode::UserError` が返ること（exit 1 に対応）|

---

#### TC-UT-206: `format_conflict_ids` — 4 件以下は全 ID をそのまま返す

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-206 |
| 対応要件 | REQ-DP-011 |
| 対応受入基準 | AC-DP-10 |
| 種別 | 境界値 |
| 前提条件 | なし |
| 操作 | 1. `format_conflict_ids(&["id-1".into(), "id-2".into(), "id-3".into(), "id-4".into()])` を呼ぶ |
| 期待結果 | `"id-1, id-2, id-3, id-4"` が返る（省略なし）|

---

#### TC-UT-207: `format_conflict_ids` — 5 件以上は先頭 4 件 + `... (N more)` 形式

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-207 |
| 対応要件 | REQ-DP-011 |
| 対応受入基準 | AC-DP-10 |
| 種別 | 境界値 |
| 前提条件 | なし |
| 操作 | 1. 6 件の ID スライス（`"a"` 〜 `"f"`）で `format_conflict_ids` を呼ぶ |
| 期待結果 | 戻り値文字列が先頭 4 件のカンマ区切りを含み、`"... (2 more)"` が含まれる |

---

#### TC-UT-208: `render_error` — `ImportValidationFailed(RedactedPayload)` → MSG-CLI-144 文面

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-208 |
| 対応要件 | REQ-DP-010 / REQ-DP-011 |
| 対応受入基準 | AC-DP-08 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `CliError::ImportValidationFailed(ImportValidationError::RedactedPayload { id: "test-id-xyz".into() })` を構築する / 2. `render_error(&err, Locale::English)` を呼ぶ（または `lines_for` の dispatch を通じた出力を取得する）|
| 期待結果 | 出力文字列に `"cannot import record test-id-xyz"` が含まれる / `"payload is redacted"` が含まれる / `"re-export"` ヒントが含まれる（MSG-CLI-144 文面と一致）|

---

#### TC-UT-209: `From<DataPortabilityError> for CliError` — `VaultBusy` → `CliError::ImportVaultBusy`（`ExitCode::UserError`）

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-209 |
| 対応要件 | REQ-DP-010 |
| 対応受入基準 | —（Issue #146 設計内部保証）|
| 種別 | 正常系 |
| 前提条件 | なし |
| 配置 | `crates/shikomi-cli/src/error.rs` `#[cfg(test)] mod tests` |
| 操作 | 1. `DataPortabilityError::VaultBusy` を `CliError::from(...)` で変換する / 2. `ExitCode::from(&cli_err)` を呼ぶ |
| 期待結果 | (1) `CliError::ImportVaultBusy` が返ること（`cli.md §From<DataPortabilityError> for CliError` の `VaultBusy → ImportVaultBusy` マッピング）/ (2) `ExitCode::UserError` が返ること（exit 1 に対応）|

---

#### TC-UT-210: `render_error(&CliError::ImportVaultBusy, Locale::English)` → MSG-CLI-146 文面

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-210 |
| 対応要件 | REQ-DP-010 / REQ-DP-011 |
| 対応受入基準 | —（Issue #146 MSG-CLI-146 文面保証）|
| 種別 | 正常系 |
| 前提条件 | なし |
| 配置 | `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)] mod tests` |
| 操作 | 1. `CliError::ImportVaultBusy` を構築する / 2. `render_error(&err, Locale::English)` を呼ぶ |
| 期待結果 | 出力文字列に `"vault is in use by shikomi-daemon"` が含まれる / `"stop shikomi-daemon"` が含まれる / `"shikomi daemon uninstall"` が含まれる（MSG-CLI-146 hint 文面: `"stop shikomi-daemon, then retry (to disable autostart: shikomi daemon uninstall)"` に準拠）|
