# テスト設計書 — data-portability / cli

<!-- feature: data-portability / sub-feature: cli / Issue #141 -->
<!-- 配置先: docs/features/data-portability/cli/test-design.md -->
<!-- Vモデル対応: 階層 3（詳細設計 → ユニットテスト + 結合テスト）& 階層 2（feature-spec.md → E2Eテスト）-->
<!-- 兄弟: basic-design.md / detailed-design.md / 親: ../feature-spec.md -->

## 1. 設計方針

- **対象**: `crates/shikomi-cli/src/` — CLI エントリポイント (`lib.rs`)・UseCase (`usecase/portability/`)・Presenter (`presenter/success.rs` / `presenter/error.rs`) の export / import 関連コード
- **テストレベルと優先順位**:
  1. **E2Eテスト**（最優先）: `shikomi export` / `shikomi import` コマンドを `assert_cmd` で実行し、stdout / stderr / exit code / ファイル内容を完全ブラックボックスで検証。AC-DP-06〜10 を直接カバー
  2. **結合テスト（IT）**: `export_records` / `import_records` UseCase 関数をエントリポイントとして呼び出し、実 SQLite vault + `tempdir` で挙動を検証
  3. **ユニットテスト（UT）**: Presenter 純粋関数・`ExitCode` マッピング・`format_conflict_ids` ヘルパー
- **E2E実行ツール**: `assert_cmd::Command::cargo_bin("shikomi")` + `predicates` + `tempfile::TempDir`（既存 `e2e_add.rs` パターンを踏襲）
- **粒度**: 1 テスト 1 主要アサーション。命名: E2E → `tc_e2e_dp_NNN_*`、IT → `tc_it_dp_NNN_*`、UT → `tc_ut_NNN_*`
- **配置**:
  - E2E: `crates/shikomi-cli/tests/e2e_portability.rs`
  - IT: `crates/shikomi-cli/tests/it_usecase_portability.rs`
  - UT: 各ソースファイルの `#[cfg(test)] mod tests`（`presenter/success.rs` / `presenter/error.rs` / `error.rs`）
- **疑似コード禁止**: Rust コードブロックは記述しない。処理手順は番号付き箇条書きで表現する
- **TC 番号**: 既存 Sub-A（TC-UT-177〜196, TC-UT-186b/187b, TC-UT-195b）との衝突を避け、E2E は `TC-E2E-DP-NNN`、IT は `TC-IT-DP-NNN`、UT は `TC-UT-201〜` を使用する

---

## 2. 外部 I/O 依存マップ

| 外部 I/O | 利用箇所 | E2E | IT | UT | Characterization |
|---------|---------|-----|-----|-----|-----------------|
| ファイルシステム（export ファイル書き込み / import ファイル読み込み）| `export_records` / `import_records` | 実接続（`TempDir`）| 実接続（`TempDir`）| モック不要（Presenter は I/O 不要）| 不要（ローカル tmpdir のみ）|
| SQLite vault（`VaultRepository`）| `export_records` / `import_records` | 実接続（`SqliteVaultRepository`）| 実接続（`SqliteVaultRepository`）| モック不要（UT 対象外）| 不要 |
| 時刻（`OffsetDateTime::now_utc()`）| `export_records` / `import_records` の `now` 引数 | 実時刻（E2E では注入不可）| 固定値 `fixed_time()` 注入 | 固定値 | 不要 |
| `tempfile` クレート（atomic write）| `export_records` ステップ 9〜11 | 実接続 | 実接続 | 不要 | 不要 |
| 標準出力 / 標準エラー（stdout / stderr）| `lib.rs::run_export` / `run_import` の `eprintln!` | `assert_cmd` で assert | 対象外（UseCase は I/O 持たない）| 対象外 | 不要 |

**外部 API / DB モック**: なし。CLI テストはすべて本物のファイルシステム + SQLite を使う。  
**Characterization fixture / factory**: 不要。外部サービスへの依存はゼロ。

---

## 3. モック方針

| 対象 | E2E | IT | UT |
|------|-----|-----|-----|
| `VaultRepository` | 実 `SqliteVaultRepository` | 実 `SqliteVaultRepository` | 対象外 |
| ファイルシステム | `TempDir` + 実ファイル書き込み | `TempDir` + 実ファイル書き込み | 対象外 |
| `shikomi` バイナリ | `Command::cargo_bin("shikomi")` | 対象外（UseCase 直呼び）| 対象外 |
| `Locale` | E2E は CLI 引数で制御不可 → デフォルト locale | IT で引数注入 | UT でインライン値 |
| `OffsetDateTime` | 実時刻（determinism 保証不要）| `fixed_time()` 注入 | インライン固定値 |

---

## 4. トレーサビリティマトリクス

| TC-ID | 対応要件 | 対応受入基準 | 種別 | 対象コマンド / 関数 / 観点 |
|-------|---------|------------|------|--------------------------|
| TC-E2E-DP-001 | REQ-DP-007/008 | AC-DP-06 | 正常 | `shikomi export --output`: 成功・`format_version:1` JSON 書き込み確認 |
| TC-E2E-DP-002 | REQ-DP-008/009 | AC-DP-07 | 正常 | export → import ラウンドトリップ: 同一レコードが vault に存在する |
| TC-E2E-DP-003 | REQ-DP-007/009 | AC-DP-08 | 異常 | `--export-secrets` なし export → import で `MSG-CLI-144` exit 1 |
| TC-E2E-DP-004 | REQ-DP-006/009 | AC-DP-09 | 正常 | `import --on-conflict skip`: 衝突レコードをスキップ・残りを追加 |
| TC-E2E-DP-005 | REQ-DP-006/009 | AC-DP-10 | 異常 | 同一ファイルを 2 回 import: 2 回目に `MSG-CLI-142` exit 1 |
| TC-E2E-DP-006 | REQ-DP-003/010 | —（R1-DP-03）| 異常 | vault ロック済み → export で `MSG-CLI-140` exit 1 |
| TC-E2E-DP-007 | REQ-DP-008 | —（R1-DP-01）| 異常 | `--force` なし + 出力先ファイル既存 → `MSG-CLI-141` exit 1 |
| TC-E2E-DP-008 | REQ-DP-007 | —（R1-DP-02）| 正常 | `--export-secrets` 指定 → `MSG-CLI-145` が stderr に出力される（`--quiet` でも）|
| TC-E2E-DP-009 | REQ-DP-006/009 | AC-DP-09 | 正常 | `import --on-conflict overwrite`: 衝突レコードを上書き |
| TC-E2E-DP-010 | REQ-DP-005/009 | —（R1-DP-05）| 異常 | 不正 JSON ファイル → import で `MSG-CLI-143` exit 1 |
| TC-E2E-DP-011 | REQ-DP-008 | —（R1-DP-01）| 正常 | `--force` 指定 + 出力先ファイル既存 → 上書き成功 |
| TC-E2E-DP-012 | REQ-DP-008 | —（非機能: `0600`）| 正常 | export ファイルのパーミッションが `0600`（Unix のみ）|
| TC-IT-DP-001 | REQ-DP-008 | AC-DP-06 | 境界値 | `export_records`: vault 空（`records: []`）→ `ExportSummary { record_count: 0 }` |
| TC-IT-DP-002 | REQ-DP-009 | —（R1-DP-05）| 異常 | `import_records`: JSON パース失敗 → `CliError::ImportDeserializationFailed` |
| TC-IT-DP-003 | REQ-DP-009/010 | —（R1-DP-05）| 異常 | `import_records`: `format_version: 999` → `CliError::ImportValidationFailed(UnknownFormatVersion)` |
| TC-IT-DP-004 | REQ-DP-009/010 | AC-DP-07 | 正常 | `import_records`: hotkey フィールドが復元される（R1-DP-10）|
| TC-UT-201 | REQ-DP-011 | AC-DP-06 | 正常 | `render_exported`: English locale — 件数・パスが含まれる |
| TC-UT-202 | REQ-DP-011 | AC-DP-06 | 正常 | `render_exported`: JapaneseEn locale — 日本語文が含まれる |
| TC-UT-203 | REQ-DP-011 | AC-DP-07 | 正常 | `render_imported`: added / skipped / overwritten の各カウンタが文字列に反映される |
| TC-UT-204 | REQ-DP-011 | —（R1-DP-02）| 正常 | `render_export_secrets_warning`: 出力に `"warning: --export-secrets"` が含まれる |
| TC-UT-205 | REQ-DP-010 | —（設計内部保証）| 正常 | `ExitCode::from(&CliError)` — 新バリアント 5 種が全て `ExitCode::UserError`（exit 1）|
| TC-UT-206 | REQ-DP-011 | AC-DP-10 | 境界値 | `format_conflict_ids`: 4 件以下 → 全 ID をそのままカンマ区切りで返す |
| TC-UT-207 | REQ-DP-011 | AC-DP-10 | 境界値 | `format_conflict_ids`: 5 件以上 → 先頭 4 件 + `... (N more)` 形式 |
| TC-UT-208 | REQ-DP-010/011 | AC-DP-08 | 正常 | `render_error`: `ImportValidationFailed(RedactedPayload)` → `MSG-CLI-144` 文面が出力される |
| TC-IT-DP-005 | REQ-DP-009 | —（Issue #146）| 異常 | `import_records`: SQLITE_BUSY `busy_timeout(2000ms)` 超過 → `CliError::ImportVaultBusy` |
| TC-UT-209 | REQ-DP-010 | —（Issue #146 設計内部保証）| 正常 | `From<DataPortabilityError> for CliError` — `VaultBusy` → `CliError::ImportVaultBusy`（`ExitCode::UserError` = exit 1）|
| TC-UT-210 | REQ-DP-010/011 | —（Issue #146 MSG-CLI-146 文面保証）| 正常 | `render_error(&CliError::ImportVaultBusy, Locale::English)` → MSG-CLI-146 文面（"vault is in use" / daemon 停止 hint）が含まれる |

上位トレーサビリティ: `TC-E2E-DP-001〜012` → `AC-DP-06〜10` / `R1-DP-01〜10`（`feature-spec.md §5 Sub-B`）  
Issue #146 追加分: `TC-IT-DP-005` / `TC-UT-209` / `TC-UT-210` → `basic-design.md §SQLITE_BUSY 設計判断` / `§MSG-CLI-146`  
下位連結: 本設計書 TC-E2E-* の検証対象 UseCase 実装に対し、`TC-IT-DP-*`・`TC-UT-*` が白箱保証を補完する

---

## 5. テストケース一覧

---

### 5.1 E2Eテスト — `shikomi export` / `shikomi import`（AC-DP-06〜10、最優先）

配置: `crates/shikomi-cli/tests/e2e_portability.rs`  
ツール: `assert_cmd::Command::cargo_bin("shikomi")` + `predicates::str::contains` / `predicates::path::exists`  
前提共通: `--no-ipc --vault-dir <TempDir>` を全コマンドに付与する。既存 `e2e_add.rs` の `shikomi()` ヘルパーパターンを踏襲する。

---

#### TC-E2E-DP-001: export 正常 — `format_version: 1` を含む JSON ファイルが書き込まれる

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-001 |
| 対応要件 | REQ-DP-007 / REQ-DP-008 |
| 対応受入基準 | AC-DP-06 |
| 種別 | 正常系 |
| 前提条件 | vault に Text レコードが 1 件存在する |
| 操作 | 1. vault に `shikomi add --kind text --label "L" --value "V"` でレコードを追加 / 2. `shikomi export --output <TempDir>/out.json` を実行 |
| 期待結果 | (1) exit code 0 / (2) stdout に `"exported 1 record(s)"` が含まれる / (3) `<TempDir>/out.json` が存在する / (4) ファイル内容に `"format_version":1` が含まれる / (5) ファイル内容に `"kind"` キーが含まれる（tagged union 構造）|

---

#### TC-E2E-DP-002: export → import ラウンドトリップ — 同一レコードが vault に存在する

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-002 |
| 対応要件 | REQ-DP-008 / REQ-DP-009 |
| 対応受入基準 | AC-DP-07 |
| 種別 | 正常系 |
| 前提条件 | vault A に Text レコードが 1 件存在する |
| 操作 | 1. vault A で `shikomi export --output <TempDir>/export.json` を実行 / 2. 新規 vault B（別 `TempDir`）に `shikomi import --input <TempDir>/export.json` を実行 / 3. vault B で `shikomi list` を実行 |
| 期待結果 | (1) import の exit code 0 / (2) import stdout に `"imported 1 record(s)"` が含まれる / (3) `shikomi list` の出力に元レコードのラベル文字列が含まれる（同一レコードが vault B に存在）|

---

#### TC-E2E-DP-003: `--export-secrets` なし → import で `MSG-CLI-144` exit 1

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-003 |
| 対応要件 | REQ-DP-007 / REQ-DP-009 |
| 対応受入基準 | AC-DP-08 |
| 種別 | 異常系 |
| 前提条件 | vault に Secret kind レコードが 1 件存在する |
| 操作 | 1. `shikomi export --output <TempDir>/out.json`（`--export-secrets` なし）を実行 / 2. `shikomi import --input <TempDir>/out.json` を実行（別または同じ vault）|
| 期待結果 | (1) import の exit code 1 / (2) import stderr に `"cannot import record"` が含まれる（MSG-CLI-144）/ (3) import stderr に `"payload is redacted"` が含まれる / (4) import stderr に `"re-export"` ヒントが含まれる |

---

#### TC-E2E-DP-004: `import --on-conflict skip` — 衝突レコードをスキップして残りを追加する

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-004 |
| 対応要件 | REQ-DP-006 / REQ-DP-009 |
| 対応受入基準 | AC-DP-09 |
| 種別 | 正常系 |
| 前提条件 | vault に Text レコード 2 件（label "A"、label "B"）が存在する |
| 操作 | 1. vault を export して `export.json` を作成 / 2. 同じ vault に `shikomi add --kind text --label "C" --value "C"` を追加（これで vault には A, B, C の 3 件）/ 3. `shikomi import --input export.json --on-conflict skip` を実行 |
| 期待結果 | (1) exit code 0 / (2) stdout に `"skipped 2"` が含まれる（A, B は衝突スキップ）/ (3) stdout に `"imported 0 record(s)"` または `added: 0` が含まれる（新規追加なし）|

---

#### TC-E2E-DP-005: 同一ファイルを 2 回 import — 2 回目に `MSG-CLI-142` exit 1

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-005 |
| 対応要件 | REQ-DP-006 / REQ-DP-009 |
| 対応受入基準 | AC-DP-10 |
| 種別 | 異常系 |
| 前提条件 | vault に Text レコード 1 件が存在する |
| 操作 | 1. vault を export して `export.json` を作成 / 2. 別 vault B に `shikomi import --input export.json` を実行（1 回目）/ 3. 同じ vault B に `shikomi import --input export.json` を再実行（2 回目）|
| 期待結果 | (1) 2 回目の exit code 1 / (2) 2 回目の stderr に `"import conflict"` が含まれる（MSG-CLI-142）/ (3) 2 回目の stderr に `"record(s) already exist in vault"` が含まれる / (4) 2 回目の stderr に `"--on-conflict skip"` ヒントが含まれる |

---

#### TC-E2E-DP-006: vault ロック済み → export で `MSG-CLI-140` exit 1

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-006 |
| 対応要件 | REQ-DP-003 / REQ-DP-010 |
| 対応受入基準 | —（R1-DP-03 実装保証）|
| 種別 | 異常系 |
| 前提条件 | 暗号化（ロック済み）vault が存在する。`e2e_encrypted.rs` / `common::fixtures::create_encrypted_vault` の既存ヘルパーを利用 |
| 操作 | 1. ロック済み暗号化 vault のディレクトリで `shikomi export --output <TempDir>/out.json` を実行 |
| 期待結果 | (1) exit code 1 / (2) stderr に `"vault is locked"` が含まれる（MSG-CLI-140）/ (3) stderr に `"unlock"` ヒントが含まれる |

---

#### TC-E2E-DP-007: `--force` なし + 出力先ファイル既存 → `MSG-CLI-141` exit 1

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-007 |
| 対応要件 | REQ-DP-008 |
| 対応受入基準 | —（R1-DP-01 Fail Fast 保証）|
| 種別 | 異常系 |
| 前提条件 | `<TempDir>/existing.json` が既に存在する（内容は問わない）|
| 操作 | 1. vault に任意レコードが存在する状態で / 2. `shikomi export --output <TempDir>/existing.json`（`--force` なし）を実行 |
| 期待結果 | (1) exit code 1 / (2) stderr に `"export output file already exists"` が含まれる（MSG-CLI-141）/ (3) stderr に `"--force"` ヒントが含まれる |

---

#### TC-E2E-DP-008: `--export-secrets` 指定 → `MSG-CLI-145` が stderr に出力される（`--quiet` でも）

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-008 |
| 対応要件 | REQ-DP-007 |
| 対応受入基準 | —（R1-DP-02 警告抑止不可保証）|
| 種別 | 正常系 |
| 前提条件 | vault に Secret kind レコードが 1 件存在する |
| 操作 | 1. `shikomi export --output <TempDir>/out.json --export-secrets --quiet` を実行（`--quiet` で通常 stdout を抑制）|
| 期待結果 | (1) exit code 0 / (2) stderr に `"warning: --export-secrets is set"` が含まれる（MSG-CLI-145 — `--quiet` でも抑止されない）/ (3) stdout には成功メッセージが含まれない（`--quiet` が有効）|

---

#### TC-E2E-DP-009: `import --on-conflict overwrite` — 衝突レコードを上書きする

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-009 |
| 対応要件 | REQ-DP-006 / REQ-DP-009 |
| 対応受入基準 | AC-DP-09 |
| 種別 | 正常系 |
| 前提条件 | vault A に Text レコード（label "old"）が 1 件存在する |
| 操作 | 1. vault A を export して `old.json` を作成 / 2. vault A の同一レコードのラベルを "new" に更新（`shikomi edit`）/ 3. vault A に `shikomi import --input old.json --on-conflict overwrite` を実行 / 4. vault A で `shikomi list` を実行 |
| 期待結果 | (1) import の exit code 0 / (2) stdout に `"overwritten 1"` が含まれる / (3) `shikomi list` の出力にラベル `"old"` が含まれる（元の値に上書きされた）|

---

#### TC-E2E-DP-010: 不正 JSON ファイル → import で `MSG-CLI-143` exit 1

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-010 |
| 対応要件 | REQ-DP-005 / REQ-DP-009 |
| 対応受入基準 | —（R1-DP-05 バリデーション保証）|
| 種別 | 異常系 |
| 前提条件 | `<TempDir>/broken.json` に不正な JSON テキスト（例: `{not valid json`）が書き込まれている |
| 操作 | 1. `shikomi import --input <TempDir>/broken.json` を実行 |
| 期待結果 | (1) exit code 1 / (2) stderr に `"failed to parse import file"` が含まれる（MSG-CLI-143）/ (3) stderr に `"format_version"` ヒントが含まれる |

---

#### TC-E2E-DP-011: `--force` + 出力先ファイル既存 → 上書き成功

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-011 |
| 対応要件 | REQ-DP-008 |
| 対応受入基準 | —（R1-DP-01 `--force` 動作保証）|
| 種別 | 正常系 |
| 前提条件 | `<TempDir>/output.json` が既に存在する（旧バージョンの export ファイルなど）|
| 操作 | 1. vault に Text レコードが 1 件存在する状態で / 2. `shikomi export --output <TempDir>/output.json --force` を実行 |
| 期待結果 | (1) exit code 0 / (2) stdout に `"exported"` が含まれる / (3) `<TempDir>/output.json` が新しい内容（`"format_version":1`）に上書きされている |

---

#### TC-E2E-DP-012: export ファイルのパーミッションが `0600`（Unix のみ）

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-012 |
| 対応要件 | REQ-DP-008 |
| 対応受入基準 | —（非機能要件 `0600` パーミッション）|
| 種別 | 正常系 |
| 前提条件 | Unix 環境。vault に任意レコードが存在する |
| 操作 | 1. `shikomi export --output <TempDir>/out.json` を実行 / 2. `std::fs::metadata("<TempDir>/out.json").permissions().mode()` でパーミッションを取得する |
| 期待結果 | (1) exit code 0 / (2) ファイルパーミッションの下位 9 ビットが `0o600`（`rw-------`）と一致する。`#[cfg(unix)]` で実行する |

---

### 5.2 結合テスト（UseCase level）— REQ-DP-008 / REQ-DP-009

配置: `crates/shikomi-cli/tests/it_usecase_portability.rs`  
前提共通: `common::fresh_repo()` で実 `SqliteVaultRepository` + `TempDir` を生成。`common::fixed_time()` で時刻を固定。

---

#### TC-IT-DP-001: `export_records` — vault 空でも `ExportSummary { record_count: 0 }` が返る

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-001 |
| 対応要件 | REQ-DP-008 |
| 対応受入基準 | AC-DP-06 |
| 種別 | 境界値 |
| 前提条件 | vault が未作成（`repo.exists() == false`）|
| 操作 | 1. `fresh_repo()` で空の `SqliteVaultRepository` を取得する / 2. `ExportArgs { output: out_path, export_secrets: false, force: false }` を構築する / 3. `export_records(&repo, &args, vault_dir, fixed_time())` を呼ぶ |
| 期待結果 | `Ok(ExportSummary { record_count: 0, output_path: out_path })` / `out_path` ファイルが存在し、内容に `"format_version":1` が含まれる |

---

#### TC-IT-DP-002: `import_records` — JSON パース失敗 → `CliError::ImportDeserializationFailed`

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-002 |
| 対応要件 | REQ-DP-009 |
| 対応受入基準 | —（R1-DP-05）|
| 種別 | 異常系 |
| 前提条件 | `<TempDir>/broken.json` に `{invalid json` が書き込まれている |
| 操作 | 1. `fresh_repo()` で実 repository を取得する / 2. `ImportArgs { input: broken_json_path, on_conflict: OnConflictArg::Error }` を構築する / 3. `import_records(&repo, &args, fixed_time())` を呼ぶ |
| 期待結果 | `Err(CliError::ImportDeserializationFailed { reason })` が返ること |

---

#### TC-IT-DP-003: `import_records` — `format_version: 999` → `CliError::ImportValidationFailed(UnknownFormatVersion)`

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-003 |
| 対応要件 | REQ-DP-009 / REQ-DP-010 |
| 対応受入基準 | —（R1-DP-05）|
| 種別 | 異常系 |
| 前提条件 | `<TempDir>/v999.json` に `{"format_version":999,"vault_name":"test","exported_at":"1970-01-01T00:00:00Z","records":[]}` が書き込まれている |
| 操作 | 1. `fresh_repo()` で実 repository を取得する / 2. `ImportArgs { input: v999_json_path, on_conflict: OnConflictArg::Error }` を構築する / 3. `import_records(&repo, &args, fixed_time())` を呼ぶ |
| 期待結果 | `Err(CliError::ImportValidationFailed(ImportValidationError::UnknownFormatVersion { found: 999 }))` が返ること |

---

#### TC-IT-DP-004: `import_records` — hotkey フィールドが復元される（R1-DP-10）

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-004 |
| 対応要件 | REQ-DP-009 / REQ-DP-010 |
| 対応受入基準 | AC-DP-07 |
| 種別 | 正常系 |
| 前提条件 | vault に `hotkey: Some("ctrl+1")` の Text レコードが 1 件存在する |
| 操作 | 1. vault A を export（`export_records`）して `out.json` を作成する / 2. 別 vault B（空）に `import_records` で import する / 3. vault B の `repo.load()` でレコードを取得し hotkey を確認する |
| 期待結果 | vault B のレコードの `hotkey` が `Some("ctrl+1")` と一致する |

---

#### TC-IT-DP-005: `import_records` — SQLITE_BUSY `busy_timeout(2000ms)` 超過 → `CliError::ImportVaultBusy`

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-005 |
| 対応要件 | REQ-DP-009 |
| 対応受入基準 | —（`basic-design.md §SQLITE_BUSY 設計判断`）|
| 種別 | 異常系 |
| 前提条件 | `fresh_repo()` で実 `SqliteVaultRepository` + `TempDir` を生成する。vault に Text レコードが 1 件存在する（`import_records` が `repo.save()` を必要とする状態）。|
| 操作 | 1. `fresh_repo()` でテスト用 repo（vault.db ファイル）を生成する / 2. `rusqlite::Connection::open(&vault_db_path)` で **別の SQLite 接続**（lock_conn）を開く / 3. `lock_conn.execute("BEGIN EXCLUSIVE", [])` で排他トランザクションを開始し vault.db の書き込みロックを確保する / 4. `import_records(&repo, &args, fixed_time())` を呼ぶ（`repo` は `busy_timeout(2000ms)` 設定済みの `SqliteVaultRepository` を使用する。`fresh_repo()` の impl が未対応の場合は `busy_timeout` 設定済み repo を構築するヘルパーを追加する）/ 5. `import_records` の戻り値を受け取る（`busy_timeout 2000ms` 超過まで待機が発生する） |
| 期待結果 | `Err(CliError::ImportVaultBusy)` が返ること。lock_conn を保持したまま 2 秒以上経過後にエラーが返ることを確認する（`#[cfg_attr(not(slow_tests), ignore)]` アノテーション推奨 — 2 秒の実時間待機が発生するため）|
| 補足 | **逆シナリオ**（lock 解放後に成功）は TC-IT-DP-001〜004 の `fresh_repo()` ベーステスト（競合なし = 即座に `save()` 成功）が担保する。`busy_timeout` 設定なしの場合は即座に SQLITE_BUSY が返り 2 秒待機しないため、テスト対象の repo に `busy_timeout` が適切に設定されていることを事前確認すること |

---

### 5.3 ユニットテスト — Presenter / ExitCode / Helper（REQ-DP-010 / REQ-DP-011）

配置:
- `render_exported` / `render_imported` / `render_export_secrets_warning`: `crates/shikomi-cli/src/presenter/success.rs` `#[cfg(test)] mod tests`
- `render_error` / `format_conflict_ids`: `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)] mod tests`
- `ExitCode::from(&CliError)` 新バリアント: `crates/shikomi-cli/src/error.rs` `#[cfg(test)] mod tests`

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
| 期待結果 | (1) `CliError::ImportVaultBusy` が返ること / (2) `ExitCode::UserError` が返ること（exit 1 に対応）|

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
| 期待結果 | 出力文字列に `"vault is in use by shikomi-daemon"` が含まれる / `"stop shikomi-daemon"` ヒントが含まれる（MSG-CLI-146 文面と一致）|

---

## 6. テストケース数サマリー

| グループ | 対象 | TC 数 |
|---------|------|-------|
| 5.1 E2E | `shikomi export` / `shikomi import` CLIコマンド（AC-DP-06〜10 + エラー経路）| 12（TC-E2E-DP-001〜012）|
| 5.2 IT | `export_records` / `import_records` UseCase 契約 | 5（TC-IT-DP-001〜005）|
| 5.3 UT | Presenter 純粋関数 / ExitCode マッピング / conflict ID フォーマット | 10（TC-UT-201〜210）|
| **合計** | | **27** |

**受入テスト（階層 1 横断）**: data-portability feature は他 feature との crossing シナリオが「端末移行（export 元 vault → 別端末 vault）」に相当する。本設計書スコープ外だが、`SC-DDM-001` / `SC-DDM-002` 系の受入テストシナリオとして将来 `docs/acceptance-tests/scenarios/` に追加する。

**Characterization fixture**: 外部サービス依存なし。生成不要。

**人間が動作確認できるタイミング**: Sub-B 実装後に `cargo run --bin shikomi -- --no-ipc --vault-dir /tmp/test-vault export --output /tmp/vault.json` で確認可能。実行例は実装担当がREADMEに追記する。
