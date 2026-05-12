# 詳細設計書 — autostart / WindowsTaskSchedulerBackend（Windows）

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/windows.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親目次: ./index.md -->
<!-- 対応 REQ: REQ-DDM-017 -->

## `crates/shikomi-cli/src/autostart/windows.rs` の詳細

`#[cfg(target_os = "windows")]` でスコープ。

### タスク名

`"shikomi\shikomi-daemon"`（バックスラッシュによるサブフォルダ構造。Task Scheduler UI で `shikomi` フォルダ配下に整理される）

### `WindowsTaskSchedulerBackend::name()` 戻り値

```
"WindowsTaskSchedulerBackend"
```

### `WindowsTaskSchedulerBackend::install()` 処理手順

1. `resolve_daemon_path()` で `{daemon_path}` を解決する（`.exe` 拡張子付き）
2. **冪等確保（事前確認）**: `Command::new("schtasks").args(["/Query", "/TN", r"shikomi\shikomi-daemon"]).output()` を実行し、exit 0 ならタスク登録済みとして `Ok(())` を早期返却する
3. `Command::new("schtasks").args(["/Create", "/SC", "ONLOGON", "/TN", r"shikomi\shikomi-daemon", "/TR", &daemon_path_str, "/F"]).output()` を実行する（失敗時 → `AutostartError::CommandFailed`）
   - `/F`: 既存タスクを強制上書き（冪等のフォールバック）

**設計判断**:
- レジストリ `Run` キーではなく Task Scheduler を採用する（UAC プロンプトを回避できるため / REQ-DDM-017）
- ステップ 2 の事前確認 → ステップ 3 の `/F` 上書きで二重の冪等保護を実現する
- `schtasks` の引数は配列渡し（`Command::new("schtasks").args([...])`）。shell injection 不可（`security.md §A03`）

### `WindowsTaskSchedulerBackend::uninstall()` 処理手順

1. `Command::new("schtasks").args(["/Delete", "/TN", r"shikomi\shikomi-daemon", "/F"]).output()` を実行する
   - `/F`: 確認プロンプトを省略する
2. exit 非 0 かつ stderr に `"The system cannot find the file"` が含まれる場合（未登録タスクの削除）は `Ok(())` に変換する（冪等）
3. その他の非 0 exit は `AutostartError::CommandFailed` を返す

**設計判断**:
- stderr 文字列判別は「未登録タスク」という特定の冪等ケースのみに使用する。汎用の stderr 解析ではない

### `WindowsTaskSchedulerBackend::is_registered()` 処理手順

1. `Command::new("schtasks").args(["/Query", "/TN", r"shikomi\shikomi-daemon"]).output()` を実行する
2. exit 0 なら `true`、それ以外は `false` を返す

### `WindowsTaskSchedulerBackend::install_hint()` 戻り値

```
Some(r#"hint: to start immediately: schtasks /Run /TN "shikomi\shikomi-daemon""#.to_string())
```

**設計判断**:
- `schtasks /Create /SC ONLOGON` はログオン時のみ実行する。即時起動が必要な場合は `/Run` を使用する（launchd の `kickstart` と同等の役割）
