# 詳細設計書 — autostart / LaunchdBackend（macOS）

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/launchd.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親目次: ./index.md -->
<!-- 対応 REQ: REQ-DDM-014 -->

## `crates/shikomi-cli/src/autostart/launchd.rs` の詳細

`#[cfg(target_os = "macos")]` でスコープ。

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
- `{daemon_path}`: `resolve_daemon_path()` 呼出で取得した絶対パス（`canonicalize()` 済み）
- `{log_dir}`: `dirs::home_dir()` + `Library/Logs/shikomi` を文字列として展開（`~/Library/Logs/shikomi` 相当）

**配置先**: `dirs::home_dir()` + `Library/LaunchAgents/dev.shikomi.daemon.plist`

### `LaunchdBackend::name()` 戻り値

```
"LaunchdBackend"
```

### `LaunchdBackend::install()` 処理手順

1. `resolve_daemon_path()` で `{daemon_path}` を解決する（失敗 → `AutostartError::IoError`）
2. `{log_dir}`（`~/Library/Logs/shikomi/`）を `std::fs::create_dir_all` で作成する（存在済みなら無視）
3. plist テンプレートの `{daemon_path}` / `{log_dir}` を文字列置換して plist 内容を生成する
4. `~/Library/LaunchAgents/` を `create_dir_all` で作成する
5. `~/Library/LaunchAgents/dev.shikomi.daemon.plist` に `std::fs::write` で書き込む（上書き = 冪等）
6. `launchctl bootout gui/{uid}/dev.shikomi.daemon` を実行する（**未登録エラーは無視** — 冪等確保）
7. `launchctl bootstrap gui/{uid} {plist_path}` を実行する（失敗時 → `AutostartError::CommandFailed`）
   - `{uid}` は `nix::unistd::getuid().as_raw()` で取得する

**設計判断**:
- ステップ 6（事前 `bootout`）→ ステップ 7（`bootstrap`）の順序で冪等性を構造的に保証する（TOCTOU 対策 / `security.md §冪等性 TOCTOU`）
- `launchctl` の引数は `Command::new("launchctl").arg("bootout").arg(...)` の配列渡し（shell injection 不可 / `security.md §A03`）

### `LaunchdBackend::uninstall()` 処理手順

1. `launchctl bootout gui/{uid}/dev.shikomi.daemon` を実行する（非 0 exit は許容 — 未登録の冪等）
2. `~/Library/LaunchAgents/dev.shikomi.daemon.plist` を `std::fs::remove_file` で削除する（`NotFound` → `Ok(())` に変換 — 冪等）

### `LaunchdBackend::is_registered()` 処理手順

1. `~/Library/LaunchAgents/dev.shikomi.daemon.plist` が `Path::exists()` で存在するかを確認する
2. `true` / `false` を返す

**設計判断**: `launchctl list` 呼出コストを避けるため plist 存在確認で代替する。plist 存在 = launchd 登録の必要十分条件（インストール後 launchd が読み込む）

### `LaunchdBackend::is_registered()` — Fail Safe

`Path::exists()` が `io::Error` を返す可能性はほぼないが、万一 panic を起こさず `false` を返す（Fail Safe / REQ-DDM-012「確認できない状態も結果として出力する」）

### `LaunchdBackend::install_hint()` 戻り値

`Some(format!("hint: to start immediately: launchctl kickstart gui/{uid}/dev.shikomi.daemon"))`

**設計判断**: `launchctl bootstrap` はログイン時の自動起動を登録するが現セッションには適用されない。`kickstart` を案内することで即時起動をユーザーが選択できる（REQ-DDM-014）
