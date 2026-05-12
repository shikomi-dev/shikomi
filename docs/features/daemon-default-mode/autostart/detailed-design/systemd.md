# 詳細設計書 — autostart / SystemdBackend（Linux systemd）

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/systemd.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親目次: ./index.md -->
<!-- 対応 REQ: REQ-DDM-015 -->

## `crates/shikomi-cli/src/autostart/systemd.rs` の詳細

`#[cfg(target_os = "linux")]` でスコープ。

### unit ファイルテンプレート

以下のテンプレートを Rust の `const &str` として定義し、`{daemon_path}` を文字列置換して書き込む:

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
- `{daemon_path}`: `resolve_daemon_path()` 呼出で取得した絶対パス（必須、相対パス不可）

**配置先**: `dirs::home_dir()` + `.config/systemd/user/shikomi-daemon.service`

### `SystemdBackend::name()` 戻り値

```
"SystemdBackend"
```

### `SystemdBackend::is_available()` 判定ロジック

以下を**すべて満たす**場合に `true` を返す:

1. `which::which("systemctl")` が `Ok(_)`（`which` crate を使用）
2. `std::env::var("DBUS_SESSION_BUS_ADDRESS")` が `Ok(_)`（D-Bus セッションバスが存在）
3. `Command::new("systemctl").args(["--user", "status", "--no-pager"]).output()` の exit code が `4` **以外**（exit 4 は systemd の "D-Bus 接続失敗" を意味する）

**設計判断**:
- `which systemctl` のみでは WSL 等の非 systemd 環境で false positive になる。D-Bus チェック（条件 2）を組み合わせる
- exit code `4`（systemd D-Bus エラー）と `0`（正常）/ `1`〜`3`（unit 状態問題）を区別するため exit code を直接確認する（stderr 文字列解析ではなく数値ベース）
- [参照: systemd man page §EXIT STATUS](https://www.freedesktop.org/software/systemd/man/systemctl.html)

### `SystemdBackend::install()` 処理手順

1. `resolve_daemon_path()` で `{daemon_path}` を解決する
2. unit テンプレートの `{daemon_path}` を文字列置換して unit 内容を生成する
3. `~/.config/systemd/user/` を `create_dir_all` で作成する
4. `~/.config/systemd/user/shikomi-daemon.service` に `write` で書き込む（上書き = 冪等）
5. `systemctl --user daemon-reload` を実行する（失敗時 → `AutostartError::CommandFailed`）
6. `systemctl --user enable --now shikomi-daemon.service` を実行する（失敗時 → `AutostartError::CommandFailed`）
   - `--now` によって即時起動 + 次回起動時の自動起動が同時に有効化される

**設計判断**:
- `enable --now` で登録と即時起動を単一ステップで行う（launchd と同様、次回ログイン待ちにならない）
- `daemon-reload` はステップ 4 の unit ファイル書き込み後に必ず実行する（systemd が新規 unit を認識するため）

### `SystemdBackend::uninstall()` 処理手順

1. `systemctl --user disable --now shikomi-daemon.service` を実行する（未登録でも無視 — 冪等）
2. `~/.config/systemd/user/shikomi-daemon.service` を `remove_file` で削除する（`NotFound` → `Ok(())` — 冪等）
3. `systemctl --user daemon-reload` を実行する（unit ファイル削除後の再読込）

### `SystemdBackend::is_registered()` 処理手順

1. `~/.config/systemd/user/shikomi-daemon.service` が `Path::exists()` で存在するかを確認する
2. `true` / `false` を返す

### `SystemdBackend::install_hint()` 戻り値

```
Some("hint: to check status: systemctl --user status shikomi-daemon".to_string())
```
