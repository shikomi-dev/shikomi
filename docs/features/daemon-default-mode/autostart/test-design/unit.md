# テスト設計書 — daemon-default-mode / autostart / ユニットテスト

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/test-design/unit.md -->
<!-- Vモデル対応: 階層 3（詳細設計 → ユニットテスト）-->
<!-- 兄弟: integration.md / 親: ../basic-design.md / ../detailed-design/ -->

## 1. 設計方針

- **対象**: `crates/shikomi-cli/src/autostart/` モジュール群の public / crate-internal 関数・型
  - `AutostartBackend::detect()` が OS ごとに正しい実装を返すこと（`#[cfg(target_os = ...)]` コンパイル時分岐の確認）
  - `resolve_daemon_path()` の正常系・異常系
  - plist / systemd unit / XDG desktop テンプレートの文字列置換
  - `AutostartError::Display` の出力フォーマット（`stderr_excerpt` 80 文字切り詰め）
  - `SystemdBackend::is_available()` の環境変数依存分岐
  - `DaemonSubcommand` の clap パース（`daemon install` / `uninstall` / `status`）
  - `lib.rs` の `no_ipc` 参照件数確認（Sub-A 2 件 → Sub-B 3 件に増加）
- **粒度**: 1 テスト 1 主要アサーション。命名 `test_何をした時_どうなるべきか`
- **配置**: Rust 慣習、`#[cfg(test)] mod tests` でソースモジュール内
- **疑似コード禁止**: Rust コードブロックは記述しない。処理手順は番号付き箇条書きで表現する

---

## 2. 外部 I/O 依存マップ

| 外部 I/O | 利用箇所 | 処置 |
|---------|---------|------|
| ファイルシステム（plist / unit / .desktop 書き込み）| Backend `install()` | IT スコープ。UT では pure fn（テンプレート展開のみ）を直接テスト |
| 環境変数 `DBUS_SESSION_BUS_ADDRESS` | `SystemdBackend::is_available()` | `std::env::remove_var` / `set_var` + `#[serial_test::serial]` で直列実行 |
| `std::env::current_exe()` | `resolve_daemon_path()` | `resolve_daemon_path_from(exe_dir: &Path)` 内部バリアントを介してテスト用ディレクトリを注入 |
| `which::which("systemctl")` | `SystemdBackend::is_available()` | `DBUS_SESSION_BUS_ADDRESS` 未設定による `false` 分岐到達で代替。`which` 結果は IT で確認 |
| OS コマンド実行（`launchctl` / `systemctl` / `schtasks`）| Backend `install()` / `uninstall()` | UT スコープ外。IT（TC-IT-120〜127）で確認 |
| clap 引数パース | `cli.rs` | OSS ライブラリ実物使用（モック不要）|

---

## 3. モック方針（UT）

| 対象 | モック方法 |
|------|-----------|
| OS 判定（`#[cfg(target_os = ...)]`）| コンパイル時分岐のため mock 不要。各 OS 固有テストに `#[cfg(target_os = "macos")]` 等を付与 |
| `resolve_daemon_path()` の daemon バイナリ存在確認 | `tempfile::TempDir` 内に `shikomi-daemon` の空ファイルを作成。不在ケースは空ディレクトリのまま |
| `DBUS_SESSION_BUS_ADDRESS` 環境変数 | `std::env::remove_var("DBUS_SESSION_BUS_ADDRESS")` + `#[serial_test::serial]` |
| plist / unit / desktop テンプレート展開 | テンプレート定数に対して置換ヘルパー pure fn を直接呼び出し（ファイル書き込みなし）|
| `AutostartError::Display` | エラー値を直接構築して `format!("{err}")` で文字列化 |

---

## 4. トレーサビリティマトリクス

| TC-ID | 対応要件 | 対応受入基準 | 種別 | 対象関数 / 観点 |
|-------|---------|------------|------|----------------|
| TC-UT-160 | REQ-DDM-013 | AC-DDM-07 | 正常 | `detect()` → macOS: `LaunchdBackend` |
| TC-UT-161 | REQ-DDM-013 | AC-DDM-07 | 正常 | `detect()` → Linux + systemd 有効: `SystemdBackend` |
| TC-UT-162 | REQ-DDM-013 | AC-DDM-07 | 正常 | `detect()` → Linux + systemd 無効: `XdgAutostartBackend` |
| TC-UT-163 | REQ-DDM-013 | AC-DDM-07 | 正常 | `detect()` → Windows: `WindowsTaskSchedulerBackend` |
| TC-UT-164 | REQ-DDM-010〜017 | AC-DDM-07 | 正常 | `resolve_daemon_path()` → daemon バイナリ存在時に `Ok(PathBuf)` |
| TC-UT-165 | REQ-DDM-010〜017 | AC-DDM-07 | 異常 | `resolve_daemon_path()` → daemon バイナリ不在時に `AutostartError::IoError(NotFound)` |
| TC-UT-166 | REQ-DDM-014 | AC-DDM-07 | 正常 | plist テンプレート展開: `{daemon_path}` 置換 |
| TC-UT-167 | REQ-DDM-014 | AC-DDM-07 | 正常 | plist テンプレート展開: `{log_dir}` 置換 |
| TC-UT-168 | REQ-DDM-015 | AC-DDM-07 | 正常 | systemd unit テンプレート展開: `{daemon_path}` が絶対パス |
| TC-UT-169 | REQ-DDM-016 | AC-DDM-07 | 正常 | XDG desktop テンプレート展開: `{daemon_path}` 置換 |
| TC-UT-170 | REQ-DDM-013 | AC-DDM-07 / AC-DDM-08 | 正常 | `AutostartError::CommandFailed` の Display が stderr_excerpt を 80 文字以内に切り詰める |
| TC-UT-171 | REQ-DDM-013 | AC-DDM-07 | 正常 | `AutostartError::CommandFailed` の Display が 80 文字未満の stderr_excerpt をそのまま出力する |
| TC-UT-172 | REQ-DDM-015 | AC-DDM-07 | 異常 | `SystemdBackend::is_available()`: `DBUS_SESSION_BUS_ADDRESS` 未設定 → `false` |
| TC-UT-173 | REQ-DDM-010 | AC-DDM-07 / AC-DDM-09 | 正常 | `DaemonSubcommand` parse: `shikomi daemon install` |
| TC-UT-174 | REQ-DDM-011 | AC-DDM-08 | 正常 | `DaemonSubcommand` parse: `shikomi daemon uninstall` |
| TC-UT-175 | REQ-DDM-012 | AC-DDM-09 | 正常 | `DaemonSubcommand` parse: `shikomi daemon status` |
| TC-UT-176 | REQ-DDM-001〜005 / REQ-DDM-012 | AC-DDM-09 | 契約 | `lib.rs` の `no_ipc` 参照が 3 件: vault dispatch + `build_handle` + daemon status IPC probe 分岐 |

上位トレーサビリティ: `TC-UT-160〜176` → `TC-IT-120〜127`（integration.md）→ `ST-DDM-020〜025`（system-test-design.md）→ `SC-DDM-002`（acceptance-tests/scenarios/）→ `AC-DDM-07〜10`（feature-spec.md §5）

---

## 5. テストケース一覧

### 5.1 `AutostartBackend::detect()` OS 判定（REQ-DDM-013）

配置: `crates/shikomi-cli/src/autostart/mod.rs` `#[cfg(test)] mod tests`

#### TC-UT-160: macOS で `detect()` が `LaunchdBackend` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| CI 条件 | `#[cfg(target_os = "macos")]` |
| 前提 | macOS ビルド環境 |
| 操作 | `autostart::detect()` を呼び出す |
| 期待 | 返値が `LaunchdBackend` 型であること（`std::any::type_name` または downcast で確認） |
| 検証方法 | `Box<dyn AutostartBackend>` を downcast して `LaunchdBackend` 型であることを確認 |

#### TC-UT-161: Linux + systemd 有効環境で `detect()` が `SystemdBackend` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| CI 条件 | `#[cfg(target_os = "linux")]` / `DBUS_SESSION_BUS_ADDRESS` 設定済み / `systemctl` 存在環境 |
| 前提 | `DBUS_SESSION_BUS_ADDRESS` をテスト用ダミー値に設定。`SystemdBackend::is_available()` が `true` を返す状態 |
| 操作 | `autostart::detect()` を呼び出す |
| 期待 | 返値が `SystemdBackend` 型であること |
| 検証方法 | 型名照合 |

#### TC-UT-162: Linux + systemd 無効環境で `detect()` が `XdgAutostartBackend` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| CI 条件 | `#[cfg(target_os = "linux")]` |
| 前提 | `std::env::remove_var("DBUS_SESSION_BUS_ADDRESS")` で環境変数をクリア（`#[serial_test::serial]`）|
| 操作 | `autostart::detect()` を呼び出す |
| 期待 | 返値が `XdgAutostartBackend` 型であること（`SystemdBackend::is_available()` が `false` のため） |
| 検証方法 | 型名照合 |

#### TC-UT-163: Windows で `detect()` が `WindowsTaskSchedulerBackend` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| CI 条件 | `#[cfg(target_os = "windows")]` |
| 前提 | Windows ビルド環境 |
| 操作 | `autostart::detect()` を呼び出す |
| 期待 | 返値が `WindowsTaskSchedulerBackend` 型であること |
| 検証方法 | 型名照合 |

---

### 5.2 `resolve_daemon_path()` パス解決（REQ-DDM-010〜017）

配置: `crates/shikomi-cli/src/autostart/mod.rs` `#[cfg(test)] mod tests`

#### TC-UT-164: daemon バイナリ存在時に `Ok(PathBuf)` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tempfile::TempDir` 内に `shikomi-daemon`（Unix）または `shikomi-daemon.exe`（Windows）の空ファイルを作成する |
| 操作 | 1. `tempfile::TempDir` を作成する / 2. `shikomi-daemon` の空ファイルをそのディレクトリに置く / 3. `resolve_daemon_path_from(dir)` を呼び出す（テスト用パス注入バリアント） |
| 期待 | `Ok(path)` が返り、`path.exists()` が `true` であること |
| 検証方法 | `assert!(result.is_ok())` / `assert!(result.unwrap().exists())` |
| 実装メモ | `current_exe()` に依存しない `resolve_daemon_path_from(exe_dir: &Path)` 内部 API を実装担当に推奨（詳細設計書 §resolve_daemon_path() に追記） |

#### TC-UT-165: daemon バイナリ不在時に `AutostartError::IoError(NotFound)` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 異常系 |
| 前提 | `tempfile::TempDir` 内に `shikomi-daemon` を**作成しない**（空ディレクトリのまま） |
| 操作 | 1. `tempfile::TempDir` を作成する / 2. `resolve_daemon_path_from(dir)` を呼び出す |
| 期待 | `Err(AutostartError::IoError(e))` が返り、`e.kind() == std::io::ErrorKind::NotFound` であること |
| 検証方法 | `matches!(result, Err(AutostartError::IoError(e)) if e.kind() == io::ErrorKind::NotFound)` |

---

### 5.3 plist テンプレート展開（REQ-DDM-014）

配置: `crates/shikomi-cli/src/autostart/launchd.rs` `#[cfg(test)] mod tests`
CI 条件: `#[cfg(target_os = "macos")]`

#### TC-UT-166: `{daemon_path}` が正しく置換されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `daemon_path = "/usr/local/bin/shikomi-daemon"` / `log_dir = "/Users/test/Library/Logs/shikomi"` |
| 操作 | 1. 上記 2 引数を plist テンプレート置換 pure fn に渡す / 2. 返値の文字列を検証する |
| 期待 | 返値に `"/usr/local/bin/shikomi-daemon"` が含まれること。`"{daemon_path}"` というリテラルが残っていないこと |
| 検証方法 | `assert!(result.contains("/usr/local/bin/shikomi-daemon"))` / `assert!(!result.contains("{daemon_path}"))` |

#### TC-UT-167: `{log_dir}` が正しく置換されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | TC-UT-166 と同じ入力 |
| 操作 | TC-UT-166 と同じ pure fn を呼び出す |
| 期待 | 返値に `"/Users/test/Library/Logs/shikomi/shikomi-daemon.log"` が含まれること。`"{log_dir}"` というリテラルが残っていないこと |
| 検証方法 | `assert!(result.contains("/Users/test/Library/Logs/shikomi/shikomi-daemon.log"))` / `assert!(!result.contains("{log_dir}"))` |

---

### 5.4 systemd unit テンプレート展開（REQ-DDM-015）

配置: `crates/shikomi-cli/src/autostart/systemd.rs` `#[cfg(test)] mod tests`
CI 条件: `#[cfg(target_os = "linux")]`

#### TC-UT-168: `{daemon_path}` が絶対パスで置換されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `daemon_path = "/home/user/.cargo/bin/shikomi-daemon"`（絶対パス） |
| 操作 | 1. 絶対パス文字列を systemd unit テンプレート置換 pure fn に渡す / 2. 返値の文字列を検証する |
| 期待 | 返値に `"ExecStart=/home/user/.cargo/bin/shikomi-daemon"` が含まれること。`"{daemon_path}"` というリテラルが残っていないこと。`ExecStart=` の値が `/` で始まること（絶対パス） |
| 検証方法 | `assert!(result.contains("ExecStart=/home/user/.cargo/bin/shikomi-daemon"))` / `assert!(!result.contains("{daemon_path}"))` |

---

### 5.5 XDG desktop テンプレート展開（REQ-DDM-016）

配置: `crates/shikomi-cli/src/autostart/xdg.rs` `#[cfg(test)] mod tests`
CI 条件: `#[cfg(target_os = "linux")]`

#### TC-UT-169: `{daemon_path}` が正しく置換されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `daemon_path = "/home/user/.local/bin/shikomi-daemon"` |
| 操作 | 1. パス文字列を XDG desktop テンプレート置換 pure fn に渡す / 2. 返値の文字列を検証する |
| 期待 | 返値に `"Exec=/home/user/.local/bin/shikomi-daemon"` が含まれること。`"{daemon_path}"` というリテラルが残っていないこと |
| 検証方法 | `assert!(result.contains("Exec=/home/user/.local/bin/shikomi-daemon"))` / `assert!(!result.contains("{daemon_path}"))` |

---

### 5.6 `AutostartError::Display` フォーマット（REQ-DDM-013）

配置: `crates/shikomi-cli/src/autostart/mod.rs` `#[cfg(test)] mod tests`

#### TC-UT-170: `CommandFailed` の Display が stderr_excerpt を 80 文字以内に切り詰めること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `cmd = "launchctl bootstrap gui/501 /tmp/dev.shikomi.daemon.plist"` / `stderr_excerpt` は 80 文字上限で構築（生成側の責務） |
| 操作 | 1. `AutostartError::CommandFailed` を構築する（`stderr_excerpt` に 80 文字の文字列を指定） / 2. `format!("{err}")` を実行する |
| 期待 | フォーマット出力が `"command failed: \`{cmd}\`: {stderr_excerpt}"` の形式であること。`stderr_excerpt` 相当部分が 80 文字以内であること |
| 検証方法 | 出力文字列を解析して stderr 部分の長さを確認。`assert!(stderr_part.len() <= 80)` |
| 設計根拠 | `detailed-design/backend-trait.md §AutostartError` / `basic-design.md §MSG-CLI-120 {reason} の制約` |

#### TC-UT-171: `CommandFailed` の Display が 80 文字未満の stderr_excerpt をそのまま出力すること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `stderr_excerpt = "Service is disabled"` (19 文字) |
| 操作 | 1. `AutostartError::CommandFailed` を構築する / 2. `format!("{err}")` を実行する |
| 期待 | 出力に `"Service is disabled"` が**全文**含まれること。切り詰めが発生しないこと |
| 検証方法 | `assert!(formatted.contains("Service is disabled"))` |

---

### 5.7 `SystemdBackend::is_available()` 環境変数依存（REQ-DDM-015）

配置: `crates/shikomi-cli/src/autostart/systemd.rs` `#[cfg(test)] mod tests`
CI 条件: `#[cfg(target_os = "linux")]`

#### TC-UT-172: `DBUS_SESSION_BUS_ADDRESS` 未設定時に `false` を返すこと

| 項目 | 内容 |
|------|------|
| 種別 | 異常系（環境変数なし） |
| 前提 | `std::env::remove_var("DBUS_SESSION_BUS_ADDRESS")` で環境変数をクリアする |
| 操作 | 1. `remove_var("DBUS_SESSION_BUS_ADDRESS")` を実行する / 2. `SystemdBackend::is_available()` を呼び出す / 3. テスト後に環境変数を復元する |
| 期待 | `false` を返すこと（D-Bus セッションバス不在のため） |
| 検証方法 | `assert!(!SystemdBackend::is_available())` |
| 注意 | 環境変数操作はテストスレッド間で競合する。`#[serial_test::serial]` を付与して直列実行を強制すること |
| 設計根拠 | `detailed-design/systemd.md §SystemdBackend::is_available()` 条件 2 |

---

### 5.8 `DaemonSubcommand` clap パース（REQ-DDM-010〜012）

配置: `crates/shikomi-cli/src/cli.rs` `#[cfg(test)] mod tests`

#### TC-UT-173: `shikomi daemon install` → `DaemonSubcommand::Install`

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "daemon", "install"])` |
| 期待 | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Install)` |
| 検証方法 | `assert!(matches!(args.subcommand, Subcommand::Daemon(DaemonSubcommand::Install)))` |

#### TC-UT-174: `shikomi daemon uninstall` → `DaemonSubcommand::Uninstall`

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "daemon", "uninstall"])` |
| 期待 | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Uninstall)` |
| 検証方法 | `assert!(matches!(args.subcommand, Subcommand::Daemon(DaemonSubcommand::Uninstall)))` |

#### TC-UT-175: `shikomi daemon status` → `DaemonSubcommand::Status`

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "daemon", "status"])` |
| 期待 | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Status)` |
| 検証方法 | `assert!(matches!(args.subcommand, Subcommand::Daemon(DaemonSubcommand::Status)))` |

---

### 5.9 `lib.rs` の `no_ipc` 参照件数確認（REQ-DDM-001〜005 / REQ-DDM-012）

配置: `crates/shikomi-cli/src/lib.rs` `#[cfg(test)] mod tests`

#### TC-UT-176: `lib.rs` の `no_ipc` 参照が 3 件であること

| 項目 | 内容 |
|------|------|
| 種別 | 契約（静的検査） |
| 前提 | Sub-B 実装完了後の状態（`run_daemon_subcommand` が `lib.rs` に追加済み） |
| 操作 | `include_str!("lib.rs").matches("no_ipc").count()` で参照件数を確認する（または CI ゲートで `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` の件数を確認する） |
| 期待 | `no_ipc` の参照が **3 件のみ**であること。内訳: 1. `build_handle` 内の IPC/SQLite 分岐 / 2. vault サブコマンド dispatch の MSG-CLI-052 出力判定 / 3. `run_daemon_subcommand` 内の daemon status IPC probe 省略分岐 |
| 検証方法 | `assert_eq!(include_str!("lib.rs").matches("no_ipc").count(), 3)` |
| 設計根拠 | `detailed-design/presenter.md §CI 確認コマンド`。Sub-A（TC-UT-159）の 2 件から daemon status probe 追加で 3 件に増加 |
| 失敗条件 | 2 件以下: daemon status IPC probe 分岐が未実装。4 件以上: `no_ipc` が意図しない箇所に漏れている |

---

## 6. テスト配置

| ファイル | TC-ID | 実行レシピ |
|---------|-------|----------|
| `crates/shikomi-cli/src/autostart/mod.rs` `#[cfg(test)]` | TC-UT-160〜165, TC-UT-170〜171 | `just test-cli` |
| `crates/shikomi-cli/src/autostart/launchd.rs` `#[cfg(test)]` `#[cfg(target_os = "macos")]` | TC-UT-166〜167 | `just test-cli`（macOS CI のみ） |
| `crates/shikomi-cli/src/autostart/systemd.rs` `#[cfg(test)]` `#[cfg(target_os = "linux")]` | TC-UT-168, TC-UT-172 | `just test-cli`（Linux CI のみ） |
| `crates/shikomi-cli/src/autostart/xdg.rs` `#[cfg(test)]` `#[cfg(target_os = "linux")]` | TC-UT-169 | `just test-cli`（Linux CI のみ） |
| `crates/shikomi-cli/src/cli.rs` `#[cfg(test)]` | TC-UT-173〜175 | `just test-cli` |
| `crates/shikomi-cli/src/lib.rs` `#[cfg(test)]` | TC-UT-176 | `just test-cli` |

---

## 7. CI 監査ゲート

| チェック | コマンド / 手段 | 失敗条件 |
|---------|--------------|---------|
| UT 全件通過 | `just test-cli` | 1 件でも FAILED |
| `no_ipc` 参照が `lib.rs` で 3 件（vault dispatch + `build_handle` + daemon status IPC probe 分岐） | `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` | 3 件以外 |
| `DaemonSubcommand` が `cli.rs` にのみ定義されていること | `grep -rn "DaemonSubcommand" crates/shikomi-cli/src/` | `cli.rs` 以外にヒット |
| `autostart::` 参照が `lib.rs` にあること | `grep -n "autostart::" crates/shikomi-cli/src/lib.rs` | 0 件 |
| `AutostartError::CommandFailed` stderr_excerpt ≤ 80 文字 | TC-UT-170 | テスト失敗 |
