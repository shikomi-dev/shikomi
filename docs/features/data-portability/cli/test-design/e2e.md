# E2Eテスト設計 — data-portability / cli

<!-- feature: data-portability / sub-feature: cli / Issue #141 / Issue #146 -->
<!-- 配置先: docs/features/data-portability/cli/test-design/e2e.md -->
<!-- 親: ../test-design.md（インデックス）-->
<!-- Vモデル対応: 階層 2（feature-spec.md AC-DP-06〜10 → E2Eテスト）-->

## 5.1 E2Eテスト — `shikomi export` / `shikomi import`（AC-DP-06〜10、最優先）

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

#### TC-E2E-DP-013: vault ロック中（daemon 相当）に import → 2 秒後 MSG-CLI-146 exit 1

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-013 |
| 対応要件 | REQ-DP-009 |
| 対応受入基準 | —（Issue #146 E2E 保証）|
| 種別 | 異常系 |
| 前提条件 | `<TempDir>/vault.db` が `rusqlite::Connection` の `BEGIN EXCLUSIVE` で排他ロック済み。ソース vault にレコードが 1 件存在し export.json が用意されている |
| 操作 | 1. ソース vault に `shikomi add` でレコードを追加し export する / 2. デスト vault の vault.db を `rusqlite::Connection::open` で作成し chmod 0600 / 3. `BEGIN EXCLUSIVE` で排他ロック / 4. `shikomi import --input export.json --vault-dir dest_dir` を実行 |
| 期待結果 | (1) exit code 1 / (2) stderr に `"vault is in use by shikomi-daemon"` が含まれる（MSG-CLI-146）/ (3) 実行に ~2 秒かかる（busy_timeout 2000ms 相当）|
| 備考 | `#[ignore = "slow: waits ~2s for SQLITE_BUSY busy_timeout"]` — 通常 CI では `--include-ignored` で実行 |

---

#### TC-E2E-DP-014: 短時間ロック（< 2s）解放後に import が透過的に成功する

| 項目 | 内容 |
|------|------|
| テストID | TC-E2E-DP-014 |
| 対応要件 | REQ-DP-009 |
| 対応受入基準 | —（Issue #146 busy_timeout リトライ成功保証）|
| 種別 | 正常系 |
| 前提条件 | デスト vault の vault.db が `rusqlite::Connection` の `BEGIN EXCLUSIVE` で排他ロック済み。ソース vault に export.json が用意されている |
| 操作 | 1. ソース vault に `shikomi add` でレコードを追加し export する / 2. デスト vault を `shikomi add` で初期化（正規スキーマ）/ 3. `BEGIN EXCLUSIVE` で排他ロック / 4. `shikomi import` を `std::process::Command::spawn()` で非ブロッキング起動 / 5. 300ms 後にロック解放（`drop(lock_conn)`）/ 6. `child.wait_with_output()` で完了待機 |
| 期待結果 | (1) exit code 0 / (2) stdout に `"imported"` が含まれる（busy_timeout(2000ms) 内にリトライ成功）|
| 備考 | `#[ignore = "slow: holds EXCLUSIVE lock 300ms"]` — 通常 CI では `--include-ignored` で実行 |
