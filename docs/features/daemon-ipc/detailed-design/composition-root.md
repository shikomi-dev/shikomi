# 詳細設計書 — composition-root（daemon run / cli run --ipc 分岐 / panic hook）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/composition-root.md -->
<!-- 兄弟: ./index.md, ./protocol-types.md, ./daemon-runtime.md, ./ipc-vault-repository.md, ./lifecycle.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。処理順序は**番号付き箇条書き**で表現する。

## `shikomi_daemon::run` のシグネチャ

- 配置: `crates/shikomi-daemon/src/lib.rs`
- シグネチャ: `pub async fn run() -> ExitCode`
- 戻り値: Rust の `std::process::Termination` trait で `ExitCode` を返す（`ExitCode::from(<u8>)`、`shikomi-daemon` 独自の `pub enum ExitCode { Success = 0, SystemError = 1, SingleInstanceUnavailable = 2, EncryptionUnsupported = 3 }` を定義）

### 処理順序

1. **panic hook 登録**: `panic_hook::install()` を最初に呼ぶ（`tokio` ランタイム初期化前、依存 crate 初期化での panic を捕捉）
2. **tracing_subscriber 初期化**: `tracing_subscriber::fmt().with_env_filter(EnvFilter::try_from_env("SHIKOMI_DAEMON_LOG").unwrap_or_else(|_| EnvFilter::new("info"))).with_target(true).init();`
3. **vault dir 解決**: `let vault_dir = resolve_vault_dir()?;`（既存 `cli-vault-commands` の解決ロジックを daemon 側で再利用、内部で `SHIKOMI_VAULT_DIR` env / OS デフォルト）
4. **シングルインスタンス先取り**: `let single_instance = SingleInstanceLock::acquire(&socket_dir)?;`
   - 失敗時: `tracing::error!(target: "shikomi_daemon::lifecycle", "single instance acquisition failed: {}", err);` → `ExitCode::from(2)` で early return
5. **repo 構築**: `let repo = SqliteVaultRepository::from_directory(&vault_dir)?;`
   - 失敗時: `tracing::error!("failed to construct SqliteVaultRepository: {}", err);` → `ExitCode::from(1)` early return
6. **vault load**: `let vault = repo.load()?;`
   - 失敗時: `tracing::error!("failed to load vault: {}", err);` → `ExitCode::from(1)` early return
7. **暗号化モード検証**: `if vault.protection_mode() == ProtectionMode::Encrypted { tracing::error!("vault is encrypted; daemon does not support encrypted vaults yet (Issue #26 scope-out)"); return ExitCode::from(3); }`
8. **共有データ構造の構築**:
   - `let repo = Arc::new(repo);`
   - `let vault = Arc::new(tokio::sync::Mutex::new(vault));`
   - `let shutdown = Arc::new(tokio::sync::Notify::new());`
9. **listener 取得**: `single_instance` から OS 別の listener を取得（unix: `UnixListener`、windows: `NamedPipeServer`）
10. **IpcServer 起動**: `let mut server = IpcServer::new(listener_enum, repo.clone(), vault.clone());`
11. **シグナルハンドラ spawn**: `tokio::spawn(async move { lifecycle::shutdown::wait_for_signal(shutdown.clone()).await; });`
12. **server 実行**: `let server_result = server.start_with_shutdown(shutdown.clone()).await;`
    - `tokio::select!` で `server.start()` と `shutdown.notified()` を競合
    - shutdown 通知 → `server.join_all().await`（in-flight 完了待機）
13. **graceful cleanup**:
    - `vault` Drop（`Arc` 最後の参照消失で `Mutex<Vault>` ドロップ）
    - `repo` Drop → `VaultLock` 解放（既存 `shikomi-infra::persistence::lock::VaultLock` の `Drop`）
    - `single_instance` Drop（ソケット削除 + flock 解放、`./lifecycle.md §SingleInstanceLock の Drop`）
14. **return**:
    - 通常終了 → `tracing::info!("graceful shutdown complete");` → `ExitCode::from(0)`
    - サーバエラー → `tracing::error!("server error: {}", err);` → `ExitCode::from(1)`

## `main.rs`（daemon bin エントリ）

- 配置: `crates/shikomi-daemon/src/main.rs`
- 内容（**実装本体は書かない**、責務のみ記述）:
  - `#[tokio::main(flavor = "multi_thread")]` 属性付き `async fn main() -> ExitCode`
  - 中身: `shikomi_daemon::run().await` の戻り値を return するのみ
- **設計意図**: bin 側にロジックを置かない。`lib.rs::run()` は結合テスト（`tokio::test`）で `#[tokio::test] async fn test_run_lifecycle() { ... }` のように呼べる。E2E は `assert_cmd::Command::cargo_bin("shikomi-daemon")` で bin プロセス起動経由

## `panic_hook::install`（daemon 側）

- 配置: `crates/shikomi-daemon/src/panic_hook.rs`
- シグネチャ: `pub fn install()`
- 処理:
  1. `std::panic::set_hook(Box::new(panic_hook_fn));`
- `panic_hook_fn` の処理（CLI と同型 fixed-message、`./basic-design/security.md §panic hook と secret 漏洩経路の遮断` 参照）:
  1. `info.payload()` / `info.message()` / `info.location()` を**一切参照しない**
  2. `tracing::error!` / `tracing::warn!` / `tracing::info!` / `tracing::debug!` / `tracing::trace!` を**呼ばない**
  3. `eprintln!` で固定文言のみ stderr 出力:
     - 英語: `"error: shikomi-daemon internal bug\nhint: please report this issue to https://github.com/shikomi-dev/shikomi/issues\n"`
     - **日本語併記**: daemon 側は **英語のみ**（運用者向け、i18n 不要）。CLI の `Locale` 仕組みを daemon に持ち込まない（YAGNI）
  4. return（panic unwind 続行、Rust ランタイムが exit 101）

**設計判断**:
- daemon は CLI と異なり、エンドユーザに直接日本語表示する場面が無い（運用者向けログ）。固定文言は英語のみ
- `OnceLock<Locale>` は不要（CLI 側のみ）

**終了コードの扱い**:
- Rust の panic 既定終了コードは `101`。daemon 独自の `ExitCode` enum とは一致しない
- `catch_unwind` で `run()` 全体をラップして揃える案もあるが、`UnwindSafe` trait 境界要求のためコストが見合わず**実装しない**
- 受入基準では daemon panic 時の終了コードを厳密検証しない（panic を意図的に起こすテストが困難）

**バックトレース抑止**:
- `RUST_BACKTRACE=1` 設定時の挙動は CLI と同等（関数名 / ソースファイル位置は出るが、ローカル変数値は出ない）
- `./future-extensions.md §運用注記` で「CI 環境では `RUST_BACKTRACE=0` を推奨」と記す

## `shikomi_cli::run` の編集（既存編集、`--ipc` 分岐追加）

`cli-vault-commands` で実装済みの `shikomi_cli::run() -> ExitCode` に **`--ipc` 分岐**を追加する。

### 既存処理（変更なし）

`cli-vault-commands` の `composition-root.md §処理順序` 1〜6 ステップは**そのまま維持**:

1. panic hook 登録
2. Locale 決定と OnceLock 格納
3. tracing_subscriber 初期化
4. clap パース（`CliArgs` には本 feature で `ipc: bool` フィールドを追加、`./clap-config.md` の編集詳細）
5. tracing レベル再設定
6. （次節で編集）

### 編集箇所: ステップ 6（Repository 構築の `--ipc` 分岐）

既存の Repository 構築処理を以下に置換:

```
6. Repository 構築:
   - args.ipc == false（既定）→ 既存処理:
     - args.vault_dir.is_some() → SqliteVaultRepository::from_directory(&path)
     - args.vault_dir.is_none() → io::paths::resolve_os_default_vault_dir()? → SqliteVaultRepository::from_directory(&os_default)
   - args.ipc == true → IPC 経路:
     - let socket_path = io::ipc_vault_repository::IpcVaultRepository::default_socket_path()?;
     - let repo = io::ipc_vault_repository::IpcVaultRepository::connect(&socket_path)?;
     - 失敗時: render_error + ExitCode::from(&err)（CliError::DaemonNotRunning / ProtocolVersionMismatch 経由）
   - quiet == false かつ args.ipc == true → MSG-CLI-051（warning: --ipc is opt-in for Phase 2 migration）を stderr に出力
```

**設計判断**:
- Repository は `Box<dyn VaultRepository>` で抽象化して保持
- 既存の `usecase::list::list_records(&*repo)` 等の呼び出しは無変更（trait 越しのアクセスのみ）
- これが `cli-vault-commands` の Phase 2 移行契約「`run()` の 1 行差し替え」の**実体化**

### `Box<dyn VaultRepository>` 化の影響

`cli-vault-commands` の現状実装は `let repo: SqliteVaultRepository = SqliteVaultRepository::from_directory(...)?;` のような具体型保持と推測される。本 feature での編集:

```
6'. Repository 構築 (typed Box):
   let repo: Box<dyn VaultRepository> = match args.ipc {
       false => Box::new(SqliteVaultRepository::from_directory(&path)?),
       true => Box::new(IpcVaultRepository::connect(&socket_path)?),
   };
   // 以降は &*repo を UseCase に渡す
```

**`VaultRepository` trait の `dyn` 互換性**:
- 既存 trait に `Sized` 制約や generic メソッドがあると `dyn VaultRepository` を作れない
- 本 feature 着手前に `shikomi-infra::persistence::repository` を確認し、`dyn` 互換でない場合は `&dyn VaultRepository` 引数化のリファクタを行う（既存 UseCase は `&dyn VaultRepository` を引数で受ける設計の `cli-vault-commands` がすでにあるため、互換性は確保されているはず）
- 確認: 既存 UseCase シグネチャ `pub fn list_records(repo: &dyn VaultRepository) -> Result<...>` で `dyn` 互換性は前提となっている。本 feature で追加変更不要

## `shikomi_cli::run` のステップ 4（clap パース）の編集

`./clap-config.md` で詳細化するが、ここでは設計判断のみ記述:

- `CliArgs` に `pub ipc: bool` フィールドを追加（`#[arg(long, global = true)]`）
- `args.ipc` の値が`run()` のステップ 6 で参照される
- 既存テストへの影響: 既存の `CliArgs` derive で全フィールドが残っているため、デフォルト値（`bool::default() == false`）でテストは継続 pass

## ステップ 7（サブコマンド分岐）以降への影響

`cli-vault-commands` のステップ 7 以降は**変更なし**（`run_list` / `run_add` / `run_edit` / `run_remove` の各関数は `repo: &dyn VaultRepository` を引数で受け、`args.ipc` を意識しない）。

**Phase 2 移行契約の完成**:
- `usecase` / `presenter` / `input` / `view` / `error` レイヤは無変更
- `args.ipc` 分岐は `lib.rs::run()` の Repository 構築 1 箇所のみ
- `cli-vault-commands` で確立した「骨格テンプレート」が本 feature で実体化される

## `MSG-CLI-051` の挿入位置

`./ipc-vault-repository.md §エラー写像` で定義した `MSG-CLI-051`（warning: `--ipc` opt-in 通知）の出力位置:

- ステップ 6'（Repository 構築）の **`--ipc == true` 採用直後**
- `quiet == false` の時のみ stderr に出力
- 設計理由: ユーザに「現在 IPC 経路を使っている」を明示することで、`shikomi list` と `shikomi --ipc list` の挙動差を観測しやすくする（デバッグ容易性）

## tokio runtime の起動方針（CLI 側）

`shikomi_cli::run()` は同期関数（`pub fn run() -> ExitCode`、既存）。`--ipc` 経路では IPC が非同期のため、**内部で tokio runtime を起動**:

- `args.ipc == true` 採用時、`run()` 内部で:
  - `let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;`
  - `let _guard = runtime.enter();` でランタイム context を有効化
  - `IpcVaultRepository::connect` 等の `block_on` 経由呼出が安全に動作

**`current_thread` ランタイム選択理由**:
- CLI は単一接続のみ扱う（並行接続不要）
- `multi_thread` ランタイムは worker スレッド spawn コストが高く、CLI 短命プロセスでは過剰
- `enable_all()` で `tokio::time` / `tokio::net` を有効化（IPC 接続に必要）

**ランタイム drop タイミング**:
- `runtime` 変数のスコープが `run()` の戻り値計算まで続く
- `IpcVaultRepository` が drop される際に `Framed` も drop され、`tokio` の async cleanup が呼ばれる
- `runtime` drop 時に未完了の `JoinHandle` があれば cancel される

## エラーメッセージ出力先の統一

CLI 側は `cli-vault-commands` の規約をそのまま継承:

- **stdout**: 成功出力・データ出力
- **stderr**: 警告（`MSG-CLI-050` / `MSG-CLI-051`）/ エラー（`MSG-CLI-100〜111`）/ panic hook 固定文言
- `quiet == true` は stdout 成功出力のみを抑止

daemon 側は:

- **stdout**: 起動メッセージ / 構造化ログの非エラー（`tracing::info!` のデフォルト出力先は stdout / stderr どちらにも設定可、本 feature では `tracing_subscriber::fmt()` のデフォルト = `stdout`）
- **stderr**: panic hook 固定文言、エラー / 警告ログ（`tracing::warn!` / `error!`）

**daemon の tracing 出力先の扱い**:
- `tracing_subscriber::fmt()` のデフォルトは stdout
- daemon は systemd / launchd 等で stdout / stderr を分けたい運用ニーズがあるため、**本 feature では stderr に統一**:
  - `tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(...)` で stderr 出力に固定
  - 設計理由: ログは「副次出力」として扱い、stdout は「将来追加するパフォーマンス計測等の正規出力」のために予約（YAGNI、本 feature では明示的 stdout 出力なし）

## テスト観点（テスト設計担当向け）

**ユニットテスト**:
- `shikomi_daemon::run()` のステップ 4-7 のエラー経路（シングルインスタンス失敗 / vault load 失敗 / 暗号化検出）の各 `ExitCode` 写像
- `panic_hook_fn` の動作（catch_unwind で意図的 panic）→ stderr 固定文言、tracing 呼出 0 件、payload 非参照を grep 検証

**結合テスト**:
- `shikomi_daemon::run()` を `tokio::test` で実行 → IpcServer 起動確認 → `tokio::time::sleep(100ms)` → shutdown notify → graceful 完了
- CLI `shikomi_cli::run()` を `--ipc` 付き / 無しで実行（`assert_cmd::Command` 経由）→ Repository 構築の分岐確認

**E2E**:
- 実 daemon プロセス起動 → `shikomi --ipc list` 実行 → SQLite 直結版と bit 同一の出力検証
- daemon 起動失敗（暗号化 vault）→ exit 3
- daemon 二重起動 → exit 2
- SIGTERM での graceful shutdown → exit 0

テストケース番号の割当は `test-design/` が担当する。
