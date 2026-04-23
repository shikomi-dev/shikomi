# テスト設計書 — cli-vault-commands / E2E

> `index.md` の §2 索引からの分割ファイル。E2E テストの全 TC と証跡方針を扱う。

## 1. ツール選択根拠

| 候補 | 採用可否 | 理由 |
|------|---------|------|
| `assert_cmd` + `predicates` + `tempfile` | **採用** | テスト戦略ガイドの「CLI ツール → bash で stdout/stderr/exit code を assert」の Rust 慣習版。Cargo workspace で完結し、追加バイナリ不要。`assert_cmd::Command::cargo_bin("shikomi")` で本物のバイナリを呼ぶため完全ブラックボックス |
| Playwright | 不採用 | Web UI 用ツール。CLI には過剰 |
| 素の `std::process::Command` | 不採用 | アサーション再発明。`assert_cmd` の `predicates` 統合の方が読みやすい |
| `expectrl` / `rexpect` | **限定採用** | TTY 擬似が必要な TC-E2E-030 のみ。CI で動かない場合は `#[ignore]` フォールバック |

## 2. テスト共通前提

**全テストケースで以下を共通前提とする**:

- `tempfile::TempDir` を作成し `--vault-dir <tempdir>` を指定（**`SHIKOMI_VAULT_DIR` 環境変数は使わない**。テスト並列実行時のレースを避ける、詳細設計 §clap-config.md 採用案 B 準拠）
- `assert_cmd::Command::cargo_bin("shikomi")` で本体バイナリを起動
- secret マーカーは `SECRET_TEST_VALUE` 固定文字列を使い、全 stdout/stderr に `predicates::str::contains("SECRET_TEST_VALUE").not()` を加える（監査横串）
- exit code は `.code(N)` で厳密一致。`.success()` は exit 0、`.failure()` は non-zero のみ

---

## 3. `list` 系

### TC-E2E-001: `list` — 空 vault

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1（部分） |
| 対応 REQ | REQ-CLI-001 |
| 種別 | 正常系（境界値: 0 件） |
| 前提条件 | `add` で vault.db を生成済み、その後全件 `remove` 済み、または init 直後で 0 件 |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 0 / stdout に「(no records)」相当の i18n メッセージ（または空のヘッダ表のみ）/ stderr 空 |
| 検証アサート | `.success()`, `.stdout(predicates::str::contains("no records").or(predicates::str::is_empty()))` |

### TC-E2E-002: `list` — 1 件（Text のみ）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1（部分）, 2 |
| 対応 REQ | REQ-CLI-001, REQ-CLI-002 |
| 種別 | 正常系 |
| 前提条件 | `add --kind text --label "L1" --value "V1"` 実行済み |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 0 / stdout に `text` `L1` `V1` を含む 1 行表示 / カラムヘッダ `ID KIND LABEL VALUE` を含む |

### TC-E2E-003: `list` — 複数件（Text + Secret 混在、Secret マスク確認）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1, 3（部分） |
| 対応 REQ | REQ-CLI-001, REQ-CLI-007 |
| 種別 | 正常系 |
| 前提条件 | `add --kind text --label "T1" --value "PUBLIC_VAL"` と `add --kind secret --label "S1" --stdin` で `"SECRET_TEST_VALUE"` 投入済み |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 0 / stdout に `PUBLIC_VAL` を含み、`SECRET_TEST_VALUE` を含まず、`****` を含む |
| 検証アサート | `.stdout(contains("PUBLIC_VAL").and(contains("****")).and(contains("SECRET_TEST_VALUE").not()))` |

---

## 4. `add` 系

### TC-E2E-010: `add --kind text` → `list` ラウンドトリップ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 2 |
| 対応 REQ | REQ-CLI-002 |
| 種別 | 正常系 |
| 前提条件 | 空 vault dir |
| 操作 | (1) `shikomi --vault-dir <tmp> add --kind text --label "L" --value "V"` → stdout から `added: <uuid>` の uuid を抽出 / (2) `shikomi --vault-dir <tmp> list` |
| 期待結果 | (1) exit 0、stdout に `added: ` + UUIDv7 形式 / (2) exit 0、stdout に当該 uuid と `L` `V` を含む |

### TC-E2E-011: `add --kind secret --stdin` で secret 露出ゼロ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 3 |
| 対応 REQ | REQ-CLI-002, REQ-CLI-007 |
| 種別 | 正常系（重要セキュリティ） |
| 前提条件 | 空 vault dir |
| 操作 | (1) `shikomi --vault-dir <tmp> add --kind secret --label "S" --stdin`、stdin に `SECRET_TEST_VALUE\n` を pipe / (2) `shikomi --vault-dir <tmp> list` |
| 期待結果 | (1) exit 0、stdout に `added: <uuid>`、**stdout / stderr のいずれにも `SECRET_TEST_VALUE` が含まれない** / (2) stdout に `****` を含み、`SECRET_TEST_VALUE` を含まない |
| 検証アサート | `.stdout(contains("SECRET_TEST_VALUE").not())`, `.stderr(contains("SECRET_TEST_VALUE").not())` を両方の実行に対して |

### TC-E2E-012: `add --kind secret --value` で警告 + exit 0

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 4 |
| 対応 REQ | REQ-CLI-002, MSG-CLI-050 |
| 種別 | 正常系（警告） |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind secret --label "S" --value "P"` |
| 期待結果 | exit 0 / stdout に `added: <uuid>` / **stderr に `warning:` を含み、`shell history` 相当の警告文** / stderr に `P`（投入 secret 値）が含まれない |

### TC-E2E-013: `add` — `--value` と `--stdin` 同時指定拒否（併用違反）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5（部分、`add` でも適用） |
| 対応 REQ | REQ-CLI-002, MSG-CLI-100 |
| 種別 | 異常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind text --label "L" --value "V" --stdin` |
| 期待結果 | exit 1 / stderr に `error:` + `--value` + `--stdin` 併用禁止文 + `hint:` 行 / stdout 空 |

### TC-E2E-014: `add` — `--value` も `--stdin` も無し

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5（補足） |
| 対応 REQ | REQ-CLI-002 |
| 種別 | 異常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind text --label "L"` |
| 期待結果 | exit 1 / stderr に `error:` + 値指定が必要である旨 |

### TC-E2E-015: `add` — 不正ラベル（空文字）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: REQ-CLI-002 + MSG-CLI-101） |
| 対応 REQ | REQ-CLI-002, MSG-CLI-101 |
| 種別 | 異常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind text --label "" --value "V"` |
| 期待結果 | exit 1 / stderr に `invalid label` |

---

## 5. `edit` 系

### TC-E2E-020: `edit --label NEW` で label のみ更新 → `list` で反映

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5 |
| 対応 REQ | REQ-CLI-003 |
| 種別 | 正常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | (1) `shikomi --vault-dir <tmp> edit --id <uuid> --label "NEW_L"` / (2) `list` |
| 期待結果 | (1) exit 0、stdout に `updated: <uuid>` / (2) stdout に `NEW_L` |

### TC-E2E-021: `edit` — `--value` と `--stdin` 併用拒否

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5 |
| 対応 REQ | REQ-CLI-003, MSG-CLI-100 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id <uuid> --value "X" --stdin` |
| 期待結果 | exit 1 / stderr に併用禁止文 |

### TC-E2E-022: `edit` — フラグ全未指定（更新内容なし）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5（補足） |
| 対応 REQ | REQ-CLI-003 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id <uuid>` |
| 期待結果 | exit 1 / stderr に「少なくとも 1 つの更新フィールドが必要」 |

### TC-E2E-023: `edit` — 不正 UUID

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: MSG-CLI-102） |
| 対応 REQ | REQ-CLI-003, MSG-CLI-102 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id "not-a-uuid" --label "L"` |
| 期待結果 | exit 1 / stderr に `invalid record id` |

### TC-E2E-024: `edit` — 存在しない id

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: MSG-CLI-106） |
| 対応 REQ | REQ-CLI-003, MSG-CLI-106 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id "018f0000-0000-7000-8000-000000000000" --label "L"` |
| 期待結果 | exit 1 / stderr に `record not found` |

### TC-E2E-025: `edit --kind` は Phase 1 スコープ外（clap レベルで拒否）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （詳細設計 §public-api.md / requirements.md REQ-CLI-003 注記） |
| 対応 REQ | REQ-CLI-003 |
| 種別 | 異常系（Phase 1 スコープ外明示） |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id <uuid> --kind secret` |
| 期待結果 | exit 1（clap `unexpected argument` または `error: unknown argument '--kind'` を含む）/ stdout 空 |
| **注記** | 詳細設計で `EditArgs` / `EditInput` から `kind` フィールド自体を削除済。よって clap の unknown arg エラーで拒否される。UsageError をアプリ層で出すより clap の自動エラーの方が UX 的に明確 |

---

## 6. `remove` 系

### TC-E2E-030: `remove` — TTY 確認プロンプト（`y` で削除）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 6 |
| 対応 REQ | REQ-CLI-004, REQ-CLI-011 |
| 種別 | 正常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `expectrl` または `rexpect` クレートで擬似 TTY を起動 → `shikomi --vault-dir <tmp> remove --id <uuid>` 実行 → プロンプト出現を待って `y\n` 送信 |
| 期待結果 | exit 0 / stdout に `Delete record` プロンプトと `removed: <uuid>` |
| **注記** | 擬似 TTY が CI で動作しない場合、`#[ignore]` 付与してローカル実行のみとする（`cargo test --test e2e_remove -- --ignored`）。主受入基準は TC-E2E-031 で完全自動カバー |

### TC-E2E-031: `remove` — 非 TTY で `--yes` 無し → exit 1（主検証）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 6（**主検証**） |
| 対応 REQ | REQ-CLI-011, MSG-CLI-105 |
| 種別 | 異常系（重要） |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `assert_cmd::Command::cargo_bin("shikomi").args(["--vault-dir", ..., "remove", "--id", uuid]).stdin(Stdio::piped()).output()`（パイプで非 TTY 化） |
| 期待結果 | exit 1 / stderr に `refusing to delete without --yes` / 当該レコードが残存（補助検証として `list` で確認） |

### TC-E2E-032: `remove --yes` で確認なし削除

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 7 |
| 対応 REQ | REQ-CLI-004 |
| 種別 | 正常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | (1) `shikomi --vault-dir <tmp> remove --id <uuid> --yes` / (2) `list` |
| 期待結果 | (1) exit 0、stdout に `removed: <uuid>` / (2) stdout に当該 uuid 不在 |

### TC-E2E-033: `remove --yes` で存在しない id

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: MSG-CLI-106） |
| 対応 REQ | REQ-CLI-004 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> remove --id "018f0000-0000-7000-8000-000000000000" --yes` |
| 期待結果 | exit 1 / stderr に `record not found` |

---

## 7. 暗号化 vault（Fail Fast）

### TC-E2E-040: 暗号化 vault → exit 3（`list` で検証）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 8 |
| 対応 REQ | REQ-CLI-009, MSG-CLI-103 |
| 種別 | 異常系（重要） |
| 前提条件 | `tests/common/fixtures.rs::create_encrypted_vault(&tempdir)` でヘッダ `protection_mode = Encrypted` の最小 SQLite ファイルを生成済み |
| 操作 | `shikomi --vault-dir <fixture-tmp> list` |
| 期待結果 | exit 3 / stderr に `encryption is not yet supported` と `shikomi vault decrypt` 誘導文 / vault 内容は触られない |

### TC-E2E-041: 暗号化 vault → exit 3（`add` / `edit` / `remove` 全サブコマンド）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 8 |
| 対応 REQ | REQ-CLI-009 |
| 種別 | 異常系（パラメタライズ: 3 サブコマンド） |
| 前提条件 | TC-E2E-040 と同じフィクスチャ |
| 操作 | (a) `add --kind text --label X --value Y` / (b) `edit --id <uuid> --label L` / (c) `remove --id <uuid> --yes` |
| 期待結果 | (a)(b)(c) いずれも exit 3 / stderr に MSG-CLI-103 / vault 内容は触られない |

---

## 8. vault 未初期化

### TC-E2E-050: vault 未初期化 — `list` は exit 1

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-CLI-010, MSG-CLI-104 |
| 種別 | 異常系 |
| 前提条件 | 空の `tempdir`（vault.db 不在） |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 1 / stderr に `vault not initialized` + `shikomi add` 誘導 |

### TC-E2E-051: vault 未初期化 — `add` で自動初期化（成功）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-CLI-010, MSG-CLI-005 |
| 種別 | 正常系 |
| 前提条件 | 空の `tempdir` |
| 操作 | (1) `shikomi --vault-dir <tmp> add --kind text --label "L" --value "V"` / (2) `list` |
| 期待結果 | (1) exit 0、stdout に `initialized plaintext vault at <path>` と `added: <uuid>` の **両方** / (2) exit 0、`L` `V` を含む |

### TC-E2E-052: vault 未初期化 — `edit` / `remove` も exit 1

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-CLI-010, MSG-CLI-104 |
| 種別 | 異常系（パラメタライズ: `edit`, `remove`） |
| 前提条件 | 空の `tempdir` |
| 操作 | (a) `edit --id <any> --label "L"` / (b) `remove --id <any> --yes` |
| 期待結果 | (a)(b) ともに exit 1、stderr に `vault not initialized` |

---

## 9. vault パス優先順位（REQ-CLI-005）

### TC-E2E-060: `--vault-dir` フラグが env var より優先

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10 |
| 対応 REQ | REQ-CLI-005 |
| 種別 | 正常系（境界: 優先順位） |
| 前提条件 | `tempdir A` と `tempdir B` を用意、`A` に 1 件 `add` 済み、`B` は空 |
| 操作 | `.env("SHIKOMI_VAULT_DIR", B).args(["--vault-dir", A, "list"])` |
| 期待結果 | exit 0 / stdout に `A` に追加したレコードを含む（B のレコード参照ではない） |

### TC-E2E-061: env var が OS デフォルトより優先

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10 |
| 対応 REQ | REQ-CLI-005 |
| 種別 | 正常系 |
| 前提条件 | `tempdir A` に 1 件 `add` 済み |
| 操作 | `.env("SHIKOMI_VAULT_DIR", A).args(["list"])`（`--vault-dir` 不指定） |
| 期待結果 | exit 0 / stdout に `A` のレコード |

### TC-E2E-062: フラグも env var もない → OS デフォルト解決

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10（境界: フォールバック） |
| 対応 REQ | REQ-CLI-005 |
| 種別 | 異常系（テスト環境では vault 未存在前提） |
| 前提条件 | `.env_remove("SHIKOMI_VAULT_DIR")`、`HOME` を `tempdir` に上書きして OS デフォルトを `tempdir/share/shikomi` 等に固定 |
| 操作 | `shikomi list` |
| 期待結果 | exit 1（vault 未初期化）または exit 0（空 vault）。**OS デフォルトの計算が正常に動いた**ことが本ケースの主目的（パニック・パス解決失敗の exit 2 にならないこと） |

---

## 10. i18n（REQ-CLI-008）

### TC-E2E-070: `LANG=ja_JP.UTF-8` で英日 2 段表示

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 11 |
| 対応 REQ | REQ-CLI-008 |
| 種別 | 正常系 |
| 前提条件 | `add` で 1 件追加済み（または vault 未初期化でエラーを誘発） |
| 操作 | `.env("LANG", "ja_JP.UTF-8").args(["--vault-dir", <missing>, "list"])` |
| 期待結果 | exit 1 / stderr に英語原文（`error: vault not initialized`）と日本語訳（`error: vault が初期化されていません`）が **両方** 含まれる |

### TC-E2E-071: `LANG=C` で英語のみ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 11 |
| 対応 REQ | REQ-CLI-008 |
| 種別 | 正常系 |
| 前提条件 | TC-E2E-070 と同じ |
| 操作 | `.env("LANG", "C").args(["--vault-dir", <missing>, "list"])` |
| 期待結果 | exit 1 / stderr に英語原文を含み、**日本語文字を一切含まない**（`predicates::str::contains("vault が").not()`） |

---

## 11. ペルソナシナリオ（統合）

### TC-E2E-100: SCN-A 山田美咲ライフサイクル統合

| 項目 | 内容 |
|------|------|
| 対応シナリオ | SCN-A |
| 対応 REQ | REQ-CLI-001〜004, 007, 010, 012 |
| 種別 | 統合シナリオ（複数 REQ 横断） |
| 前提条件 | 空 `tempdir` |
| 操作 | (1) `add --kind text --label "SSH: prod" --value "ssh -J ..."` / (2) `add --kind secret --label "AWS_KEY" --stdin` で `SECRET_TEST_VALUE` / (3) `list` で 2 件確認・Secret マスク確認 / (4) `edit --id <text-uuid> --label "SSH: prod-v2"` / (5) `list` で更新確認 / (6) `remove --id <secret-uuid> --yes` / (7) `list` で 1 件残存確認 |
| 期待結果 | 全ステップ exit 0、最終 `list` に `SSH: prod-v2` のみ、`SECRET_TEST_VALUE` はどこにも露出しない |

### TC-E2E-101: SCN-B 田中俊介初心者保護

| 項目 | 内容 |
|------|------|
| 対応シナリオ | SCN-B |
| 対応 REQ | REQ-CLI-001, REQ-CLI-011, REQ-CLI-008 |
| 種別 | 統合シナリオ |
| 前提条件 | `add` 済みの 1 件存在、`LANG=ja_JP.UTF-8` |
| 操作 | (1) `shikomi list` で日本語併記表示確認 / (2) `shikomi remove --id <uuid>` を非 TTY で実行（`--yes` 忘れた田中の誤操作） |
| 期待結果 | (1) 日本語併記出力 / (2) exit 1、削除は実行されず、日本語ヒント「--yes を付けて再実行してください」が出力される / 続けて `list` で当該レコード残存 |

### TC-E2E-102: SCN-C 自己記述性

| 項目 | 内容 |
|------|------|
| 対応シナリオ | SCN-C |
| 対応 REQ | clap 設定（詳細設計 §clap-config.md） |
| 種別 | 正常系（自己文書化） |
| 前提条件 | なし |
| 操作 | (a) `shikomi --help` / (b) `shikomi --version` / (c) `shikomi add --help` / (d) `shikomi edit --help` で `--kind` が**含まれない**（Phase 1 スコープ外確認） |
| 期待結果 | (a) exit 0、stdout に 4 サブコマンド全列挙 / (b) exit 0、stdout に `CARGO_PKG_VERSION` と一致する文字列 / (c) exit 0、stdout に `--kind` `--label` `--value` `--stdin` / (d) exit 0、stdout に `--kind` を**含まない** |

---

## 12. E2E テストの証跡

実行結果を Markdown レポートにまとめ、`/app/shared/attachments/マユリ/cli-vault-commands-e2e-report.md` として Discord に添付する。レポートに含めるもの:

- 各 TC の `assert_cmd` 実行コマンド
- stdout / stderr / exit code（テキストで全文）
- `cargo test --test 'e2e_*'` の集計（`X passed; 0 failed`）
- 失敗時は失敗 TC の `assert_cmd` 出力 diff
- `SECRET_TEST_VALUE` の stdout/stderr 全文 grep 結果（不在保証の証跡）

擬似 TTY ケース（TC-E2E-030）が CI 環境で動作しない場合、ローカル実行のスクリーンキャスト or `script(1)` ログを証跡として併載する。

---

*この文書は `index.md` の分割成果。トレーサビリティ・モック方針・カバレッジ基準は `index.md` を参照*
