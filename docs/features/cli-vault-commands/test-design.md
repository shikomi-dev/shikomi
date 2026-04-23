# テスト設計書 — cli-vault-commands（`shikomi list / add / edit / remove`）

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | cli-vault-commands（`shikomi-cli` に vault CRUD 4 サブコマンドを実装） |
| 対象 PR | [#18](https://github.com/shikomi-dev/shikomi/pull/18) |
| 対象ブランチ | `feat/cli-vault-commands-design` → `develop` |
| 上位設計 | `requirements-analysis.md`（受入基準 16 項目）/ `requirements.md`（REQ-CLI-001〜012）/ `basic-design.md`（モジュール構成）/ `detailed-design.md`（クラス・公開 API・テスト観点注記） |
| MVP フェーズ | Phase 1（CLI 直結）— `process-model.md` §4.1.1 で正当化 |
| 対応 vault モード | 平文モードのみ。暗号化 vault は Fail Fast（exit 3）で検証する |
| テスト実行タイミング | 実装担当（坂田銀時）が `feat/cli-vault-commands-impl` に積み上げた直後、`develop` マージ前 |
| Vモデル対応 | E2E ↔ 受入基準（要件定義） / 結合 ↔ UseCase 単位（基本設計） / ユニット ↔ Presenter・Mapper・Resolver（詳細設計） |

> **テスト戦略の核**: 完璧な設計など存在しない——だからこそ実験するのだヨ。E2E から下りる「上流優先」順で書き、上位ケースの網羅を最優先とする。カバレッジ数値目標は受入基準ガイド §13 に準拠して `cargo llvm-cov` で 80% を測るが、**判定基準は受入基準 16 項目の網羅**である。

---

## 2. テストレベル戦略

| 種別 | 対象 | 視点 | モック | 検証スタイル | テスト配置（Rust 慣習） |
|------|------|------|-------|------------|----------------------|
| **E2E** | `shikomi` バイナリ全体 | 完全ブラックボックス（ユーザが叩くコマンドと同じ） | なし（`SqliteVaultRepository` 実物 + `tempfile::TempDir` で独立 vault） | 振る舞い検証（stdout / stderr / exit code / vault 内容のラウンドトリップ） | `crates/shikomi-cli/tests/e2e_*.rs`（`assert_cmd` + `predicates` + `tempfile`） |
| **結合** | `usecase::*` 関数（CLI 層エントリポイント相当） | 半ブラックボックス | DB は実接続（`SqliteVaultRepository` + `tempfile`）、外部 API は無し | 契約検証（戻り値型・`CliError` バリアント・`save()` 後のラウンドトリップ） | `crates/shikomi-cli/tests/it_usecase_*.rs` |
| **ユニット** | `presenter::*` / `error::ExitCode::from` / `Locale::detect_from_env` / `RecordView::from_record` / `io::paths::resolve_vault_dir` 等の pure function | ホワイトボックス | I/O バウンダリ全部（環境変数・ファイル）— ただし pure function は素のまま検証 | 1 テスト 1 アサーション原則。`test_<対象>_<状況>_<期待>` 命名 | 各モジュール内 `#[cfg(test)] mod tests` |

**Rust 慣習との整合**:
- ユニットテストは `#[cfg(test)] mod tests` でソースに埋め込む（テスト戦略ガイド「Rust: unit test は `#[cfg(test)]` でソースモジュール内」準拠）
- 結合テストと E2E テストはともに `crates/shikomi-cli/tests/` 配下に配置（言語慣習で `tests/` 直下が integration test）。**ファイル名 prefix で分離**: `e2e_*.rs` は `assert_cmd` で本体プロセス起動、`it_usecase_*.rs` は `shikomi_cli` を lib として使う……が **`shikomi-cli` は bin crate であり lib を公開しない**ため、結合テストは `#[path = "../src/usecase/list.rs"]` の `mod` インポート、または **`crates/shikomi-cli/Cargo.toml` に `[lib]` セクションを追加して bin と lib の両 target にする** リファクタを実装担当に提案する（後述 §10「実装担当への引き継ぎ」）

---

## 3. テストマトリクス（トレーサビリティ）

### 3.1 受入基準 ↔ REQ ↔ TC 対応表

| 受入基準 # | 要約 | 関連 REQ | 関連クラス／メソッド | TC-ID（主） | TC-ID（補助） |
|----------|------|---------|------------------|-----------|------------|
| 1 | `list` 空 / 1 件 / 複数件、Secret マスク | REQ-CLI-001, REQ-CLI-007 | `usecase::list::list_records`, `presenter::list::render_list`, `RecordView::from_record` | TC-E2E-001, TC-E2E-002, TC-E2E-003 | TC-IT-001, TC-UT-010, TC-UT-011 |
| 2 | `add --kind text` → `list` で反映 | REQ-CLI-002, REQ-CLI-001 | `usecase::add::add_record` | TC-E2E-010 | TC-IT-010 |
| 3 | `add --kind secret --stdin` で stdout/stderr に secret 一切露出禁止 | REQ-CLI-002, REQ-CLI-007 | `usecase::add::add_record`, `io::terminal::read_password`, `presenter::list::render_list` | TC-E2E-011 | TC-IT-011, TC-UT-012 |
| 4 | `add --kind secret --value` で警告だが exit 0 | REQ-CLI-002, MSG-CLI-050 | `presenter::warning::render_shell_history_warning` | TC-E2E-012 | TC-UT-013 |
| 5 | `edit --label NEW`、`--value` と `--stdin` 併用拒否 | REQ-CLI-003 | `usecase::edit::edit_record`, clap `try_parse` 後の併用検証 | TC-E2E-020, TC-E2E-021 | TC-IT-020, TC-IT-021 |
| 6 | `remove` TTY 確認、非 TTY + `--yes` 無しなら exit 1 | REQ-CLI-004, REQ-CLI-011 | `io::terminal::is_stdin_tty`, `RemoveInput::confirmed` | TC-E2E-030, TC-E2E-031 | TC-IT-030 |
| 7 | `remove --yes` で確認なし削除 | REQ-CLI-004 | `usecase::remove::remove_record` | TC-E2E-032 | TC-IT-031 |
| 8 | 暗号化 vault → exit 3 + MSG-CLI-103 | REQ-CLI-009 | UseCase 全関数の `protection_mode` チェック | TC-E2E-040, TC-E2E-041 | TC-IT-040 |
| 9 | vault 未初期化: `list/edit/remove` exit 1、`add` 自動作成 | REQ-CLI-010 | `usecase::*` の `exists()` 分岐 | TC-E2E-050, TC-E2E-051, TC-E2E-052 | TC-IT-050 |
| 10 | `SHIKOMI_VAULT_DIR` / `--vault-dir` / OS デフォルト 優先順位 | REQ-CLI-005 | `io::paths::resolve_vault_dir` | TC-E2E-060, TC-E2E-061, TC-E2E-062 | TC-UT-020, TC-UT-021, TC-UT-022 |
| 11 | `MSG-CLI-xxx` 英日 2 段、`LANG=C` 英語のみ | REQ-CLI-008 | `presenter::error::render_error`, `Locale::detect_from_env` | TC-E2E-070, TC-E2E-071 | TC-UT-030〜TC-UT-038, TC-UT-040〜TC-UT-041 |
| 12 | `cargo clippy / fmt / deny` 全 pass | — | リポジトリ全体 | TC-CI-001, TC-CI-002, TC-CI-003 | — |
| 13 | `cargo test -p shikomi-cli` 全 pass、行カバレッジ 80%+ | — | テストスイート全体 | TC-CI-004, TC-CI-005 | — |
| 14 | `process-model.md` §4.1 にフェーズ区分追記済み | — | ドキュメント | TC-CI-010 | — |
| 15 | `shikomi-cli/src/` が `presenter/` / `usecase/` / 共通 main.rs の 3 層構造 | REQ-CLI-012 | ディレクトリ構造 | TC-CI-011 | — |
| 16 | `SqliteVaultRepository` 具体型参照は `main.rs` のみ | REQ-CLI-012 | コード grep | TC-CI-012 | — |

### 3.2 REQ-CLI-006（終了コード契約） / REQ-CLI-007（マスキング）の横断検証

終了コード契約は全 E2E ケースで `assert_cmd::assert::Assert::code(...)` により検証する（横串検証）。
secret マスキングは TC-E2E-011 が主検証だが、TC-E2E-001〜003（`list` 系）でも `predicates::str::contains("SECRET_TEST_VALUE").not()` を全アサートに含める。

---

## 4. E2E テスト設計

### 4.1 ツール選択根拠

| 候補 | 採用可否 | 理由 |
|------|---------|------|
| `assert_cmd` + `predicates` + `tempfile` | **採用** | テスト戦略ガイドの「CLI ツール → bash で stdout/stderr/exit code を assert」の Rust 慣習版。Cargo workspace で完結し、追加バイナリ不要。`assert_cmd::Command::cargo_bin("shikomi")` で本物のバイナリを呼ぶため完全ブラックボックス |
| Playwright | 不採用 | Web UI 用ツール。CLI には過剰 |
| 素の `std::process::Command` | 不採用 | アサーション再発明。`assert_cmd` の `predicates` 統合の方が読みやすい |

### 4.2 ペルソナシナリオ設計

`requirements-analysis.md` §ペルソナのプライマリ／セカンダリに対応する**ユーザ視点シナリオ**として組む。

| シナリオ ID | ペルソナ | シナリオ概要 | 対応 TC |
|------------|---------|------------|---------|
| SCN-A | 山田 美咲（FE エンジニア、CLI 主、開発機） | 開発中に SSH コマンド断片を `add` し、後日 `list` → `edit` → `remove` する一連のライフサイクル | TC-E2E-100（ライフサイクル統合） |
| SCN-B | 田中 俊介（営業、GUI 主、CLI 非常用） | コマンドプロンプトを初めて起動し、`shikomi list` でエントリ確認、誤削除を非 TTY で防げる | TC-E2E-101（初心者保護） |
| SCN-C | 野木 拓海（後続実装担当） | `shikomi --help` のサブコマンド一覧確認、`--version` 出力、`-h` で各サブヘルプを参照 | TC-E2E-102（自己記述性） |

### 4.3 テストケース一覧（受入基準対応 + ペルソナシナリオ）

> 全テストケースで前提条件「`tempfile::TempDir` を作成し `--vault-dir <tempdir>` を指定」を共通とする。**`SHIKOMI_VAULT_DIR` 環境変数は使わない**（テスト並列実行時のレースを避ける、詳細設計 §6 採用案 B 準拠）。

#### TC-E2E-001: `list` — 空 vault

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1（部分） |
| 対応 REQ | REQ-CLI-001 |
| 種別 | 正常系（境界値: 0 件） |
| 前提条件 | `add` で vault.db を生成済み、その後一意なケースとして「全件 remove 済み」または「init 直後」で 0 件 |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit code = 0 / stdout に「(no records)」相当の i18n メッセージ（または空のヘッダ表のみ）/ stderr 空 |
| 検証アサート | `.success()`, `.stdout(predicates::str::contains("no records").or(predicates::str::is_empty()))` |

#### TC-E2E-002: `list` — 1 件（Text のみ）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1（部分）, 2 |
| 対応 REQ | REQ-CLI-001, REQ-CLI-002 |
| 種別 | 正常系 |
| 前提条件 | `add --kind text --label "L1" --value "V1"` 実行済み |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 0 / stdout に `text` `L1` `V1` を含む 1 行表示 / カラムヘッダ `ID KIND LABEL VALUE` を含む |

#### TC-E2E-003: `list` — 複数件（Text + Secret 混在、Secret マスク確認）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1, 3（部分） |
| 対応 REQ | REQ-CLI-001, REQ-CLI-007 |
| 種別 | 正常系 |
| 前提条件 | `add --kind text --label "T1" --value "PUBLIC_VAL"` と `add --kind secret --label "S1" --stdin` で `"SECRET_TEST_VALUE"` 投入済み |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 0 / stdout に `PUBLIC_VAL` を含み、`SECRET_TEST_VALUE` を含まず、`****` を含む |
| 検証アサート | `.stdout(contains("PUBLIC_VAL").and(contains("****")).and(contains("SECRET_TEST_VALUE").not()))` |

#### TC-E2E-010: `add --kind text` → `list` ラウンドトリップ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 2 |
| 対応 REQ | REQ-CLI-002 |
| 種別 | 正常系 |
| 前提条件 | 空 vault dir |
| 操作 | (1) `shikomi --vault-dir <tmp> add --kind text --label "L" --value "V"` → stdout から `added: <uuid>` の uuid を抽出 / (2) `shikomi --vault-dir <tmp> list` |
| 期待結果 | (1) exit 0、stdout に `added: ` + UUIDv7 形式 / (2) exit 0、stdout に当該 uuid と `L` `V` を含む |

#### TC-E2E-011: `add --kind secret --stdin` で secret 露出ゼロ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 3 |
| 対応 REQ | REQ-CLI-002, REQ-CLI-007 |
| 種別 | 正常系（重要セキュリティ） |
| 前提条件 | 空 vault dir |
| 操作 | (1) `shikomi --vault-dir <tmp> add --kind secret --label "S" --stdin`、stdin に `SECRET_TEST_VALUE\n` を pipe / (2) `shikomi --vault-dir <tmp> list` |
| 期待結果 | (1) exit 0、stdout に `added: <uuid>`、**stdout / stderr のいずれにも `SECRET_TEST_VALUE` が含まれない** / (2) stdout に `****` を含み、`SECRET_TEST_VALUE` を含まない |
| 検証アサート | `.stdout(contains("SECRET_TEST_VALUE").not())`, `.stderr(contains("SECRET_TEST_VALUE").not())` を両方の実行に対して |

#### TC-E2E-012: `add --kind secret --value` で警告 + exit 0

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 4 |
| 対応 REQ | REQ-CLI-002, MSG-CLI-050 |
| 種別 | 正常系（警告） |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind secret --label "S" --value "P"` |
| 期待結果 | exit 0 / stdout に `added: <uuid>` / **stderr に `warning:` を含み、`shell history` 相当の警告文** / stderr に `P`（投入 secret 値）が含まれない |

#### TC-E2E-013: `add` — `--value` と `--stdin` 同時指定拒否（併用違反）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5（部分、`add` でも適用） |
| 対応 REQ | REQ-CLI-002, MSG-CLI-100 |
| 種別 | 異常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind text --label "L" --value "V" --stdin` |
| 期待結果 | exit 1 / stderr に `error:` + `--value` + `--stdin` 併用禁止文 + `hint:` 行 / stdout 空 |

#### TC-E2E-014: `add` — `--value` も `--stdin` も無し

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5（補足） |
| 対応 REQ | REQ-CLI-002 |
| 種別 | 異常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind text --label "L"` |
| 期待結果 | exit 1 / stderr に `error:` + 値指定が必要である旨 |

#### TC-E2E-015: `add` — 不正ラベル（空文字）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: REQ-CLI-002 + MSG-CLI-101） |
| 対応 REQ | REQ-CLI-002, MSG-CLI-101 |
| 種別 | 異常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi --vault-dir <tmp> add --kind text --label "" --value "V"` |
| 期待結果 | exit 1 / stderr に `invalid label` |

#### TC-E2E-020: `edit --label NEW` で label のみ更新 → `list` で反映

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5 |
| 対応 REQ | REQ-CLI-003 |
| 種別 | 正常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | (1) `shikomi --vault-dir <tmp> edit --id <uuid> --label "NEW_L"` / (2) `list` |
| 期待結果 | (1) exit 0、stdout に `updated: <uuid>` / (2) stdout に `NEW_L` |

#### TC-E2E-021: `edit` — `--value` と `--stdin` 併用拒否

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5 |
| 対応 REQ | REQ-CLI-003, MSG-CLI-100 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id <uuid> --value "X" --stdin` |
| 期待結果 | exit 1 / stderr に併用禁止文 |

#### TC-E2E-022: `edit` — フラグ全未指定（更新内容なし）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5（補足） |
| 対応 REQ | REQ-CLI-003 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id <uuid>` |
| 期待結果 | exit 1 / stderr に「少なくとも 1 つの更新フィールドが必要」 |

#### TC-E2E-023: `edit` — 不正 UUID

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: MSG-CLI-102） |
| 対応 REQ | REQ-CLI-003, MSG-CLI-102 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id "not-a-uuid" --label "L"` |
| 期待結果 | exit 1 / stderr に `invalid record id` |

#### TC-E2E-024: `edit` — 存在しない id

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: MSG-CLI-106） |
| 対応 REQ | REQ-CLI-003, MSG-CLI-106 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id "018f0000-0000-7000-8000-000000000000" --label "L"`（実在しない uuid） |
| 期待結果 | exit 1 / stderr に `record not found` |

#### TC-E2E-025: `edit --kind` 指定で「未対応」エラー（Phase 1 スコープ外）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （詳細設計 §usecase::edit 注記） |
| 対応 REQ | REQ-CLI-003 |
| 種別 | 異常系（Phase 1 スコープ外明示） |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> edit --id <uuid> --kind secret` |
| 期待結果 | exit 1 / stderr に `--kind change is not supported` |

#### TC-E2E-030: `remove` — TTY 確認プロンプト（`y` で削除）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 6 |
| 対応 REQ | REQ-CLI-004, REQ-CLI-011 |
| 種別 | 正常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | TTY エミュレーションが必要——`assert_cmd` 単体では擬似 TTY を作れないため、本ケースは **`expectrl` または `rexpect` クレートによる擬似 TTY 経由** で実行（実装担当への注記）。最低限のフォールバックとして「`echo y \| shikomi remove --id <uuid>` 経由で stdin はパイプ＝非 TTY」を別 TC（TC-E2E-031）でカバーする |
| 期待結果 | exit 0 / stdout に `Delete record` プロンプトと `removed: <uuid>` |
| **注記** | 擬似 TTY が CI 環境で動作しない場合、本ケースは **手動検証ケース（証跡: ローカル実行ログ）** に格下げ可能。代替の自動検証は TC-E2E-031（非 TTY での Fail Fast）で受入基準 6 の主要部分を担保 |

#### TC-E2E-031: `remove` — 非 TTY で `--yes` 無し → exit 1（重要）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 6（**主検証**） |
| 対応 REQ | REQ-CLI-011, MSG-CLI-105 |
| 種別 | 異常系（重要） |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `assert_cmd::Command::cargo_bin("shikomi").args(["--vault-dir", ..., "remove", "--id", uuid]).stdin(Stdio::piped()).output()`（パイプで非 TTY 化） |
| 期待結果 | exit 1 / stderr に `refusing to delete without --yes` / 当該レコードが残存している（補助検証として `list` で確認） |

#### TC-E2E-032: `remove --yes` で確認なし削除

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 7 |
| 対応 REQ | REQ-CLI-004 |
| 種別 | 正常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | (1) `shikomi --vault-dir <tmp> remove --id <uuid> --yes` / (2) `list` |
| 期待結果 | (1) exit 0、stdout に `removed: <uuid>` / (2) stdout に当該 uuid 不在（または `no records`） |

#### TC-E2E-033: `remove --yes` で存在しない id

| 項目 | 内容 |
|------|------|
| 対応受入基準 | （横断: MSG-CLI-106） |
| 対応 REQ | REQ-CLI-004 |
| 種別 | 異常系 |
| 前提条件 | `add` 済みの 1 件存在 |
| 操作 | `shikomi --vault-dir <tmp> remove --id "018f0000-0000-7000-8000-000000000000" --yes` |
| 期待結果 | exit 1 / stderr に `record not found` |

#### TC-E2E-040: 暗号化 vault → exit 3（`list` で検証）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 8 |
| 対応 REQ | REQ-CLI-009, MSG-CLI-103 |
| 種別 | 異常系（重要） |
| 前提条件 | `tests/fixtures/vault_encrypted.db` を準備済み（vault ヘッダ `protection_mode = Encrypted` を持つ最小 SQLite ファイル。詳細は §10「暗号化 vault フィクスチャ作成」） |
| 操作 | `shikomi --vault-dir <fixture-tmp> list` |
| 期待結果 | exit 3 / stderr に `encryption is not yet supported` と `shikomi vault decrypt` 誘導文 / vault 内容は触られない |

#### TC-E2E-041: 暗号化 vault → exit 3（`add` でも同じ挙動）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 8 |
| 対応 REQ | REQ-CLI-009 |
| 種別 | 異常系 |
| 前提条件 | TC-E2E-040 と同じフィクスチャ |
| 操作 | `shikomi --vault-dir <fixture-tmp> add --kind text --label "X" --value "Y"` |
| 期待結果 | exit 3 / stderr に MSG-CLI-103 / vault 内容は触られない |

#### TC-E2E-050: vault 未初期化 — `list` は exit 1

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-CLI-010, MSG-CLI-104 |
| 種別 | 異常系 |
| 前提条件 | 空の `tempdir`（vault.db 不在） |
| 操作 | `shikomi --vault-dir <tmp> list` |
| 期待結果 | exit 1 / stderr に `vault not initialized` + `shikomi add` 誘導 |

#### TC-E2E-051: vault 未初期化 — `add` で自動初期化（成功）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-CLI-010, MSG-CLI-005 |
| 種別 | 正常系 |
| 前提条件 | 空の `tempdir` |
| 操作 | (1) `shikomi --vault-dir <tmp> add --kind text --label "L" --value "V"` / (2) `shikomi --vault-dir <tmp> list` |
| 期待結果 | (1) exit 0、stdout に `initialized plaintext vault at <path>` と `added: <uuid>` の **両方** / (2) exit 0、`L` `V` を含む |

#### TC-E2E-052: vault 未初期化 — `edit` / `remove` も exit 1

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-CLI-010, MSG-CLI-104 |
| 種別 | 異常系（パラメタライズ: `edit`, `remove`） |
| 前提条件 | 空の `tempdir` |
| 操作 | (a) `shikomi --vault-dir <tmp> edit --id <any> --label "L"` / (b) `shikomi --vault-dir <tmp> remove --id <any> --yes` |
| 期待結果 | (a)(b) ともに exit 1、stderr に `vault not initialized` |

#### TC-E2E-060: `--vault-dir` フラグが env var より優先

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10 |
| 対応 REQ | REQ-CLI-005 |
| 種別 | 正常系（境界: 優先順位） |
| 前提条件 | `tempdir A` と `tempdir B` を用意、`A` に 1 件 `add` 済み、`B` は空 |
| 操作 | `assert_cmd::Command` で `.env("SHIKOMI_VAULT_DIR", B).args(["--vault-dir", A, "list"])` |
| 期待結果 | exit 0 / stdout に `A` に追加したレコードを含む（B のレコード参照ではない） |

#### TC-E2E-061: env var が OS デフォルトより優先

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10 |
| 対応 REQ | REQ-CLI-005 |
| 種別 | 正常系 |
| 前提条件 | `tempdir A` に 1 件 `add` 済み |
| 操作 | `.env("SHIKOMI_VAULT_DIR", A).args(["list"])`（`--vault-dir` 不指定） |
| 期待結果 | exit 0 / stdout に `A` のレコード |

#### TC-E2E-062: フラグも env var もない → OS デフォルト解決の試行

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10（境界: フォールバック） |
| 対応 REQ | REQ-CLI-005 |
| 種別 | 異常系（テスト環境では vault 未存在前提） |
| 前提条件 | env var を `.env_remove("SHIKOMI_VAULT_DIR")`、`HOME` を `tempdir` に上書きして OS デフォルトを `tempdir/share/shikomi` 等に固定 |
| 操作 | `shikomi list` |
| 期待結果 | exit 1（vault 未初期化）または exit 0（空 vault）。**OS デフォルトの計算が正常に動いた**ことが本ケースの主目的（パニック・パス解決失敗の exit 2 にならないこと） |

#### TC-E2E-070: `LANG=ja_JP.UTF-8` で英日 2 段表示

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 11 |
| 対応 REQ | REQ-CLI-008 |
| 種別 | 正常系 |
| 前提条件 | `add` で 1 件追加済み（または vault 未初期化のいずれかでエラーを発生させる） |
| 操作 | `.env("LANG", "ja_JP.UTF-8").args(["--vault-dir", <missing>, "list"])` |
| 期待結果 | exit 1 / stderr に英語原文（`error: vault not initialized`）と日本語訳（`error: vault が初期化されていません`）が **両方** 含まれる |

#### TC-E2E-071: `LANG=C` で英語のみ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 11 |
| 対応 REQ | REQ-CLI-008 |
| 種別 | 正常系 |
| 前提条件 | TC-E2E-070 と同じ |
| 操作 | `.env("LANG", "C").args(["--vault-dir", <missing>, "list"])` |
| 期待結果 | exit 1 / stderr に英語原文を含み、**日本語文字を一切含まない**（`predicates::str::contains("vault が").not()`） |

#### TC-E2E-100: SCN-A 山田美咲ライフサイクル統合

| 項目 | 内容 |
|------|------|
| 対応シナリオ | SCN-A |
| 対応 REQ | REQ-CLI-001〜004, 007, 010, 012 |
| 種別 | 統合シナリオ（複数 REQ 横断） |
| 前提条件 | 空 `tempdir` |
| 操作 | (1) `add text "SSH: prod" --value "ssh -J ..."` / (2) `add secret "AWS_KEY" --stdin` で `SECRET_TEST_VALUE` / (3) `list` で 2 件確認・Secret マスク確認 / (4) `edit --id <text-uuid> --label "SSH: prod-v2"` / (5) `list` で更新確認 / (6) `remove --id <secret-uuid> --yes` / (7) `list` で 1 件残存確認 |
| 期待結果 | 全ステップ exit 0、最終 `list` に `SSH: prod-v2` のみ、`SECRET_TEST_VALUE` はどこにも露出しない |

#### TC-E2E-101: SCN-B 田中俊介初心者保護

| 項目 | 内容 |
|------|------|
| 対応シナリオ | SCN-B |
| 対応 REQ | REQ-CLI-001, REQ-CLI-011, REQ-CLI-008 |
| 種別 | 統合シナリオ |
| 前提条件 | `add` 済みの 1 件存在、`LANG=ja_JP.UTF-8` |
| 操作 | (1) `shikomi list` で日本語併記表示確認 / (2) `shikomi remove --id <uuid>` を非 TTY で実行（`--yes` 忘れた田中の誤操作） |
| 期待結果 | (1) 日本語併記出力 / (2) exit 1、削除は実行されず、日本語ヒント「--yes を付けて再実行してください」が出力される / 続けて `list` で当該レコード残存 |

#### TC-E2E-102: SCN-C 自己記述性

| 項目 | 内容 |
|------|------|
| 対応シナリオ | SCN-C |
| 対応 REQ | clap 設定（詳細設計 §clap 設定の詳細） |
| 種別 | 正常系（自己文書化） |
| 前提条件 | なし |
| 操作 | (a) `shikomi --help` / (b) `shikomi --version` / (c) `shikomi add --help` |
| 期待結果 | (a) exit 0、stdout に 4 サブコマンド全列挙 / (b) exit 0、stdout に `CARGO_PKG_VERSION` と一致する文字列 / (c) exit 0、stdout に `--kind` `--label` `--value` `--stdin` |

### 4.4 E2E テストの証跡

実行結果を Markdown レポートにまとめ、`/app/shared/attachments/マユリ/cli-vault-commands-e2e-report.md` として Discord に添付する。レポートに含めるもの:

- 各 TC の `assert_cmd` 実行コマンド
- stdout / stderr / exit code（テキストで全文）
- `cargo test --test e2e_*` の集計（`X passed; 0 failed`）
- 失敗時は失敗 TC の `assert_cmd` 出力 diff

擬似 TTY ケース（TC-E2E-030）が CI 環境で動作しない場合、ローカル実行のスクリーンキャスト or `script(1)` ログを証跡として併載する。

---

## 5. 結合テスト設計（UseCase 単位、実 SQLite）

### 5.1 設計方針

- **テスト対象**: `usecase::list::list_records`、`usecase::add::add_record`、`usecase::edit::edit_record`、`usecase::remove::remove_record` の 4 関数
- **エントリポイント**: 各 UseCase 関数を直接呼ぶ（CLI バイナリは経由しない、clap パースもしない）
- **DB は実接続**: テスト戦略ガイド準拠で `SqliteVaultRepository::from_paths(VaultPaths::new(<tempdir>))` を実物として渡す。**モック `VaultRepository` は使わない**（DB は実接続が原則）
- **検証スタイル**: 契約検証。戻り値の型・`CliError` のバリアント・`save()` 後の状態を別エンドポイント（`load()` か `list_records`）で**ラウンドトリップ**確認
- **`debug_assert` 検証**: `usecase::remove` の `debug_assert!(input.confirmed)` が `confirmed=false` で発火することを debug ビルドで確認

### 5.2 テストケース一覧

| TC-ID | 対象モジュール | 種別 | 入力 / 操作 | 期待結果 |
|-------|------------|------|-----------|---------|
| TC-IT-001 | `list_records` | 正常系 | 空 vault（`exists()=true` だが record 0 件）→ 呼び出し | `Ok(Vec::new())` |
| TC-IT-002 | `list_records` | 正常系 | 3 件 mixed kind の vault | `Ok(Vec<RecordView>)` 長さ 3、Secret は `ValueView::Masked` |
| TC-IT-003 | `list_records` | 異常系 | `exists()=false` の vault | `Err(CliError::VaultNotInitialized(_))` |
| TC-IT-010 | `add_record` | 正常系 | vault 未作成 + Text 入力 | `Ok(RecordId)`、続く `load()` で 1 件存在、payload は Plaintext |
| TC-IT-011 | `add_record` | 正常系（セキュリティ） | Secret 入力 `SecretString::from_string("SECRET_TEST_VALUE")` | `Ok(RecordId)`、`load()` で 1 件、`format!("{:?}", record.payload())` が `"[REDACTED]"` を含み投入値を含まない |
| TC-IT-012 | `add_record` | 異常系 | 暗号化 vault フィクスチャ | `Err(CliError::EncryptionUnsupported)` |
| TC-IT-020 | `edit_record` | 正常系 | `EditInput { label: Some(_), value: None }` | `Ok(RecordId)`、`load()` で当該レコードの label のみ更新、`updated_at` が `now` パラメタと一致 |
| TC-IT-021 | `edit_record` | 正常系 | `EditInput { label: Some(_), value: Some(_) }` | `Ok(RecordId)`、両フィールド更新 |
| TC-IT-022 | `edit_record` | 異常系 | 存在しない id | `Err(CliError::RecordNotFound(_))` |
| TC-IT-023 | `edit_record` | 異常系 | 暗号化 vault フィクスチャ | `Err(CliError::EncryptionUnsupported)` |
| TC-IT-030 | `remove_record` | 正常系 | 既存 1 件、`RemoveInput { confirmed: true }` | `Ok(RecordId)`、`load()` で record 消失 |
| TC-IT-031 | `remove_record` | 異常系 | 存在しない id、`confirmed: true` | `Err(CliError::RecordNotFound(_))` |
| TC-IT-032 | `remove_record` | 異常系（debug） | `confirmed: false` で呼び出し | debug ビルドで panic（`should_panic` テスト）、release ビルドはこのテスト対象外 |
| TC-IT-040 | UseCase 横断 | 異常系 | 4 UseCase 全てで暗号化 vault | 全て `Err(CliError::EncryptionUnsupported)` を返す（パラメタライズ） |
| TC-IT-050 | UseCase 横断 | 異常系 | `list` / `edit` / `remove` を `exists()=false` で呼ぶ | 全て `Err(CliError::VaultNotInitialized(_))` |

### 5.3 結合テストの I/O 物理化

- 各テストで `tempfile::TempDir::new()` を作り、その中に `VaultPaths::new(tempdir.path())` で構築
- `SqliteVaultRepository::from_paths(paths)` を直接生成（**`new()` を呼ばない** — env var の影響を排除）
- テスト終了時に `TempDir` の `Drop` で自動クリーンアップ
- 並列実行は cargo のデフォルトに任せる（各テストが独立 `TempDir` のため衝突なし）

---

## 6. ユニットテスト設計（pure function ホワイトボックス）

### 6.1 テストケース一覧

| TC-ID | 対象 | 種別 | 入力 | 期待結果 |
|-------|------|------|------|---------|
| **`error::ExitCode::from(&CliError)`** | | | | |
| TC-UT-001 | `From<&CliError> for ExitCode` | 正常 | `CliError::UsageError(_)` | `ExitCode::UserError` (1) |
| TC-UT-002 | 同上 | 正常 | `CliError::InvalidLabel(_)` | `ExitCode::UserError` (1) |
| TC-UT-003 | 同上 | 正常 | `CliError::InvalidId(_)` | `ExitCode::UserError` (1) |
| TC-UT-004 | 同上 | 正常 | `CliError::RecordNotFound(_)` | `ExitCode::UserError` (1) |
| TC-UT-005 | 同上 | 正常 | `CliError::VaultNotInitialized(_)` | `ExitCode::UserError` (1) |
| TC-UT-006 | 同上 | 正常 | `CliError::NonInteractiveRemove` | `ExitCode::UserError` (1) |
| TC-UT-007 | 同上 | 正常 | `CliError::Persistence(_)` | `ExitCode::SystemError` (2) |
| TC-UT-008 | 同上 | 正常 | `CliError::Domain(_)` | `ExitCode::SystemError` (2) |
| TC-UT-009 | 同上 | 正常 | `CliError::EncryptionUnsupported` | `ExitCode::EncryptionUnsupported` (3) |
| **`view::RecordView::from_record`** | | | | |
| TC-UT-010 | `from_record` | 正常 | Text kind, value 短い | `ValueView::Plain(value)` |
| TC-UT-011 | `from_record` | 境界 | Text kind, value 41 文字 | `ValueView::Plain(40 文字 + "…")` |
| TC-UT-012 | `from_record` | 正常（セキュリティ） | Secret kind | `ValueView::Masked`（`Plain` ではない） |
| **`presenter::warning::render_shell_history_warning`** | | | | |
| TC-UT-013 | 正常 | `Locale::English` | 英語の警告文を含む文字列 |
| TC-UT-014 | 正常 | `Locale::JapaneseEn` | 英語 + 日本語 2 段の警告文 |
| **`io::paths::resolve_vault_dir`** | | | | |
| TC-UT-020 | フラグ優先 | フラグ Some(P)、env var 設定済み | `Ok(P)`（フラグ値を返す） |
| TC-UT-021 | env var 優先 | フラグ None、env var 設定済み | `Ok(env var の値)` |
| TC-UT-022 | デフォルト | フラグ None、env var 未設定 | `Ok(dirs::data_dir().join("shikomi"))`（OS デフォルト） |
| TC-UT-023 | デフォルト失敗 | `dirs::data_dir()` が None を返すよう細工（環境変数 HOME / APPDATA を unset） | `Err(CliError::Persistence(_))` |
| **`presenter::error::render_error`** | | | | |
| TC-UT-030 | 全 9 バリアント × English | パラメタライズ: `CliError::*`, `Locale::English` | `error: ...\nhint: ...` の 2 行、英語のみ |
| TC-UT-031〜038 | （TC-UT-030 のパラメタライズで 9 ケース実体化） | — | 各バリアントで MSG-CLI-xxx の英語文を含む |
| TC-UT-040 | 全 9 バリアント × JapaneseEn | パラメタライズ | 4 行（英語 error / 日本語 error / 英語 hint / 日本語 hint）の整合 |
| TC-UT-041 | （TC-UT-040 のパラメタライズで 9 ケース実体化） | — | — |
| **`presenter::list::render_list`** | | | | |
| TC-UT-050 | 空 | `&[]`, English | `render_empty(English)` の出力と一致 |
| TC-UT-051 | 1 件 Text | `Plain("V")` | ヘッダ + 1 行、`V` を含む |
| TC-UT-052 | 1 件 Secret | `Masked` | ヘッダ + 1 行、`****` を含み投入値を含まない |
| TC-UT-053 | 複数件 + ラベル長すぎ | label 100 文字 | ラベルが `LIST_LABEL_MAX_WIDTH` (40) で truncate + `…` |
| **`presenter::success::*`** | | | | |
| TC-UT-060 | `render_added` | `RecordId`, English | `added: <uuid>` |
| TC-UT-061 | `render_added` | `RecordId`, JapaneseEn | 英 + 日 2 段 |
| TC-UT-062 | `render_updated` / `render_removed` / `render_cancelled` / `render_initialized_vault` | パラメタライズ | 各 MSG-CLI-001〜005 の英 / 日 確認 |
| **`error::CliError` の `Display`** | | | | |
| TC-UT-070 | Display 実装 | 全バリアント | 英語固定文字列のみ（日本語を含まない） |
| **`Locale::detect_from_env`** | | | | |
| TC-UT-080 | LANG 未設定 | env unset | `Locale::English` |
| TC-UT-081 | LANG=C | `"C"` | `Locale::English` |
| TC-UT-082 | LANG=en_US.UTF-8 | `"en_US.UTF-8"` | `Locale::English` |
| TC-UT-083 | LANG=ja_JP.UTF-8 | `"ja_JP.UTF-8"` | `Locale::JapaneseEn` |
| TC-UT-084 | LANG=ja | `"ja"` | `Locale::JapaneseEn` |
| TC-UT-085 | LANG=JA_JP（大文字） | `"JA_JP"` | `Locale::JapaneseEn`（大文字小文字無視） |
| **`KindArg → RecordKind` 変換** | | | | |
| TC-UT-090 | `From<KindArg> for RecordKind` | `KindArg::Text` | `RecordKind::Text` |
| TC-UT-091 | 同上 | `KindArg::Secret` | `RecordKind::Secret` |

### 6.2 ユニットテストでの環境変数操作の注意

`Locale::detect_from_env` は内部で `std::env::var("LANG")` を読むため、テスト中に環境変数を変更することになる。**`std::env::set_var` は thread-unsafe** のため、`#[test]` を `#[serial]`（`serial_test` クレート）で逐次化するか、またはテスト関数を pure 化（`detect_from_str(s: &str)` を内部に切り出す）する選択がある。

**推奨**: 詳細設計を一段精緻化し、`Locale::detect_from_env` を `Locale::detect_from_lang_env_value(lang: Option<&str>)` の純関数に分解し、`detect_from_env` はその薄い wrapper にする。これでユニットテストは pure 関数のみを叩け、env 操作不要。**実装担当への引き継ぎ事項として §10 に明記**。

---

## 7. CI 検証ケース

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-001 | 12 | `cargo fmt --check --all` | exit 0 |
| TC-CI-002 | 12 | `cargo clippy --workspace -- -D warnings` | exit 0 |
| TC-CI-003 | 12 | `cargo deny check` | exit 0 |
| TC-CI-004 | 13 | `cargo test -p shikomi-cli --all-targets` | exit 0、全テスト pass |
| TC-CI-005 | 13 | `cargo llvm-cov -p shikomi-cli --summary-only` | line coverage >= 80% |
| TC-CI-010 | 14 | `grep -E "MVP\s*Phase\s*1\|Phase\s*2" docs/architecture/context/process-model.md` | マッチ行 >= 1 |
| TC-CI-011 | 15 | `find crates/shikomi-cli/src/usecase crates/shikomi-cli/src/presenter -type d` がそれぞれ存在 | 両ディレクトリ存在確認 |
| TC-CI-012 | 16 | `grep -rn "SqliteVaultRepository" crates/shikomi-cli/src/` の **マッチが `main.rs` のみ** であること | `usecase/` `presenter/` `error.rs` 等に `SqliteVaultRepository` の文字列を含まない |

---

## 8. モック方針

| 対象 | レベル | モック方法 | フィクスチャ |
|------|------|---------|------------|
| `VaultRepository` 実装 | E2E | **モック不要**（実 `SqliteVaultRepository` + `tempfile`） | — |
| `VaultRepository` 実装 | 結合 | **モック不要**（実 `SqliteVaultRepository` + `tempfile`） | — |
| `VaultRepository` 実装 | ユニット | 該当なし（UseCase は結合の責務、ユニットは pure function のみ） | — |
| 暗号化 vault | E2E / 結合 | **フィクスチャ vault.db** を `tests/fixtures/vault_encrypted.db` に保存 | `tests/fixtures/vault_encrypted.db`（生成手順は §10 参照） |
| `dirs::data_dir()` | ユニット（TC-UT-022, 023） | 環境変数 `HOME` / `APPDATA` を `serial_test` 経由で操作、または pure 関数化（推奨） | — |
| `std::env::var("LANG")` | ユニット（TC-UT-080〜085） | 推奨: `Locale::detect_from_lang_env_value(s: Option<&str>)` に切り出して env 操作不要 | — |
| 時刻（`OffsetDateTime::now_utc()`） | 結合 | UseCase の引数で `now: OffsetDateTime` を受けるため、テスト側で固定時刻を渡す | — |
| TTY 判定（`is_terminal`） | E2E | `assert_cmd` の `.stdin(Stdio::piped())` で非 TTY 化、TTY 化は `expectrl` 等の擬似 TTY（CI 制約あり） | — |
| TTY 判定（`is_terminal`） | 結合 | UseCase は TTY を見ない（`RemoveInput::confirmed` で受け取る設計）ため不要 | — |

**Characterization テスト**: 本 feature は外部 API を呼ばない（vault は完全ローカル）。Characterization テストは**作成不要**。

**assumed mock の禁止**: 本 feature ではモックを使う箇所がほぼ無い（DB は実接続）。例外的に「pure 関数化のラッパで env を内部から呼ぶ箇所」のみ環境変数操作が必要だが、推奨設計（pure 関数化）を採用すればここも消える。

---

## 9. テスト実装の参考コマンド

### 9.1 開発者向け実行手順

```bash
# 全テスト（ユニット + 結合 + E2E）を実行
cargo test -p shikomi-cli --all-targets

# E2E のみ
cargo test -p shikomi-cli --test 'e2e_*'

# 結合のみ
cargo test -p shikomi-cli --test 'it_usecase_*'

# ユニットのみ（lib テスト）
cargo test -p shikomi-cli --lib

# CI 一式
cargo fmt --check --all
cargo clippy --workspace -- -D warnings
cargo deny check
cargo llvm-cov -p shikomi-cli --summary-only
```

### 9.2 人間が動作確認できるタイミング

実装完了後、以下のコマンドで**初めて `shikomi` が実機で動作**する。READMEの "Try it" セクション or PR description に記載すること:

```bash
# ビルド
cargo build -p shikomi-cli --release

# 平文 vault を作成しつつ最初のレコード追加（自動初期化）
./target/release/shikomi --vault-dir ~/shikomi-test add --kind text --label "test" --value "hello"

# 一覧表示
./target/release/shikomi --vault-dir ~/shikomi-test list

# 削除
./target/release/shikomi --vault-dir ~/shikomi-test remove --id <uuid> --yes
```

これが Issue #1 完了後初めて「動くもの」を実機で触れるマイルストーンとなる。

---

## 10. 実装担当（坂田銀時）への引き継ぎ事項

テスト設計を完成させるために、**実装段階で以下の調整を希望する**。詳細設計の意図と矛盾しないが、テスト容易性向上のための提案である。

### 10.1 `Locale::detect_from_env` の pure 関数化（強く推奨）

詳細設計では `Locale::detect_from_env() -> Locale` 1 関数のみだが、**内部で `std::env::var("LANG")` を呼ぶとユニットテストで env 操作が必要となり並列実行と相性が悪い**。

**推奨**:
- `pub fn detect_from_lang_env_value(lang: Option<&str>) -> Locale` を pure として実装
- `pub fn detect_from_env() -> Locale` は `detect_from_lang_env_value(std::env::var("LANG").ok().as_deref())` の薄い wrapper

これで TC-UT-080〜085 が pure 関数テストで完結する。

### 10.2 `shikomi-cli` の `[lib]` 化（提案、必須ではない）

詳細設計では「`shikomi-cli` は bin crate であり lib を公開しない」とあるが、**`tests/it_usecase_*.rs`（結合テスト）で `usecase::*` を呼ぶには `shikomi_cli` が lib として import 可能である必要がある**。

**選択肢**:
- **A**: `crates/shikomi-cli/Cargo.toml` に `[lib]` を追加し、`src/lib.rs` で `pub mod usecase; pub mod presenter; ...` を公開する。bin の `main.rs` は `use shikomi_cli::*;` で利用
- **B**: 結合テストを `#[path = "../src/usecase/list.rs"] mod list;` の hack で読み込む（Rust では一般的な pattern だが循環依存に弱い）
- **C**: 結合テストを bin から切り出さず、ユニットテスト相当として `#[cfg(test)] mod tests` でモジュール内に書く（テストレベルの分離が崩れる）

**推奨は A**。`pub` API としては内部用途と明示するため `lib.rs` の冒頭に `//! Internal API. Not stable; subject to change without notice.` を doc 化する。詳細設計 §3「`shikomi-cli` は bin crate であり lib を公開するのは本 feature のスコープ外」記述と衝突するため、**設計担当（セル）と協議の上で詳細設計を更新するか、提案 A を Phase 2 へ繰り延べるかを判断**する。本テスト設計書は「A 採用前提」で書いているが、B / C を選んだ場合は結合テストの import 方式のみ修正で済む。

### 10.3 暗号化 vault フィクスチャ作成

TC-E2E-040, TC-E2E-041, TC-IT-012, TC-IT-023 で必要。

**作成方法（推奨）**: `tests/common/fixtures.rs` に **テストヘルパー関数** `create_encrypted_vault_fixture(path: &Path)` を実装し、`shikomi-infra` の内部 API（`VaultHeader::new_encrypted(...)` 等。**未実装なら `shikomi-infra` 側に test-only API として追加**）を呼んで生成。これにより `tests/fixtures/vault_encrypted.db` をコミットせず、テスト実行時に毎回生成（バイナリファイルの git 管理回避）。

**未対応の場合のフォールバック**: `shikomi-infra` の暗号化モード書き出し API がそもそも未実装なら、本 feature の暗号化検証は「`Vault::protection_mode()` が `Encrypted` を返すモック実装」で代用してユニット内に閉じ、E2E TC-E2E-040/041 を **Phase 2 へ繰り延べる** 判断もあり得る。設計担当・実装担当と協議。

### 10.4 擬似 TTY が CI で動かない場合のフォールバック

TC-E2E-030（TTY 確認プロンプトで `y` 入力）は CI 環境では擬似 TTY が動かない可能性がある。フォールバックとして:

- TC-E2E-030 を `#[ignore]` 属性付きでローカル専用に置き、PR の test plan に「ローカル `cargo test --test e2e_remove -- --ignored` で手動検証」と明記
- 主受入基準（6 の Fail Fast 部分）は TC-E2E-031（非 TTY） で完全自動カバー

---

## 11. カバレッジ基準

| 観点 | 基準 |
|------|------|
| **受入基準の網羅** | 受入基準 1〜16 が全て **少なくとも 1 つの TC に対応**（§3.1 マトリクス参照） |
| **REQ の網羅** | REQ-CLI-001〜012 が全て少なくとも 1 つの TC に対応 |
| **MSG の網羅** | MSG-CLI-001〜005, 050, 100〜108 の全 ID が presenter ユニットテスト（TC-UT-030〜041, 060〜062）で文言確認される。MSG-CLI-109（panic 時）は擬似 panic 注入が困難のため、結合テストで `panic_hook` 関数単体を叩く軽量テストで代替 |
| **正常系** | 全 REQ で少なくとも 1 件の正常系 TC |
| **異常系** | `CliError` 全 9 バリアントが少なくとも 1 件の TC で発生する |
| **境界値** | 空 vault（0 件）/ 1 件 / 複数件、ラベル 1 文字 / 40 文字 / 41 文字 / 255 grapheme、UUID 不正、env 未設定 |
| **数値目標** | `cargo llvm-cov` 行カバレッジ 80%（受入基準 13 準拠）。**ただし数値達成のためのテスト水増しは禁止**。受入基準網羅が優先 |

---

## 12. テスト実装ファイル配置（実装担当への指示）

```
crates/shikomi-cli/
├── Cargo.toml          # [dev-dependencies] に assert_cmd, predicates, tempfile, serial_test
├── src/
│   ├── lib.rs          # （§10.2 推奨案 A 採用時）pub mod usecase; pub mod presenter; ...
│   ├── main.rs
│   ├── error.rs        # #[cfg(test)] mod tests { TC-UT-001〜009, 070 }
│   ├── view.rs         # #[cfg(test)] mod tests { TC-UT-010〜012 }
│   ├── input.rs
│   ├── cli.rs          # #[cfg(test)] mod tests { TC-UT-090, 091 }
│   ├── usecase/
│   │   ├── mod.rs
│   │   ├── list.rs     # （結合テストは tests/it_usecase_list.rs 側）
│   │   ├── add.rs
│   │   ├── edit.rs
│   │   └── remove.rs
│   ├── presenter/
│   │   ├── mod.rs      # Locale enum + tests { TC-UT-080〜085 }
│   │   ├── list.rs     # tests { TC-UT-050〜053 }
│   │   ├── error.rs    # tests { TC-UT-030〜041 }
│   │   ├── success.rs  # tests { TC-UT-060〜062 }
│   │   └── warning.rs  # tests { TC-UT-013, 014 }
│   └── io/
│       ├── mod.rs
│       ├── paths.rs    # tests { TC-UT-020〜023 }
│       └── terminal.rs # （TTY 操作のため tests は最小、E2E でカバー）
└── tests/
    ├── common/
    │   ├── mod.rs
    │   ├── fixtures.rs       # 暗号化 vault フィクスチャ生成、ヘルパー
    │   └── cli.rs            # assert_cmd::Command::cargo_bin("shikomi") の wrapper
    ├── e2e_list.rs           # TC-E2E-001〜003
    ├── e2e_add.rs            # TC-E2E-010〜015
    ├── e2e_edit.rs           # TC-E2E-020〜025
    ├── e2e_remove.rs         # TC-E2E-030〜033
    ├── e2e_encrypted.rs      # TC-E2E-040〜041
    ├── e2e_uninitialized.rs  # TC-E2E-050〜052
    ├── e2e_paths.rs          # TC-E2E-060〜062
    ├── e2e_i18n.rs           # TC-E2E-070〜071
    ├── e2e_scenarios.rs      # TC-E2E-100〜102
    ├── it_usecase_list.rs    # TC-IT-001〜003
    ├── it_usecase_add.rs     # TC-IT-010〜012
    ├── it_usecase_edit.rs    # TC-IT-020〜023
    ├── it_usecase_remove.rs  # TC-IT-030〜032
    └── it_usecase_cross.rs   # TC-IT-040, 050（横断パラメタライズ）
```

ファイル名 docstring に対応 REQ-ID と Issue 番号を必ず書くこと（テスト戦略ガイド準拠）。

---

## 13. 証跡提出方針

| 種別 | ファイル名 | 内容 |
|------|----------|------|
| E2E 実行ログ | `cli-vault-commands-e2e-report.md` | TC-E2E-001〜102 の `assert_cmd` 出力（stdout/stderr/exit code/diff） |
| 結合・ユニット集計 | `cli-vault-commands-test-summary.md` | `cargo test -p shikomi-cli` の集計（X passed; Y failed の TC 別表）|
| カバレッジ | `cli-vault-commands-coverage.html` | `cargo llvm-cov --html` のレポート（受入基準 13 検証） |
| CI チェック | `cli-vault-commands-ci-checks.md` | `cargo fmt / clippy / deny` の実行ログ（TC-CI-001〜003） |
| バグレポート（発見時） | `cli-vault-commands-bugs.md` | ファイル名・行番号・期待動作・実際動作・再現手順 |

全て `/app/shared/attachments/マユリ/` に保存して Discord に添付する。**コミットだけ・添付だけは禁止**（テスト戦略ガイド準拠）。

---

*作成: 涅マユリ（テスト担当）/ 2026-04-23*
*対応 PR: [#18 docs(cli-vault-commands)](https://github.com/shikomi-dev/shikomi/pull/18)*
*対応 feature: cli-vault-commands*
*Vモデル対応: E2E ↔ requirements-analysis.md（受入基準 16 項目）/ 結合 ↔ basic-design.md（モジュール連携）/ ユニット ↔ detailed-design.md（クラス・メソッド・テスト観点注記）*

> 完璧な仕様など存在しないネ。だからこそ実験するのだヨ。本 16 受入基準をテストが上から下まで 1 滴も漏らさず実証することが、本 feature が「動くもの」と呼ばれる唯一の根拠だヨ……クックック。
