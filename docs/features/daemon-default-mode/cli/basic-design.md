# 基本設計書 — daemon-default-mode / cli（モジュール契約）

<!-- feature: daemon-default-mode / sub-feature: cli / Issue #126 -->
<!-- 配置先: docs/features/daemon-default-mode/cli/basic-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature モジュール契約）-->
<!-- 親: ../feature-spec.md -->

## §モジュール契約（機能要件）

### REQ-DDM-001: `CliArgs::no_ipc` フィールド追加・`ipc` フィールド廃止

| 項目 | 内容 |
|------|------|
| 入力 | `--no-ipc` グローバルフラグ（clap `bool`、デフォルト `false`）|
| 処理 | `CliArgs` から `pub ipc: bool` フィールドを削除し、`pub no_ipc: bool` フィールドを追加する。clap の `#[arg(long = "no-ipc", global = true)]` で公開。`--ipc` フラグは廃止（認識不可）|
| 出力 | `args.no_ipc == false`（デフォルト）→ IPC 経路 / `args.no_ipc == true` → SQLite 直結経路 |
| エラー時 | ユーザーが `--ipc` を指定 → clap が `error: unexpected argument '--ipc'` を stderr に出力して exit 2 |
| 設計原則 | Fail Fast（旧フラグを受け入れず即失敗）/ YAGNI（後方互換ラッパなし）|

### REQ-DDM-002: `build_handle` の分岐反転（IPC 既定化）

| 項目 | 内容 |
|------|------|
| 入力 | `args.no_ipc: bool` / `args.vault_dir: Option<PathBuf>` |
| 処理 | `args.no_ipc == false`（既定）→ `RepositoryHandle::Ipc(IpcVaultRepository::connect(...))` / `args.no_ipc == true` → `RepositoryHandle::Sqlite(SqliteVaultRepository::from_directory(...))` |
| 出力 | `Result<RepositoryHandle, CliError>` |
| エラー時（IPC 経路）| 接続失敗 → `CliError::DaemonNotRunning` → `MSG-CLI-110` + exit 1 / プロトコル不一致 → `CliError::ProtocolVersionMismatch` → `MSG-CLI-111` + exit 1 |
| エラー時（SQLite 経路）| vault dir 解決失敗 / DB 初期化失敗 → 既存 `PersistenceError` 写像（変更なし）|
| 設計原則 | Composition Root が経路選択の唯一の責務者（`cli-vault-commands` 設計踏襲）|

### REQ-DDM-003: `MSG-CLI-051` 廃止

| 項目 | 内容 |
|------|------|
| 入力 | 該当なし（廃止）|
| 処理 | `presenter::warning::render_ipc_opt_in_notice` 関数を削除する。`build_handle` 内の `MSG-CLI-051` 出力コードを削除する |
| 出力 | IPC 経路採用時に警告は出力されない（IPC が既定のため不要）|
| エラー時 | 該当なし |
| 設計原則 | YAGNI（IPC が既定となったため警告は意味を失った）|

### REQ-DDM-004: `MSG-CLI-110` hint 文面更新

| 項目 | 内容 |
|------|------|
| 入力 | daemon 接続失敗（`CliError::DaemonNotRunning`）|
| 処理 | `MSG-CLI-110` の hint 行から `--ipc` フラグへの言及を削除する。daemon 起動コマンドのみを案内する（文面変更、ID `MSG-CLI-110` は維持）|
| 出力（英語）| `error: shikomi-daemon is not running (socket {path} unreachable)` + hint 行（下表参照）|
| 出力（日本語）| `error: shikomi-daemon が起動していません（ソケット {path} に接続できません）` + hint 行 |
| エラー時 | 該当なし（エラーメッセージ自体の変更）|
| 設計原則 | `cli-vault-commands` の MSG-CLI 規約を維持 |

#### MSG-CLI-110 新 hint 文面（Phase 2）

| OS | hint 行（英語） |
|----|--------------|
| Linux / macOS 基本 | `hint: start the daemon with: shikomi-daemon &` |
| macOS launchd（Sub-B 完了後追記予定）| `hint: or enable autostart: shikomi daemon enable-autostart` |
| Windows | `hint: start the daemon with: Start-Process -NoNewWindow shikomi-daemon` |

**Phase 2 過渡期（Sub-B 未完了）**: Sub-B（Issue #127）完了まで、autostart hint は出力しない。3 OS の手動起動コマンドのみ案内する。Sub-B 完了後に hint 文面を更新する別 PR を立てる。

### REQ-DDM-005: vault サブコマンドの IPC 強制を `--no-ipc` から保護

| 項目 | 内容 |
|------|------|
| 入力 | `Subcommand::Vault(_)` + `args.no_ipc == true` |
| 処理 | vault サブコマンド経路は `args.no_ipc` フラグを**無視**して IPC 強制を維持する（既存 `connect_vault_ipc` 関数は変更なし）。`args.no_ipc == true` を検出した時点で `MSG-CLI-052`（note 行）を stderr に先行出力してから IPC 接続処理に進む |
| 出力 | (1) `quiet == false` かつ `args.no_ipc == true` の場合 → `MSG-CLI-052` を stderr に出力してから IPC 経由での vault 操作を続行 / (2) `quiet == true` の場合 → `MSG-CLI-052` を抑止して IPC 経由での vault 操作のみ実行 |
| エラー時 | daemon 未起動 → `MSG-CLI-052` 出力後に `MSG-CLI-110` + exit 1（`--no-ipc` が指定されていても同じ）|
| 設計原則 | vault 管理は daemon の責務（Phase 2 設計規定 / `process-model.md §4.1`）/ Tell, Don't Ask（ユーザーのフラグ指定に対してシステムが沈黙で上書きしない）|

## ユーザー向けメッセージ一覧

### 廃止するメッセージ

| ID | 廃止理由 |
|----|---------|
| MSG-CLI-051 | IPC が既定となったため opt-in 警告が不要 |

**MSG-CLI-051 の ID は再利用禁止**（`daemon-ipc/detailed-design/future-extensions.md §バイナリ正規形仕様` の ID 固定契約に準じる）。

### 新規追加するメッセージ

| ID | メッセージ（英語） | メッセージ（日本語） | 表示条件 |
|----|----------------|------------------|---------|
| MSG-CLI-052 | `note: vault commands always use IPC; --no-ipc does not apply` | `注: vault サブコマンドは常に IPC 経由です。--no-ipc は適用されません` | `Subcommand::Vault(_)` かつ `args.no_ipc == true` かつ `quiet == false` の時（情報通知、終了コード 0 維持。`--quiet` で抑止）|

### 更新するメッセージ

| ID | 変更前（Phase 1） | 変更後（Phase 2） |
|----|----------------|----------------|
| MSG-CLI-110 hint | `--ipc` オプションへの言及あり | `--ipc` 言及を削除、daemon 起動コマンドのみ案内 |

### 影響を受けないメッセージ

- MSG-CLI-001〜005（レコード操作成功系）: 変更なし
- MSG-CLI-110 原因文・MSG-CLI-111（エラー原因・ヒント体裁）: ID・フォーマット変更なし
- MSG-CLI-100〜109 / MSG-CLI-112 以降: 変更なし

## 依存関係・前提条件

| 依存先 | 理由 |
|--------|------|
| `daemon-ipc` feature 完了（Issue #26 / #30）| IPC プロトコル / `IpcVaultRepository` / `RepositoryHandle` が既に実装済みであること |
| Issue #89（daemon-hotkey-clipboard）完了 | Phase 2 移行のトリガー条件（`feature-spec.md §7` / `daemon-hotkey-clipboard/feature-spec.md §7`）|

## セキュリティ考慮（Sub-A スコープ）

→ `cli/security.md` 参照。本 sub-feature のセキュリティ設計（脅威モデル / OWASP Top 10 / CI 監査ゲート）はすべて `cli/security.md` に一元化している。`basic-design.md` にインライン記述すると `security.md` との二重管理・矛盾が生じるため、ここには記述しない。

## テスト戦略（テスト設計 Issue で詳細化）

| テストレベル | 観点 |
|-------------|------|
| UT | `CliArgs` パース: `--no-ipc` → `no_ipc = true`、`--ipc` → clap error |
| UT | `build_handle`: デフォルト → IPC 経路、`no_ipc = true` → Sqlite 経路 |
| IT | `shikomi list`（daemon mock）→ IPC 経路が使われること |
| IT | `shikomi --no-ipc list` → Sqlite 経路が使われること |
| E2E | AC-DDM-01 〜 AC-DDM-06（`../feature-spec.md §5 受入基準`）|
