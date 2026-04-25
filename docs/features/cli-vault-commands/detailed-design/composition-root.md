# 詳細設計書 — composition-root（run() の処理順序 / panic hook）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD (Phase 1) / Issue #26 (daemon-ipc Phase 1) / Issue #30 (daemon-ipc Phase 1.5) -->
<!-- 配置先: docs/features/cli-vault-commands/detailed-design/composition-root.md -->
<!-- 兄弟: ./index.md, ./data-structures.md, ./public-api.md, ./clap-config.md, ./infra-changes.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。処理順序は**番号付き箇条書き**で表現する。

## `shikomi_cli::run` のシグネチャ

- `pub fn run() -> ExitCode`
- Rust の `std::process::Termination` trait で `ExitCode` を返す。`exit()` 呼び出しでなく型で表現（テスト容易性 + 結合テストから呼び出し可能）

## 処理順序

1. **panic hook 登録**: `std::panic::set_hook(Box::new(panic_hook_fn))`（§panic hook 参照）。clap パース前に呼ぶ（clap 内部で panic することは想定しないが、依存 crate の初期化で panic する可能性がゼロでないため最優先で登録）
2. **Locale 決定と格納**: `let locale = Locale::detect_from_env();` → `LOCALE_CACHE.set(locale)` で `static LOCALE_CACHE: OnceLock<Locale>` に保存（panic hook から参照される）
3. **tracing_subscriber 初期化**（verbose / quiet フラグはこの時点で未パースのため、デフォルト `info` レベルで一旦初期化）
4. **clap パース**: `let args = match CliArgs::try_parse() { Ok(a) => a, Err(e) => return handle_clap_error(e, locale) };`（clap エラー扱いは `./clap-config.md §clap エラー扱い` 参照）
5. **tracing レベル再設定**（`args.verbose == true` なら `debug` へ）
6. **Repository 構築**（`daemon-ipc` feature で `args.ipc` 分岐を追加、Issue #30 で `RepositoryHandle` enum に確定）:
   - **`args.ipc == false`（既定、Phase 1）**:
     - `args.vault_dir.is_some()` → `SqliteVaultRepository::from_directory(args.vault_dir.as_ref().unwrap())`
     - `args.vault_dir.is_none()` → `let os_default = io::paths::resolve_os_default_vault_dir()?` → `SqliteVaultRepository::from_directory(&os_default)`
     - 結果を `RepositoryHandle::Sqlite(repo)` で保持
   - **`args.ipc == true`（オプトイン、Phase 1.5 = list/add/edit/remove 全透過、`daemon-ipc` feature で追加）**:
     - `quiet == false` なら `eprintln!("{}", render_warning(WarningKind::IpcOptInNotice, locale))` で `MSG-CLI-051` を stderr に出力
     - `let socket_path = io::ipc_vault_repository::IpcVaultRepository::default_socket_path()?;`
     - `let ipc = io::ipc_vault_repository::IpcVaultRepository::connect(&socket_path)?;`（内部で current-thread tokio runtime を構築・所有、`block_on` で daemon 接続 + ハンドシェイク）
     - 結果を `RepositoryHandle::Ipc(ipc)` で保持
   - エラー時は `render_error(&err, locale)` を stderr に流し `ExitCode::from(&err)` で return（`CliError::DaemonNotRunning` / `ProtocolVersionMismatch` を含む全 `PersistenceError` 経路を統一処理）
7. **サブコマンド分岐**: `match args.subcommand` で 4 分岐。各 `run_*` 関数に `&handle` を渡し、関数内部で `match handle` の 2 アーム（`Sqlite` / `Ipc`）でさらに経路分岐する（§RepositoryHandle 経路ディスパッチ 参照）:
   - `Subcommand::List` → `run_list(&handle, locale, args.quiet)`
   - `Subcommand::Add(a)` → `run_add(&handle, a, locale, args.quiet)`
   - `Subcommand::Edit(a)` → `run_edit(&handle, a, locale, args.quiet)`
   - `Subcommand::Remove(a)` → `run_remove(&handle, a, locale, args.quiet)`
8. **各 `run_*` 関数**は `Result<(), CliError>` を返す。`Ok(())` は `ExitCode::Success` に、`Err(e)` は `render_error(&e, locale)` を stderr に流して `ExitCode::from(&e)` に写像

## `RepositoryHandle` enum（Issue #30 で確定）

**配置**: `crates/shikomi-cli/src/lib.rs` の `run()` 内部 / non-public enum

**定義**: `enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }`

- `#[non_exhaustive]` を**付けない**（CLI 内部限定 enum、`match` の網羅性検査をコンパイル時の修正漏れ検出に活用）
- `Sized` 実装可能なため `Box` 不要、stack 配置でオーバーヘッドなし
- `IpcVaultRepository` は `VaultRepository` trait を**実装しない**（`docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md §設計方針の確定` の単一真実源）。よって `Box<dyn VaultRepository>` は構造的に表現不能、`enum dispatch` を採用する

**設計判断**:
- PR #29 段階では `Box<dyn VaultRepository>` で「Sqlite / Ipc 両方が `VaultRepository` を実装する」前提で設計したが、Issue #30 で `IpcVaultRepository` の trait 非実装方針が確定したため `enum` 型に切り替え
- 旧設計の「`run()` の 1 行差し替えで Phase 2 移行」契約は、新設計では「`RepositoryHandle::Sqlite` バリアント削除 1 行 + 各 `run_*` の `match` 1 アーム削除」に置き換わる。コンパイル時に網羅性検査が変更箇所を全列挙するため**修正漏れがない**
- 将来 `args.ipc` を既定値 `true` に切替えるだけなら enum 構造はそのまま、`construct_handle` 側で構築バリアントを変えるだけで済む

## RepositoryHandle 経路ディスパッチ（`run_*` 各ハンドラの内部）

| サブコマンド | `RepositoryHandle::Sqlite(repo)` 経路 | `RepositoryHandle::Ipc(ipc)` 経路 |
|-----------|-----------------------------------|--------------------------------|
| `list` | `usecase::list::list_records(repo)?` → 戻り値 `Vec<RecordView>` | `ipc.list_summaries()?` → `RecordSummary` を `RecordView::from_summary` で射影 → `Vec<RecordView>` |
| `add` | `usecase::add::add_record(repo, input, now)?` | `ipc.add_record(input.kind, input.label, input.value, now)?`（`AddInput` を分解して渡す） |
| `edit` | `usecase::edit::edit_record(repo, input, now)?` | `ipc.edit_record(input.id, input.label, input.value, now)?` |
| `remove` | `usecase::remove::remove_record(repo, input)?` | `ipc.remove_record(input.id().clone())?`（`ConfirmedRemoveInput::id()` から取得） |

**両経路で共通する責務**（`run_*` 関数の前段 / 後段で実行、`match` 分岐の外）:
- 入力 DTO 構築（`AddInput` / `EditInput` / `ConfirmedRemoveInput`）
- TTY プロンプト / `--yes` 処理（`run_remove` のみ）
- shell 履歴警告（`run_add` / `run_edit`）
- `now: OffsetDateTime` の取得（`OffsetDateTime::now_utc()`）
- 戻り値 `RecordId` の presenter 整形（`render_added` / `render_updated` / `render_removed`）
- stdout への出力 / `quiet` 抑止判定

**両経路で共通しない責務**:
- ID 生成: Sqlite 経路は `usecase::add::add_record` 内で `Uuid::now_v7()` 生成（CLI 側）。IPC 経路は **daemon 側で生成**し、`IpcResponse::Added { id }` で受け取る。CLI 側の IPC 経路は `Uuid` を**呼ばない**（嘘 ID 出荷の構造的排除、`docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md §add_record`）
- vault 未作成時の初期化: Sqlite 経路は `repo.exists()` false 時に `Vault::new` で空 vault を構築・保存。IPC 経路は daemon 側で `repo.load()` 失敗 → `IpcResponse::Error(Persistence)` 返却 → CLI で終了コード 2 で fail fast（vault 未作成時の自動初期化は Phase 1.5 スコープ外、後続 feature `daemon-vault-init` で対応）
- 暗号化 vault 検出: Sqlite 経路は `usecase::list::list_records` 等で `EncryptionUnsupported` 検出。IPC 経路は daemon 起動時に検出済（exit 3）、CLI 側は接続失敗で `DaemonNotRunning` 経由（運用上は同じ「使えない」状態）

## `run_list` 関数の内部

1. `match handle`:
   - `RepositoryHandle::Sqlite(repo)` → `let views = usecase::list::list_records(repo)?;`
   - `RepositoryHandle::Ipc(ipc)` → `let summaries = ipc.list_summaries()?;` → `let views: Vec<RecordView> = summaries.iter().map(RecordView::from_summary).collect();`
2. `let rendered = presenter::list::render_list(&views, locale);`
3. `args.quiet == false` なら `println!("{}", rendered)` で stdout に書き出し（`quiet == true` なら出力抑止）
4. `Ok(())`

## `run_add` 関数の内部

1. `run()` 側で **`--value` / `--stdin` の衝突検出**: `(args.value.is_some(), args.stdin)` の 4 パターン評価:
   - `(true, true)` → `CliError::UsageError("--value and --stdin are mutually exclusive")`
   - `(false, false)` → `CliError::UsageError("either --value or --stdin is required")`
   - `(true, false)` → `SecretString::from_string(args.value.clone().unwrap())`（値取得）
   - `(false, true)` → stdin 読取（kind に応じて `read_password` / `read_line`）→ `SecretString::from_string(buf)`
2. **shell 履歴警告**: `args.kind == KindArg::Secret && args.value.is_some()` なら `eprintln!("{}", render_shell_history_warning(locale))`
3. `RecordLabel::try_new(args.label.clone())?` → `CliError::InvalidLabel` 写像
4. `let input = AddInput { kind: args.kind.into(), label, value };`
5. `let now = OffsetDateTime::now_utc();`
6. `match handle`:
   - `RepositoryHandle::Sqlite(repo)`:
     - vault 未作成時の初期化メッセージ判定: `let initially_existed = repo.exists();`
     - `let id = usecase::add::add_record(repo, input, now)?;`（UseCase 内で `exists()` 再確認・空 vault 構築・`Uuid::now_v7()` 生成）
     - `if !initially_existed && args.quiet == false { println!("{}", render_initialized_vault(&path, locale)); }`
   - `RepositoryHandle::Ipc(ipc)`:
     - vault 初期化メッセージは**出さない**（IPC 経路では daemon が vault の存在を保証している前提、未作成時は daemon 側で `IpcResponse::Error(Persistence)` 返却 → CLI 終了コード 2 で fail fast）
     - `let id = ipc.add_record(input.kind, input.label, input.value, now)?;`（id は **daemon 側生成**）
7. `println!("{}", render_added(&id, locale))`（`quiet` でなければ）
8. `Ok(())`

## `run_edit` 関数の内部

1. `run()` で `--value` / `--stdin` 衝突検出（`run_add` と同様）
2. `run()` で「最低 1 つの更新フラグ必須」検証: `(args.label.is_some() || args.value.is_some() || args.stdin) == false` なら `CliError::UsageError("at least one of --label/--value/--stdin is required")`
3. `RecordId::try_from_str(&args.id)?` → `CliError::InvalidId`
4. `args.label.as_deref().map(|s| RecordLabel::try_new(s.to_string())).transpose()?` → `CliError::InvalidLabel`
5. value 取得（`run_add` と同様、ただし `Option<SecretString>` に包む）
6. `let input = EditInput { id, label, value };`
7. shell 履歴警告: 既存レコード（まだ load していないので警告判定は `args.value.is_some() && args.stdin == false` だけで済ますか、load 後の既存 kind と照合するかは実装時判断。設計方針は前者（`run_add` と挙動を揃える））
8. `let now = OffsetDateTime::now_utc();`
9. `match handle`:
   - `RepositoryHandle::Sqlite(repo)` → `let id = usecase::edit::edit_record(repo, input, now)?;`
   - `RepositoryHandle::Ipc(ipc)` → `let id = ipc.edit_record(input.id, input.label, input.value, now)?;`
10. `println!("{}", render_updated(&id, locale))`
11. `Ok(())`

## `run_remove` 関数の内部

1. `RecordId::try_from_str(&args.id)?`
2. **確認判定**:
   - `args.yes == true` → 確認済み扱い（後述 `ConfirmedRemoveInput::new(id)` 構築）
   - `args.yes == false`:
     - `is_stdin_tty() == true` → プロンプト表示 + 1 行読取:
       - `y` / `Y` → 確認済み（構築）
       - それ以外 → `println!("{}", render_cancelled(locale))` で stdout 出力 → `Ok(())` で early return（`ExitCode::Success`）
     - `is_stdin_tty() == false` → `CliError::NonInteractiveRemove`
3. `let input = ConfirmedRemoveInput::new(id);`
4. `match handle`:
   - `RepositoryHandle::Sqlite(repo)` → `let id = usecase::remove::remove_record(repo, input)?;`
   - `RepositoryHandle::Ipc(ipc)` → `let id = ipc.remove_record(input.id().clone())?;`（`ConfirmedRemoveInput::id()` の read-only アクセサで `&RecordId` を取得し clone、確認経由の型保証は match の Ipc アームに到達する時点で既に成立済み）
5. `println!("{}", render_removed(&id, locale))`
6. `Ok(())`

## 確認プロンプトの label 表示（`run_remove` 補足、Issue #30 で経路追加）

Sqlite 経路では `repo.load()` を呼んで `find_record(&id)` で label を取得し、プロンプトに含める（`"Delete record {id} ({label})? [y/N]: "`）。

IPC 経路でも同等の UX を提供するため、`run_remove` の確認プロンプト前段で:

- `match handle`:
  - `RepositoryHandle::Sqlite(repo)` → `repo.load()?` 経由で `find_record` → label 取得
  - `RepositoryHandle::Ipc(ipc)` → `ipc.list_summaries()?` で全 summary を取得 → `iter().find(|s| s.id == id)` で当該 label 取得（`RecordSummary.label` を表示）

両経路で label 取得失敗（id 非存在）時は、確認前に `CliError::RecordNotFound(id)` で early return（プロンプト表示前に Fail Fast）。これにより「存在しない id を確認プロンプトで聞いてから NotFound を返す」という冗長なフローを避ける。

**プロンプトに表示する label**: プロンプトは削除対象の label も表示する（`"Delete record {id} ({label})? [y/N]: "`）。label を取得するため、プロンプト表示前に `repo.load()` を一度呼んで `find_record` する必要がある。この load は後続の UseCase 内でも再度呼ばれるが、パフォーマンス上は無視できる（vault 全体を毎回 load する既存 infra の粒度）。実装簡素化のため、プロンプト表示用の label 取得は `run_remove` 内で独自に load するのを許容する。

**代替案**: プロンプトに label を含めない（`"Delete record {id}? [y/N]: "`）→ load 不要。ただし UX が劣化（どのレコードかユーザがわかりにくい）。本設計は **label 表示を採用** し、load を 2 回呼ぶトレードオフを受容する。

## panic hook

**関数シグネチャ**: `fn panic_hook_fn(info: &std::panic::PanicHookInfo<'_>)`

**処理方針**（`../basic-design/security.md §panic hook と secret 漏洩経路の遮断` の契約に従う）:

1. `info.payload()` / `info.message()` / `info.location()` を**一切参照しない**
2. `tracing::error!` / `tracing::warn!` / `tracing::info!` / `tracing::debug!` / `tracing::trace!` を**呼ばない**
3. `LOCALE_CACHE.get()` から `Locale` を読む（`OnceLock<Locale>`、`Copy` type のため副作用なし）
   - `None` の場合（`run()` 起動直後の panic 等）は `Locale::English` にフォールバック
4. `eprintln!` で以下の固定文言を stderr に出力:
   - 英語: `"error: internal bug\nhint: please report this issue to https://github.com/shikomi-dev/shikomi/issues"`
   - 英日併記: 上に加えて日本語行を併記（`render_error` と同じフォーマット）
5. return（panic unwind は続行、Rust ランタイムが `process::exit(101)` を呼ぶ）

**終了コードの扱い**:

- Rust の panic 既定終了コードは `101`。本 feature の `ExitCode::SystemError = 2` とは一致しない
- `ExitCode::SystemError = 2` に揃えるため、`std::panic::catch_unwind` で `run()` 全体をラップして catch → `ExitCode::SystemError` を return する選択肢もあるが、**本 feature では実装しない**。理由:
  - panic は本来到達しないべき内部バグ。発生時は `exit 101` で OS 側に異常終了が伝わる方が運用上わかりやすい
  - `catch_unwind` は `UnwindSafe` trait 境界を要求し、実装コストが見合わない
- 受入基準では「panic 時の終了コード」を**厳密には検証しない**（E2E テストで panic を意図的に起こすのが難しいため）

**バックトレース抑止の検討**:

- `RUST_BACKTRACE=1` が設定されている場合、Rust ランタイムはバックトレースを stderr に出す。バックトレースには関数名 / ソースファイル位置が含まれるが、ローカル変数値は含まれない（リリースビルドの `panic = "unwind"` デフォルト）
- 本 feature はバックトレースを抑止しない（ユーザが明示的に有効化したものを削らない）
- `../detailed-design/future-extensions.md §運用注記` で「CI 環境では `RUST_BACKTRACE=0` を推奨」と記す

## `run()` の外側（`main.rs`）

`main.rs` は以下のみ:

- `fn main() -> ExitCode`
- 中身は `shikomi_cli::run()` の戻り値を return するのみ

**設計意図**: `main.rs` にロジックを書かない。bin のテスト容易性は低いが、`lib.rs::run()` は結合テストで `#[test] fn test_run_list_flow() { ... }` のように呼べる。E2E は `assert_cmd::Command::cargo_bin("shikomi")` で bin プロセス起動経由で検証する。

## `static LOCALE_CACHE: OnceLock<Locale>` の配置

- **配置先**: `crates/shikomi-cli/src/lib.rs` の crate ルート
- **型**: `std::sync::OnceLock<Locale>`
- **`Locale` の `Copy` 実装**: `enum Locale` は `Copy + Clone` を derive 可能（フィールドを持たない単純列挙）
- **設定**: `run()` 起動時に `LOCALE_CACHE.set(locale).ok()` で 1 度だけ設定（2 回目は silently 無視、テスト時の再入を許容）
- **読取**: panic hook 内で `LOCALE_CACHE.get().copied().unwrap_or(Locale::English)` で取得

**テスト容易性**: UT で `LOCALE_CACHE` を参照したい場合、`#[cfg(test)]` 下で `LOCALE_CACHE.set(Locale::English)` を明示的に呼ぶテストヘルパを用意可能。パラレルテストでは `set` が競合するが、`OnceLock::set` は最初の 1 回のみ成功するため、テスト順序に依存する。`LOCALE_CACHE` の UT は実装時に**独立プロセス化** or **serial test**（`serial_test` crate は既に workspace dependency）で対処する。

## エラーメッセージ出力先の統一

- **stdout**: 成功出力・データ出力（`added:` / `list` の表 / 確認プロンプト）
- **stderr**: 警告 / エラー / panic hook 固定文言
- `quiet == true` は stdout 成功出力のみを抑止（stderr のエラーは通常通り、ログの可読性のため）
