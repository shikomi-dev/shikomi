# 詳細設計書 — composition-root（run() の処理順序 / panic hook）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
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
6. **Repository 構築**:
   - `args.vault_dir.is_some()` → `SqliteVaultRepository::from_directory(args.vault_dir.as_ref().unwrap())`
   - `args.vault_dir.is_none()` → `let os_default = io::paths::resolve_os_default_vault_dir()?` → `SqliteVaultRepository::from_directory(&os_default)`
   - エラー時は `render_error(&err, locale)` を stderr に流し `ExitCode::from(&err)` で return
7. **サブコマンド分岐**: `match args.subcommand` で 4 分岐:
   - `Subcommand::List` → `run_list(&repo, locale, args.quiet)`
   - `Subcommand::Add(a)` → `run_add(&repo, a, locale, args.quiet)`
   - `Subcommand::Edit(a)` → `run_edit(&repo, a, locale, args.quiet)`
   - `Subcommand::Remove(a)` → `run_remove(&repo, a, locale, args.quiet)`
8. **各 `run_*` 関数**は `Result<(), CliError>` を返す。`Ok(())` は `ExitCode::Success` に、`Err(e)` は `render_error(&e, locale)` を stderr に流して `ExitCode::from(&e)` に写像

## `run_list` 関数の内部

1. `usecase::list::list_records(repo)?`
2. `presenter::list::render_list(&views, locale)`
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
6. `let id = usecase::add::add_record(repo, input, now)?;`
7. vault 未作成時の初期化メッセージ（stdout）: `if !initially_existed { println!("{}", render_initialized_vault(&path, locale)); }` — ただし `initially_existed` を UseCase から戻す必要があるため、UseCase シグネチャを `Result<(RecordId, bool), CliError>` に拡張するか、`render_initialized_vault` を UseCase の第一書込前に `run()` 側で repo.exists() を事前確認するかを実装時に決定（本設計では後者を採用、repo.exists() を 2 回呼ぶコストは些少）
8. `println!("{}", render_added(&id, locale))`（`quiet` でなければ）
9. `Ok(())`

## `run_edit` 関数の内部

1. `run()` で `--value` / `--stdin` 衝突検出（`run_add` と同様）
2. `run()` で「最低 1 つの更新フラグ必須」検証: `(args.label.is_some() || args.value.is_some() || args.stdin) == false` なら `CliError::UsageError("at least one of --label/--value/--stdin is required")`
3. `RecordId::try_from_str(&args.id)?` → `CliError::InvalidId`
4. `args.label.as_deref().map(|s| RecordLabel::try_new(s.to_string())).transpose()?` → `CliError::InvalidLabel`
5. value 取得（`run_add` と同様、ただし `Option<SecretString>` に包む）
6. `let input = EditInput { id, label, value };`
7. shell 履歴警告: 既存レコード（まだ load していないので警告判定は `args.value.is_some() && args.stdin == false` だけで済ますか、load 後の既存 kind と照合するかは実装時判断。設計方針は前者（`run_add` と挙動を揃える））
8. `let now = OffsetDateTime::now_utc();`
9. `let id = usecase::edit::edit_record(repo, input, now)?;`
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
4. `let id = usecase::remove::remove_record(repo, input)?;`
5. `println!("{}", render_removed(&id, locale))`
6. `Ok(())`

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
