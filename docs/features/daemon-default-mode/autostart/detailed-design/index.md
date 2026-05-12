# 詳細設計書 — daemon-default-mode / autostart（目次・CLI 変更詳細）

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/index.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 兄弟: ../basic-design.md -->

## 記述ルール

疑似コード禁止。処理順序は**番号付き箇条書き**で表現する。変更箇所は「変更前 → 変更後」形式で明示する。型シグネチャは Rust 記法で記述する。

## ファイル構成

本詳細設計は 500 行超えを避けるため以下のファイルに分割する（`daemon-ipc/detailed-design/` の 7 ファイル分割を踏襲）:

| ファイル | 内容 |
|---------|------|
| `index.md`（本書）| 目次・変更対象ファイル一覧・`cli.rs` / `lib.rs` 変更詳細 |
| `backend-trait.md` | `AutostartBackend` trait / `AutostartError` / `detect()` / `resolve_daemon_path()` |
| `launchd.md` | `LaunchdBackend`（macOS plist 実装詳細）|
| `systemd.md` | `SystemdBackend`（Linux systemd user unit 実装詳細）|
| `xdg.md` | `XdgAutostartBackend`（Linux XDG Autostart フォールバック実装詳細）|
| `windows.md` | `WindowsTaskSchedulerBackend`（Windows Task Scheduler 実装詳細）|
| `presenter.md` | `presenter/error.rs` MSG-CLI-120/121 / `presenter/success.rs` 成功メッセージ / 実装担当引き継ぎメモ |

## 変更対象ファイル一覧

### 新規作成ファイル

| ファイル | 内容 |
|---------|------|
| `crates/shikomi-cli/src/autostart/mod.rs` | `AutostartBackend` trait / `AutostartError` / `detect()` / `resolve_daemon_path()` |
| `crates/shikomi-cli/src/autostart/launchd.rs` | `LaunchdBackend`（`#[cfg(target_os = "macos")]`）|
| `crates/shikomi-cli/src/autostart/systemd.rs` | `SystemdBackend`（`#[cfg(target_os = "linux")]`）|
| `crates/shikomi-cli/src/autostart/xdg.rs` | `XdgAutostartBackend`（`#[cfg(target_os = "linux")]`）|
| `crates/shikomi-cli/src/autostart/windows.rs` | `WindowsTaskSchedulerBackend`（`#[cfg(target_os = "windows")]`）|

### 編集ファイル

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/cli.rs` | 編集 | `Subcommand::Daemon(DaemonSubcommand)` バリアント追加 / `DaemonSubcommand` enum 新規定義 |
| `crates/shikomi-cli/src/lib.rs` | 編集 | `Subcommand::Daemon` early-return dispatch 追加 / `run_daemon_subcommand` 関数追加 |
| `crates/shikomi-cli/src/presenter/error.rs` | 編集 | MSG-CLI-120 / MSG-CLI-121 関数追加 |
| `crates/shikomi-cli/src/presenter/success.rs` | 編集 | `render_autostart_installed()` / `render_autostart_uninstalled()` 関数追加 |

### 変更不要ファイル

| ファイル | 理由 |
|---------|------|
| `crates/shikomi-cli/src/record_runners.rs` | `DaemonSubcommand` は `RepositoryHandle` 不要のため影響なし |
| `crates/shikomi-cli/src/usecase/` 全ファイル | autostart は usecase 層を経由しない（OS 操作・IPC probe のみ）|
| `crates/shikomi-daemon/` | autostart 登録は CLI 側の責務。daemon 本体は変更なし |

## `crates/shikomi-cli/src/cli.rs` の変更詳細

### `Subcommand::Daemon` バリアント追加

`Subcommand` enum（`cli.rs §Subcommand`）の `Gui` バリアントの後に追加する:

```
    Gui,

    /// OS 自動起動の管理と daemon 稼働状態確認（Sub-B Issue #127）。
    /// 設計根拠: docs/features/daemon-default-mode/autostart/basic-design.md
    #[command(about = "Manage daemon autostart registration and check daemon status")]
    Daemon(DaemonSubcommand),
```

### `DaemonSubcommand` enum 新規定義

`VaultSubcommand` 定義の後に追記する:

```
// -------------------------------------------------------------------
// DaemonSubcommand（Sub-B Issue #127）
// -------------------------------------------------------------------

/// `shikomi daemon {subcommand}` の 3 サブコマンド group。
///
/// 設計根拠: docs/features/daemon-default-mode/autostart/basic-design.md §DaemonSubcommand の CLI 仕様
#[derive(ClapSubcommand, Debug)]
pub enum DaemonSubcommand {
    /// OS の自動起動機能に shikomi-daemon を登録する。
    #[command(about = "Register shikomi-daemon as an OS autostart service")]
    Install,

    /// OS の自動起動登録を解除する。
    #[command(about = "Unregister shikomi-daemon from OS autostart")]
    Uninstall,

    /// daemon の稼働状態と自動起動登録状態を表示する。
    #[command(about = "Show daemon running status and autostart registration")]
    Status,
}
```

**設計判断**:
- `install` / `uninstall` を採用（launchd / systemd の用語と整合し、OS 横断で統一）
- `start` / `stop` は不採用（autostart **登録** とプロセス **起動** の概念混同を避ける）
- `DaemonSubcommand` を `ClapSubcommand` 派生型にする（clap の nested subcommand 規約に準拠）

## `crates/shikomi-cli/src/lib.rs` の変更詳細

### 変更箇所 1: `Subcommand::Daemon` early-return dispatch 追加

`run_gui` early-return の直後に追加する:

```
    if let Subcommand::Gui = &args.subcommand {
        return run_gui(locale);
    }

    // Sub-B (#127): daemon サブコマンドは RepositoryHandle 不要のため early return する。
    if let Subcommand::Daemon(daemon_sub) = &args.subcommand {
        return run_daemon_subcommand(daemon_sub, args.no_ipc, locale, quiet);
    }
```

### 変更箇所 2: `match &args.subcommand` に `unreachable!` アームを追加

```
        Subcommand::Vault(_) => unreachable!("vault subcommand handled above"),
        Subcommand::Gui => unreachable!("gui subcommand handled above"),
        Subcommand::Daemon(_) => unreachable!("daemon subcommand handled above"),
```

### 変更箇所 3: `run_daemon_subcommand` 関数の追加

`run_gui` 関数定義の後に追加する:

```
// -------------------------------------------------------------------
// Sub-B (#127): daemon サブコマンド dispatch
// -------------------------------------------------------------------

fn run_daemon_subcommand(
    sub: &DaemonSubcommand,
    no_ipc: bool,
    locale: Locale,
    quiet: bool,
) -> ExitCode
```

### `run_daemon_subcommand` — Install 処理手順（REQ-DDM-010）

1. `autostart::detect()` を呼び出して OS 別 `backend: Box<dyn AutostartBackend>` を取得する
2. `backend.install()` を実行する
3. **成功時**:
   a. `tracing::info!(target: "shikomi_cli::autostart", "autostart install: backend={}", backend.name())` を出力する（**A09 監査証跡 / `security.md §A09`**）
   b. `quiet == false` の場合: `presenter::success::render_autostart_installed(locale)` の戻り値を `println!` で stdout に出力する
   c. `backend.install_hint()` が `Some(hint)` かつ `quiet == false` の場合: `println!("{hint}")` で stdout に追記する
   d. `ExitCode::Success` を返す
4. **失敗時**: `presenter::error::render_autostart_install_error(&err, locale)` を `eprint_stderr` で出力し `ExitCode::Failure` を返す

**設計判断**:
- `tracing::info!` は `quiet` フラグの影響を受けない（tracing ログは監査チャネルとして独立 / `quiet` は stdout 成功出力の抑止のみ）
- `tracing::info!` を 3.a で先行出力する（失敗時には出力しない = 成功の証跡のみ記録）

### `run_daemon_subcommand` — Uninstall 処理手順（REQ-DDM-011）

1. `autostart::detect()` で `backend` を取得する
2. `backend.uninstall()` を実行する
3. **成功時**:
   a. `tracing::info!(target: "shikomi_cli::autostart", "autostart uninstall: backend={}", backend.name())` を出力する（A09 監査証跡）
   b. `quiet == false` の場合: `presenter::success::render_autostart_uninstalled(locale)` の戻り値を `println!` で stdout に出力する
   c. `ExitCode::Success` を返す
4. **失敗時**: `presenter::error::render_autostart_uninstall_error(&err, locale)` を `eprint_stderr` で出力し `ExitCode::Failure` を返す

### `run_daemon_subcommand` — Status 処理手順（REQ-DDM-012）

1. **IPC probe**（`no_ipc == true` の場合は省略）:
   - `IpcVaultRepository::default_socket_path()` → `IpcVaultRepository::connect(&p)` を試みる
   - **タイムアウト**: 200ms 以内で `Connect` 失敗なら `"daemon: not running"` 扱い（`basic-design.md §REQ-DDM-012 IPC probe タイムアウト`）
   - 成功: `daemon_line = "daemon: running"`
   - 失敗 / タイムアウト: `daemon_line = "daemon: not running"`
   - `no_ipc == true`: `daemon_line = "daemon: unknown (--no-ipc)"`（IPC probe 省略）
2. `backend.is_registered()` で自動起動登録状態を確認する
   - `true`: `autostart_line = "autostart: enabled"`
   - `false`: `autostart_line = "autostart: disabled"`
3. `println!("{daemon_line}")` / `println!("{autostart_line}")` を出力する
4. `ExitCode::Success` を返す（**常に exit 0** / REQ-DDM-012「情報提供のみ、副作用なし」）

**設計判断**:
- Status は成功・失敗に関わらず `ExitCode::Success` を返す（確認できない状態も結果として出力する）
- Status には `tracing::info!` を追加しない（副作用なし・読み取り専用操作のため監査対象外）
- 200ms タイムアウトは `IpcVaultRepository::connect` のタイムアウトオプション（`connect_with_timeout(200ms)` または `set_nonblocking + poll`）で実現する。実装担当が既存 IPC クライアントの timeout API を確認すること

## 実装担当注意事項（Sub-B 実装時に必須）

**Sub-A の TC-UT-159（`args.no_ipc` 参照件数アサート）の期待値を 2 → 3 に更新すること。** `daemon status` IPC probe 分岐が `lib.rs` に追加されることで `no_ipc` 参照が 2 件（vault dispatch + build_handle）から 3 件（+ daemon status probe）に増加する（TC-UT-176 参照）。この更新を怠ると Sub-B 実装後に TC-UT-159 が FAIL し、監査ゲートが機能しているように見えて内側の期待値が陳腐化する。詳細引き継ぎ: `presenter.md §既存テストケースの期待値更新`
