# テスト設計書 — data-portability / domain（ユニットテスト）

<!-- feature: data-portability / sub-feature: domain / Issue #140 -->
<!-- 配置先: docs/features/data-portability/domain/test-design.md -->
<!-- Vモデル対応: 階層 3（詳細設計 → ユニットテスト）-->
<!-- 兄弟: basic-design.md / detailed-design.md / 親: ../feature-spec.md -->

## 1. 設計方針

- **対象**: `crates/shikomi-core/src/portability/` モジュール群の型変換・バリデーション純粋関数
  - `ExportRecordPayload::from_record` — Secret リダクション tagged union 変換
  - `ExportRecord` フィールドマッピング（`From<(&Record, bool)>` 実装）
  - `ExportPayload::new` — `format_version: 1` 定数埋め込み
  - `ExportPayload` → JSON → `ImportPayload` serde ラウンドトリップ
  - `ImportValidator::validate` — バリデーション順序（フォーマットバージョン → 重複 ID → Redacted payload → 既存衝突）
  - `ImportValidationError::Display` — エラーメッセージに record id が含まれること
- **テストレベル**: ユニットテストのみ。`domain` 型は外部 I/O を持たない（`basic-design.md §テスト戦略` 参照）
- **粒度**: 1 テスト 1 主要アサーション。命名 `tc_ut_NNN_何をした時_どうなるべきか`
- **配置**: Rust 慣習、`#[cfg(test)] mod tests` でソースモジュール内
- **疑似コード禁止**: Rust コードブロックは記述しない。処理手順は番号付き箇条書きで表現する

---

## 2. 外部 I/O 依存マップ

| 外部 I/O | 利用箇所 | 状態 |
|---------|---------|------|
| ファイルシステム（JSON 読み書き）| `ImportPayload::from_str` / export ファイル出力 | Sub-B（CLI UseCase）スコープ。domain 型は JSON 文字列↔型変換のみ。UT スコープ外 |
| `VaultRepository` trait | `ExportUseCase` / `ImportUseCase` | Sub-B スコープ。domain 型には I/O 依存なし |
| 時刻（`OffsetDateTime::now_utc()`）| `ExportPayload::new` の `exported_at` | テスト内で `OffsetDateTime::UNIX_EPOCH` を固定値注入。外部依存なし |

**依存する外部 I/O ゼロ** — Characterization fixture / factory は不要。`serde_json` は OSS ライブラリ実物を使用（モック不要）。

---

## 3. モック方針（UT）

| 対象 | モック方法 |
|------|-----------|
| `RecordPayload` / `Record` | テスト用ビルダー（`RecordBuilder` 既存実装）またはインライン値で構築。外部 I/O なし |
| `serde_json` シリアライゼーション | OSS ライブラリ実物を使用。モック不要 |
| `OffsetDateTime` | `OffsetDateTime::UNIX_EPOCH` などの固定値をコンストラクタ引数として渡す |
| `existing_ids: HashSet<String>` | テスト内でインライン `HashSet::from([...])` を構築。DB 接続なし |

---

## 4. トレーサビリティマトリクス

| TC-ID | 対応要件 | 対応受入基準 | 種別 | 対象関数 / 観点 |
|-------|---------|------------|------|----------------|
| TC-UT-177 | REQ-DP-001 | AC-DP-02 | 正常 | `ExportRecordPayload::from_record`: Secret kind + `include_secrets=false` → `Redacted` |
| TC-UT-178 | REQ-DP-001 | AC-DP-02 | 正常 | `ExportRecordPayload::from_record`: Secret kind + `include_secrets=true` → `Plaintext` |
| TC-UT-179 | REQ-DP-001 | AC-DP-01 | 正常 | `ExportRecordPayload::from_record`: Text kind + `include_secrets=false` → `Plaintext`（リダクト対象外）|
| TC-UT-180 | REQ-DP-001 | AC-DP-02 | 正常 | `Redacted` の JSON 表現が `{"kind":"redacted"}` で `value` キーを含まない |
| TC-UT-181 | REQ-DP-001 | AC-DP-01 | 正常 | `Plaintext` の JSON 表現が `{"kind":"plaintext","value":"..."}` |
| TC-UT-182 | REQ-DP-002 | AC-DP-01 | 正常 | `ExportRecord::from`: 全フィールド（id / kind / label / payload / created_at / updated_at / hotkey=Some）が正しくマッピングされる |
| TC-UT-183 | REQ-DP-002 | AC-DP-01 | 正常 | `ExportRecord::from`: `hotkey=None` → JSON `null` |
| TC-UT-184 | REQ-DP-003 | AC-DP-01 | 正常 | `ExportPayload::new` の `format_version` フィールドが常に `1` |
| TC-UT-185 | REQ-DP-003 / REQ-DP-004 | AC-DP-01 | 正常 | `ExportPayload` → JSON 文字列 → `ImportPayload` serde ラウンドトリップ（全フィールド一致）|
| TC-UT-186 | REQ-DP-005 | AC-DP-05 | 異常 | `ImportValidator::validate`: `format_version > 1` → `ImportValidationError::UnknownFormatVersion { found }` |
| TC-UT-187 | REQ-DP-005 | AC-DP-01 | 正常 | `ImportValidator::validate`: `format_version == 1`・重複なし・Redacted なし → `Ok(ImportValidationReport)` |
| TC-UT-188 | REQ-DP-005 | AC-DP-04 | 異常 | `ImportValidator::validate`: ファイル内 ID 重複 → `ImportValidationError::DuplicateIdInFile { id }` |
| TC-UT-189 | REQ-DP-005 | AC-DP-03 | 異常 | `ImportValidator::validate`: `payload.kind == "redacted"` レコード → `ImportValidationError::RedactedPayload { id }` |
| TC-UT-190 | REQ-DP-005 | AC-DP-01 | 正常 | `ImportValidator::validate`: 既存 vault と ID 衝突 → `Ok` かつ `report.conflicting_ids` に衝突 ID が含まれる |
| TC-UT-191 | REQ-DP-005 | AC-DP-01 | 境界値 | `ImportValidator::validate`: `records` が空 → `Ok` かつ `report.warnings` に `ImportWarning::EmptyImport` が含まれる |
| TC-UT-192 | REQ-DP-005 | AC-DP-05 | 正常 | バリデーション順序: `format_version=999` + ファイル内 ID 重複 → `UnknownFormatVersion` を先に返す |
| TC-UT-193 | REQ-DP-005 | AC-DP-04 | 正常 | バリデーション順序: ファイル内 ID 重複 + Redacted payload → `DuplicateIdInFile` を先に返す |
| TC-UT-194 | REQ-DP-006 | AC-DP-03 | 正常 | `ImportValidationError::RedactedPayload` の `Display` 出力に record id が含まれる |

上位トレーサビリティ: `TC-UT-177〜194` → `ST-DP-*`（system-test-design.md）→ `AC-DP-01〜05`（feature-spec.md §5 Sub-A）

---

## 5. テストケース一覧

### 5.1 `ExportRecordPayload::from_record` — Secret リダクション（REQ-DP-001）

配置: `crates/shikomi-core/src/portability/export.rs` `#[cfg(test)] mod tests`

#### TC-UT-177: Secret kind + include_secrets=false → Redacted

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-177 |
| 対応要件 | REQ-DP-001 |
| 対応受入基準 | AC-DP-02 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `RecordPayload::Plaintext("p@ssword".into())` と `RecordKind::Secret` を用意する / 2. `ExportRecordPayload::from_record(&payload, RecordKind::Secret, false)` を呼ぶ |
| 期待結果 | 戻り値が `ExportRecordPayload::Redacted` であること |

#### TC-UT-178: Secret kind + include_secrets=true → Plaintext

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-178 |
| 対応要件 | REQ-DP-001 |
| 対応受入基準 | AC-DP-02 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `RecordPayload::Plaintext("p@ssword".into())` と `RecordKind::Secret` を用意する / 2. `ExportRecordPayload::from_record(&payload, RecordKind::Secret, true)` を呼ぶ |
| 期待結果 | 戻り値が `ExportRecordPayload::Plaintext { value }` であり `value == "p@ssword"` |

#### TC-UT-179: Text kind + include_secrets=false → Plaintext（リダクト対象外）

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-179 |
| 対応要件 | REQ-DP-001 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `RecordPayload::Plaintext("hello".into())` と `RecordKind::Text` を用意する / 2. `ExportRecordPayload::from_record(&payload, RecordKind::Text, false)` を呼ぶ |
| 期待結果 | 戻り値が `ExportRecordPayload::Plaintext { value: "hello" }` であること（Text kind は `include_secrets` に関わらずリダクトされない）|

#### TC-UT-180: `Redacted` の JSON 表現に `value` キーが存在しない

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-180 |
| 対応要件 | REQ-DP-001 |
| 対応受入基準 | AC-DP-02 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `ExportRecordPayload::Redacted` を構築する / 2. `serde_json::to_string` でシリアライズする |
| 期待結果 | JSON 文字列が `{"kind":"redacted"}` であり `"value"` キーを含まない（tagged union の構造的安全性）|

#### TC-UT-181: `Plaintext` の JSON 表現

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-181 |
| 対応要件 | REQ-DP-001 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `ExportRecordPayload::Plaintext { value: "hello".into() }` を構築する / 2. `serde_json::to_string` でシリアライズする |
| 期待結果 | JSON 文字列に `"kind":"plaintext"` と `"value":"hello"` が含まれる |

---

### 5.2 `ExportRecord` フィールドマッピング（REQ-DP-002）

配置: `crates/shikomi-core/src/portability/export.rs` `#[cfg(test)] mod tests`

#### TC-UT-182: 全フィールドが正しくマッピングされる（hotkey=Some）

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-182 |
| 対応要件 | REQ-DP-002 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | `Record` のテスト用ビルダーが利用可能 |
| 操作 | 1. `RecordKind::Text`・label `"my-label"`・hotkey `Some("Ctrl+1")` を持つ `Record` を構築する / 2. `ExportRecord::from((&record, false))` を呼ぶ |
| 期待結果 | (1) `export_record.id` が元 record の id 文字列表現と一致 / (2) `export_record.kind` が `RecordKind::Text` / (3) `export_record.label == "my-label"` / (4) `export_record.hotkey == Some("Ctrl+1".into())` / (5) `created_at` / `updated_at` が空でない RFC 3339 文字列 |

#### TC-UT-183: `hotkey=None` → JSON `null`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-183 |
| 対応要件 | REQ-DP-002 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | `Record` のテスト用ビルダーが利用可能 |
| 操作 | 1. `hotkey` が `None` の `Record` を構築する / 2. `ExportRecord::from((&record, false))` を呼ぶ / 3. `serde_json::to_value` でシリアライズする |
| 期待結果 | JSON オブジェクトの `"hotkey"` フィールドが `null` |

---

### 5.3 `ExportPayload` 構造と serde ラウンドトリップ（REQ-DP-003 / REQ-DP-004）

配置: `crates/shikomi-core/src/portability/export.rs` / `import.rs` `#[cfg(test)] mod tests`

#### TC-UT-184: `format_version` が常に `1`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-184 |
| 対応要件 | REQ-DP-003 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `ExportPayload::new(vec![], "test-vault".into(), OffsetDateTime::UNIX_EPOCH)` を呼ぶ / 2. `serde_json::to_value` でシリアライズする |
| 期待結果 | JSON の `"format_version"` フィールドが `1` |

#### TC-UT-185: `ExportPayload` → JSON → `ImportPayload` ラウンドトリップ

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-185 |
| 対応要件 | REQ-DP-003 / REQ-DP-004 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `ExportRecord`（Text kind・`include_secrets=false`）を含む `ExportPayload` を構築する / 2. `serde_json::to_string` でシリアライズする / 3. `serde_json::from_str::<ImportPayload>` でデシリアライズする |
| 期待結果 | デシリアライズ成功 / `import_payload.format_version == 1` / `import_payload.records` の件数・label が元データと一致 |

---

### 5.4 `ImportValidator::validate` — バリデーション順序（REQ-DP-005）

配置: `crates/shikomi-core/src/portability/import.rs` `#[cfg(test)] mod tests`

#### TC-UT-186: `format_version > 1` → `UnknownFormatVersion`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-186 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-05 |
| 種別 | 異常系 |
| 前提条件 | なし |
| 操作 | 1. `format_version: 999`・`records: vec![]` の `ImportPayload` を JSON 文字列から構築する / 2. `ImportValidator::validate(&payload, &HashSet::new())` を呼ぶ |
| 期待結果 | `Err(ImportValidationError::UnknownFormatVersion { found: 999 })` |

#### TC-UT-187: 正常バリデーション通過 → `Ok(ImportValidationReport)`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-187 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `format_version: 1`・重複なし・Redacted なし の `ImportPayload` を構築する / 2. `existing_ids` が空の `HashSet` を用意する / 3. `ImportValidator::validate(&payload, &existing_ids)` を呼ぶ |
| 期待結果 | `Ok(report)` / `report.conflicting_ids` が空 / `report.warnings` が空 |

#### TC-UT-188: ファイル内 ID 重複 → `DuplicateIdInFile`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-188 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-04 |
| 種別 | 異常系 |
| 前提条件 | なし |
| 操作 | 1. `format_version: 1`・同一 id を持つ 2 レコードを含む `ImportPayload` を構築する / 2. `ImportValidator::validate(&payload, &HashSet::new())` を呼ぶ |
| 期待結果 | `Err(ImportValidationError::DuplicateIdInFile { id })` かつ `id` が重複した ID 文字列と一致する |

#### TC-UT-189: Redacted payload レコード → `RedactedPayload`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-189 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-03 |
| 種別 | 異常系 |
| 前提条件 | なし |
| 操作 | 1. `format_version: 1`・`payload: {"kind": "redacted"}` を持つレコードを含む `ImportPayload` を JSON 文字列から構築する / 2. `ImportValidator::validate(&payload, &HashSet::new())` を呼ぶ |
| 期待結果 | `Err(ImportValidationError::RedactedPayload { id })` かつ `id` が当該レコードの id と一致する |

#### TC-UT-190: 既存 vault と ID 衝突 → `conflicting_ids` に格納される

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-190 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `format_version: 1`・id=`"aaaaa"` の Plaintext レコードを含む `ImportPayload` を構築する / 2. `existing_ids = HashSet::from(["aaaaa".into()])` を用意する / 3. `ImportValidator::validate(&payload, &existing_ids)` を呼ぶ |
| 期待結果 | `Ok(report)` / `report.conflicting_ids` に `"aaaaa"` が含まれる |

#### TC-UT-191: `records` が空 → `EmptyImport` 警告を含む `Ok`

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-191 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-01 |
| 種別 | 境界値 |
| 前提条件 | なし |
| 操作 | 1. `format_version: 1`・`records: []` の `ImportPayload` を構築する / 2. `ImportValidator::validate(&payload, &HashSet::new())` を呼ぶ |
| 期待結果 | `Ok(report)` / `report.warnings` に `ImportWarning::EmptyImport` が含まれる |

#### TC-UT-192: バリデーション順序確認 — `format_version=999` + ID 重複 → `UnknownFormatVersion` 優先

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-192 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-05 |
| 種別 | 正常系（順序検証）|
| 前提条件 | なし |
| 操作 | 1. `format_version: 999`・同一 id を持つ 2 レコードを含む `ImportPayload` を構築する / 2. `ImportValidator::validate(&payload, &HashSet::new())` を呼ぶ |
| 期待結果 | `Err(ImportValidationError::UnknownFormatVersion { found: 999 })`（`DuplicateIdInFile` ではなく）|

#### TC-UT-193: バリデーション順序確認 — ID 重複 + Redacted payload → `DuplicateIdInFile` 優先

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-193 |
| 対応要件 | REQ-DP-005 |
| 対応受入基準 | AC-DP-04 |
| 種別 | 正常系（順序検証）|
| 前提条件 | なし |
| 操作 | 1. `format_version: 1`・同一 id を持つ 2 レコード（うち 1 件が Redacted）を含む `ImportPayload` を構築する / 2. `ImportValidator::validate(&payload, &HashSet::new())` を呼ぶ |
| 期待結果 | `Err(ImportValidationError::DuplicateIdInFile { id })`（`RedactedPayload` ではなく）|

---

### 5.5 `ImportValidationError::Display`（REQ-DP-006）

配置: `crates/shikomi-core/src/portability/error.rs` `#[cfg(test)] mod tests`

#### TC-UT-194: `RedactedPayload` の `Display` に record id が含まれる

| 項目 | 内容 |
|------|------|
| テストID | TC-UT-194 |
| 対応要件 | REQ-DP-006 |
| 対応受入基準 | AC-DP-03 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | 1. `ImportValidationError::RedactedPayload { id: "test-id-123".into() }` を構築する / 2. `format!("{err}")` で Display 文字列を取得する |
| 期待結果 | 文字列に `"test-id-123"` が含まれる（CLI エラーメッセージの `{reason}` 展開に必要）|

---

## 6. テストケース数サマリー

| グループ | 対象 | TC 数 |
|---------|------|-------|
| 5.1 | `ExportRecordPayload::from_record` | 5（TC-UT-177〜181）|
| 5.2 | `ExportRecord` フィールドマッピング | 2（TC-UT-182〜183）|
| 5.3 | `ExportPayload` 構造 / serde ラウンドトリップ | 2（TC-UT-184〜185）|
| 5.4 | `ImportValidator::validate` バリデーション順序 | 8（TC-UT-186〜193）|
| 5.5 | `ImportValidationError::Display` | 1（TC-UT-194）|
| **合計** | | **18** |

結合テスト: **なし**（`basic-design.md §テスト戦略` の「IT: 該当なし — domain 型はファイル I/O を持たない」に従う）

受入テスト: AC-DP-01〜05（Sub-A スコープ）の検証は上記 UT で全件カバーされる。E2E（CLI 操作）は Sub-B `cli/test-design.md` で実施する。
