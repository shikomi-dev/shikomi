# ユニットテスト設計書 — daemon-default-mode / autostart

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/test-design/unit.md -->
<!-- Vモデル対応: 階層 3（UT — 詳細設計・クラス/メソッドレベル）-->
<!-- 親: ../basic-design.md / ../detailed-design.md -->

## 目的

`autostart` サブシステムの純粋な関数・型・テンプレート展開・CLI パース・エラー表現を
ユニットテストで検証する。外部プロセス（`launchctl` / `systemctl` / `schtasks`）・
実ファイルシステム・環境変数への依存はモックまたは環境変数オーバーライドで隔離する。

---

## テストマトリクス（トレーサビリティ）

| TC-ID | 対象 REQ / 詳細設計箇所 | 検証内容 | 実装先 |
|-------|----------------------|---------|--------|
| TC-UT-160 | REQ-DDM-010 / `cli.rs §DaemonSubcommand` | `shikomi daemon install` clap パース | `crates/shikomi-cli/src/cli.rs #[cfg(test)]` |
| TC-UT-161 | REQ-DDM-011 / `cli.rs §DaemonSubcommand` | `shikomi daemon uninstall` clap パース | 同上 |
| TC-UT-162 | REQ-DDM-012 / `cli.rs §DaemonSubcommand` | `shikomi daemon status` clap パース | 同上 |
| TC-UT-163 | REQ-DDM-013 / `AutostartError::Display` | `CommandFailed` の stderr_excerpt が 80 文字で切り詰め | `crates/shikomi-cli/src/autostart/mod.rs #[cfg(test)]` |
| TC-UT-164 | REQ-DDM-013 / `AutostartError::Display` | `IoError` の Display 形式 | 同上 |
| TC-UT-165 | REQ-DDM-013 / `AutostartError::Display` | `Unsupported` の Display 形式 | 同上 |
| TC-UT-166 | REQ-DDM-013 / `resolve_daemon_path()` | daemon 不在 → `AutostartError::IoError(NotFound)` | 同上 |
| TC-UT-167 | REQ-DDM-014 / `launchd.rs §plist テンプレート` | plist テンプレート展開 — `{daemon_path}` / `{log_dir}` 置換 | `crates/shikomi-cli/src/autostart/launchd.rs #[cfg(test)]` |
| TC-UT-168 | REQ-DDM-015 / `systemd.rs §unit テンプレート` | systemd unit テンプレート展開 — `{daemon_path}` 置換 | `crates/shikomi-cli/src/autostart/systemd.rs #[cfg(test)]` |
| TC-UT-169 | REQ-DDM-016 / `xdg.rs §desktop テンプレート` | XDG desktop テンプレート展開 — `{daemon_path}` 置換 | `crates/shikomi-cli/src/autostart/xdg.rs #[cfg(test)]` |
| TC-UT-170 | REQ-DDM-015 / `SystemdBackend::is_available()` | `DBUS_SESSION_BUS_ADDRESS` 未設定 → `false` | `crates/shikomi-cli/src/autostart/systemd.rs #[cfg(test)]` |
| TC-UT-171 | REQ-DDM-010 / `presenter/error.rs §MSG-CLI-120` | `render_autostart_install_error` — MSG-CLI-120 フォーマット | `crates/shikomi-cli/src/presenter/error.rs #[cfg(test)]` |
| TC-UT-172 | REQ-DDM-011 / `presenter/error.rs §MSG-CLI-121` | `render_autostart_uninstall_error` — MSG-CLI-121 フォーマット | 同上 |

---

## 外部 I/O 依存マップ

| 依存先 | 対象 TC | モック方針 | fixture 状態 |
|--------|---------|-----------|-------------|
| `std::env::current_exe()` | TC-UT-166 | `tempfile::NamedTempFile` を使い、テスト時に PATH を制御するか、`resolve_daemon_path` の内部ロジックを分割して存在確認関数を差し替え可能にする | 要起票（#resolve-daemon-path-testability）|
| `DBUS_SESSION_BUS_ADDRESS` 環境変数 | TC-UT-170 | テスト内で `std::env::remove_var` → テスト後に復元。`#[serial_test::serial]` で環境変数競合を防ぐ | raw fixture 不要（env var 操作のみ） |
| `dirs::home_dir()` | TC-UT-167〜169 (テンプレート展開のみ) | テンプレート展開は文字列置換のみで `home_dir` 不要。UT レベルでは入力文字列を直接渡す | 不要 |
| ファイルシステム（`Path::exists`） | TC-UT-166 | `tempfile::TempDir` + 存在しないパスを渡して `IoError(NotFound)` を誘発 | 不要 |

> **注記**: `resolve_daemon_path()` のフル経路テスト（`current_exe()` 同ディレクトリ解決）は
> IT レベル（TC-IT-127）で `HOME` + 実バイナリパスを使って検証する。UT では「daemon
> バイナリが存在しない」パスのエラー系のみを対象とする。

---

## テストケース詳細

### TC-UT-160: `shikomi daemon install` clap パース

| 項目 | 内容 |
|------|------|
| 対象 | `CliArgs::try_parse_from(["shikomi", "daemon", "install"])` |
| 前提 | なし |
| 手順 | `CliArgs::try_parse_from` に引数スライスを渡す |
| 期待 | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Install)` |
| 正常 / 異常 | 正常系 |
| 実装先 | `cli.rs #[cfg(test)] fn ut_160_daemon_install_parses()` |

### TC-UT-161: `shikomi daemon uninstall` clap パース

| 項目 | 内容 |
|------|------|
| 対象 | `CliArgs::try_parse_from(["shikomi", "daemon", "uninstall"])` |
| 期待 | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Uninstall)` |
| 正常 / 異常 | 正常系 |
| 実装先 | `cli.rs #[cfg(test)] fn ut_161_daemon_uninstall_parses()` |

### TC-UT-162: `shikomi daemon status` clap パース

| 項目 | 内容 |
|------|------|
| 対象 | `CliArgs::try_parse_from(["shikomi", "daemon", "status"])` |
| 期待 | `args.subcommand == Subcommand::Daemon(DaemonSubcommand::Status)` |
| 正常 / 異常 | 正常系 |
| 実装先 | `cli.rs #[cfg(test)] fn ut_162_daemon_status_parses()` |

---

### TC-UT-163: `AutostartError::Display` — `CommandFailed` 80 文字切り詰め

| 項目 | 内容 |
|------|------|
| 対象 | `AutostartError::CommandFailed { cmd, stderr_excerpt }` の `Display` 実装 |
| 前提 | `stderr_excerpt` が 80 文字以内（型の構造的保証）。UT では 80 文字のダミー文字列を渡す |
| 手順 | `AutostartError::CommandFailed { cmd: "launchctl bootstrap ...".to_string(), stderr_excerpt: "x".repeat(80) }` を作成し `format!("{err}")` を検証 |
| 期待 | (1) 出力が `"command failed: \`launchctl bootstrap ...\`: "` で始まる / (2) `stderr_excerpt` の 80 文字が含まれる / (3) 出力が 200 文字以内（パス情報 + 80 文字の組み合わせ上限の妥当性チェック）|
| セキュリティ観点 | stderr_excerpt は秘密情報を含まない。80 文字上限は `AutostartError::CommandFailed` 生成側の責務（UT では型の Display 出力のみ検証、切り詰めロジックは生成側 IT で検証）|
| 正常 / 異常 | 正常系 |
| 実装先 | `autostart/mod.rs #[cfg(test)] fn ut_163_command_failed_display_format()` |

### TC-UT-164: `AutostartError::Display` — `IoError`

| 項目 | 内容 |
|------|------|
| 対象 | `AutostartError::IoError(std::io::Error)` の `Display` |
| 手順 | `AutostartError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "shikomi-daemon not found at /usr/bin/shikomi-daemon"))` を作成し Display を確認 |
| 期待 | `"I/O error: shikomi-daemon not found at /usr/bin/shikomi-daemon"` |
| 正常 / 異常 | 正常系 |
| 実装先 | `autostart/mod.rs #[cfg(test)] fn ut_164_io_error_display_format()` |

### TC-UT-165: `AutostartError::Display` — `Unsupported`

| 項目 | 内容 |
|------|------|
| 対象 | `AutostartError::Unsupported { reason }` の `Display` |
| 手順 | `AutostartError::Unsupported { reason: "no D-Bus session bus".to_string() }` を作成し Display を確認 |
| 期待 | `"unsupported: no D-Bus session bus"` |
| 正常 / 異常 | 正常系 |
| 実装先 | `autostart/mod.rs #[cfg(test)] fn ut_165_unsupported_display_format()` |

---

### TC-UT-166: `resolve_daemon_path()` — daemon バイナリ不在 → `IoError`

| 項目 | 内容 |
|------|------|
| 対象 | `resolve_daemon_path()` |
| 前提 | `current_exe()` が返すパスと同ディレクトリに `shikomi-daemon` が存在しない状況を再現する |
| 手順 | (1) `tempfile::TempDir` を作成し、そこに `shikomi` ダミーファイルを作成 / (2) `resolve_daemon_path()` を直接呼ぶか、またはダミーパスを受け取る内部関数 `resolve_daemon_path_from(exe_path)` を分割してテスト / (3) `shikomi-daemon` が存在しないディレクトリを指定 |
| 期待 | `Err(AutostartError::IoError(_))` かつ `err.to_string().contains("not found")` |
| 正常 / 異常 | 異常系（エッジケース: daemon バイナリが削除された / 未インストール）|
| 実装先 | `autostart/mod.rs #[cfg(test)] fn ut_166_resolve_daemon_path_not_found()` |
| 補足 | `resolve_daemon_path()` が `current_exe()` に依存するため、テスト容易性のため `resolve_daemon_path_from(exe_dir: &Path)` 内部 API を設けることを実装担当に推奨する（詳細設計書 §resolve_daemon_path() に追記要）|

---

### TC-UT-167: plist テンプレート展開（`LaunchdBackend`）

```
#[cfg(target_os = "macos")]
```

| 項目 | 内容 |
|------|------|
| 対象 | `launchd.rs` 内のテンプレート定数 + 文字列置換ロジック |
| 手順 | (1) `daemon_path = "/usr/local/bin/shikomi-daemon"` / `log_dir = "/Users/test/Library/Logs/shikomi"` を入力 / (2) テンプレート展開関数（内部 helper または `install()` の展開ロジックを切り出した pure fn）を呼び出す / (3) 出力 plist 文字列を検証 |
| 期待 | (1) `<string>/usr/local/bin/shikomi-daemon</string>` を含む / (2) `<string>/Users/test/Library/Logs/shikomi/shikomi-daemon.log</string>` を含む / (3) `{daemon_path}` / `{log_dir}` という未展開プレースホルダが残っていない |
| 正常 / 異常 | 正常系 |
| 実装先 | `autostart/launchd.rs #[cfg(all(test, target_os = "macos"))] fn ut_167_plist_template_expansion()` |

### TC-UT-168: systemd unit テンプレート展開（`SystemdBackend`）

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 対象 | `systemd.rs` 内のテンプレート定数 + 文字列置換ロジック |
| 手順 | `daemon_path = "/home/user/.cargo/bin/shikomi-daemon"` を入力し、展開 pure fn を呼び出す |
| 期待 | (1) `ExecStart=/home/user/.cargo/bin/shikomi-daemon` を含む / (2) `{daemon_path}` プレースホルダが残っていない / (3) `[Unit]` / `[Service]` / `[Install]` セクションが存在する |
| 正常 / 異常 | 正常系 |
| 実装先 | `autostart/systemd.rs #[cfg(all(test, target_os = "linux"))] fn ut_168_systemd_unit_template_expansion()` |

### TC-UT-169: XDG desktop テンプレート展開（`XdgAutostartBackend`）

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 対象 | `xdg.rs` 内のテンプレート定数 + 文字列置換ロジック |
| 手順 | `daemon_path = "/opt/shikomi/shikomi-daemon"` を入力 |
| 期待 | (1) `Exec=/opt/shikomi/shikomi-daemon` を含む / (2) `{daemon_path}` プレースホルダが残っていない / (3) `[Desktop Entry]` セクションが存在し `Type=Application` を含む |
| 正常 / 異常 | 正常系 |
| 実装先 | `autostart/xdg.rs #[cfg(all(test, target_os = "linux"))] fn ut_169_xdg_desktop_template_expansion()` |

---

### TC-UT-170: `SystemdBackend::is_available()` — `DBUS_SESSION_BUS_ADDRESS` 未設定で `false`

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 対象 | `SystemdBackend::is_available()` |
| 前提 | `DBUS_SESSION_BUS_ADDRESS` 環境変数が未設定 |
| 手順 | (1) `std::env::remove_var("DBUS_SESSION_BUS_ADDRESS")` / (2) `SystemdBackend::is_available()` を呼ぶ / (3) 環境変数を復元 |
| 期待 | `false` が返る（D-Bus 未設定は systemd ユーザセッション不在とみなす）|
| 注意 | 環境変数操作は他テストと競合する。`#[serial_test::serial]` アトリビュートを付与すること（`serial_test` crate は `shikomi-cli` に dev-dependency として追加要）|
| 正常 / 異常 | 異常系（環境変数欠如）|
| 実装先 | `autostart/systemd.rs #[cfg(all(test, target_os = "linux"))] fn ut_170_is_available_false_when_no_dbus()` |

---

### TC-UT-171: `render_autostart_install_error` — MSG-CLI-120 フォーマット

| 項目 | 内容 |
|------|------|
| 対象 | `presenter::error::render_autostart_install_error(err, locale)` |
| 手順 | `AutostartError::CommandFailed { cmd: "launchctl".to_string(), stderr_excerpt: "Service is disabled".to_string() }` を渡し、`Locale::English` で呼び出す |
| 期待 | `"error: failed to enable autostart: command failed: \`launchctl\`: Service is disabled"` |
| 正常 / 異常 | 正常系 |
| セキュリティ観点 | 出力に `"password"` / `"secret"` / `"token"` が含まれないことも確認 |
| 実装先 | `presenter/error.rs #[cfg(test)] fn ut_171_msg_cli_120_format()` |

### TC-UT-172: `render_autostart_uninstall_error` — MSG-CLI-121 フォーマット

| 項目 | 内容 |
|------|------|
| 対象 | `presenter::error::render_autostart_uninstall_error(err, locale)` |
| 手順 | `AutostartError::IoError(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Permission denied (os error 13)"))` を渡し、`Locale::English` で呼び出す |
| 期待 | `"error: failed to disable autostart: I/O error: Permission denied (os error 13)"` |
| 正常 / 異常 | 異常系 |
| 実装先 | `presenter/error.rs #[cfg(test)] fn ut_172_msg_cli_121_format()` |

---

## モック方針

| 外部依存 | UT での扱い |
|---------|-----------|
| `launchctl` / `systemctl` / `schtasks` | UT レベルでは呼び出さない。テンプレート展開 pure fn を抽出してファイル I/O / コマンド実行なしに検証する |
| `std::env::current_exe()` | TC-UT-166 のみ: `resolve_daemon_path_from(dir: &Path)` を内部 API として分割し、任意 `dir` を渡せるようにする（実装担当推奨事項）|
| `DBUS_SESSION_BUS_ADDRESS` | `remove_var` / `set_var` + `#[serial]` でスレッド安全に制御 |
| `dirs::home_dir()` | UT では不使用（テンプレート展開 pure fn はパス文字列を引数で受け取る設計）|

---

## 実行方法

```sh
# shikomi-cli クレートのユニットテストのみ実行
just test-cli

# または cargo で直接
cargo test -p shikomi-cli --lib
```

---

*百年後まで御機嫌よう。*
