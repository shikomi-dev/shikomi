# 基本設計書 — daemon-default-mode / autostart（モジュール契約）

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/basic-design.md -->
<!-- Vモデル対応: 階層 3（sub-feature モジュール契約）-->
<!-- 親: ../feature-spec.md -->

## §モジュール契約（機能要件）

### REQ-DDM-010: `shikomi daemon install` — OS 自動起動登録

| 項目 | 内容 |
|------|------|
| 入力 | `Subcommand::Daemon(DaemonSubcommand::Install)` / `args.vault_dir: Option<PathBuf>` |
| 処理 | OS を判定し、対応する `AutostartBackend::install()` を呼び出す。登録済みの場合は冪等（再登録せず成功扱い）|
| 出力 | stdout: `"shikomi-daemon autostart enabled"` + OS 固有の有効化コマンド案内（1 行）|
| エラー時 | インストール失敗（権限不足 / コマンド不在）→ stderr: `MSG-CLI-120` + exit 1 |
| 設計原則 | Fail Fast（インストール途中失敗はロールバック不要——ファイル書き込み失敗なら次回 install で上書き可能）/ 冪等性 |

### REQ-DDM-011: `shikomi daemon uninstall` — OS 自動起動解除

| 項目 | 内容 |
|------|------|
| 入力 | `Subcommand::Daemon(DaemonSubcommand::Uninstall)` |
| 処理 | OS を判定し、対応する `AutostartBackend::uninstall()` を呼び出す。未登録の場合は冪等（成功扱い）|
| 出力 | stdout: `"shikomi-daemon autostart disabled"` |
| エラー時 | 解除失敗（ファイル削除権限なし / `launchctl` / `systemctl` エラー）→ stderr: `MSG-CLI-121` + exit 1 |
| 設計原則 | Fail Fast / 冪等性 |

### REQ-DDM-012: `shikomi daemon status` — daemon 稼働状態確認

| 項目 | 内容 |
|------|------|
| 入力 | `Subcommand::Daemon(DaemonSubcommand::Status)` |
| 処理 | (1) IPC 接続試行で daemon 稼働確認 / (2) `AutostartBackend::is_registered()` で自動起動登録確認 |
| 出力 | stdout: `"daemon: running"` / `"daemon: not running"` + `"autostart: enabled"` / `"autostart: disabled"` の 2 行 |
| エラー時 | status 自体は exit 0（確認できない状態も結果として出力する）|
| 設計原則 | 情報提供のみ、副作用なし |

**IPC probe タイムアウト**: `IpcVaultRepository::connect` は **200ms 以内** で Connect 失敗なら `"daemon: not running"` 扱いとする。タイムアウト超過時もユーザーが応答待ちでハングしない（ペガサス指摘③対応）。`--no-ipc` 指定時は IPC probe 自体を省略し `"daemon: unknown (--no-ipc)"` を出力する。

### REQ-DDM-013: `AutostartBackend` trait — OS 別自動起動抽象

| 項目 | 内容 |
|------|------|
| 入力 | `AutostartBackend::detect() -> Box<dyn AutostartBackend>` — OS / 環境を判定して実装を返す |
| 処理 | (1) `install()` — OS 別の登録処理を実行 / (2) `uninstall()` — OS 別の解除処理を実行 / (3) `is_registered() -> bool` — 登録状態を確認 / (4) `name() -> &'static str` — tracing 監査証跡用の識別名を返す |
| 出力 | `Result<(), AutostartError>` |
| エラー時 | `AutostartError::CommandFailed { cmd, stderr }` / `AutostartError::IoError(std::io::Error)` / `AutostartError::Unsupported { reason }` |
| 設計原則 | Strategy パターン（OS 別実装を差し替え可能）/ Composition over Inheritance |

### REQ-DDM-014: macOS launchd LaunchAgent 登録・解除

| 項目 | 内容 |
|------|------|
| 入力 | `LaunchdBackend::install()` / `LaunchdBackend::uninstall()` |
| 処理 | (1) plist ファイルを `~/Library/LaunchAgents/dev.shikomi.daemon.plist` に書き込む / (2) `launchctl bootstrap gui/$(id -u) <plist_path>` で有効化 / (3) uninstall: `launchctl bootout gui/$(id -u)/dev.shikomi.daemon` → plist ファイル削除 |
| 出力 | install: stdout に `"hint: to start immediately: launchctl kickstart gui/$(id -u)/dev.shikomi.daemon"` を追記 |
| エラー時 | `launchctl` が非 0 exit → `AutostartError::CommandFailed` |
| 設計原則 | `process-model.md §4.1 ルール 3` の macOS 規定を実装 |

### REQ-DDM-015: Linux systemd user unit 登録・解除

| 項目 | 内容 |
|------|------|
| 入力 | `SystemdBackend::install()` / `SystemdBackend::uninstall()` |
| 処理 | (1) unit ファイルを `~/.config/systemd/user/shikomi-daemon.service` に書き込む / (2) `systemctl --user daemon-reload` / (3) `systemctl --user enable --now shikomi-daemon.service` で有効化 + 即時起動 / (4) uninstall: `systemctl --user disable --now shikomi-daemon.service` → unit ファイル削除 |
| 出力 | install: stdout に `"hint: to check status: systemctl --user status shikomi-daemon"` を追記 |
| エラー時 | `systemctl` が非 0 exit → `AutostartError::CommandFailed` / `DBUS_SESSION_BUS_ADDRESS` 未設定 → `AutostartError::Unsupported { reason: "no D-Bus session bus" }` |
| 設計原則 | `process-model.md §4.1 ルール 3` の Linux systemd 規定を実装 |

### REQ-DDM-016: Linux XDG Autostart フォールバック

| 項目 | 内容 |
|------|------|
| 入力 | `XdgAutostartBackend::install()` — systemd が検出できない場合に `detect()` が選択する |
| 処理 | (1) `.desktop` ファイルを `~/.config/autostart/shikomi-daemon.desktop` に書き込む / (2) uninstall: ファイル削除のみ |
| 出力 | install: stdout に `"hint: this uses XDG Autostart; shikomi-daemon will start on next login"` を追記 |
| エラー時 | ファイル書き込み失敗 → `AutostartError::IoError` |
| 設計原則 | Fail Safe（systemd 未搭載環境への最大互換）/ YAGNI（XDG のみ実装、OpenRC 等は非スコープ）|

### REQ-DDM-017: Windows タスクスケジューラ登録・解除

| 項目 | 内容 |
|------|------|
| 入力 | `WindowsTaskSchedulerBackend::install()` / `WindowsTaskSchedulerBackend::uninstall()` |
| 処理 | (1) install: `schtasks /Create /SC ONLOGON /TN "shikomi\\shikomi-daemon" /TR "<daemon_path>" /F` / (2) uninstall: `schtasks /Delete /TN "shikomi\\shikomi-daemon" /F` |
| 出力 | install: stdout に `"hint: to start immediately: schtasks /Run /TN "shikomi\\shikomi-daemon""` を追記 |
| エラー時 | `schtasks` が非 0 exit → `AutostartError::CommandFailed` |
| 設計原則 | `process-model.md §4.1 ルール 3` の Windows 規定を実装（レジストリ Run キーは UAC プロンプトで不利なため使わない）|

## OS 判定ロジック（`AutostartBackend::detect()`）

| 優先順位 | 条件 | 選択 Backend |
|---------|------|------------|
| 1 | `cfg(target_os = "macos")` | `LaunchdBackend` |
| 2 | `cfg(target_os = "windows")` | `WindowsTaskSchedulerBackend` |
| 3 | `cfg(target_os = "linux")` かつ `which systemctl` が成功 かつ `systemctl --user status` が非 `Unit not found` | `SystemdBackend` |
| 4 | `cfg(target_os = "linux")` | `XdgAutostartBackend`（フォールバック）|

**設計判断**: `DBUS_SESSION_BUS_ADDRESS` の存在確認と `systemctl --user status` の実行（`which systemctl` + 軽量 probe）を組み合わせて systemd ユーザセッションの有効性を確認する。probe 失敗時は `XdgAutostartBackend` にフォールバック。

## `DaemonSubcommand` の CLI 仕様

`shikomi daemon <subcommand>` という新 subcommand family を `CliArgs` に追加する。

| サブコマンド | 説明 | IPC 要否 |
|------------|------|---------|
| `shikomi daemon install` | OS 自動起動を登録する | 不要（ファイル操作 + OS コマンド）|
| `shikomi daemon uninstall` | OS 自動起動を解除する | 不要 |
| `shikomi daemon status` | daemon 稼働状態と自動起動登録状態を確認する | 任意（IPC 失敗 = not running として扱う）|

**`--no-ipc` との関係**:
- `daemon install` / `uninstall` はファイル操作・外部コマンドのみ。`--no-ipc` フラグに影響されない
- `daemon status` の daemon 稼働確認部分は IPC を試みる。`--no-ipc` 指定時は IPC 試行を省略して `"daemon: unknown (--no-ipc)"` を出力

## ユーザー向けメッセージ一覧

### 新規追加するメッセージ

#### 成功メッセージ（`presenter/success.rs` に追加 / ロケール対応）

| 関数名 | 表示条件 | English 出力 | JapaneseEn 追加行 |
|--------|---------|------------|-----------------|
| `render_autostart_installed(locale)` | `install` 成功時（`quiet == false`）| `shikomi-daemon autostart enabled` | `shikomi-daemon の自動起動を有効にしました` |
| `render_autostart_uninstalled(locale)` | `uninstall` 成功時（`quiet == false`）| `shikomi-daemon autostart disabled` | `shikomi-daemon の自動起動を無効にしました` |

**設計判断**: `render_vault_ipc_forced_note`（`warning.rs`）と同型のロケール対応。`println!` ハードコードではなく `presenter/success.rs` に集約することで、将来の多言語対応・テスト容易性を確保する。

#### エラーメッセージ（`presenter/error.rs` に追加 / stdout ではなく stderr）

| ID | メッセージ（英語） | 表示条件 | 終了コード |
|----|----------------|---------|---------|
| MSG-CLI-120 | `error: failed to enable autostart: {reason}` | `install` 失敗時 | 1 |
| MSG-CLI-121 | `error: failed to disable autostart: {reason}` | `uninstall` 失敗時 | 1 |

**`{reason}` の制約**: `AutostartError` の `Display` 実装が出力する文字列。`CommandFailed { cmd, stderr }` では stderr の最初の 80 文字のみ（secret 非含有、パス情報のみ）。

### Sub-B 完了後に更新するメッセージ

| ID | 変更内容 |
|----|---------|
| MSG-CLI-110 hint（Sub-A で更新済み）| Sub-B 完了後: `"hint: or enable autostart: shikomi daemon install"` を追加（完了 — Issue #134）|

## セキュリティ考慮

→ `autostart/security.md` 参照（本 PR で作成するが500行制限に達した場合は別 PR で追加）

## テスト戦略（テスト設計 Issue で詳細化）

| テストレベル | 観点 |
|-------------|------|
| UT | `AutostartBackend::detect()` が正しい実装を返すこと（`cfg` attribute mock）|
| UT | plist / unit ファイル / desktop エントリのテンプレート展開が正しいこと |
| IT | `LaunchdBackend` / `SystemdBackend` / `XdgAutostartBackend` の install / uninstall のファイル I/O（実ファイルシステム）|
| IT | `WindowsTaskSchedulerBackend` は Windows CI のみ（`#[cfg(target_os = "windows")]`）|
| E2E | AC-DDM-07〜10（`../feature-spec.md §5` に追記）|

## 依存関係・前提条件

| 依存先 | 理由 |
|--------|------|
| Sub-A（Issue #126）完了（PR #129 マージ済み）| `DaemonSubcommand` は `CliArgs` の `Subcommand` enum に追加するため、Sub-A の `--no-ipc` 変更が先に develop に入っている必要がある |
| `shikomi-daemon` バイナリのパス解決 | `install` 時に daemon の実行ファイルパスを取得する（`std::env::current_exe()` を `shikomi-daemon` と同ディレクトリで解決）|
