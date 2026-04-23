# 要件定義書

<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/requirements.md -->

## 機能要件

### REQ-CLI-001: `shikomi list`

| 項目 | 内容 |
|------|------|
| 入力 | サブコマンド `list`。フラグなし（将来 `--json` 等追加予定、本 feature では plain-text 表のみ） |
| 処理 | (1) vault path を解決、(2) `VaultRepository::load()` を呼んで `Vault` を取得、(3) `Vault::protection_mode()` が `Encrypted` なら Fail Fast（REQ-CLI-009）、(4) `Vault::records()` を走査し、各レコードを「ID / 種別 / ラベル / 値プレビュー」の表形式で stdout に 1 件 1 行出力、(5) Secret レコードの値は `"[secret — use `shikomi show` (future)]"` 等の**マスク文字列固定**で表示 |
| 出力 | stdout に表形式。カラム: `ID`（**UUIDv7 の全長 36 文字をそのまま出力。トランケート / 短縮しない** — ペガサス指摘により案 A 採用: MVP シンプル + `list` で表示した ID を `remove --id` にコピペした際 `RecordId::try_from_str` で即通る導線を保つ。短縮表示は将来別 feature `cli-list-short-id` で検討）/ `KIND`（`text` / `secret`）/ `LABEL`（最大 40 文字で切り詰め、省略記号付き）/ `VALUE`（Text は先頭 40 文字、Secret はマスク） |
| エラー時 | vault 未作成 → 終了コード 1 / 暗号化モード → 終了コード 3 / I/O エラー → 終了コード 2 |

### REQ-CLI-002: `shikomi add`

| 項目 | 内容 |
|------|------|
| 入力 | サブコマンド `add` + 必須フラグ `--kind <text\|secret>` + `--label <STRING>` + 値指定フラグいずれか 1 つ（`--value <STRING>` / `--stdin`）。`--yes` フラグは本コマンドでは無効（warn を出して無視） |
| 処理 | (1) vault path 解決、(2) `RecordLabel::try_new` で label 検証（Fail Fast）、(3) `--kind secret` かつ `--value` 指定の場合は stderr に shell 履歴漏洩警告を出す、(4) 値を `SecretString` として取得（Text kind も同じ経路で型安全に扱い、`SecretString` の中身は**一切 expose せず**所有権移動のみで `RecordPayload::Plaintext` まで運搬する — `basic-design/security.md §expose_secret 経路監査` 参照）、(5) `load()` → vault 未作成なら `Vault::new(VaultHeader::new_plaintext(...))` で初期化（REQ-CLI-010）、(6) モードが `Encrypted` なら Fail Fast（REQ-CLI-009）、(7) `Vault::add_record(Record::new(...))` を呼び、(8) `save()` で atomic write、(9) 成功時は新レコードの ID を stdout に 1 行出力 |
| 出力 | stdout: `added: <uuid>` の 1 行 / stderr: 警告（該当時のみ） |
| エラー時 | 値指定フラグの同時指定 / どちらも無指定 → 終了コード 1 / ラベル検証失敗 → 終了コード 1 / 暗号化モード → 終了コード 3 / save 失敗 → 終了コード 2 |

### REQ-CLI-003: `shikomi edit`

| 項目 | 内容 |
|------|------|
| 入力 | サブコマンド `edit` + 必須フラグ `--id <UUID>`、任意フラグ `--label <STRING>` / `--value <STRING>` / `--stdin` のうち**1 つ以上**。`--value` と `--stdin` の同時指定は Fail Fast。**`--kind` フラグは Phase 1 スコープ外**（下記「スコープ外注記」参照） |
| 処理 | (1) vault path 解決、(2) `RecordId::try_from_str` で id 検証、(3) `load()` → 暗号化モードなら Fail Fast、(4) `Vault::find_record` で対象レコード取得 → 無ければ Fail Fast、(5) label 更新時は `Record::with_updated_label(now)`、value 更新時は `Record::with_updated_payload(now)` を集約メソッド経由で呼ぶ（Tell, Don't Ask）、(6) `Vault::update_record` で置換、(7) `save()` で atomic write |
| 出力 | stdout: `updated: <uuid>` / stderr: shell 履歴警告（`--kind secret` でなく Text kind 既存レコードへの `--value` 更新時も、既存 kind が `Secret` なら警告対象） |
| エラー時 | 全フラグ未指定 / 併用禁止違反 / id 不存在 / 暗号化モード / save 失敗 → 各終了コード（REQ-CLI-006 参照） |

**スコープ外注記（`edit --kind` の扱い）**: `edit` サブコマンドでの `--kind` 指定（text ↔ secret 変換）は**本 feature のスコープ外**とする。理由: (a) kind 変更は `shikomi-core` の `Record` に `with_updated_kind` メソッド追加を要し、さらに「kind 変更時の payload 整合性（Text ↔ Secret の変換規則）」という新規ドメインルールの設計を伴うため、本 feature のスコープを越える（`text_preview` のような純粋な read-only アクセサ追加とは別物）、(b) Text → Secret 変換時に既存値の secret 扱い開始は UX として曖昧（過去の値が shell 履歴や clipboard 履歴に残っている可能性）。将来 feature（`cli-record-kind-migration` 等、未起票）で要件分析から取り組む。本 feature では `edit` の CLI に `--kind` フラグを**そもそも定義しない**（clap の不明フラグエラーで終了コード 1）。誤って `--kind` を書いたユーザには clap のエラーが案内として十分機能する。

### REQ-CLI-004: `shikomi remove`

| 項目 | 内容 |
|------|------|
| 入力 | サブコマンド `remove` + 必須フラグ `--id <UUID>`、任意フラグ `--yes`（対話確認スキップ） |
| 処理 | (1) vault path 解決、(2) `RecordId::try_from_str` で id 検証、(3) `load()` → 暗号化モードなら Fail Fast、(4) 対象レコードが存在するか `find_record` で確認 → 無ければ Fail Fast、(5) `--yes` 未指定かつ TTY 接続 → `"Delete record <uuid> (<label>)? [y/N]: "` を stdout に出力し 1 行読み込み、`y` / `Y` 以外は中止（終了コード 0、メッセージ「cancelled」）、(6) `--yes` 未指定かつ非 TTY → REQ-CLI-011 により終了コード 1 で即エラー、(7) `Vault::remove_record` → `save()` |
| 出力 | stdout: 確認プロンプト（TTY 時）/ `removed: <uuid>` / `cancelled` | 
| エラー時 | id 不存在 / 暗号化モード / 非 TTY で `--yes` 未指定 / save 失敗 → 各終了コード |

### REQ-CLI-005: vault path 解決

| 項目 | 内容 |
|------|------|
| 入力 | 環境変数 `SHIKOMI_VAULT_DIR`（clap attribute `#[arg(env = "SHIKOMI_VAULT_DIR")]` で `--vault-dir` へ自動フォールバック）/ グローバルフラグ `--vault-dir <PATH>` / OS デフォルト（`dirs::data_dir().join("shikomi")`） |
| 処理 | 優先順位: **`--vault-dir` フラグ > `SHIKOMI_VAULT_DIR` 環境変数（clap 経由）> OS デフォルト**。env の読取は **clap 単一ルート**とし、アプリ層で重ねて `std::env::var` を読まない（DRY / 真実源の一本化）。`--vault-dir` または env 由来の path が `Some` なら `SqliteVaultRepository::from_directory(&path)`（本 feature で新規追加する infra API）で構築、`None` なら OS デフォルトで構築 |
| 出力 | `SqliteVaultRepository` インスタンス |
| エラー時 | path 検証失敗 → `CliError::Persistence(PersistenceError::InvalidVaultDir{..})` → 終了コード 2 / path 未解決（OS デフォルトも取れない） → `CliError::Persistence(PersistenceError::CannotResolveVaultDir)` → 終了コード 2 |

**採用理由（案 B に確定）**: `std::env::set_var` を CLI 層で呼ぶ案 A は thread-unsafe で並列 E2E テストとの相性が悪い。`shikomi-infra` に**プリミティブ引数**の `SqliteVaultRepository::from_directory(path: &Path) -> Result<Self, PersistenceError>` を追加し、CLI はフラグ値 / env 値をそのまま渡す。`VaultPaths` は既存どおり `pub(crate)` のまま公開しない（公開 API 契約を増やさない、詳細設計 `detailed-design/infra-changes.md` 参照）。既存 `SqliteVaultRepository::new()` は内部的に OS デフォルトの `from_directory` を呼ぶリファクタとなる（Boy Scout Rule、既存テストは無変更）。

### REQ-CLI-006: 終了コード契約

| コード | 意味 | 典型例 |
|-------|------|-------|
| 0 | 成功（`remove` の `cancelled` も 0） | すべてのコマンド成功時、`remove` で y 以外を入力 |
| 1 | ユーザ入力エラー | フラグ不足 / 併用違反 / 不正 UUID / 不正ラベル / 存在しない id / vault 未作成（`list`/`edit`/`remove`）/ 非 TTY で `--yes` 未指定 |
| 2 | システムエラー | I/O / 権限 / SQLite / `PersistenceError` 全般 |
| 3 | 暗号化モード未対応 | 暗号化 vault を `list` / `add` / `edit` / `remove` しようとした |

### REQ-CLI-007: secret マスキング

| 項目 | 内容 |
|------|------|
| 対象 | Secret レコードの `RecordPayload::Plaintext(SecretString)` の中身 |
| マスキング箇所 | stdout / stderr / panic メッセージ / `tracing::{info,warn,error}!` マクロ / `Debug` trait 経由のすべての出力 |
| 保証機構 | (1) `SecretString` の `Debug` は `"[REDACTED]"` 固定（既存、`shikomi-core`）、(2) **`crates/shikomi-cli/src/` 内で `SecretString::expose_secret()` を呼ぶのは 0 箇所**（Secret は `SecretString` を**所有権移動のみで運搬**し `RecordPayload::Plaintext(secret)` に格納、Text の preview 生成は `shikomi-core::Record::text_preview` に委譲し `expose_secret` を core 内に封じる — 基本設計 `basic-design/security.md §expose_secret 経路監査` / 詳細設計 `detailed-design/data-structures.md §ValueView の構築ルール` 参照）、(3) `list` 出力時は Secret を文字列変換せず固定マスク文字列（`"****"`）を直接出力 |
| 検証 | E2E テストで、stdout / stderr を全文 grep して投入値（`"SECRET_TEST_VALUE"` 等のマーカー）が出ないこと |

### REQ-CLI-008: エラーメッセージ出力

| 項目 | 内容 |
|------|------|
| 出力先 | 必ず stderr（stdout は成功出力・データ出力専用） |
| 形式 | 2 行構成: 1 行目 `error: <原因文>` / 2 行目 `hint: <次の行動>`。日本語併記環境では 2 段表示（`LANG=C` / `LANG` 未設定 は英語のみ） |
| 原因文 / ヒント文 | 本文書「ユーザー向けメッセージ一覧」で `MSG-CLI-xxx` として定義 |
| panic の扱い | CLI の panic は `anyhow::Error` でキャッチし、上記形式の `error: internal bug; please report this issue to github.com/shikomi-dev/shikomi/issues` で出力。panic メッセージ本文は **stderr に出すが secret 値は露出しない** ことを保証（panic hook で `SecretString` 露出経路を監査する工程を結合テストで検証） |

### REQ-CLI-009: 暗号化 vault 拒否（Fail Fast）

| 項目 | 内容 |
|------|------|
| 入力 | `Vault::protection_mode() == ProtectionMode::Encrypted` の vault |
| 処理 | 即時 stderr に `MSG-CLI-103` を出力、終了コード 3 で終了。`load()` 後・CRUD 実行前の 1 箇所で判定（Tell, Don't Ask の Ask だが、モード選別は Vault 集約の責務外。CLI 側で 1 箇所に集約） |
| 出力 | stderr: `error: this vault is encrypted; encryption is not yet supported in this CLI version` + `hint: future "shikomi vault decrypt" will convert it; for now, use a plaintext vault` |
| エラー時 | — |

### REQ-CLI-010: vault 未初期化時の自動作成

| 項目 | 内容 |
|------|------|
| 入力 | `VaultRepository::exists() == false` の状態で `add` / `list` / `edit` / `remove` を実行 |
| 処理 | `add` のみ: `Vault::new(VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap())` で空 vault を構築してから `add_record` を追加し save（atomic write で初期化と追加が 1 トランザクション）。他コマンド: Fail Fast で終了コード 1 |
| 出力 | `add` の stdout: `initialized plaintext vault at <path>` （初回のみ）+ `added: <uuid>` / 他コマンドの stderr: `MSG-CLI-104` |
| エラー時 | — |

### REQ-CLI-011: 非 TTY での `remove` 確認プロンプト扱い + 型による確認強制

| 項目 | 内容 |
|------|------|
| 入力 | `remove --id <uuid>`（`--yes` なし）で stdin が TTY でない状況 |
| 処理 | 即時エラー（削除しない）。`is_terminal::IsTerminal` で stdin を判定し、false なら `MSG-CLI-105` を stderr に出力して終了コード 1 |
| 出力 | stderr: `error: refusing to delete without --yes in non-interactive mode` + `hint: re-run with --yes to confirm deletion` |
| 理由 | スクリプト誤実行で意図せぬ削除が発生することを Fail Fast で防ぐ（Unix の `rm` に `-i` を付けずスクリプト実行するのは一般慣習だが、本 CLI は削除が取り消せない性質を重視して**明示的確認を必須化**） |
| 型レベル強制 | UseCase の入力型は **`ConfirmedRemoveInput { id: RecordId }`**（タプル構造体でなく名前付きフィールドにして拡張余地を残す）。`confirmed: bool` フィールドは**持たない**。確認を経ていない呼び出しは**そもそもこの型を構築できない**ため、`debug_assert!` に頼らずコンパイル時に Fail Fast が成立する。`main` 側で TTY + プロンプト or `--yes` 指定の両経路を経て初めて `ConfirmedRemoveInput::new(id)` を構築できる（Parse, don't validate パターン） |

### REQ-CLI-012: Presenter / UseCase / Repository の 3 層分離

| 項目 | 内容 |
|------|------|
| 構造 | `shikomi-cli/src/` 直下を `main.rs`（clap パースとコンポジションルート）/ `presenter/`（出力整形・マスキング）/ `usecase/`（ドメイン操作の orchestration）/ `error.rs`（CLI エラー型）に分離 |
| 依存方向 | `main.rs` → `usecase` → `shikomi-core` + `shikomi-infra::VaultRepository` trait（具体型 `SqliteVaultRepository` の参照は `main.rs` のコンポジションルート 1 箇所のみ）／ `presenter` は `shikomi-core` の読み取り型のみ参照 |
| 狙い | Phase 2（daemon IPC 経由）への移行時、`run()` の 1 行（`SqliteVaultRepository::from_directory(&path)` を `IpcVaultRepository::connect(...)` に差し替え）で済ませる |

## 画面・CLI仕様

### サブコマンド一覧

```
shikomi                         ヘルプ表示
shikomi list                    vault 内のレコード一覧
shikomi add --kind <K> --label <L> (--value <V> | --stdin)
shikomi edit --id <ID> [--label <L>] [--value <V> | --stdin]    # --kind は Phase 1 スコープ外（未定義）
shikomi remove --id <ID> [--yes]
shikomi --help / shikomi <sub> --help
shikomi --version
```

### グローバルフラグ

| フラグ | 型 | 意味 | デフォルト |
|-------|---|------|----------|
| `--vault-dir <PATH>` | `PathBuf` | vault ディレクトリを上書き | `SHIKOMI_VAULT_DIR` 環境変数 → OS デフォルト |
| `-q, --quiet` | `bool` | 成功出力を抑止（stderr のみに出す。エラーは通常通り） | `false` |
| `-v, --verbose` | `bool` | `tracing` レベルを `debug` に上げる | `false`（`info`） |
| `-h, --help` | — | ヘルプ | — |
| `-V, --version` | — | バージョン | — |

### 出力フォーマット（`shikomi list`）

```
ID                                    KIND    LABEL                                     VALUE
--                                    ----    -----                                     -----
018f1234-5678-7abc-9def-123456789abc  text    SSH: prod                                 ssh -J bastion prod01
018f9abc-cdef-7012-8345-67890abcdef0  secret  my password                               ****
```

- **ID カラムは UUIDv7 の全長 36 文字をそのまま表示する**。省略形（`018f1234-...-7890` 等）は採用しない
- 理由: `list` で表示された ID を `remove --id` / `edit --id` にそのままコピペ可能にする（`RecordId::try_from_str` は全長 UUID を要求するため、短縮形だと Fail Fast で弾かれユーザの次の一手が詰む — ペガサス指摘による案 A 採用）
- 短縮表示は将来別 feature（`cli-list-short-id` 相当、未起票）で検討。`RecordId` の prefix match は domain 型の変更を伴うため Phase 1 スコープ外
- カラム幅は `--vault-dir` 指定 vault のレコード長に合わせて動的調整（ラベル / 値の truncate のみ。ID は固定 36 文字）

### 確認プロンプト（`shikomi remove`）

```
$ shikomi remove --id 018f1234-5678-7abc-9def-123456789abc
Delete record 018f1234-5678-7abc-9def-123456789abc (SSH: prod)? [y/N]: y
removed: 018f1234-5678-7abc-9def-123456789abc
```

ID は UUIDv7 の全長 36 文字。`list` 出力から直接コピペ可能（ペガサス指摘による案 A 採用の一貫性）。

`N`（デフォルト）で `cancelled` を stdout に出し終了コード 0。

## API仕様

本 feature は HTTP エンドポイントを持たない。CLI の外部 I/F は「サブコマンド + フラグ」と「終了コード」で表現される。以下に**内部公開 Rust API**を列挙する。

**`shikomi-cli` の crate 構成（決定事項）**: 本 feature で `shikomi-cli` を **`[lib] + [[bin]]` の 2 ターゲット構成**にする。

- `[lib] name = "shikomi_cli"`（`src/lib.rs`）— UseCase / Presenter / input / view / error などの**内部公開 Rust API** を提供。結合テスト（`tests/` 配下）から `shikomi_cli::usecase::add::add_record` 等を直接呼び出す目的。
- `[[bin]] name = "shikomi"`（`src/main.rs`）— `shikomi_cli::run()` を呼ぶ薄い bin。コンポジションルートは `lib` 側の `run()` に移す。
- **外部公開契約ではない**: lib の全 `pub` 項目は `#[doc(hidden)]` 属性を付け、`cargo doc` で隠す。workspace `publish = false` は既存のまま維持し、crates.io 公開はしない。ABI / API 安定化の保証は行わない（将来 bin 内部実装変更でシグネチャ変更可能）。
- 判断理由: `[[bin]]` のみの場合、結合テストから UseCase の pure 関数を呼べず、E2E（プロセス起動）に過度に依存する。`lib` 化することで結合テストが書けるようになり、テストピラミッドの中段（Integration）を厚くできる（テスト設計との整合）。`doc(hidden)` により「crate 内部構造の公開 = 外部契約化」の副作用を抑止する。

**内部公開 API 一覧**:

| モジュール | 公開型 / 関数 | 用途 |
|----------|-------------|------|
| `shikomi_cli::usecase::list` | `fn list_records(repo: &dyn VaultRepository) -> Result<Vec<RecordView>, CliError>` | list 操作の orchestration |
| `shikomi_cli::usecase::add` | `fn add_record(repo: &dyn VaultRepository, input: AddInput, now: OffsetDateTime) -> Result<RecordId, CliError>` | add 操作 |
| `shikomi_cli::usecase::edit` | `fn edit_record(repo: &dyn VaultRepository, input: EditInput, now: OffsetDateTime) -> Result<RecordId, CliError>` | edit 操作 |
| `shikomi_cli::usecase::remove` | `fn remove_record(repo: &dyn VaultRepository, input: ConfirmedRemoveInput) -> Result<RecordId, CliError>` | remove 操作（確認プロンプトは外側責務。**`ConfirmedRemoveInput` は「確認済み」を型で表現** — 下記 REQ-CLI-011 注記） |
| `shikomi_cli::presenter::list` | `fn render_list(records: &[RecordView], locale: Locale) -> String` | 表整形 |
| `shikomi_cli::presenter::error` | `fn render_error(err: &CliError, locale: Locale) -> String` | エラーメッセージ整形 |
| `shikomi_cli::error` | `enum CliError`, `enum ExitCode` | CLI エラー型と終了コード写像 |
| `shikomi_cli::input` | `struct AddInput`, `struct EditInput`, `struct ConfirmedRemoveInput` | UseCase への入力 DTO |
| `shikomi_cli::view` | `struct RecordView`, `enum ValueView` | Presenter への出力 DTO（Secret は `ValueView::Masked`、Text は `ValueView::Plain(String)`） |
| `shikomi_cli` | `fn run() -> ExitCode` | コンポジションルート（clap パース + ディスパッチ + 終了コード）。bin の `main.rs` はこれを呼ぶだけ |

**依存方向の厳守**:

- `usecase` → `shikomi-core` + `shikomi-infra::VaultRepository`（**trait のみ**、具体実装は依存しない）
- `presenter` → `shikomi-core`（読み取り型のみ）
- `run()` / `main.rs` → `usecase` + `presenter` + `shikomi-infra::SqliteVaultRepository`（具体型参照はコンポジションルート 1 箇所のみ）
- `presenter` と `usecase` の相互参照は禁止（単方向）

## データモデル

本 feature は独自の永続化スキーマを持たない（vault.db は既存 `shikomi-infra` のスキーマを流用）。CLI 層で扱う**一時的な入出力 DTO**を列挙する。

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| AddInput | kind | `RecordKind` | 必須 | — |
| AddInput | label | `RecordLabel` | 必須、検証済み | — |
| AddInput | value | `SecretString` | 必須、`SecretString` で受取 | — |
| EditInput | id | `RecordId` | 必須、検証済み | — |
| EditInput | label | `Option<RecordLabel>` | 任意、検証済み | — |
| EditInput | value | `Option<SecretString>` | 任意 | — |
| ConfirmedRemoveInput | id | `RecordId` | 必須（**`confirmed: bool` フィールドは持たない**。型の存在そのものが「確認済み」を表す — REQ-CLI-011） | — |
| RecordView | id | `RecordId` | 必須 | Record から射影 |
| RecordView | kind | `RecordKind` | 必須 | — |
| RecordView | label | `RecordLabel` | 必須 | — |
| RecordView | value | `ValueView` | 必須（Secret は `Masked`、Text は `Plain`） | — |
| ValueView | — | `enum { Plain(String), Masked }` | — | — |
| CliError | — | `enum` | 下表参照 | — |
| ExitCode | — | `enum { Success=0, UserError=1, SystemError=2, EncryptionUnsupported=3 }` | — | — |

**注記**: 空構造体 `ListInput` / `EditInput.kind` は**定義しない**（YAGNI）。`list` UseCase は引数に `repo: &dyn VaultRepository` のみ取り、将来 `--json` / `--filter` 等のフラグが追加される際に**その時点で**入力 DTO を導入する（空構造体の予約は不採用）。`edit --kind` は Phase 1 スコープ外のため `EditInput` からも除外（REQ-CLI-003 注記）。

**`CliError` バリアント**（詳細設計で thiserror フィールドを確定）:

| バリアント | 用途 | 写像される ExitCode |
|-----------|------|------------------|
| `UsageError(String)` | clap の usage error / フラグ併用違反 | `UserError (1)` |
| `InvalidLabel(DomainError)` | `RecordLabel::try_new` 失敗 | `UserError (1)` |
| `InvalidId(DomainError)` | `RecordId::try_from_str` 失敗 | `UserError (1)` |
| `RecordNotFound(RecordId)` | 対象 id が vault に存在しない | `UserError (1)` |
| `VaultNotInitialized(PathBuf)` | `list` / `edit` / `remove` で vault.db 不在 | `UserError (1)` |
| `NonInteractiveRemove` | TTY でない stdin で `--yes` 未指定 | `UserError (1)` |
| `Persistence(PersistenceError)` | infra 由来の I/O / SQLite / Lock エラー | `SystemError (2)` |
| `Domain(DomainError)` | Vault 集約の整合性エラー（id 重複等） | `SystemError (2)` |
| `EncryptionUnsupported` | 暗号化 vault を検出 | `EncryptionUnsupported (3)` |

## ユーザー向けメッセージ一覧

### 成功系（stdout）

| ID | メッセージ（英語） | メッセージ（日本語） | 表示条件 |
|----|----------------|------------------|---------|
| MSG-CLI-001 | `added: {id}` | `追加しました: {id}` | `add` 成功 |
| MSG-CLI-002 | `updated: {id}` | `更新しました: {id}` | `edit` 成功 |
| MSG-CLI-003 | `removed: {id}` | `削除しました: {id}` | `remove` 成功 |
| MSG-CLI-004 | `cancelled` | `キャンセルしました` | `remove` で y 以外入力 |
| MSG-CLI-005 | `initialized plaintext vault at {path}` | `平文 vault を {path} に初期化しました` | vault 未作成時の `add` 初回成功 |

### 警告系（stderr）

| ID | メッセージ（英語） | メッセージ（日本語） | 表示条件 |
|----|----------------|------------------|---------|
| MSG-CLI-050 | `warning: '--value' for a secret leaks into shell history; prefer '--stdin'` | `警告: secret を --value で渡すと shell 履歴に残ります。--stdin を推奨します` | `add --kind secret --value` / `edit --kind secret --value`  |

### エラー系（stderr、`error:` 接頭辞 + `hint:` 行）

| ID | 原因文（英語） | 原因文（日本語） | ヒント（英語 / 日本語） | 表示条件 | 終了コード |
|----|-------------|--------------|-------------------|---------|---------|
| MSG-CLI-100 | `error: '--value' and '--stdin' cannot be used together` | `error: --value と --stdin は同時に使えません` | `hint: choose one` / `hint: どちらか一方を指定してください` | `add` / `edit` で値指定フラグ競合 | 1 |
| MSG-CLI-101 | `error: invalid label: {reason}` | `error: 不正なラベル: {reason}` | `hint: labels must be non-empty and at most 255 graphemes; control chars except \t\n\r are not allowed` / `hint: ラベルは 1 文字以上 255 grapheme 以下で、\t\n\r 以外の制御文字は禁止です` | `RecordLabel::try_new` 失敗 | 1 |
| MSG-CLI-102 | `error: invalid record id: {reason}` | `error: 不正なレコード ID: {reason}` | `hint: use the uuid shown by "shikomi list"` / `hint: "shikomi list" で表示された UUID を指定してください` | `RecordId::try_from_str` 失敗 | 1 |
| MSG-CLI-103 | `error: this vault is encrypted; encryption is not yet supported in this CLI version` | `error: この vault は暗号化されています。本バージョンの CLI は暗号化モード未対応です` | `hint: future "shikomi vault decrypt" will convert it; for now, use a plaintext vault` / `hint: 将来の "shikomi vault decrypt" で変換可能になります。暫定的には平文 vault をご利用ください` | `Vault::protection_mode() == Encrypted` | 3 |
| MSG-CLI-104 | `error: vault not initialized at {path}` | `error: vault が初期化されていません: {path}` | `hint: run "shikomi add" to create a plaintext vault` / `hint: "shikomi add" で平文 vault を初期化できます` | `exists() == false` で `list`/`edit`/`remove` | 1 |
| MSG-CLI-105 | `error: refusing to delete without --yes in non-interactive mode` | `error: 非対話モードでは --yes なしの削除を拒否します` | `hint: re-run with --yes to confirm deletion` / `hint: 削除を確認するには --yes を付けて再実行してください` | 非 TTY で `remove --yes` 未指定 | 1 |
| MSG-CLI-106 | `error: record not found: {id}` | `error: レコードが見つかりません: {id}` | `hint: check with "shikomi list"` / `hint: "shikomi list" で確認してください` | `edit`/`remove` で id 不存在 | 1 |
| MSG-CLI-107 | `error: failed to access vault: {reason}` | `error: vault へのアクセスに失敗しました: {reason}` | `hint: check permissions of {path} and re-run` / `hint: {path} のパーミッションを確認して再実行してください` | `PersistenceError::Io` / `Locked` / `Permission` | 2 |
| MSG-CLI-108 | `error: vault is corrupted: {reason}` | `error: vault が破損しています: {reason}` | `hint: restore from backup or start a new vault` / `hint: バックアップから復元するか、新規 vault を作成してください` | `PersistenceError::Corrupted` | 2 |
| MSG-CLI-109 | `error: internal bug: {reason}` | `error: 内部バグ: {reason}` | `hint: please report this issue to https://github.com/shikomi-dev/shikomi/issues` / `hint: https://github.com/shikomi-dev/shikomi/issues に報告してください` | panic / 予期せぬ `DomainError` | 2 |

**i18n 切替規則**: `LANG` 環境変数が `ja_JP*` / `ja` で始まる場合は日本語併記、それ以外（`C` / `en_*` / 未設定）は英語のみ。将来の `--locale` フラグ導入余地を残して `presenter::Locale` enum で抽象化。

## 依存関係

| crate | バージョン | feature | 用途 |
|-------|----------|--------|------|
| `clap` | 4.x | `derive`, `env`（env var 連携）, `wrap_help` | サブコマンド / フラグ定義 |
| `anyhow` | 1.x | — | アプリ層のエラーラップ（main.rs の戻り値） |
| `thiserror` | 2.x（既存） | — | `CliError` 定義 |
| `is-terminal` | 0.4.x | — | stdin / stdout の TTY 判定（`remove` 確認プロンプト / i18n 切替のスマート化） |
| `rpassword` | 7.x | — | 非エコー stdin 入力（secret 値）|
| `shikomi-core` | workspace path | — | ドメイン型 |
| `shikomi-infra` | workspace path | — | `VaultRepository` trait / `SqliteVaultRepository`（`from_directory` 新規追加）/ `PersistenceError`。**`VaultPaths` は公開しない**（プリミティブ引数 `&Path` で受け取る設計） |
| `time` | 0.3.x（workspace 既存） | `serde`, `macros` | `OffsetDateTime` — UseCase に `now` を引数で渡すため（`OffsetDateTime::now_utc()` 呼び出しは `main` 側に隔離し、UseCase を pure に保つ）|
| `assert_cmd` | 2.x（dev） | — | E2E テストのプロセス実行 |
| `predicates` | 3.x（dev） | — | `assert_cmd` のアサーション |
| `tempfile` | 3.x（dev、workspace 既存） | — | E2E テストの独立 vault ディレクトリ |

全て `Cargo.toml` ルートの `[workspace.dependencies]` 経由で指定し、`crates/shikomi-cli/Cargo.toml` では `{ workspace = true }` で参照する（`docs/architecture/tech-stack.md` §4.4）。

## 関連 feature

| feature | 関係 | 参照先 |
|---------|------|--------|
| `vault`（Issue #7） | 本 feature は `shikomi-core` の `Vault` / `Record` / `RecordLabel` / `RecordId` / `SecretString` / `ProtectionMode` / `DomainError` を利用する。**本 feature で `shikomi-core::Record` に `text_preview(&self, max_chars: usize) -> Option<String>` メソッドを 1 つ追加する**（`expose_secret` を `shikomi-core` 内に封じ込め、`shikomi-cli/src/` からの expose 呼び出しを 0 件に抑える目的。詳細設計 `detailed-design/public-api.md §shikomi-core への 1 メソッド追加` / `detailed-design/data-structures.md §ValueView の構築ルール` 参照）。それ以外の `Vault` / `RecordLabel` / `RecordId` / `SecretString` / `ProtectionMode` / `DomainError` は**変更せずそのまま組み立てる**。`with_updated_kind` 等の新規メソッド追加は本 feature では行わない（`edit --kind` が Phase 1 スコープ外の理由、REQ-CLI-003 注記参照） | `docs/features/vault/` |
| `vault-persistence`（Issue #10） | 本 feature は `VaultRepository` trait の呼び出し側。`SqliteVaultRepository::from_directory(path: &Path) -> Result<Self, PersistenceError>` を新規追加（既存の `new()` は内部で `from_directory(OS_default)` を呼ぶリファクタ、既存テストは無変更）。**`VaultPaths` は公開しない**（crate 内部実装のまま、公開 API 契約を増やさない）。`PersistenceError` を `CliError::Persistence(...)` でラップ | `docs/features/vault-persistence/` |
| `workspace-init`（Issue #4） | 本 feature は `shikomi-cli` crate 内に実体を積む。既存の `fn main() {}` を置換 | `docs/features/workspace-init/` |
| **未起票 — cli-daemon-bridge（Phase 2 相当）** | 将来、`IpcVaultRepository` を `shikomi-infra` に追加し、`shikomi-cli/src/main.rs` のコンポジションルートを差し替える。本 feature のレイヤ分離がそのまま活きる | （将来 Issue） |
| **未起票 — cli-vault-encryption** | `shikomi vault encrypt` / `decrypt` は別 feature。本 feature の `MSG-CLI-103` が誘導先となる | （将来 Issue） |

## アーキテクチャ文書への影響

本 feature は `docs/architecture/context/process-model.md` §4.1 ルール1 と**正面衝突**する（「CLI/GUI は直接 vault を開かない」原則）。キャプテン決定により、以下を同一 PR で更新する:

- **`docs/architecture/context/process-model.md` §4.1**: MVP Phase 1（CLI 直結）/ Phase 2（daemon 経由）のフェーズ区分を追記
- **`docs/architecture/tech-stack.md` §2.1 / §4.4**: CLI パーサ `clap` は既記載のまま。`anyhow` / `is-terminal` / `rpassword` / `assert_cmd` / `predicates` を `[workspace.dependencies]` に追加する旨は basic-design.md に記述

`docs/architecture/context/overview.md` / `threat-model.md` / `nfr.md` への変更は**発生しない**。
