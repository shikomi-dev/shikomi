# 詳細設計書 — autostart / presenter 変更詳細・実装担当引き継ぎメモ

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/presenter.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親目次: ./index.md -->

## `crates/shikomi-cli/src/presenter/error.rs` の変更詳細

### MSG-CLI-120 / MSG-CLI-121 追加

`presenter/error.rs` に以下の 2 関数を追加する:

```
pub fn render_autostart_install_error(err: &AutostartError, locale: Locale) -> String
```

| Locale | 出力文 |
|--------|-------|
| English | `error: failed to enable autostart: {err}` |
| JapaneseEn | 英語行 + `エラー: 自動起動の有効化に失敗しました: {err}` の 2 行 |

```
pub fn render_autostart_uninstall_error(err: &AutostartError, locale: Locale) -> String
```

| Locale | 出力文 |
|--------|-------|
| English | `error: failed to disable autostart: {err}` |
| JapaneseEn | 英語行 + `エラー: 自動起動の無効化に失敗しました: {err}` の 2 行 |

**`{err}` の内容（`AutostartError::Display` 実装）**:

| バリアント | 出力例 |
|-----------|-------|
| `CommandFailed` | `command failed: \`launchctl bootstrap gui/501 ...\`: Service is disabled` |
| `IoError` | `I/O error: Permission denied (os error 13)` |
| `Unsupported` | `unsupported: no D-Bus session bus` |

## `crates/shikomi-cli/src/presenter/success.rs` の変更詳細（ペガサス指摘②対応）

`run_daemon_subcommand` 内の成功メッセージを `println!` ハードコードではなく `presenter/success.rs` に集約する。`render_vault_ipc_forced_note`（`warning.rs`）と同型のロケール対応関数として実装する。

### `render_autostart_installed()` 追加

```
pub fn render_autostart_installed(locale: Locale) -> String
```

| Locale | 出力文 |
|--------|-------|
| English | `shikomi-daemon autostart enabled` |
| JapaneseEn | `shikomi-daemon autostart enabled` + `shikomi-daemon の自動起動を有効にしました` の 2 行 |

### `render_autostart_uninstalled()` 追加

```
pub fn render_autostart_uninstalled(locale: Locale) -> String
```

| Locale | 出力文 |
|--------|-------|
| English | `shikomi-daemon autostart disabled` |
| JapaneseEn | `shikomi-daemon autostart disabled` + `shikomi-daemon の自動起動を無効にしました` の 2 行 |

**設計判断**:
- `basic-design.md §ユーザー向けメッセージ一覧` の成功メッセージ確定文面と一致させる
- `quiet == true` の場合は呼出側（`run_daemon_subcommand`）で `render_*` を呼ばない（`quiet` 制御は presenter ではなく呼出側の責務 / Sub-A `cli/detailed-design.md` の設計方針を踏襲）
- 成功メッセージは `println!`（stdout）に出力する。エラーメッセージ（MSG-CLI-120/121）は `eprint_stderr`（stderr）に出力する

## 実装担当（坂田銀時）への引き継ぎメモ

### 実装手順（推奨順序）

1. `crates/shikomi-cli/src/autostart/` ディレクトリを作成する
2. `autostart/mod.rs` に `AutostartBackend` trait / `AutostartError` / `detect()` / `resolve_daemon_path()` を実装する
3. 各 OS Backend を `launchd.rs` / `systemd.rs` / `xdg.rs` / `windows.rs` に実装する（`#[cfg(target_os = ...)]` でスコープ）
4. `cli.rs` に `DaemonSubcommand` enum と `Subcommand::Daemon` バリアントを追加する
5. `cargo check` でコンパイルエラー（`match` exhaustiveness）を確認 → `lib.rs` の `match &args.subcommand` に `Daemon(_) => unreachable!` を追加する
6. `lib.rs` に `run_daemon_subcommand` 関数を追加し、`Daemon` early-return dispatch を追加する
7. `presenter/success.rs` に `render_autostart_installed()` / `render_autostart_uninstalled()` を追加する
8. `presenter/error.rs` に MSG-CLI-120 / MSG-CLI-121 関数を追加する
9. `cargo test` で全テスト pass を確認する

### 依存 crate

| crate | 用途 | 既存 / 新規 |
|-------|------|------------|
| `thiserror` | `AutostartError` 派生 | 既存（`shikomi-cli` が使用中）|
| `nix` | `getuid()` (macOS / Linux) | 既存（Unix のみ）|
| `which` | `which::which("systemctl")` probe | 既存確認要 |
| `dirs` | `home_dir()` | **新規追加**（`dirs = "5"` を `shikomi-cli/Cargo.toml` に追加）|

`dirs` crate 追加根拠: [docs.rs/dirs](https://docs.rs/dirs/latest/dirs/) — `HOME` 環境変数オーバーライドや macOS `NSFileManager` 等の OS 固有挙動を正しく処理する。

### CI 確認コマンド（実装完了後）

```
# DaemonSubcommand が cli.rs にのみ定義されていること
grep -rn "DaemonSubcommand" crates/shikomi-cli/src/

# no_ipc 参照が lib.rs で 3 件（vault dispatch + build_handle + daemon status IPC probe 分岐）
grep -n "no_ipc" crates/shikomi-cli/src/lib.rs

# autostart モジュールが no_ipc を直接参照していないこと
grep -rn "no_ipc" crates/shikomi-cli/src/autostart/

# tracing::info! が run_daemon_subcommand に含まれていること
grep -n "tracing::info!" crates/shikomi-cli/src/lib.rs
```

### 既存テストケースの期待値更新（Sub-B 実装時に必須）

**Sub-A の TC-UT-159（`args.no_ipc` 参照件数アサート）の期待値を 2 → 3 に更新すること。** `daemon status` IPC probe 分岐が `lib.rs` に追加されることで `no_ipc` 参照が 2 件（vault dispatch + build_handle）から 3 件（+ daemon status probe）に増加する（TC-UT-176 参照）。この更新を怠ると Sub-B 実装後に TC-UT-159 が FAIL してCI が赤になり、監査ゲートが機能しているように見えて内側の期待値が陳腐化する。

### コンパイル時 `#[cfg]` 注意事項

- CI matrix（Linux / macOS / Windows）全 OS でコンパイルエラーがないことを確認する
- `cargo check --target x86_64-pc-windows-msvc` 等でクロスコンパイル確認推奨
- `autostart/mod.rs` の `detect()` 末尾に `#[cfg(not(any(...)))]` フォールバック（`UnsupportedBackend`）を忘れずに追加する
