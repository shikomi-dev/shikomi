# テスト設計書 — cli-vault-commands（索引）

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | cli-vault-commands（`shikomi-cli` に vault CRUD 4 サブコマンドを実装） |
| 対象 PR | [#18](https://github.com/shikomi-dev/shikomi/pull/18) |
| 対象ブランチ | `feat/cli-vault-commands-design` → `develop` |
| 上位設計 | `../requirements-analysis.md`（受入基準 16 項目）/ `../requirements.md`（REQ-CLI-001〜012）/ `../basic-design/` / `../detailed-design/` |
| MVP フェーズ | Phase 1（CLI 直結）— `../../../architecture/context/process-model.md` §4.1.1 で正当化 |
| 対応 vault モード | 平文モードのみ。暗号化 vault は Fail Fast（exit 3）で検証する |
| テスト実行タイミング | 実装担当（坂田銀時）が `feat/cli-vault-commands-impl` に積み上げた直後、`develop` マージ前 |
| Vモデル対応 | E2E ↔ 受入基準（要件定義） / 結合 ↔ UseCase 単位（基本設計） / ユニット ↔ Presenter・Mapper・Resolver（詳細設計） |
| 分割方針 | ペガサス指摘による `<name>/` 配下分割。全 5 ファイル、各 500 行以内 |

> **テスト戦略の核**: 完璧な設計など存在しない——だからこそ実験するのだヨ。E2E から下りる「上流優先」順で書き、上位ケースの網羅を最優先とする。`cargo llvm-cov` で 80% を測るが、**判定基準は受入基準 16 項目の網羅**である。

## 2. 索引（分割ファイル一覧）

| ファイル | 内容 | 主要 TC-ID |
|----------|------|-----------|
| `index.md`（本書） | 概要、レベル戦略、トレーサビリティマトリクス、モック方針、カバレッジ基準 | — |
| `e2e.md` | E2E テスト設計（`assert_cmd` + `tempfile`）と証跡 | TC-E2E-001〜102 |
| `integration.md` | UseCase 単位 結合テスト（実 SQLite + `tempfile`） | TC-IT-001〜050 |
| `unit.md` | pure function ユニットテスト + 実装担当への引き継ぎ事項 | TC-UT-001〜091 |
| `ci.md` | CI 検証ケース、ファイル配置、証跡提出方針、実行コマンド | TC-CI-001〜015 |

---

## 3. テストレベル戦略

| 種別 | 対象 | 視点 | モック | 検証スタイル | テスト配置（Rust 慣習） |
|------|------|------|-------|------------|----------------------|
| **E2E** | `shikomi` バイナリ全体 | 完全ブラックボックス | なし（`SqliteVaultRepository` 実物 + `tempfile::TempDir`） | 振る舞い検証（stdout / stderr / exit code / vault 内容のラウンドトリップ） | `crates/shikomi-cli/tests/e2e_*.rs`（`assert_cmd` + `predicates` + `tempfile`） |
| **結合** | `usecase::*` 関数 | 半ブラックボックス | DB 実接続、外部 API なし | 契約検証（戻り値型・`CliError` バリアント・ラウンドトリップ） | `crates/shikomi-cli/tests/it_usecase_*.rs` |
| **ユニット** | `presenter::*` / `error::ExitCode::from` / `Locale::detect_from_lang_env_value` / `RecordView::from_record` / `io::paths::resolve_vault_dir` / `shikomi_core::Record::text_preview` 等の pure function | ホワイトボックス | I/O バウンダリ全部 | 1 テスト 1 アサーション原則。`test_<対象>_<状況>_<期待>` 命名 | 各モジュール内 `#[cfg(test)] mod tests` |

**Rust 慣習との整合**:
- ユニットテストは `#[cfg(test)] mod tests` でソースに埋め込む（テスト戦略ガイド「Rust: unit test は `#[cfg(test)]` でソースモジュール内」準拠）
- 結合テストと E2E テストは `crates/shikomi-cli/tests/` 配下に配置し、ファイル名 prefix（`e2e_*.rs` / `it_usecase_*.rs`）で分離
- **`shikomi-cli` は `[lib] + [[bin]]`** 構成（詳細設計 §3 採用案 A）で、`lib.rs` に `#[doc(hidden)]` 付きの `pub mod usecase; pub mod presenter; ...` を配置。結合テストはこの lib を使って UseCase を直接叩く

---

## 4. テストマトリクス（トレーサビリティ）

### 4.1 受入基準 ↔ REQ ↔ TC 対応表

| 受入基準 # | 要約 | 関連 REQ | 関連クラス／メソッド | TC-ID（主） | TC-ID（補助） |
|----------|------|---------|------------------|-----------|------------|
| 1 | `list` 空 / 1 件 / 複数件、Secret マスク | REQ-CLI-001, REQ-CLI-007 | `usecase::list::list_records`, `presenter::list::render_list`, `RecordView::from_record`, `Record::text_preview` | TC-E2E-001〜003 | TC-IT-001, TC-UT-010〜012, TC-UT-100〜103 |
| 2 | `add --kind text` → `list` で反映 | REQ-CLI-002, REQ-CLI-001 | `usecase::add::add_record` | TC-E2E-010 | TC-IT-010 |
| 3 | `add --kind secret --stdin` で stdout/stderr に secret 一切露出禁止 | REQ-CLI-002, REQ-CLI-007 | `usecase::add::add_record`, `io::terminal::read_password`, `presenter::list::render_list` | TC-E2E-011 | TC-IT-011, TC-UT-012 |
| 4 | `add --kind secret --value` で警告だが exit 0 | REQ-CLI-002, MSG-CLI-050 | `presenter::warning::render_shell_history_warning` | TC-E2E-012 | TC-UT-013 |
| 5 | `edit --label NEW`、`--value` と `--stdin` 併用拒否 | REQ-CLI-003 | `usecase::edit::edit_record`, clap `try_parse` 後の併用検証 | TC-E2E-020, TC-E2E-021 | TC-IT-020, TC-IT-021 |
| 6 | `remove` TTY 確認、非 TTY + `--yes` 無しなら exit 1 | REQ-CLI-004, REQ-CLI-011 | `io::terminal::is_stdin_tty`, `ConfirmedRemoveInput::new` | TC-E2E-030, TC-E2E-031 | TC-IT-030 |
| 7 | `remove --yes` で確認なし削除 | REQ-CLI-004 | `usecase::remove::remove_record` | TC-E2E-032 | TC-IT-031 |
| 8 | 暗号化 vault → exit 3 + MSG-CLI-103 | REQ-CLI-009 | UseCase 全関数の `protection_mode` チェック | TC-E2E-040, TC-E2E-041 | TC-IT-040 |
| 9 | vault 未初期化: `list/edit/remove` exit 1、`add` 自動作成 | REQ-CLI-010 | `usecase::*` の `exists()` 分岐 | TC-E2E-050, TC-E2E-051, TC-E2E-052 | TC-IT-050 |
| 10 | `SHIKOMI_VAULT_DIR` / `--vault-dir` / OS デフォルト 優先順位 | REQ-CLI-005 | clap `#[arg(env)]` 単一ルート、`SqliteVaultRepository::from_directory(&Path)` | TC-E2E-060, TC-E2E-061, TC-E2E-062 | TC-UT-020, TC-UT-022 |
| 11 | `MSG-CLI-xxx` 英日 2 段、`LANG=C` 英語のみ | REQ-CLI-008 | `presenter::error::render_error`, `Locale::detect_from_lang_env_value` | TC-E2E-070, TC-E2E-071 | TC-UT-030〜038, TC-UT-040〜041, TC-UT-080〜085 |
| 12 | `cargo clippy / fmt / deny` 全 pass | — | リポジトリ全体 | TC-CI-001〜003 | — |
| 13 | `cargo test -p shikomi-cli` 全 pass、行カバレッジ 80%+ | — | テストスイート全体 | TC-CI-004, TC-CI-005 | — |
| 14 | `process-model.md` §4.1.1 にフェーズ区分追記済み | — | ドキュメント | TC-CI-010 | — |
| 15 | `shikomi-cli/src/` が `presenter/` / `usecase/` / `io/` / 共通 `main.rs` + `lib.rs` の 3 層構造 | REQ-CLI-012 | ディレクトリ構造 | TC-CI-011 | — |
| 16 | `SqliteVaultRepository` 具体型参照は `main.rs`（`run()`）のみ、`expose_secret` は `shikomi-cli/src/` 0 件 | REQ-CLI-012 | コード grep | TC-CI-012, TC-CI-013 | TC-CI-014, TC-CI-015 |

### 4.2 REQ-CLI-006（終了コード契約） / REQ-CLI-007（マスキング）の横断検証

終了コード契約は全 E2E ケースで `assert_cmd::assert::Assert::code(...)` により検証する（横串検証）。
secret マスキングは TC-E2E-011 が主検証だが、TC-E2E-001〜003（`list` 系）でも `predicates::str::contains("SECRET_TEST_VALUE").not()` を全アサートに含める。

### 4.3 設計変更（ペテルギウス／服部平次／ペガサス review 差戻し）の反映一覧

本 review 差戻しで以下のテスト観点が追加／変更された。各ファイルでの反映箇所を明示する:

| 指摘者 | 設計変更 | 反映先 |
|--------|---------|--------|
| ペテルギウス ③ | `shikomi-cli` `[lib] + [[bin]]` 化採用 | `index.md` §3、`integration.md` §2（lib 経由呼び出し明記） |
| ペテルギウス ⑤ | `ConfirmedRemoveInput { id }` 型化、bool 撤廃 | `integration.md` TC-IT-030 / TC-IT-032 削除、`unit.md` TC-UT-110（型構築 doc-test） |
| ペテルギウス ⑥ | `ListInput` 空構造体削除 | `integration.md` TC-IT-001〜003 の入力記述から撤去 |
| ペテルギウス ② | `edit --kind` Phase 1 スコープ外 | `e2e.md` TC-E2E-025（`EditArgs` に `--kind` が存在する場合のみ UsageError、存在しないなら clap の unknown arg エラー） |
| ペテルギウス ④ | env 真実源 clap 単一化、`resolve_vault_dir` 内 env 分岐削除 | `unit.md` TC-UT-020〜022（env 分岐検証は廃止、pure な優先順位検証のみ残す） |
| ペテルギウス ⑦ | `VaultPaths` pub 昇格撤回 → `from_directory(&Path)` | `integration.md` §3（`from_directory` 直接呼び出しに修正） |
| 服部平次 ① | panic hook で `tracing` 呼ばず、`info.payload()` 非参照 | `ci.md` TC-CI-014, TC-CI-015（grep 検証） |
| 服部平次 ③ | `expose_secret` `shikomi-cli/src/` 0 件契約 | `ci.md` TC-CI-013（grep）、`unit.md` TC-UT-100〜103（`Record::text_preview` 単体） |
| ペガサス | ファイル 500 行以内 | 本分割により全 5 ファイルが 500 行以内 |

---

## 5. ペルソナシナリオ設計（E2E 対応）

`../requirements-analysis.md` §ペルソナのプライマリ／セカンダリに対応する**ユーザ視点シナリオ**として組む。TC 詳細は `e2e.md` §ペルソナシナリオを参照。

| シナリオ ID | ペルソナ | シナリオ概要 | 対応 TC |
|------------|---------|------------|---------|
| SCN-A | 山田 美咲（FE エンジニア、CLI 主、開発機） | 開発中に SSH コマンド断片を `add` し、後日 `list` → `edit` → `remove` する一連のライフサイクル | TC-E2E-100 |
| SCN-B | 田中 俊介（営業、GUI 主、CLI 非常用） | コマンドプロンプトを初めて起動し、`shikomi list` でエントリ確認、誤削除を非 TTY で防げる | TC-E2E-101 |
| SCN-C | 野木 拓海（後続実装担当） | `shikomi --help` のサブコマンド一覧確認、`--version` 出力、`-h` で各サブヘルプを参照 | TC-E2E-102 |

---

## 6. モック方針

| 対象 | レベル | モック方法 | フィクスチャ |
|------|------|---------|------------|
| `VaultRepository` 実装 | E2E | **モック不要**（実 `SqliteVaultRepository` + `tempfile`） | — |
| `VaultRepository` 実装 | 結合 | **モック不要**（実 `SqliteVaultRepository::from_directory(&Path)` + `tempfile`） | — |
| `VaultRepository` 実装 | ユニット | 該当なし（UseCase は結合の責務、ユニットは pure function のみ） | — |
| 暗号化 vault | E2E / 結合 | **フィクスチャ vault.db** を `tests/common/fixtures.rs` のヘルパーで**テスト実行時に生成**（git 管理しない） | `shikomi-infra` の test-only API で `VaultHeader::protection_mode = Encrypted` を書き出す。詳細は `unit.md §引き継ぎ §10.3` |
| `dirs::data_dir()` | ユニット（TC-UT-022） | 環境変数 `HOME` / `APPDATA` を `serial_test` 経由で操作、または pure 関数化 | — |
| `std::env::var("LANG")` | ユニット（TC-UT-080〜085） | **`Locale::detect_from_lang_env_value(s: Option<&str>)` に pure 切り出し**（`unit.md §引き継ぎ §10.1` 指示）→ env 操作不要で完結 | — |
| 時刻（`OffsetDateTime::now_utc()`） | 結合 | UseCase 引数で `now: OffsetDateTime` を受けるため固定時刻注入 | — |
| TTY 判定（`is_terminal`） | E2E | `assert_cmd` の `.stdin(Stdio::piped())` で非 TTY 化。TTY 化は `expectrl` 等の擬似 TTY（CI 制約あり、`#[ignore]` フォールバック） | — |

**Characterization テスト**: 本 feature は外部 API を呼ばない（vault は完全ローカル）。Characterization テストは**作成不要**。

**assumed mock の禁止**: 本 feature ではモックを使う箇所がほぼ無い（DB は実接続）。pure 関数化の推奨で env 経路からも env 操作を排除する。

---

## 7. カバレッジ基準

| 観点 | 基準 |
|------|------|
| **受入基準の網羅** | 受入基準 1〜16 が全て **少なくとも 1 つの TC に対応**（§4.1 マトリクス参照） |
| **REQ の網羅** | REQ-CLI-001〜012 が全て少なくとも 1 つの TC に対応 |
| **MSG の網羅** | MSG-CLI-001〜005, 050, 100〜108 の全 ID が presenter ユニットテスト（TC-UT-030〜041, 060〜062）で文言確認される。MSG-CLI-109（panic 時）は `ci.md §panic hook` の grep 契約で代替 |
| **正常系** | 全 REQ で少なくとも 1 件の正常系 TC |
| **異常系** | `CliError` 全 9 バリアントが少なくとも 1 件の TC で発生する |
| **境界値** | 空 vault（0 件）/ 1 件 / 複数件、ラベル 1 文字 / 40 文字 / 41 文字 / 255 grapheme、UUID 不正、env 未設定、`text_preview` の `max_chars=0` / 境界長 / Secret kind での `None` |
| **数値目標** | `cargo llvm-cov` 行カバレッジ 80%（受入基準 13 準拠）。**ただし数値達成のためのテスト水増しは禁止**。受入基準網羅が優先 |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-23 / review 差戻し対応版*
*対応 PR: [#18](https://github.com/shikomi-dev/shikomi/pull/18)*
*対応 feature: cli-vault-commands*
*Vモデル対応: E2E ↔ requirements-analysis.md（受入基準 16 項目）/ 結合 ↔ basic-design/（モジュール連携）/ ユニット ↔ detailed-design/（クラス・メソッド・テスト観点注記）*

> 完璧な仕様など存在しないネ。だからこそ実験するのだヨ。本 16 受入基準をテストが上から下まで 1 滴も漏らさず実証することが、本 feature が「動くもの」と呼ばれる唯一の根拠だヨ……クックック。
