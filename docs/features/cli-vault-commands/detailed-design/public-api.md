# 詳細設計書 — public-api（モジュール別公開シグネチャ）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/detailed-design/public-api.md -->
<!-- 兄弟: ./index.md, ./data-structures.md, ./clap-config.md, ./composition-root.md, ./infra-changes.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。Rust シグネチャはインライン `code` で示し、実装本体は書かない。

## 前提: crate 構成

`shikomi-cli` は `[lib] + [[bin]]` の 2 ターゲット構成（`../basic-design/index.md §モジュール構成`）:

- `[lib] name = "shikomi_cli"` — UseCase / Presenter / input / view / error などの**内部公開 Rust API** を提供
- `[[bin]] name = "shikomi"` — `shikomi_cli::run()` を呼ぶ 3 行ラッパ

すべての `pub` 項目に `#[doc(hidden)]` を付け、外部公開 API 契約化を避ける。

## `shikomi_cli::run` — コンポジションルート

- `pub fn run() -> ExitCode`
  - 詳細な処理順序は `./composition-root.md` 参照
  - bin の `main.rs` は `fn main() -> ExitCode { shikomi_cli::run() }` のみ

## `shikomi_cli::usecase::list`

- `pub fn list_records(repo: &dyn VaultRepository) -> Result<Vec<RecordView>, CliError>`
  - 手順: `exists()` → false は `VaultNotInitialized`、true なら `load()` → 暗号化モード検証 → `records().iter().map(RecordView::from_record).collect()`
  - `PersistenceError` → `CliError::Persistence` に `?` で写像（`From<PersistenceError>` 実装経由）
  - 入力 DTO を持たない（YAGNI、将来フラグ追加時にその時点で導入）

## `shikomi_cli::usecase::add`

- `pub fn add_record(repo: &dyn VaultRepository, input: AddInput, now: OffsetDateTime) -> Result<RecordId, CliError>`
  - 手順:
    1. `exists()` で分岐
    2. false: `VaultHeader::new_plaintext(VaultVersion::CURRENT, now)?` → `Vault::new(header)`
    3. true: `repo.load()?` → 暗号化モードチェック
    4. `uuid::Uuid::now_v7()` → `RecordId::new(uuid)?`（`DomainError::InvalidRecordId` は通常発生しないが、`Domain(DomainError)` として保険）
    5. `payload = RecordPayload::Plaintext(input.value)` / `record = Record::new(id, input.kind, input.label, payload, now)`
    6. `vault.add_record(record).map_err(CliError::Domain)?` → `repo.save(&vault)?`
    7. `Ok(id)`
  - **注記**: Phase 1 は Text も Secret も `RecordPayload::Plaintext` バリアントで保存する。`RecordKind` は「Secret として扱う」ヒントメタデータであり、vault モードは `Plaintext` のまま（ヘッダ mode と payload variant の整合は保たれる）

## `shikomi_cli::usecase::edit`

- `pub fn edit_record(repo: &dyn VaultRepository, input: EditInput, now: OffsetDateTime) -> Result<RecordId, CliError>`
  - 手順:
    1. `exists()` false なら `VaultNotInitialized`
    2. `repo.load()?` → 暗号化モードチェック
    3. `vault.find_record(&input.id)` で存在確認 → `None` なら `RecordNotFound`
    4. `vault.update_record(&input.id, |old_record| { ... })` のクロージャ内で:
       - label 更新時: `old.with_updated_label(input.label.unwrap(), now)?`
       - value 更新時: `old.with_updated_payload(RecordPayload::Plaintext(input.value.unwrap()), now)?`
       - label + value 両方: `with_updated_label` を先に適用、その `Record` に `with_updated_payload` を連鎖
    5. `repo.save(&vault)?` → `Ok(input.id)`
  - **注記**: `--kind` の kind 変更は Phase 1 スコープ外（`requirements.md` REQ-CLI-003）。`EditInput` は `kind: Option<RecordKind>` フィールドを持たない

## `shikomi_cli::usecase::remove`

- `pub fn remove_record(repo: &dyn VaultRepository, input: ConfirmedRemoveInput) -> Result<RecordId, CliError>`
  - 事前条件: 入力型が `ConfirmedRemoveInput`（`bool` フィールドなし）のため、`debug_assert!` 不要。`run()` のプロンプト経路 or `--yes` 経路を通ったことが型で保証される
  - 手順:
    1. `exists()` false なら `VaultNotInitialized`
    2. `repo.load()?` → 暗号化モードチェック
    3. `vault.remove_record(&input.id)` → `DomainError::VaultConsistencyError(RecordNotFound(_))` をキャッチして `CliError::RecordNotFound`
    4. `repo.save(&vault)?` → `Ok(input.id)`

## `shikomi_cli::presenter::list`

- `pub fn render_list(views: &[RecordView], locale: Locale) -> String`
  - 空なら `render_empty(locale)` を返す
  - それ以外: ヘッダ行 `ID\tKIND\tLABEL\tVALUE\n` と区切り行、各 `RecordView` を 1 行に整形
  - **ID カラムの出力ルール**: `view.id`（`RecordId`）の `Display` 実装で得られる **UUIDv7 全長 36 文字をそのまま出力する**。トランケート / 短縮形（`...` による省略） / prefix match 用の先頭抜粋**いずれも行わない**。理由: ペガサス指摘により案 A を採用（`requirements.md §REQ-CLI-001 / 出力フォーマット` 参照）。`list` で表示された ID を `remove --id` / `edit --id` にコピペしても `RecordId::try_from_str` を通過する導線を維持するため（短縮形採用すると Fail Fast で弾かれ、次の一手が詰む）。短縮表示は将来別 feature（`cli-list-short-id` 相当、未起票）で検討
  - ラベル / 値のトランケート: Unicode char 単位（`s.chars().take(40).collect::<String>()`）で簡易実装。CLI 表示のため grapheme 単位必須ではない。**ID はトランケート対象外**
  - 整形幅調整: `tabwriter` crate 採用可否は実装時に判断。導入するなら `../basic-design/index.md §依存` に追記し、`tech-stack.md` に反映。ID カラムは固定 36 文字幅で右パディング
  - `ValueView::Masked` → `"****"`
  - `ValueView::Plain(s)` → `s.chars().take(40).collect::<String>()` + 必要に応じ `"…"`
- `pub fn render_empty(locale: Locale) -> String`
  - `"no records\n"` / `"レコードはありません\n"`

## `shikomi_cli::presenter::success`

- `pub fn render_added(id: &RecordId, locale: Locale) -> String`
- `pub fn render_updated(id: &RecordId, locale: Locale) -> String`
- `pub fn render_removed(id: &RecordId, locale: Locale) -> String`
- `pub fn render_cancelled(locale: Locale) -> String`
- `pub fn render_initialized_vault(path: &Path, locale: Locale) -> String`

いずれも pure function、`String` を返すのみ。副作用なし。

## `shikomi_cli::presenter::error`

- `pub fn render_error(err: &CliError, locale: Locale) -> String`
  - `match err` で各バリアントを `MSG-CLI-xxx` テーブルに写像
  - `locale == English` なら英語 1 行 + hint 1 行の計 2 行
  - `locale == JapaneseEn` なら英語 1 行 + 日本語 1 行 + hint 英語 + hint 日本語 の計 4 行
  - **`err` 内に `PersistenceError` / `DomainError` が含まれていても、それらの `Display` 経由で secret 値が出ないことは `shikomi-core` / `shikomi-infra` 側で既に保証済み**（`SecretString::Debug` は `"[REDACTED]"` 固定、`PersistenceError` は path 名しか持たない）。本 presenter は追加 sanitization を行わない

## `shikomi_cli::presenter::warning`

- `pub fn render_shell_history_warning(locale: Locale) -> String`
  - MSG-CLI-050 の英語 + 必要に応じて日本語併記

## `shikomi_cli::io::terminal`

- `pub fn is_stdin_tty() -> bool`
  - 実装: `is_terminal::IsTerminal::is_terminal(&std::io::stdin())` を返すだけの薄い wrapper
  - 薄い wrapper にする理由: テストで差し替え可能にするため、将来 `dyn TerminalIo` trait 化する余地を残す（本 feature では trait 化しない、関数で十分）
- `pub fn read_line(prompt: &str) -> io::Result<String>`
  - `std::io::stdout` に prompt を書き（TTY でない場合も書く、リダイレクトで消えてもよい）、`std::io::stdin().read_line(&mut buf)` で 1 行取得
  - 末尾 `\n` / `\r\n` を trim
- `pub fn read_password(prompt: &str) -> io::Result<SecretString>`
  - `rpassword::prompt_password(prompt)` で非エコー入力（Unix `termios` / Windows `SetConsoleMode`）
  - 返り値を `SecretString::from_string(s)` に即座にラップ
  - **注記**: `rpassword::prompt_password` は内部で `String` を返すため、返却直前まで生文字列がスタックに存在する。`zeroize` は行わない（`../basic-design/security.md §依存 crate の CVE 確認結果 ※1` 参照）

## `shikomi_cli::io::paths`

- `pub fn resolve_os_default_vault_dir() -> Result<PathBuf, CliError>`
  - 手順:
    1. `dirs::data_dir()` を呼び出す
    2. `Some(base)` なら `Ok(base.join("shikomi"))`
    3. `None` なら `Err(CliError::Persistence(PersistenceError::CannotResolveVaultDir))`（既存 infra エラー型を流用）
  - **env を見ない**（`SHIKOMI_VAULT_DIR` の吸収は clap 側で完結、真実源の二重化防止 — ペテルギウス指摘 ④）
  - **フラグ値を見ない**（`run()` 側で `args.vault_dir.is_some()` 分岐を行い、`Some` の場合は本関数を呼ばない）

## `shikomi_cli::error`

- `pub enum CliError { ... }`（9 バリアント、`./data-structures.md §CliError バリアント詳細` 参照）
- `pub enum ExitCode { Success = 0, UserError = 1, SystemError = 2, EncryptionUnsupported = 3 }`
- `impl std::process::Termination for ExitCode { ... }`（`main()` と `run()` が返せるように）
- `impl From<PersistenceError> for CliError`
- `impl From<&CliError> for ExitCode`
- `impl fmt::Display for CliError` — 英語原文のみ
- `impl fmt::Debug for CliError` — derive

## `shikomi_cli::input`

- `pub struct AddInput { pub kind: RecordKind, pub label: RecordLabel, pub value: SecretString }`
- `pub struct EditInput { pub id: RecordId, pub label: Option<RecordLabel>, pub value: Option<SecretString> }`
- `pub struct ConfirmedRemoveInput { id: RecordId }`
  - **フィールドは `private`**（`id` は pub でない）。外部から直接構築を禁止
  - `pub fn ConfirmedRemoveInput::new(id: RecordId) -> Self` で構築（確認経由の文脈は呼び出し側責任）
  - `pub fn id(&self) -> &RecordId` でアクセサ（Tell, Don't Ask だが `remove` UseCase が id を知る必要があるため read-only アクセサ）

## `shikomi_cli::view`

- `pub struct RecordView { pub id: RecordId, pub kind: RecordKind, pub label: RecordLabel, pub value: ValueView }`
- `pub enum ValueView { Plain(String), Masked }`
- `impl RecordView { pub fn from_record(record: &Record) -> Self }`
  - `record.kind() == Secret` → `value = ValueView::Masked`
  - `record.kind() == Text` → `record.text_preview(40)` を呼んで `Some(s)` → `ValueView::Plain(s)`、`None`（本来到達しない） → `ValueView::Masked` にフォールバック
  - 本メソッドは `shikomi-cli` 内で `SecretString::expose_secret()` を**呼ばない**（`shikomi-core::Record::text_preview` に委譲、`./data-structures.md §ValueView の構築ルール`）

## `shikomi_cli::cli`

clap 派生型の詳細は `./clap-config.md` 参照。本書ではモジュール名と公開型名のみ列挙:

- `pub struct CliArgs` — `#[derive(Parser)]`
- `pub enum Subcommand` — `#[derive(Subcommand)]`
- `pub struct AddArgs` / `EditArgs` / `RemoveArgs` — `#[derive(Args)]`
- `pub enum KindArg` — `#[derive(ValueEnum, Clone)]`
- `impl From<KindArg> for RecordKind`

## `shikomi-core` への 1 メソッド追加（本 feature のコア変更）

`./data-structures.md §ValueView の構築ルール` で決定:

- `shikomi_core::Record::text_preview(&self, max_chars: usize) -> Option<String>`
  - `RecordKind::Text` かつ `RecordPayload::Plaintext(SecretString)` のときのみ `Some(先頭 max_chars の chars を collect した String)` を返す
  - それ以外（`Secret` kind / `Encrypted` variant）は `None`
  - 内部で `SecretString::expose_secret()` を呼ぶが、`shikomi-core` 内部で完結するため `shikomi-cli` の CI grep 対象には現れない
- 配置: `crates/shikomi-core/src/vault/record.rs` にメソッド追加
- テスト: `crates/shikomi-core/src/vault/record.rs` 末尾の `#[cfg(test)] mod tests` に UT 追加

**本追加の影響範囲**:

- 既存の `shikomi-core` ユニットテストは無変更で pass
- `shikomi-infra` のコードは無変更
- 本 feature の `RecordView::from_record` 実装を簡潔にし、かつ `crates/shikomi-cli/src/` で `expose_secret` を 0 回に抑える

**実装担当への注記**: `text_preview` の `max_chars` は `usize`。境界値は `0`（空 String を返す）/ `max` が String 長を超える場合（全体を返す）。grapheme 境界は考慮せず char 単位で切る（CLI preview 用途のため簡易で十分、UI では grapheme 対応要検討）。
