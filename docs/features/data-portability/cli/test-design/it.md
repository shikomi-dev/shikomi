# 結合テスト設計 — data-portability / cli

<!-- feature: data-portability / sub-feature: cli / Issue #141 / Issue #146 -->
<!-- 配置先: docs/features/data-portability/cli/test-design/it.md -->
<!-- 親: ../test-design.md（インデックス）-->
<!-- Vモデル対応: 階層 3（basic-design.md §モジュール契約 → 結合テスト）-->

## 5.2 結合テスト（UseCase level）— REQ-DP-008 / REQ-DP-009

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

#### TC-IT-DP-005: `import_records` — `load()` 経路で SQLITE_BUSY `busy_timeout(2000ms)` 超過 → `CliError::ImportVaultBusy`

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-005 |
| 対応要件 | REQ-DP-009 |
| 対応受入基準 | —（`basic-design.md §SQLITE_BUSY 設計判断`）|
| 種別 | 異常系 |
| 前提条件 | `fresh_repo()` で実 `SqliteVaultRepository` + `TempDir` を生成する。vault に Text レコードが 1 件存在する（`import_records` が `repo.save()` を必要とする状態）。|
| 操作 | 1. `fresh_repo()` でテスト用 repo（vault.db ファイル）を生成する / 2. `rusqlite::Connection::open(&vault_db_path)` で **別の SQLite 接続**（lock_conn）を開く / 3. `lock_conn.execute("BEGIN EXCLUSIVE", [])` で排他トランザクションを開始し vault.db の書き込みロックを確保する / 4. `import_records(&repo, &args, fixed_time())` を呼ぶ（`repo` は `busy_timeout(2000ms)` 設定済みの `SqliteVaultRepository` を使用する。`SqliteVaultRepository::from_directory_with_busy_timeout()` ファクトリを使用する — `usecase.md §busy_timeout 設定` 参照）/ 5. `import_records` の戻り値を受け取る（`busy_timeout 2000ms` 超過まで待機が発生する） |
| 期待結果 | `Err(CliError::ImportVaultBusy)` が返ること。lock_conn を保持したまま 2 秒以上経過後にエラーが返ることを確認する（`#[cfg_attr(not(slow_tests), ignore)]` アノテーション推奨 — 2 秒の実時間待機が発生するため）|
| 補足 | **逆シナリオ**（lock 解放後に成功）は TC-IT-DP-001〜004 の `fresh_repo()` ベーステスト（競合なし = 即座に `save()` 成功）が担保する。`busy_timeout` 設定なしの場合は即座に SQLITE_BUSY が返り 2 秒待機しないため、テスト対象の repo に `busy_timeout` が適切に設定されていることを事前確認すること |

---

#### TC-IT-DP-006: `import_records` — `from_directory_with_busy_timeout` repo で save() 経路が正常完了する（リグレッション確認）

| 項目 | 内容 |
|------|------|
| テストID | TC-IT-DP-006 |
| 対応要件 | REQ-DP-009 |
| 対応受入基準 | —（Issue #146 服部平次指摘対応: `AtomicWriteSession` `busy_timeout` 伝搬 + `map_err(PersistenceError::from)` リグレッション確認）|
| 種別 | 正常系（リグレッション） |
| 前提条件 | vault A に Text レコードが 1 件存在し、export 済みの JSON ファイルがある |
| 操作 | 1. vault A から `export_records` で `export.json` を生成する / 2. `SqliteVaultRepository::from_directory_with_busy_timeout(path, Duration::from_secs(2))` で vault B の repo を取得する（ロック競合なし）/ 3. `import_records(&repo_b, &args, now)` を呼ぶ |
| 期待結果 | `Ok(ImportSummary { added: 1, .. })` が返ること。`save()` 経路（`AtomicWriteSession::new(busy_timeout=Some(2s))` → `finalize`）が正常完了したことを示す |
| 補足 | TC-IT-DP-005 が `load()` 経路の SQLITE_BUSY を検証するのに対し、本テストは `save()` 経路のリグレッションを確認する。`vault.db.new` は新規ファイルのため外部接続によるロック競合は発生しない。`busy_timeout` が `AtomicWriteSession::new` に正しく伝搬されても通常 save が壊れないことを証明する |
