# 詳細設計書 — daemon-default-mode / autostart

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 兄弟: ./basic-design.md -->

## 記述ルール

疑似コード禁止。処理順序は**番号付き箇条書き**で表現する。変更箇所は「変更前 → 変更後」形式で明示する。型シグネチャは Rust 記法で記述する。

## 変更対象ファイル一覧

### 新規作成ファイル

| ファイル | 内容 |
|---------|------|
| `crates/shikomi-cli/src/autostart/mod.rs` | `AutostartBackend` trait / `AutostartError` 型 / `detect()` OS 判定エントリポイント |
| `crates/shikomi-cli/src/autostart/launchd.rs` | `LaunchdBackend`（`#[cfg(target_os = "macos")]`）|
| `crates/shikomi-cli/src/autostart/systemd.rs` | `SystemdBackend`（`#[cfg(target_os = "linux")]`）|
| `crates/shikomi-cli/src/autostart/xdg.rs` | `XdgAutostartBackend`（`#[cfg(target_os = "linux")]`）|
| `crates/shikomi-cli/src/autostart/windows.rs` | `WindowsTaskSchedulerBackend`（`#[cfg(target_os = "windows")]`）|

### 編集ファイル

| ファイル | 変更種別 | 変更内容 |
|---------|---------|---------|
| `crates/shikomi-cli/src/cli.rs` | 編集 | `Subcommand::Daemon(DaemonSubcommand)` バリアント追加 / `DaemonSubcommand` enum 新規定義 |
| `crates/shikomi-cli/src/lib.rs` | 編集 | `Subcommand::Daemon` early-return dispatch 追加 / `run_daemon_subcommand` 関数追加 |
| `crates/shikomi-cli/src/presenter/error.rs` | 編集 | `MSG-CLI-120` / `MSG-CLI-121` 追加 |

### 変更不要ファイル

| ファイル | 理由 |
|---------|------|
| `crates/shikomi-cli/src/record_runners.rs` | `DaemonSubcommand` は `RepositoryHandle` 不要のため影響なし |
| `crates/shikomi-cli/src/usecase/` 全ファイル | 本サブコマンドは usecase 層を経由しない（OS 操作・IPC probe のみ）|
| `crates/shikomi-daemon/` | autostart 登録は CLI 側の責務。daemon 本体は変更なし |

## `crates/shikomi-cli/src/cli.rs` の変更詳細

### `Subcommand::Daemon` バリアント追加

`Subcommand` enum（`cli.rs §Subcommand`）に以下を追加する:

**変更前（`Gui` バリアントが最後）**:

```
    Gui,
```

**変更後（`Daemon` バリアントを `Gui` の後に追加）**:

```
    Gui,

    /// OS 自動起動の管理と daemon 稼働状態確認（Sub-B Issue #127）。
    /// 設計根拠: docs/features/daemon-default-mode/autostart/basic-design.md
    #[command(about = "Manage daemon autostart registration and check daemon status")]
    Daemon(DaemonSubcommand),
```

### `DaemonSubcommand` enum 新規定義

`VaultSubcommand` 定義（`cli.rs §VaultSubcommand`）の後に追記する:

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
    /// macOS: launchd LaunchAgent / Linux: systemd user unit または XDG Autostart / Windows: Task Scheduler
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
- `enable` / `disable` ではなく `install` / `uninstall` を採用する（launchd / systemd の用語に合わせる。`systemctl enable` と `launchctl bootstrap` の動詞は違うが、ユーザー向けには OS 横断で統一する）
- `start` / `stop` は採用しない（OS 自動起動**登録**と daemon プロセス**起動**は別概念。混同を避けるため）
- `DaemonSubcommand` を `VaultSubcommand` と同型の `ClapSubcommand` 派生型にする（clap の nested subcommand 規約に準拠）

## `crates/shikomi-cli/src/lib.rs` の変更詳細

### 変更箇所 1: `Subcommand::Daemon` early-return dispatch 追加

`run_gui` early-return（`if let Subcommand::Gui = ...`）の**直後**に追加する:

**変更前**:

```
    if let Subcommand::Gui = &args.subcommand {
        return run_gui(locale);
    }

    // Sub-F (#44) Phase 2: vault サブコマンドは daemon IPC 経路に強制する。
    if let Subcommand::Vault(vault) = &args.subcommand {
```

**変更後**:

```
    if let Subcommand::Gui = &args.subcommand {
        return run_gui(locale);
    }

    // Sub-B (#127): daemon サブコマンドは RepositoryHandle 不要のため early return する。
    // `--no-ipc` は `daemon status` の IPC probe 省略のみに影響する（install / uninstall は無影響）。
    if let Subcommand::Daemon(daemon_sub) = &args.subcommand {
        return run_daemon_subcommand(daemon_sub, args.no_ipc, locale, quiet);
    }
```

### 変更箇所 2: `Subcommand::Daemon` を `match` の `unreachable!` アームに追加

`lib.rs` の `match &args.subcommand` ブロック:

**変更後**:

```
        Subcommand::Vault(_) => unreachable!("vault subcommand handled above"),
        Subcommand::Gui => unreachable!("gui subcommand handled above"),
        Subcommand::Daemon(_) => unreachable!("daemon subcommand handled above"),
```

### 変更箇所 3: `run_daemon_subcommand` 関数の追加

`run_gui` 関数定義（`lib.rs §run_gui`）の後に追加する:

```
// -------------------------------------------------------------------
// Sub-B (#127): daemon サブコマンド dispatch
// -------------------------------------------------------------------

fn run_daemon_subcommand(
    sub: &DaemonSubcommand,
    no_ipc: bool,
    locale: Locale,
    quiet: bool,
) -> ExitCode {
    use crate::autostart;

    let backend = autostart::detect();

    match sub {
        DaemonSubcommand::Install => {
            match backend.install() {
                Ok(()) => {
                    if !quiet {
                        println!("shikomi-daemon autostart enabled");
                        if let Some(hint) = backend.install_hint() {
                            println!("{hint}");
                        }
                    }
                    ExitCode::Success
                }
                Err(err) => {
                    let msg = presenter::error::render_autostart_install_error(&err, locale);
                    eprint_stderr(&msg);
                    ExitCode::Failure
                }
            }
        }

        DaemonSubcommand::Uninstall => {
            match backend.uninstall() {
                Ok(()) => {
                    if !quiet {
                        println!("shikomi-daemon autostart disabled");
                    }
                    ExitCode::Success
                }
                Err(err) => {
                    let msg = presenter::error::render_autostart_uninstall_error(&err, locale);
                    eprint_stderr(&msg);
                    ExitCode::Failure
                }
            }
        }

        DaemonSubcommand::Status => {
            // IPC probe（--no-ipc 時は省略）
            let daemon_line = if no_ipc {
                "daemon: unknown (--no-ipc)".to_string()
            } else {
                let socket_path = IpcVaultRepository::default_socket_path()
                    .ok()
                    .and_then(|p| IpcVaultRepository::connect(&p).ok());
                if socket_path.is_some() {
                    "daemon: running".to_string()
                } else {
                    "daemon: not running".to_string()
                }
            };

            // 自動起動登録状態
            let autostart_line = if backend.is_registered() {
                "autostart: enabled".to_string()
            } else {
                "autostart: disabled".to_string()
            };

            println!("{daemon_line}");
            println!("{autostart_line}");
            ExitCode::Success  // status は常に exit 0（REQ-DDM-012）
        }
    }
}
```

**設計判断**:
- `daemon status` は `ExitCode::Success` 固定（REQ-DDM-012: 確認できない状態も結果として出力する）
- `IpcVaultRepository::connect` の `Result` を probe として使う（新 API 不要、既存 API の再利用）
- `backend.install_hint()` は `Option<String>` を返す追加メソッド（各 Backend が OS 固有の hint を返す）

## `crates/shikomi-cli/src/autostart/mod.rs` の詳細

### `AutostartBackend` trait 型シグネチャ

```rust
pub trait AutostartBackend {
    /// OS 自動起動に daemon を登録する。冪等（登録済みなら再登録せず Ok を返す）。
    fn install(&self) -> Result<(), AutostartError>;

    /// OS 自動起動登録を解除する。冪等（未登録なら Ok を返す）。
    fn uninstall(&self) -> Result<(), AutostartError>;

    /// 自動起動登録状態を返す。probe 失敗時は `false`（Fail Safe）。
    fn is_registered(&self) -> bool;

    /// install 成功時に stdout へ追記する OS 固有の hint（None なら追記なし）。
    fn install_hint(&self) -> Option<String> {
        None
    }
}
```

### `AutostartError` 型定義

```rust
#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    #[error("command failed: `{cmd}`: {stderr_excerpt}")]
    CommandFailed {
        cmd: String,
        /// stderr の最初の 80 文字のみ（secret 非含有、パス情報のみ）
        stderr_excerpt: String,
    },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("unsupported: {reason}")]
    Unsupported { reason: String },
}
```

**設計判断**:
- `thiserror` crate を使用する（既存 `CliError` と同じ依存）
- `CommandFailed::stderr_excerpt` は 80 文字上限（security.md §脅威モデル「secret 漏洩」対応）

### `detect()` 関数定義

```rust
/// OS を判定して適切な `AutostartBackend` 実装を返す。
///
/// 優先順位: basic-design.md §OS 判定ロジック
pub fn detect() -> Box<dyn AutostartBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(launchd::LaunchdBackend::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsTaskSchedulerBackend::new())
    }
    #[cfg(target_os = "linux")]
    {
        if systemd::SystemdBackend::is_available() {
            Box::new(systemd::SystemdBackend::new())
        } else {
            Box::new(xdg::XdgAutostartBackend::new())
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Box::new(UnsupportedBackend)
    }
}
```

**設計判断**:
- `#[cfg(...)]` ブロックはコンパイル時分岐（実行時 `std::env::consts::OS` 文字列比較ではない）。OS 不正解パスのコードがバイナリに混入しない
- `UnsupportedBackend` は `install` / `uninstall` で `AutostartError::Unsupported` を返す（FreeBSD 等の非対応 OS でも panic しない）

### モジュール宣言

```rust
mod launchd;
mod systemd;
mod xdg;
mod windows;
```

（各モジュールは `#[cfg(target_os = ...)]` で適切にスコープされる）

## `crates/shikomi-cli/src/autostart/launchd.rs` の詳細（macOS）

### plist テンプレート

以下のテンプレートを Rust の `const &str` として定義し、`{daemon_path}` / `{log_dir}` を文字列置換して書き込む:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.shikomi.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{daemon_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>{log_dir}/shikomi-daemon.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/shikomi-daemon.log</string>
</dict>
</plist>
```

**変数解決**:
- `{daemon_path}`: `std::env::current_exe()` で `shikomi` バイナリのパスを取得 → 同ディレクトリの `shikomi-daemon`（または `shikomi-daemon.exe`）に解決
- `{log_dir}`: `~/Library/Logs/shikomi` (macOS 標準ログディレクトリ、`dirs::home_dir()` + `Library/Logs/shikomi`)

### `LaunchdBackend::install()` 処理手順

1. `{daemon_path}` を解決する（`resolve_daemon_path()` 共通ヘルパー呼出）
2. `~/Library/Logs/shikomi/` を `std::fs::create_dir_all` で作成する（存在済みなら無視）
3. plist テンプレートを文字列置換して plist 内容を生成する
4. `~/Library/LaunchAgents/` を `create_dir_all` で作成する
5. `~/Library/LaunchAgents/dev.shikomi.daemon.plist` に `std::fs::write` で書き込む（上書き = 冪等）
6. `launchctl bootout gui/{uid}/dev.shikomi.daemon` を実行する（未登録なら無視 = 冪等確保）
7. `launchctl bootstrap gui/{uid} {plist_path}` を実行する（失敗時 → `AutostartError::CommandFailed`）
8. `uid` は `nix::unistd::getuid()` で取得する（`nix` crate — 既存依存）

### `LaunchdBackend::uninstall()` 処理手順

1. `launchctl bootout gui/{uid}/dev.shikomi.daemon` を実行する（非 0 exit は許容 — 未登録の冪等）
2. `~/Library/LaunchAgents/dev.shikomi.daemon.plist` を `std::fs::remove_file` で削除する（`NotFound` エラーは `Ok(())` に変換 — 冪等）

### `LaunchdBackend::is_registered()` 処理手順

1. `~/Library/LaunchAgents/dev.shikomi.daemon.plist` が存在するかを `Path::exists()` で確認する
2. `true` / `false` を返す（launchctl 呼出コストを避けるため plist 存在で代替）

### `LaunchdBackend::install_hint()` 戻り値

```
hint: to start immediately: launchctl kickstart gui/{uid}/dev.shikomi.daemon
```

**設計判断**:
- `launchctl bootstrap` は**次回ログイン時**の自動起動を登録するが即時起動はしない。hint で `kickstart` を案内する（REQ-DDM-014）
- plist の `RunAtLoad: true` は reboot/login 時の自動起動を保証するが、現セッションには適用されない

## `crates/shikomi-cli/src/autostart/systemd.rs` の詳細（Linux + systemd）

### unit ファイルテンプレート

```ini
[Unit]
Description=shikomi credential vault daemon
After=default.target

[Service]
ExecStart={daemon_path}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

**変数解決**:
- `{daemon_path}`: `resolve_daemon_path()` 共通ヘルパー呼出（絶対パス必須）

### `SystemdBackend::is_available()` 判定ロジック

以下を**すべて満たす**場合に `true` を返す:

1. `which::which("systemctl")` が成功する（`which` crate — 既存依存）
2. `std::env::var("DBUS_SESSION_BUS_ADDRESS")` が `Ok(_)`（D-Bus セッションバスが存在）
3. `systemctl --user status` の exit code が非 `4`（exit 4 = "no units loaded / D-Bus 接続失敗" の systemd 規約）

条件 3 の probe コマンド: `systemctl --user status --no-pager 2>&1`

**設計判断**:
- `which systemctl` のみでは WSL 等の非 systemd 環境で false positive になる。D-Bus チェックを組み合わせる
- exit code `4`（systemd の "no units"）と接続エラーを区別するために、exit code を直接確認する（stderr の文字列解析ではなく exit code ベース）
- [参照: systemd man page §EXIT STATUS](https://www.freedesktop.org/software/systemd/man/systemctl.html)

### `SystemdBackend::install()` 処理手順

1. `{daemon_path}` を解決する
2. unit ファイルテンプレートを文字列置換して unit 内容を生成する
3. `~/.config/systemd/user/` を `create_dir_all` で作成する
4. `~/.config/systemd/user/shikomi-daemon.service` に `write` で書き込む（上書き = 冪等）
5. `systemctl --user daemon-reload` を実行する（失敗時 → `CommandFailed`）
6. `systemctl --user enable --now shikomi-daemon.service` を実行する（失敗時 → `CommandFailed`）
   - `--now` によって即時起動 + 次回起動時の自動起動が同時に有効化される

### `SystemdBackend::uninstall()` 処理手順

1. `systemctl --user disable --now shikomi-daemon.service` を実行する（未登録でも無視 — 冪等）
2. `~/.config/systemd/user/shikomi-daemon.service` を `remove_file` で削除する（`NotFound` → `Ok(())` — 冪等）
3. `systemctl --user daemon-reload` を実行する（unit ファイル削除後の再読込）

### `SystemdBackend::is_registered()` 処理手順

1. `~/.config/systemd/user/shikomi-daemon.service` が存在するかを `Path::exists()` で確認する

### `SystemdBackend::install_hint()` 戻り値

```
hint: to check status: systemctl --user status shikomi-daemon
```

## `crates/shikomi-cli/src/autostart/xdg.rs` の詳細（Linux XDG Autostart フォールバック）

### desktop エントリテンプレート

```ini
[Desktop Entry]
Type=Application
Name=shikomi-daemon
Comment=shikomi credential vault daemon
Exec={daemon_path}
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
```

**変数解決**:
- `{daemon_path}`: `resolve_daemon_path()` 共通ヘルパー呼出

### `XdgAutostartBackend::install()` 処理手順

1. `{daemon_path}` を解決する
2. desktop エントリテンプレートを文字列置換する
3. `~/.config/autostart/` を `create_dir_all` で作成する
4. `~/.config/autostart/shikomi-daemon.desktop` に `write` で書き込む（上書き = 冪等）

**設計判断**:
- `systemctl` がない環境（OpenRC, runit, s6 等）のフォールバック。`.desktop` ファイルを `/etc/xdg/autostart/` ではなく `~/.config/autostart/` に配置することでシステム権限不要
- uninstall は `.desktop` ファイル削除のみ（外部コマンド不要）

### `XdgAutostartBackend::install_hint()` 戻り値

```
hint: this uses XDG Autostart; shikomi-daemon will start on next login
```

### `XdgAutostartBackend::is_registered()` 処理手順

1. `~/.config/autostart/shikomi-daemon.desktop` が存在するかを `Path::exists()` で確認する

## `crates/shikomi-cli/src/autostart/windows.rs` の詳細（Windows Task Scheduler）

### `WindowsTaskSchedulerBackend::install()` 処理手順

1. `{daemon_path}` を解決する（`resolve_daemon_path()` — Windows では `.exe` 拡張子付き）
2. 冪等確保: `schtasks /Query /TN "shikomi\shikomi-daemon"` を実行し、既登録なら `Ok(())` を返す
3. `schtasks /Create /SC ONLOGON /TN "shikomi\shikomi-daemon" /TR "{daemon_path}" /F` を実行する
   - `/F`: 既存タスクを強制上書き（冪等のフォールバック）
   - 失敗時 → `AutostartError::CommandFailed`

### `WindowsTaskSchedulerBackend::uninstall()` 処理手順

1. `schtasks /Delete /TN "shikomi\shikomi-daemon" /F` を実行する
   - `/F`: 確認プロンプトを省略
   - 存在しないタスクへの `/Delete` は exit 1 + `ERROR: The system cannot find the file specified.` を返す → stderr で判別して `Ok(())` に変換（冪等）

### `WindowsTaskSchedulerBackend::is_registered()` 処理手順

1. `schtasks /Query /TN "shikomi\shikomi-daemon"` を実行し、exit 0 なら `true`、それ以外は `false`

### `WindowsTaskSchedulerBackend::install_hint()` 戻り値

```
hint: to start immediately: schtasks /Run /TN "shikomi\shikomi-daemon"
```

**設計判断**:
- タスク名のサブフォルダ `shikomi\shikomi-daemon` を使用する。Task Scheduler の UI で `shikomi` フォルダ配下に整理される
- レジストリ `Run` キーではなく Task Scheduler を採用する（REQ-DDM-017 §設計原則: UAC プロンプト回避）

## `resolve_daemon_path()` 共通ヘルパー

全 Backend から参照する共通ヘルパー関数を `autostart/mod.rs` に定義する:

```rust
/// `shikomi-daemon` バイナリのパスを解決する。
///
/// 戦略: 現在の実行ファイル（`shikomi` CLI）と同ディレクトリに `shikomi-daemon[.exe]` があることを前提とする。
///
/// # Errors
/// `current_exe()` の失敗 / シンボリックリンク解決失敗 / `shikomi-daemon` が存在しない場合に `AutostartError::IoError` を返す。
pub fn resolve_daemon_path() -> Result<PathBuf, AutostartError> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let dir = exe.parent().ok_or_else(|| AutostartError::IoError(
        std::io::Error::new(std::io::ErrorKind::NotFound, "cannot determine exe directory"),
    ))?;
    let daemon_name = if cfg!(target_os = "windows") {
        "shikomi-daemon.exe"
    } else {
        "shikomi-daemon"
    };
    let daemon_path = dir.join(daemon_name);
    if !daemon_path.exists() {
        return Err(AutostartError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("shikomi-daemon not found at {}", daemon_path.display()),
        )));
    }
    Ok(daemon_path)
}
```

**設計判断**:
- `current_exe()` + `canonicalize()` でシンボリックリンクを解決し、実バイナリのディレクトリを確実に取得する
- `PATH` 検索（`which::which("shikomi-daemon")`）ではなく同ディレクトリ解決を優先する（配布パッケージでは `shikomi` と `shikomi-daemon` が必ず同ディレクトリに置かれる前提 = basic-design.md §依存関係）
- 存在確認（`daemon_path.exists()`）を行う（Fail Fast: `install` 途中でパス解決失敗するより早期検知）

## `crates/shikomi-cli/src/presenter/error.rs` の変更詳細

### MSG-CLI-120 / MSG-CLI-121 追加

```rust
/// MSG-CLI-120: autostart install 失敗
pub fn render_autostart_install_error(err: &AutostartError, locale: Locale) -> String {
    match locale {
        Locale::English => format!("error: failed to enable autostart: {err}"),
        Locale::JapaneseEn => format!(
            "error: failed to enable autostart: {err}\n\
             エラー: 自動起動の有効化に失敗しました: {err}"
        ),
    }
}

/// MSG-CLI-121: autostart uninstall 失敗
pub fn render_autostart_uninstall_error(err: &AutostartError, locale: Locale) -> String {
    match locale {
        Locale::English => format!("error: failed to disable autostart: {err}"),
        Locale::JapaneseEn => format!(
            "error: failed to disable autostart: {err}\n\
             エラー: 自動起動の無効化に失敗しました: {err}"
        ),
    }
}
```

**`{err}` の内容（`AutostartError::Display` 実装）**:

| バリアント | 出力例 |
|-----------|-------|
| `CommandFailed { cmd, stderr_excerpt }` | `command failed: \`launchctl bootstrap gui/501 ...\`: Service is disabled` |
| `IoError` | `I/O error: Permission denied (os error 13)` |
| `Unsupported { reason }` | `unsupported: no D-Bus session bus` |

**設計判断**:
- `{reason}` / `{err}` は動的フィールドだが、`AutostartError::Display` 実装が secret を含まない（`CommandFailed::stderr_excerpt` は 80 文字上限 + パス情報のみ）。security.md §脅威モデルの制約を `AutostartError` 型が構造的に強制する

## セキュリティ考慮

→ `autostart/security.md` 参照（本 PR で別途作成する。行数制限に達した場合は後続 PR で追加）

## テスト設計（本詳細設計から派生するテスト観点）

### UT 観点（テスト設計 Issue で詳細化）

| 観点 | 期待 |
|------|------|
| `shikomi daemon install` のパース | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Install)` |
| `shikomi daemon status` のパース | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Status)` |
| plist テンプレート展開 | `{daemon_path}` / `{log_dir}` が正しく置換されること |
| systemd unit テンプレート展開 | `{daemon_path}` が絶対パスに展開されること |
| XDG desktop テンプレート展開 | `{daemon_path}` が展開されること |
| `resolve_daemon_path()` — daemon 不在 | `AutostartError::IoError(NotFound)` を返すこと |
| `AutostartError::Display` — `CommandFailed` | stderr_excerpt が 80 文字以内に切り詰められること |
| `SystemdBackend::is_available()` | `DBUS_SESSION_BUS_ADDRESS` 未設定 → `false` を返すこと（環境変数 mock）|

### IT 観点（テスト設計 Issue で詳細化）

| 観点 | 期待 |
|------|------|
| `LaunchdBackend::install()` + `uninstall()` | `tempfile::TempDir` を `XDG_CONFIG_HOME` / `HOME` に見立てたファイル I/O 検証（CI macOS ランナー）|
| `SystemdBackend::install()` + `uninstall()` | `tempfile::TempDir` を `~/.config/systemd/user/` 代替ディレクトリとして使用（CI Linux ランナー）|
| `XdgAutostartBackend::install()` + `uninstall()` | `tempfile::TempDir` を `~/.config/autostart/` 代替ディレクトリとして使用 |
| `WindowsTaskSchedulerBackend` | Windows CI のみ（`#[cfg(target_os = "windows")]`）|
| `run_daemon_subcommand(Status, no_ipc=true)` | stdout に `"daemon: unknown (--no-ipc)"` が含まれること |

## 実装担当（坂田銀時）への引き継ぎメモ

### 実装手順（推奨順序）

1. `crates/shikomi-cli/src/autostart/` ディレクトリを作成する
2. `autostart/mod.rs` に `AutostartBackend` trait / `AutostartError` / `detect()` / `resolve_daemon_path()` を実装する
3. 各 OS Backend を `launchd.rs` / `systemd.rs` / `xdg.rs` / `windows.rs` に実装する（`#[cfg(target_os = ...)]` でスコープ）
4. `cli.rs` に `DaemonSubcommand` enum と `Subcommand::Daemon` バリアントを追加する
5. `cargo check` でコンパイルエラー（`match` exhaustiveness）を確認 → `lib.rs` の `match &args.subcommand` に `Daemon(_) => unreachable!` を追加する
6. `lib.rs` に `run_daemon_subcommand` 関数を追加し、`Daemon` early-return dispatch を追加する
7. `presenter/error.rs` に MSG-CLI-120 / MSG-CLI-121 関数を追加する
8. `cargo test` で全テスト pass を確認する

### 依存 crate

| crate | 用途 | 既存 / 新規 |
|-------|------|------------|
| `thiserror` | `AutostartError` 派生 | 既存（`shikomi-cli` が使用中）|
| `nix` | `getuid()` (macOS / Linux) | 既存（`io/windows_sid.rs` 等で使用済み — Unix のみ）|
| `which` | `which systemctl` probe | 既存（確認要）|
| `dirs` | `home_dir()` | 新規追加要（`dirs = "5"` を `shikomi-cli/Cargo.toml` に追加）|

**`dirs` crate 追加の設計根拠**: [docs.rs/dirs](https://docs.rs/dirs/latest/dirs/) — `~` の展開を手動で行わず `dirs::home_dir()` に委ねることで、`HOME` 環境変数オーバーライドや macOS `NSFileManager` 等の OS 固有挙動を正しく処理する。

### コンパイル時 `#[cfg]` 注意事項

- `autostart/launchd.rs` は `#[cfg(target_os = "macos")]` でモジュール宣言を囲む
- `mod.rs` の `detect()` 末尾に `#[cfg(not(any(...)))]` フォールバック（`UnsupportedBackend`）を追加する
- CI matrix（Linux / macOS / Windows）全 OS でコンパイルエラーがないことを確認する（`cargo check --target x86_64-pc-windows-msvc` 等）

### grep 確認コマンド

実装完了後、以下で回帰確認する:

```
# DaemonSubcommand が cli.rs にのみ定義されていること
grep -rn "DaemonSubcommand" crates/shikomi-cli/src/

# no_ipc の参照が 3 件に増えていること (vault dispatch + build_handle + daemon status IPC probe 分岐)
grep -n "no_ipc" crates/shikomi-cli/src/lib.rs

# autostart 参照が lib.rs に追加されていること
grep -n "autostart::" crates/shikomi-cli/src/lib.rs
```
