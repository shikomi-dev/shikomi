# テスト設計書（インデックス）— data-portability / cli

<!-- feature: data-portability / sub-feature: cli / Issue #141 / Issue #146 -->
<!-- 配置先: docs/features/data-portability/cli/test-design.md -->
<!-- Vモデル対応: 階層 3（詳細設計 → ユニットテスト + 結合テスト）& 階層 2（feature-spec.md → E2Eテスト）-->
<!-- 兄弟: basic-design.md / detailed-design/ / 親: ../feature-spec.md -->

## テストケース配置（サブファイル）

| 種別 | ファイル | TC 数 | 対象 |
|------|---------|-------|------|
| E2Eテスト | [test-design/e2e.md](test-design/e2e.md) | 14（TC-E2E-DP-001〜014）| `shikomi export` / `shikomi import` CLIコマンド（AC-DP-06〜10）|
| 結合テスト | [test-design/it.md](test-design/it.md) | 5（TC-IT-DP-001〜005）| `export_records` / `import_records` UseCase 契約 |
| ユニットテスト | [test-design/ut.md](test-design/ut.md) | 10（TC-UT-201〜210）| Presenter 純粋関数 / ExitCode マッピング / conflict ID フォーマット |
| **合計** | | **29** | |

---

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
| TC-E2E-DP-013 | REQ-DP-009 | —（Issue #146 MSG-CLI-146）| 異常 | daemon が保持するロック（EXCLUSIVE 継続）→ import で `MSG-CLI-146` exit 1（~2s 待機）|
| TC-E2E-DP-014 | REQ-DP-009 | —（Issue #146 透過的成功）| 正常 | 300ms 後にロック解放 → `busy_timeout(2000ms)` 内に import 成功（exit 0）|
| TC-IT-DP-001 | REQ-DP-008 | AC-DP-06 | 境界値 | `export_records`: vault 空（`records: []`）→ `ExportSummary { record_count: 0 }` |
| TC-IT-DP-002 | REQ-DP-009 | —（R1-DP-05）| 異常 | `import_records`: JSON パース失敗 → `CliError::ImportDeserializationFailed` |
| TC-IT-DP-003 | REQ-DP-009/010 | —（R1-DP-05）| 異常 | `import_records`: `format_version: 999` → `CliError::ImportValidationFailed(UnknownFormatVersion)` |
| TC-IT-DP-004 | REQ-DP-009/010 | AC-DP-07 | 正常 | `import_records`: hotkey フィールドが復元される（R1-DP-10）|
| TC-IT-DP-005 | REQ-DP-009 | —（Issue #146）| 異常 | `import_records`: SQLITE_BUSY `busy_timeout(2000ms)` 超過 → `CliError::ImportVaultBusy` |
| TC-UT-201 | REQ-DP-011 | AC-DP-06 | 正常 | `render_exported`: English locale — 件数・パスが含まれる |
| TC-UT-202 | REQ-DP-011 | AC-DP-06 | 正常 | `render_exported`: JapaneseEn locale — 日本語文が含まれる |
| TC-UT-203 | REQ-DP-011 | AC-DP-07 | 正常 | `render_imported`: added / skipped / overwritten の各カウンタが文字列に反映される |
| TC-UT-204 | REQ-DP-011 | —（R1-DP-02）| 正常 | `render_export_secrets_warning`: 出力に `"warning: --export-secrets"` が含まれる |
| TC-UT-205 | REQ-DP-010 | —（設計内部保証）| 正常 | `ExitCode::from(&CliError)` — 新バリアント 5 種が全て `ExitCode::UserError`（exit 1）|
| TC-UT-206 | REQ-DP-011 | AC-DP-10 | 境界値 | `format_conflict_ids`: 4 件以下 → 全 ID をそのままカンマ区切りで返す |
| TC-UT-207 | REQ-DP-011 | AC-DP-10 | 境界値 | `format_conflict_ids`: 5 件以上 → 先頭 4 件 + `... (N more)` 形式 |
| TC-UT-208 | REQ-DP-010/011 | AC-DP-08 | 正常 | `render_error`: `ImportValidationFailed(RedactedPayload)` → `MSG-CLI-144` 文面が出力される |
| TC-UT-209 | REQ-DP-010 | —（Issue #146 設計内部保証）| 正常 | `From<DataPortabilityError> for CliError` — `VaultBusy` → `CliError::ImportVaultBusy`（`ExitCode::UserError` = exit 1）|
| TC-UT-210 | REQ-DP-010/011 | —（Issue #146 MSG-CLI-146 文面保証）| 正常 | `render_error(&CliError::ImportVaultBusy, Locale::English)` → MSG-CLI-146 文面（"vault is in use" / "stop shikomi-daemon" / "shikomi daemon uninstall"）が含まれる |

上位トレーサビリティ: `TC-E2E-DP-001〜012` → `AC-DP-06〜10` / `R1-DP-01〜10`（`feature-spec.md §5 Sub-B`）  
Issue #146 追加分: `TC-E2E-DP-013〜014` / `TC-IT-DP-005` / `TC-UT-209` / `TC-UT-210` → `basic-design.md §SQLITE_BUSY 設計判断` / `§MSG-CLI-146`  
下位連結: 本設計書 TC-E2E-* の検証対象 UseCase 実装に対し、`TC-IT-DP-*`・`TC-UT-*` が白箱保証を補完する

---

## 6. テストケース数サマリー

| グループ | ファイル | TC 数 |
|---------|---------|-------|
| E2E | [test-design/e2e.md](test-design/e2e.md) | 14（TC-E2E-DP-001〜014）|
| IT | [test-design/it.md](test-design/it.md) | 5（TC-IT-DP-001〜005）|
| UT | [test-design/ut.md](test-design/ut.md) | 10（TC-UT-201〜210）|
| **合計** | | **29** |

**受入テスト（階層 1 横断）**: data-portability feature は他 feature との crossing シナリオが「端末移行（export 元 vault → 別端末 vault）」に相当する。本設計書スコープ外だが、`SC-DDM-001` / `SC-DDM-002` 系の受入テストシナリオとして将来 `docs/acceptance-tests/scenarios/` に追加する。

**Characterization fixture**: 外部サービス依存なし。生成不要。

**人間が動作確認できるタイミング**: Sub-B 実装後に `cargo run --bin shikomi -- --no-ipc --vault-dir /tmp/test-vault export --output /tmp/vault.json` で確認可能。実行例は実装担当がREADMEに追記する。
