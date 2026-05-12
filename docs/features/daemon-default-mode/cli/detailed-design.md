# 詳細設計書 — daemon-default-mode / cli

<!-- feature: daemon-default-mode / sub-feature: cli / Issue #126 -->
<!-- 配置先: docs/features/daemon-default-mode/cli/detailed-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 兄弟: ./basic-design.md -->

## 記述ルール

疑似コード禁止。処理順序は**番号付き箇条書き**で表現する。変更箇所は「変更前 → 変更後」形式で明示する。

## 変更対象ファイル一覧

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/cli.rs` | 編集 | `CliArgs::ipc: bool` 削除 → `CliArgs::no_ipc: bool` 追加 |
| `crates/shikomi-cli/src/lib.rs` | 編集 | `build_handle` の分岐反転 / `MSG-CLI-051` 出力コード削除 |
| `crates/shikomi-cli/src/presenter/warning.rs` | 編集 | `render_ipc_opt_in_notice` 関数削除（`MSG-CLI-051` 廃止）|
| `crates/shikomi-cli/src/presenter/error.rs` | 編集 | `MSG-CLI-110` hint 文面更新（`--ipc` 言及削除）|
| `CHANGELOG.md` | 編集 | Phase 2 移行ガイド追記（`--ipc` → `--no-ipc` 変更の破壊的変更告知）|

変更不要ファイル:

| ファイル | 理由 |
|---------|------|
| `crates/shikomi-cli/src/record_runners.rs` | `match handle { Sqlite => ..., Ipc => ... }` は変更なし（`RepositoryHandle` バリアント自体は不変）|
| `crates/shikomi-cli/src/usecase/` 全ファイル | `&dyn VaultRepository` 経由のため経路に依存しない |
| `crates/shikomi-cli/src/presenter/` (`warning.rs` 以外) | 変更なし |
| `crates/shikomi-daemon/` | 本 Issue スコープ外 |

## `crates/shikomi-cli/src/cli.rs` の変更詳細

### 変更前（Phase 1 / `CliArgs` の `--ipc` フィールド）

```
/// Use the running shikomi-daemon over IPC instead of opening the vault file directly.
/// Currently supported only with the `list` subcommand; requires shikomi-daemon to be running.
// NOTE: daemon-ipc feature (Issue #26) で追加。...
#[arg(long, global = true)]
pub ipc: bool,
```

### 変更後（Phase 2 / `CliArgs` の `--no-ipc` フィールド）

- `pub ipc: bool` フィールド（`#[arg(long, global = true)]`）を**削除**する
- 以下の `pub no_ipc: bool` フィールドを追加する:

```
/// Use direct SQLite access instead of the daemon (escape hatch; use when daemon is unavailable).
/// By default, shikomi routes all operations through shikomi-daemon over IPC.
#[arg(long = "no-ipc", global = true)]
pub no_ipc: bool,
```

**設計判断**:
- `long = "no-ipc"` を明示する（Rust フィールド名 `no_ipc` から clap が生成する `--no-ipc` と一致するが、明示することで読者に意図が明確になる）
- doc comment は `--help` に表示される。エスケープハッチであることと、既定が IPC であることを英語で端的に説明する
- `short` は設定しない（`-n` を確保しない。誤用リスクが高い destructive オプションには short alias を設けない方針）

## `crates/shikomi-cli/src/lib.rs` の変更詳細

### 変更箇所 1: `build_handle` 関数の分岐反転（REQ-DDM-002）

**変更前**:

```
fn build_handle(args: &CliArgs, locale: Locale, quiet: bool) -> Result<RepositoryHandle, CliError> {
    if args.ipc {
        if !quiet {
            let notice = presenter::warning::render_ipc_opt_in_notice(locale);
            eprint_stderr(&notice);
        }
        let socket_path = IpcVaultRepository::default_socket_path()?;
        let ipc = IpcVaultRepository::connect(&socket_path)?;
        Ok(RepositoryHandle::Ipc(ipc))
    } else {
        let path = match args.vault_dir.as_deref() { ... };
        let repo = SqliteVaultRepository::from_directory(&path)?;
        Ok(RepositoryHandle::Sqlite(repo))
    }
}
```

**変更後**:

```
fn build_handle(args: &CliArgs, _locale: Locale, _quiet: bool) -> Result<RepositoryHandle, CliError> {
    if args.no_ipc {
        let path = match args.vault_dir.as_deref() { ... };
        let repo = SqliteVaultRepository::from_directory(&path)?;
        Ok(RepositoryHandle::Sqlite(repo))
    } else {
        let socket_path = IpcVaultRepository::default_socket_path()?;
        let ipc = IpcVaultRepository::connect(&socket_path)?;
        Ok(RepositoryHandle::Ipc(ipc))
    }
}
```

変更点の要約:
1. `args.ipc` 参照を `args.no_ipc` 参照に変更（true/false の意味が反転）
2. IPC 経路が `else`（既定）、Sqlite 経路が `if args.no_ipc`（明示時のみ）に入れ替え
3. `MSG-CLI-051` 出力コード（`render_ipc_opt_in_notice` 呼出 + `eprint_stderr` 呼出）を削除
4. `locale` / `quiet` 引数が未使用になる場合は `_` プレフィックスを付ける（後述の `presenter/error.rs` で locale を使い続ける可能性があるため、実装者が判断する）

**設計判断**:
- `MSG-CLI-051` の削除は 1 行（`if !quiet { ... }`）の丸ごと削除。関連する `quiet` 引数自体は `emit_error_and_exit` 等で引き続き使われるため `build_handle` シグネチャから削除しない
- `connect_vault_ipc`（vault サブコマンド用 IPC 構築）は変更しない（vault 経路は `--no-ipc` に影響されない）

### 変更箇所 2: `build_handle` の doc comment 更新

変更前の doc comment が `MSG-CLI-051` に言及している箇所を削除し、「IPC が既定」に書き直す:

**変更前**:
```
/// IPC 経路では `MSG-CLI-051`（opt-in 警告）を `quiet` 抑止下を除き先に出力した上で、
/// daemon に接続してハンドシェイクまで完了させる。
```

**変更後**:
```
/// `args.no_ipc` フラグから `RepositoryHandle` を構築する。
///
/// 既定（`no_ipc == false`）は IPC 経路。daemon 未起動時は `MSG-CLI-110` で Fail Fast。
/// `--no-ipc` 指定時のみ SQLite 直結経路（Phase 1 相当）を使用する。
```

### 変更箇所 3: `RepositoryHandle::Ipc` バリアントの doc comment 更新

変更前:
```
/// `--ipc` opt-in の daemon 経由経路。
Ipc(IpcVaultRepository),
```

変更後:
```
/// 既定の daemon 経由経路（IPC）。`--no-ipc` 指定時は使用しない。
Ipc(IpcVaultRepository),
```

（`Sqlite` バリアントの doc comment も対称的に更新する）

## `crates/shikomi-cli/src/presenter/warning.rs` の変更詳細（REQ-DDM-003）

### 削除する関数

以下の関数を**丸ごと削除**する:

```
pub fn render_ipc_opt_in_notice(locale: Locale) -> String { ... }
```

削除対象範囲:
- 関数本体（`pub fn render_ipc_opt_in_notice` から閉じ括弧まで）
- 関数の doc comment（`/// ...`）
- 対応するユニットテスト（`test_render_ipc_opt_in_notice_english_matches_spec_wording` / `test_render_ipc_opt_in_notice_japanese_en_contains_both_spec_wordings`）

**設計判断**:
- `MSG-CLI-051` のメッセージ文言は `daemon-ipc/basic-design/module-contracts.md §MSG-CLI-051` に記録が残るため、削除してもトレーサビリティは維持される
- `warning.rs` に残る `MSG-CLI-050`（別の警告）は影響を受けない

## `crates/shikomi-cli/src/presenter/error.rs`（または `render_error` 相当関数）の変更詳細（REQ-DDM-004）

### MSG-CLI-110 hint 文面変更

**変更前（Phase 1 / `--ipc` フラグ言及あり）**:

`MSG-CLI-110` の hint 行に `--ipc` フラグへの言及が含まれている場合、削除する。実装ファイルを確認し、以下の文言を探して更新する（実装担当が確認すること）:

想定される変更前の hint 文言例:
```
hint: run shikomi-daemon in the background, then retry with --ipc
```

**変更後（Phase 2 / daemon 起動コマンドのみ）**:

```
hint: start the daemon first: shikomi-daemon &   (Linux/macOS)
hint: start the daemon first: Start-Process -NoNewWindow shikomi-daemon   (Windows)
```

**Sub-B 移行後（Issue #127 完了後に別 PR で追記）**:

```
hint: or enable autostart: shikomi daemon enable-autostart
```

**設計判断**:
- Sub-B 未完了の現時点では autostart hint を出力しない（YAGNI / Fail Fast 原則）
- hint の OS 別案内は既存の 3 OS 併記フォーマット（`daemon-ipc/basic-design/error.md §MSG-CLI-110 確定文面`）を踏襲する
- Phase 2 での `MSG-CLI-110` は「daemon が起動していない」が唯一の原因になるため、`--ipc` というフラグへの言及はユーザーを混乱させる（フラグ自体が廃止されるため）

## CHANGELOG.md への追記

Phase 2 移行は **破壊的変更**（`--ipc` フラグ廃止）を含む。実装担当は CHANGELOG.md の `## [Unreleased]` セクションに以下を追記する:

```markdown
### Breaking Changes

- **`--ipc` flag removed**: The `--ipc` flag is no longer recognized. IPC is now the default.
  - Migration: Remove `--ipc` from scripts; it is no longer needed.
  - To opt out of IPC (direct SQLite): use `--no-ipc` instead.
```

## 実装担当（坂田銀時）への引き継ぎメモ

### 実装手順（推奨順序）

1. `crates/shikomi-cli/src/cli.rs` で `ipc: bool` → `no_ipc: bool` に変更する
2. 全コンパイルエラーを確認する（`cargo check`）。`args.ipc` を参照していた箇所がすべて列挙される
3. `lib.rs` の `build_handle` を変更する（分岐の反転）
4. `presenter/warning.rs` の `render_ipc_opt_in_notice` を削除する
5. `presenter/error.rs`（または hint 文面が定義されているファイル）を確認・更新する
6. テストを更新する（削除したテスト / 既存テストの `--ipc` → `--no-ipc` 置換）
7. CHANGELOG.md に追記する
8. `cargo test` で全テスト pass を確認する

### コンパイルエラー解消のヒント

`cli.rs` の `ipc: bool` 削除後に `cargo check` を実行すると、以下の参照箇所が `E0609`（フィールド未存在）で列挙される:
- `lib.rs`: `if args.ipc { ... }` → `if args.no_ipc { ... }` に変更（分岐を反転させること）
- テストコード内の `CliArgs { ipc: true, ... }` → `CliArgs { no_ipc: true, ... }` に変更

### 既存テストへの影響

| テスト場所 | 影響 | 対処 |
|-----------|------|------|
| `cli.rs` 内の `--ipc` パーステスト | 削除または `--no-ipc` に書き換え | 書き換え |
| `lib.rs` / `record_runners.rs` の `RepositoryHandle` 関連テスト | `RepositoryHandle` バリアント自体は不変のため影響なし | 変更不要 |
| `warning.rs` の `test_render_ipc_opt_in_notice_*` | 削除 | 削除 |
| `tests/` 配下の E2E テスト（`--ipc` フラグ使用）| `--ipc` → 削除 / `--no-ipc` テスト追加 | 書き換え + 追加 |

### `MSG-CLI-051` の参照確認

以下の grep で `MSG-CLI-051` の残存参照を確認し、全件削除・置換する:

```
grep -rn "MSG-CLI-051\|ipc_opt_in\|render_ipc" crates/shikomi-cli/src/
```

期待結果: 0 件（`presenter/warning.rs` の関数削除後）

### `--no-ipc` の `vault` サブコマンドへの影響確認

`run_vault` / `connect_vault_ipc` は `args` を受け取らず `vault_dir` のみを受け取るため、`--no-ipc` は vault 経路に自動的に影響しない。追加の実装変更は不要。

実装担当は以下の grep で vault 経路が `no_ipc` を参照していないことを確認する:

```
grep -n "no_ipc" crates/shikomi-cli/src/lib.rs
```

期待結果: `build_handle` 関数内の 1 箇所のみ（`run_vault` / `connect_vault_ipc` では参照しない）。

## テスト設計（本詳細設計から派生するテスト観点）

### UT 観点（テスト設計 Issue で詳細化）

| 観点 | 期待 |
|------|------|
| `shikomi --no-ipc list` のパース | `args.no_ipc == true` |
| `shikomi list` のパース（引数なし）| `args.no_ipc == false` |
| `shikomi --ipc list` のパース | clap error（認識不能フラグ）|
| `build_handle(no_ipc = false)` | `RepositoryHandle::Ipc(_)` を返す（daemon mock 使用）|
| `build_handle(no_ipc = true)` | `RepositoryHandle::Sqlite(_)` を返す |
| IPC 接続失敗時 | `CliError::DaemonNotRunning` → exit 1 |

### E2E 観点（AC-DDM-01 〜 06 に対応）

| AC ID | テストシナリオ |
|-------|-------------|
| AC-DDM-01 | daemon 起動後に `shikomi list` → IPC 経由で成功 |
| AC-DDM-02 | daemon 未起動で `shikomi --no-ipc list` → SQLite 直結で成功 |
| AC-DDM-03 | daemon 未起動で `shikomi list` → exit 1 + MSG-CLI-110 |
| AC-DDM-04 | `shikomi --ipc list` → exit 2（clap error）|
| AC-DDM-05 | daemon 起動後 `shikomi list` の stderr に MSG-CLI-051 が出力されないこと |
| AC-DDM-06 | `shikomi --no-ipc vault encrypt` → exit 1（vault サブコマンドは IPC 強制）|
