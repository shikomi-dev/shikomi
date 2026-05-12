# 詳細設計書 — autostart / XdgAutostartBackend（Linux XDG Autostart フォールバック）

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/xdg.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親目次: ./index.md -->
<!-- 対応 REQ: REQ-DDM-016 -->

## `crates/shikomi-cli/src/autostart/xdg.rs` の詳細

`#[cfg(target_os = "linux")]` でスコープ。`detect()` が `SystemdBackend::is_available() == false` の場合に選択する。

### desktop エントリテンプレート

以下のテンプレートを Rust の `const &str` として定義し、`{daemon_path}` を文字列置換して書き込む:

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
- `{daemon_path}`: `resolve_daemon_path()` 呼出で取得した絶対パス

**配置先**: `dirs::home_dir()` + `.config/autostart/shikomi-daemon.desktop`

**設計判断**:
- `~/.config/autostart/` はユーザースコープ（システム権限不要）。XDG Autostart 仕様準拠
- `/etc/xdg/autostart/` ではなくユーザーディレクトリを使用する（root 権限を一切要求しない設計原則）

### `XdgAutostartBackend::name()` 戻り値

```
"XdgAutostartBackend"
```

### `XdgAutostartBackend::install()` 処理手順

1. `resolve_daemon_path()` で `{daemon_path}` を解決する
2. desktop エントリテンプレートの `{daemon_path}` を文字列置換する
3. `~/.config/autostart/` を `create_dir_all` で作成する
4. `~/.config/autostart/shikomi-daemon.desktop` に `write` で書き込む（上書き = 冪等）

**設計判断**:
- 外部コマンド呼出は一切なし（`launchctl` / `systemctl` と異なり、ファイル書き込みのみで完結）
- Fail Fast: `resolve_daemon_path()` 失敗時は `AutostartError::IoError(NotFound)` で即時終了

### `XdgAutostartBackend::uninstall()` 処理手順

1. `~/.config/autostart/shikomi-daemon.desktop` を `remove_file` で削除する（`NotFound` → `Ok(())` — 冪等）

### `XdgAutostartBackend::is_registered()` 処理手順

1. `~/.config/autostart/shikomi-daemon.desktop` が `Path::exists()` で存在するかを確認する
2. `true` / `false` を返す

### `XdgAutostartBackend::install_hint()` 戻り値

```
Some("hint: this uses XDG Autostart; shikomi-daemon will start on next login".to_string())
```

**設計判断**:
- systemd と異なり `--now` 相当の即時起動コマンドがない（XDG Autostart はログイン時トリガーのみ）
- hint でログイン時起動であることをユーザーに明示する（ペルソナ整合 / 期待値管理）
