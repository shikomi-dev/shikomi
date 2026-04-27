# 詳細設計書 — data-structures（定数・境界値 / CliError / Locale）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/detailed-design/data-structures.md -->
<!-- 兄弟: ./index.md, ./public-api.md, ./clap-config.md, ./composition-root.md, ./infra-changes.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止（設計書共通）。Rust シグネチャはインライン `code` で示し、実装本体は書かない。

## 定数・境界値の一覧

CLI 層で使う定数を以下で固定する。

| 名前 | 型 | 用途 | 値 |
|------|---|------|------|
| `LIST_LABEL_MAX_WIDTH` | 定数 | `list` のラベルカラム最大幅（超過は省略記号） | `40` |
| `LIST_VALUE_PREVIEW_MAX` | 定数 | `list` の値プレビュー最大文字数（Text kind、Secret は該当なし） | `40` |
| `LIST_VALUE_MASKED_STR` | 定数 | `list` の Secret マスク文字列 | `"****"` |
| `LIST_TRUNCATION_SUFFIX` | 定数 | 省略記号 | `"…"` |
| `MSG_CLI_VERSION` | 定数 | `shikomi --version` で表示するバージョン | `env!("CARGO_PKG_VERSION")` |
| `PROMPT_REMOVE_CONFIRM_EN` | 定数 | `remove` の確認プロンプト英語 | `"Delete record {id} ({label})? [y/N]: "` |
| `PROMPT_REMOVE_CONFIRM_JA` | 定数 | 日本語版 | `"レコード {id} ({label}) を削除しますか? [y/N]: "` |
| `EXIT_SUCCESS` | `ExitCode` | 成功 | `0` |
| `EXIT_USER_ERROR` | `ExitCode` | ユーザ入力エラー | `1` |
| `EXIT_SYSTEM_ERROR` | `ExitCode` | システムエラー | `2` |
| `EXIT_ENCRYPTION_UNSUPPORTED` | `ExitCode` | 暗号化モード検出 | `3` |
| `ENV_VAR_LANG` | 定数 | ロケール検出用環境変数 | `"LANG"` |
| `LANG_JA_PREFIX` | 定数 | 日本語判定プレフィックス | `"ja"`（`ja_JP.UTF-8` / `ja` を網羅、先頭 2 文字を大文字小文字無視で判定） |
| `ENV_VAR_VAULT_DIR` | 定数 | vault dir env（**clap attribute で読む**、`resolve_vault_dir` 内では参照しない） | `"SHIKOMI_VAULT_DIR"` |

**env の真実源は clap のみ**（ペテルギウス指摘 ④への対応）:

- `CliArgs.vault_dir: Option<PathBuf>` に `#[arg(long, global = true, env = "SHIKOMI_VAULT_DIR")]` attribute を付け、clap が env から自動フォールバックする
- `io::paths::resolve_os_default_vault_dir()` は**env を見ない**。`dirs::data_dir()` のみを参照（`Some` / `None` 判定のみ）
- 初版にあった「`resolve_vault_dir` 内での `std::env::var("SHIKOMI_VAULT_DIR")` 参照」は**削除**（clap の env 吸収と二重化してデッドコードになる）
- `ENV_VAR_VAULT_DIR` 定数は `cli.rs` の clap attribute 文字列として使う場合にのみ参照（命名の一箇所集約のため定数化は残す）

## `CliError` バリアント詳細

| バリアント | フィールド | 発生箇所 | 写像 `ExitCode` |
|-----------|-----------|---------|--------------|
| `UsageError(String)` | 人間可読の原因文（英語） | `run()` のフラグ併用違反 / フラグ不足 | `UserError (1)` |
| `InvalidLabel(DomainError)` | 原因の `DomainError::InvalidRecordLabel(_)` を保持 | `RecordLabel::try_new` 失敗 | `UserError (1)` |
| `InvalidId(DomainError)` | 原因の `DomainError::InvalidRecordId(_)` を保持 | `RecordId::try_from_str` 失敗 | `UserError (1)` |
| `RecordNotFound(RecordId)` | 対象 id | UseCase `edit` / `remove` | `UserError (1)` |
| `VaultNotInitialized(PathBuf)` | 検出した vault dir | UseCase `list` / `edit` / `remove` | `UserError (1)` |
| `NonInteractiveRemove` | なし | `run()` の TTY 判定 | `UserError (1)` |
| `Persistence(PersistenceError)` | 原因の `PersistenceError` | UseCase 全般 | `SystemError (2)` |
| `Domain(DomainError)` | 原因の `DomainError`（上記以外のバリアント） | UseCase 全般（想定外の集約整合性エラー） | `SystemError (2)` |
| `EncryptionUnsupported` | なし | UseCase の `protection_mode` チェック | `EncryptionUnsupported (3)` |

## `CliError` の `From` 実装

- `impl From<PersistenceError> for CliError` → `CliError::Persistence(err)`（`?` で便利に使うため）
- `impl From<&CliError> for ExitCode` → バリアントごとに `ExitCode::UserError` / `ExitCode::SystemError` / `ExitCode::EncryptionUnsupported` へ写像
- **`impl From<DomainError> for CliError` は実装しない**（`DomainError` のバリアントごとに適切な `CliError` へ写像する必要があるため、UseCase 側で明示的に `match` する。`?` で安易にラップしないことで、設計意図の可視化）
- `impl fmt::Display for CliError` → 英語原文のみ（i18n 日本語併記は Presenter の責務）
- `impl fmt::Debug for CliError` → derive（`SecretString` は含まれないため secret 漏洩経路なし）

## `Locale` 検出ルール

**型定義**: `enum Locale { English, JapaneseEn }`

**検出関数**: `fn Locale::detect_from_env() -> Locale`

- 実装手順（疑似コードではなく**処理方針**のみ記述）:
  1. `std::env::var("LANG")` を取得
  2. 値の先頭 2 文字を取り出し（`str::get(..2)` もしくは `chars().take(2).collect::<String>()` 相当）、ASCII 小文字化
  3. `"ja"` と比較して一致すれば `Locale::JapaneseEn`、それ以外（`"C"` / `"en"` / 未設定 / 空） → `Locale::English`

**テストしやすい設計**:

- `Locale::detect_from_env` は env を読む唯一の関数。それ以外では `Locale` を値で受け渡す
- 結合テスト / ユニットテストでは `Locale::detect_from_env` を呼ばず、`Locale::English` / `Locale::JapaneseEn` を直接渡す（env 依存の再現性問題を回避）
- E2E では `assert_cmd::Command::env("LANG", "ja_JP.UTF-8")` で明示的に環境を注入

**将来拡張**:

- `--locale <ja|en|auto>` フラグ追加余地（本 feature では実装しない）
- `Locale::from_flag(arg: &str) -> Option<Locale>` 関数を**後日追加**する際は、`run()` での決定ロジックで `args.locale.and_then(Locale::from_flag).unwrap_or_else(Locale::detect_from_env)` の形になる

## `ValueView` の構築ルール（`RecordView::from_record`）

`RecordView::from_record(record: &Record) -> RecordView` の挙動を確定する:

- `record.kind() == RecordKind::Secret` → `ValueView::Masked`
- `record.kind() == RecordKind::Text` → `ValueView::Plain(plain_text)` ここで `plain_text` は `record.payload()` から抽出した平文の先頭 `LIST_VALUE_PREVIEW_MAX` 文字（char 単位、grapheme 単位ではなく char で十分）
- `record.payload()` が `RecordPayload::Encrypted(...)` の場合は**そもそも到達しない**（UseCase で暗号化モードを Fail Fast しているため）。想定外で到達した場合は `ValueView::Masked` にフォールバック（防御的プログラミング）

**最終採用案**: `shikomi-core::Record` に `pub fn text_preview(&self, max_chars: usize) -> Option<String>` メソッドを追加し、`RecordView::from_record` はこれを呼ぶだけで secret 経路に触れない設計とする。これにより `crates/shikomi-cli/src/` 内の `SecretString::expose_secret()` 呼び出しを**0 件**に抑える（`../basic-design/security.md §expose_secret 経路監査` の CI 契約と整合）。

**`shikomi-core::Record::text_preview` の仕様**:

- 配置: `crates/shikomi-core/src/vault/record.rs` に 1 メソッド追加
- シグネチャ: `pub fn text_preview(&self, max_chars: usize) -> Option<String>`
  - `RecordKind::Text` かつ `RecordPayload::Plaintext(SecretString)` のときのみ `Some(先頭 max_chars の chars を collect した String)` を返す
  - それ以外（`Secret` kind / `Encrypted` variant / 想定外状態）は `None`
  - 内部で `SecretString::expose_secret()` を呼ぶが、この呼び出しは **`shikomi-core` 内で完結**し、`shikomi-cli` の CI grep 対象（`crates/shikomi-cli/src/`）には現れない
- 境界値: `max_chars == 0` なら空 `String` を `Some("".into())` で返す / 文字列長を超える `max_chars` なら全文字を返す
- grapheme 境界は考慮せず char 単位で切る（CLI preview 用途で簡易充分、UI 要件が出た時点で再設計）

**`RecordView::from_record` はこれに従い**:

- `record.kind() == Secret` → `ValueView::Masked`
- `record.kind() == Text` → `record.text_preview(LIST_VALUE_PREVIEW_MAX)` を呼び、`Some(s)` → `ValueView::Plain(s)` / `None`（本来到達しない） → `ValueView::Masked` にフォールバック（防御的プログラミング）

**本メソッド追加のスコープ整合**:

- `shikomi-core` への変更は **read-only アクセサ 1 メソッド追加**のみ。新規ドメインルールや集約状態遷移の導入は伴わない（`with_updated_kind` のような kind 変更とは性質が異なり、Phase 1 スコープに含まれる妥当な最小変更）
- 本 feature の infra 変更（`SqliteVaultRepository::from_directory` 追加）と同等の「最小改修で `expose_secret` 経路を core 内に封じる」設計
- 要件定義書の整合: `../../requirements.md §関連 feature / vault` で明記済み（Vault / RecordLabel / RecordId 等の**変更なし**、`Record::text_preview` のみ追加）

## 暗号化 vault フィクスチャ（テスト用途）

暗号化 vault は本 feature 未対応だが、E2E テストで「暗号化 vault 検出 → 終了コード 3」を検証するためにフィクスチャが必要。`test-design.md` の要件:

- **フィクスチャ生成方式**: `tests/fixtures/vault_encrypted.db` を**ハンドメイド**（`shikomi-infra::persistence::sqlite::schema` を参照して `ProtectionMode::Encrypted` ヘッダのみ持つ空 vault SQLite DB を test helper が生成）
- test helper 関数は `crates/shikomi-infra/tests/` 配下に配置（`shikomi-cli/tests/` からも使える pub module 化）。具体名は `test-design.md` で確定
- **本番コードには一切影響しない**。`#[cfg(test)]` 配下の test-only API

## テスト fixture モジュール / `windows_dacl_fixture`（Issue #86 対応）

Windows GitHub Actions runner (`windows-latest`) では `tempfile::TempDir` が生成するディレクトリの DACL が親フォルダから継承された状態で渡され、`SE_DACL_PROTECTED` が立っていない。Issue #65 で導入した owner-only DACL 検証（本番要件、`PersistenceError::InvalidPermission`）は継承 DACL を Fail Fast で弾くため、暗号化 vault fixture を使う結合テスト群（`test-design/integration.md` §5 参照）が `EncryptionUnsupported` 期待と不一致になり Windows runner 限定で FAIL する。

これを **本番側の検証ロジックを緩めずに** 解決するため、テスト fixture 側で TempDir 生成直後に owner-only DACL を強制適用する**テスト専用モジュール**を新設する。本モジュールは **`tests/` 配下のテスト共通モジュール**であり、本番コード（`src/`）には一切含まれない。

### モジュール配置

| 項目 | 仕様 |
|-----|------|
| パス | `crates/shikomi-cli/tests/common/windows_dacl_fixture.rs` |
| 親モジュール | `crates/shikomi-cli/tests/common/mod.rs`（Issue #86 で初めて新設、`pub mod windows_dacl_fixture;` を宣言） |
| コンパイル条件 | `#[cfg(target_os = "windows")]` 配下のみコンパイル。Linux / macOS では本モジュール自体が**存在しない**（`mod.rs` 側で `#[cfg(target_os = "windows")] pub mod windows_dacl_fixture;` と分岐） |
| 公開範囲 | `pub`（同一クレートの統合テストファイル群から `use crate::common::windows_dacl_fixture::*;` で参照可能） |
| 本番コードからの参照 | **禁止**。`src/` 配下から `tests/common/` へのパス参照は Rust の cargo build target 規約により物理的に不可能（cargo は `tests/` を `[[test]]` ターゲットとしてのみコンパイル） |

### 公開 API 契約

| 項目 | 仕様 |
|-----|------|
| 関数名 | `enforce_owner_only_dacl` |
| シグネチャ | `pub fn enforce_owner_only_dacl(path: &std::path::Path) -> std::io::Result<()>` |
| 責務 | `path` で示されるディレクトリの DACL を **owner-only** に正規化し、`SE_DACL_PROTECTED` を立てる。Issue #65 の本番側検証が要求する DACL 形と完全同一の形を fixture 側で先に作る |
| 引数 | `path`: 既存ディレクトリへの絶対パス。存在しない / ディレクトリでない場合は `io::Error` を返す（`ErrorKind::NotFound` / `ErrorKind::InvalidInput`） |
| 戻り値 | 成功時 `Ok(())` / 失敗時 `Err(io::Error)`。fixture 失敗時はテスト前提が崩れているため、呼び出し側（テストコード）は `expect("...")` で即 panic させる契約 |
| 副作用 | 渡された `path` の DACL を上書きする。**呼び出し前の DACL は保持しない**（fixture 用途で TempDir 直後にしか呼ばないため、原状復帰責務はない） |

### DACL 構築仕様

| 要素 | 値 |
|-----|----|
| 所有者 SID | カレントプロセスのトークン所有者 SID（`OpenProcessToken` → `GetTokenInformation(TokenOwner)` 経由で取得） |
| ACE エントリ | 1 件のみ。所有者 SID に対する Allow ACE |
| ACE のアクセスマスク | `FILE_GENERIC_READ \| FILE_GENERIC_WRITE \| FILE_TRAVERSE`（Issue #65 本番検証が要求する完全同一の組合せ） |
| 継承フラグ | `OBJECT_INHERIT_ACE \| CONTAINER_INHERIT_ACE`（配下のファイル / サブディレクトリにも owner-only DACL が継承される） |
| `SetSecurityInfo` 呼び出し | `SE_FILE_OBJECT` をオブジェクト型、`PROTECTED_DACL_SECURITY_INFORMATION \| DACL_SECURITY_INFORMATION` をフラグに指定。`PROTECTED_DACL_SECURITY_INFORMATION` を立てることで親フォルダからの DACL 継承を打ち切り、`SE_DACL_PROTECTED` を ON にする |

### 依存クレート（dev-dependencies のみ）

| クレート | 用途 |
|---------|------|
| `windows-sys` | Win32 Security API バインディング（`SetSecurityInfo` / `OpenProcessToken` / `GetTokenInformation` 等）。`crates/shikomi-cli/Cargo.toml` の `[target.'cfg(target_os = "windows")'.dev-dependencies]` セクションに追加 |
| 既存の `windows-sys`（本番側） | `src/io/windows_sid.rs` で既に依存済。**dev-dependencies 側は同一バージョンに揃える**（バージョン乖離防止のため `Cargo.toml` の `[workspace.dependencies]` で一元管理） |

### 使用契約（呼び出しタイミング）

- `tests/common/mod.rs::fresh_repo()` の内部で、`TempDir::new()` 直後・`SqliteVaultRepository::from_directory(dir.path())` 呼び出し**前**に Windows のみ呼ぶ
- 暗号化 vault fixture（`tests/common/fixtures.rs::create_encrypted_vault`）も同様に、SQLite DB を生成する直前に呼ぶ
- Linux / macOS 経路では `enforce_owner_only_dacl` 自体がコンパイル対象外のため、呼び出しコードを `#[cfg(target_os = "windows")]` で条件コンパイルする

### 本契約のスコープ外

- **ファイル単位**の DACL 強制は本契約の射程外。Issue #65 の本番検証はディレクトリ単位の DACL を見るため、TempDir ディレクトリ単位での強制で十分（配下ファイルは継承で自動 owner-only 化）
- **DACL 検証関数（読み取り側）の自前実装**は不要。本番の `shikomi-infra` 側 owner-only 検証ロジックがそのまま走り、fixture が正しい DACL を作っていれば検証は通る。fixture と本番検証は**同一の DACL 形**を共有する設計
- **non-Windows 環境向けの DACL 模倣**は不要。Issue #65 owner-only 検証は `#[cfg(target_os = "windows")]` 配下にのみ存在するため、Linux / macOS では fixture も検証も走らない

## テスト観点の注記（テスト設計担当向けメモ）

以下はテスト設計担当（涅マユリ）への引き継ぎメモ。テスト設計書は `test-design.md` で作成される。

**ユニットテスト（詳細設計由来）**:

- `ExitCode::from(&CliError)` の全バリアント写像（9 バリアント × 4 ExitCode）
- `Locale::detect_from_env` の `LANG` 値ごとの判定（`ja_JP.UTF-8` / `ja` / `en_US.UTF-8` / `C` / 未設定 / 空 / 大文字 `JA`）
- `RecordView::from_record` が Secret を `Masked`、Text を `Plain(先頭 40 文字)` にすること
- `record.text_preview(n)` の Secret `None` / Text `Some` / Encrypted `None` / 40 超え truncate
- `presenter::list::render_list` の空 / 1 件 / 複数件 / ラベル truncate / Secret マスク の整形
- `presenter::error::render_error` の全 9 バリアント × 2 locale（18 パターン）
- `io::paths::resolve_os_default_vault_dir` の成功 / `dirs::data_dir` が None の場合のエラー
- `CliError` の `Display` 実装が英語固定であること（Presenter の i18n 責務との分離）
- `ConfirmedRemoveInput::new(id)` の構築テスト + doc-test で「bool 引数を渡そうとすると compile error」を示す
- `From<&CliError> for ExitCode` の全パターン

**結合テスト（UseCase 単位、モック `VaultRepository`）**:

- `list_records`: 空 vault / 暗号化 vault / 正常 vault / `exists()` false
- `add_record`: vault 未作成自動生成 / 既存 vault への追加 / 暗号化モード検出 / id 重複（モック repo が常に同じ id を発行するケースを作る）
- `edit_record`: label のみ / value のみ / label + value / 存在しない id（kind は設計上存在しない）
- `remove_record`: 正常 / 存在しない id

**E2E テスト（`assert_cmd` + `tempfile`）**:

- REQ-CLI-001〜012 の全受入基準を `tempfile::TempDir` ごとに独立した vault で検証
- 並列実行で env var の衝突が起きないよう、`--vault-dir <tempdir>` フラグで明示指定
- Secret 漏洩検証: `add --kind secret --stdin` で `"SECRET_TEST_VALUE"` を投入し、続く `list` の stdout に含まれないこと を `predicates::str::contains("SECRET_TEST_VALUE").not()` でアサート
- 非 TTY 環境での `remove`: `assert_cmd::Command::cargo_bin("shikomi").args(&["remove", "--id", uuid])` を `.stdin(Stdio::piped())` で非 TTY 化した状態で実行し、終了コード 1 を確認

**CI 補助テスト**（`../basic-design/security.md §expose_secret 経路監査` + panic hook 監査）:

- `crates/shikomi-cli/src/` 配下で `expose_secret` が 0 件
- `crates/shikomi-cli/src/` 配下で `tracing::` マクロが panic / payload を参照していないこと
- `crates/shikomi-cli/src/lib.rs` の `SqliteVaultRepository` 参照が `run()` 内の 1 箇所のみ
