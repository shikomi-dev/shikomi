# テスト設計書 — daemon-default-mode / autostart / 結合テスト

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/test-design/integration.md -->
<!-- Vモデル対応: 階層 3（basic-design.md §モジュール契約 → 結合テスト）-->
<!-- 兄弟: unit.md / 親: ../basic-design.md / ../detailed-design.md -->

## 1. 設計方針

- **対象**: `AutostartBackend` 実装のファイル I/O・外部コマンド呼び出しと CLI サブコマンドのエンドツーエンド挙動
  - `LaunchdBackend::install()` + `uninstall()`: `tempfile::TempDir` を `HOME` 代替として使用（macOS CI のみ）
  - `SystemdBackend::install()` + `uninstall()`: `~/.config/systemd/user/` 代替ディレクトリでのファイル I/O（Linux CI のみ）
  - `XdgAutostartBackend::install()` + `uninstall()`: `~/.config/autostart/` 代替でのファイル I/O（Linux CI）
  - `WindowsTaskSchedulerBackend`: Windows CI のみ（`#[cfg(target_os = "windows")]`）
  - `shikomi daemon install` のファイル存在確認（OS 横断）
  - `shikomi daemon uninstall` のファイル削除確認（OS 横断）
  - `shikomi daemon status --no-ipc` → `"daemon: unknown (--no-ipc)"` + exit 0
  - `shikomi daemon install` 冪等性（2 回実行 → 2 回目も exit 0）
- **視点**: 半ブラックボックス。`assert_cmd::Command::cargo_bin("shikomi")` でサブプロセス起動し、stdout / stderr / exit code / ファイル存在で検証
- **疑似コード禁止**: Rust コードブロックは記述しない。処理手順は番号付き箇条書きで表現する

---

## 2. 外部 I/O 依存マップ

| 外部 I/O | 利用箇所 | 隔離方法 |
|---------|---------|---------|
| `dirs::home_dir()` / `HOME` 環境変数 | Backend の設定ファイルパス解決 | `std::env::set_var("HOME", tempdir.path())` + `#[serial_test::serial]` でスレッド安全に制御 |
| ファイルシステム（`fs::write` / `fs::remove_file` / `Path::exists`）| TC-IT-120〜126 | `tempfile::TempDir` を使用（テスト終了時に自動削除） |
| `shikomi` CLI バイナリ | TC-IT-127〜132 | `assert_cmd::cargo::cargo_bin("shikomi")` でビルド済み artifact を使用 |
| `launchctl` コマンド | TC-IT-125〜126 | macOS CI ランナーのみ実行。`launchctl bootstrap` はファイル作成後の工程（ファイル書き込みのみ検証し `launchctl` 呼び出しは `#[ignore]` で CI 分離） |
| `systemctl --user` コマンド | TC-IT-123〜124 | Linux CI で `DBUS_SESSION_BUS_ADDRESS` が未設定の場合は `CommandFailed` になる。ファイル書き込みまでを検証し、コマンド呼び出し以降は `#[ignore]` で分離 |
| `schtasks` コマンド | TC-IT-125W〜126W | `#[cfg(target_os = "windows")]` でスコープ。Windows CI のみ |
| `std::env::current_exe()` + `resolve_daemon_path()` | TC-IT-127〜131 | `cargo test --test it_autostart_cli` 実行時は `target/debug/` に `shikomi` と `shikomi-daemon` が両方存在する前提 |

---

## 3. モック方針（IT）

| 依存先 | モック方法 |
|--------|-----------|
| `HOME` 環境変数 | `std::env::set_var("HOME", tempdir.path())` + `#[serial_test::serial]` で実ディレクトリへの副作用を排除 |
| `launchctl` / `systemctl` | 実コマンド呼び出し。CI が対応 OS のランナーを持つ場合のみ。非対応 CI は `#[ignore]` |
| `schtasks` | Windows CI のみ。`#[cfg(target_os = "windows")]` でスコープ |
| `shikomi-daemon` バイナリ | `target/debug/` の artifact。`resolve_daemon_path()` が同ディレクトリの `shikomi-daemon` を解決 |
| SQLite / IPC | IT-autostart テストでは不使用（daemon サブコマンドは `RepositoryHandle` 不要） |

---

## 4. トレーサビリティマトリクス

| TC-ID | 対応要件 | 対応受入基準 | 種別 | 検証観点 |
|-------|---------|------------|------|---------|
| TC-IT-120 | REQ-DDM-016 | AC-DDM-07 / AC-DDM-08 | 正常 | `XdgAutostartBackend::install()` → .desktop ファイルが tempdir に作成される（Linux） |
| TC-IT-121 | REQ-DDM-016 | AC-DDM-08 | 正常 | `XdgAutostartBackend::uninstall()` → .desktop ファイルが削除される（冪等、Linux） |
| TC-IT-122 | REQ-DDM-015 | AC-DDM-07 / AC-DDM-08 | 正常 | `SystemdBackend::install()` → unit ファイルが tempdir に作成される（Linux） |
| TC-IT-123 | REQ-DDM-015 | AC-DDM-08 | 正常 | `SystemdBackend::uninstall()` → unit ファイルが削除される（冪等、Linux） |
| TC-IT-124 | REQ-DDM-014 | AC-DDM-07 / AC-DDM-08 | 正常 | `LaunchdBackend::install()` → plist ファイルが tempdir に作成される（macOS） |
| TC-IT-125 | REQ-DDM-014 | AC-DDM-08 | 正常 | `LaunchdBackend::uninstall()` → plist ファイルが削除される（冪等、macOS） |
| TC-IT-126W | REQ-DDM-017 | AC-DDM-07 / AC-DDM-08 | 正常 | `WindowsTaskSchedulerBackend::install()` + `uninstall()` (Windows CI のみ) |
| TC-IT-127 | REQ-DDM-010 | AC-DDM-07 | 正常 | `shikomi daemon install` → exit 0 + stdout に `"autostart enabled"` + OS 固有 hint + ファイル存在確認 |
| TC-IT-128 | REQ-DDM-011 | AC-DDM-08 | 正常 | `shikomi daemon uninstall` → exit 0 + stdout に `"autostart disabled"` + ファイル削除確認 |
| TC-IT-129 | REQ-DDM-012 | AC-DDM-09 | 正常 | `shikomi daemon status --no-ipc` → stdout に `"daemon: unknown (--no-ipc)"` + exit 0 |
| TC-IT-130 | REQ-DDM-010 | AC-DDM-10 | エッジ | `shikomi daemon install` 冪等性（2 回実行 → 2 回目も exit 0） |
| TC-IT-131 | REQ-DDM-012 | AC-DDM-09 | 正常 | install 後 `status --no-ipc` → `"autostart: enabled"` / uninstall 後 → `"autostart: disabled"` |
| TC-IT-132 | REQ-DDM-010 | AC-DDM-07 | 異常 | `shikomi daemon install` 失敗（書き込み権限なし）→ stderr に MSG-CLI-120 + exit 1 |

上位トレーサビリティ: `TC-IT-120〜132` → `ST-DDM-020〜025`（system-test-design.md）→ `SC-DDM-002`（acceptance-tests/scenarios/）→ `AC-DDM-07〜10`（feature-spec.md §5）

---

## 5. テストケース一覧

### 5.1 XDG Autostart Backend（Linux）

配置: `crates/shikomi-cli/tests/it_autostart.rs`
CI 条件: `#[cfg(target_os = "linux")]`

#### TC-IT-120: `XdgAutostartBackend::install()` — .desktop ファイルが `~/.config/autostart/` 代替に作成されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tempfile::TempDir` を作成し、`HOME` をそのパスにオーバーライドする（`#[serial_test::serial]`） |
| 操作 | 1. `HOME` を tempdir パスに設定する / 2. `XdgAutostartBackend::new().install()` を呼び出す / 3. `{tempdir}/.config/autostart/shikomi-daemon.desktop` の存在を確認する |
| 期待 | `Ok(())` が返ること。`.desktop` ファイルが作成されること。ファイル内容に `Exec=` と daemon のパスが含まれること。`{daemon_path}` プレースホルダが残っていないこと |
| 検証方法 | `assert!(result.is_ok())` / `assert!(desktop_path.exists())` / ファイル内容の文字列検証 |

#### TC-IT-121: `XdgAutostartBackend::uninstall()` — .desktop ファイルの削除（冪等）

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 + エッジケース（冪等） |
| 前提 | TC-IT-120 と同じ環境設定 |
| 操作 | (A) install() 実行後に uninstall() → ファイル不在を確認する。(B) ファイル不在状態から uninstall() のみを実行する |
| 期待 | (A) ファイルが削除されていること。(B) `Ok(())` が返ること（`NotFound` を透過して冪等が成立する） |
| 検証方法 | (A) `assert!(!desktop_path.exists())` / (B) `assert!(result.is_ok())` |

---

### 5.2 Systemd User Unit Backend（Linux）

配置: `crates/shikomi-cli/tests/it_autostart.rs`
CI 条件: `#[cfg(target_os = "linux")]`

#### TC-IT-122: `SystemdBackend::install()` — unit ファイルが `~/.config/systemd/user/` 代替に作成されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tempfile::TempDir` を `HOME` にオーバーライドする。`systemctl` が存在し D-Bus セッションが有効な Linux CI ランナーが前提（非対応 CI は `#[ignore]` を付与する） |
| 操作 | 1. `HOME` を tempdir パスに設定する / 2. `SystemdBackend::new().install()` を呼び出す / 3. `{tempdir}/.config/systemd/user/shikomi-daemon.service` の存在を確認する |
| 期待 | `Ok(())` が返ること（または `systemctl` 呼び出し以前のファイル作成が完了していること）。unit ファイルが作成されること。ファイル内容に `ExecStart=` + 絶対パスが含まれること |
| 検証方法 | `assert!(unit_path.exists())` / ファイル内容の文字列検証 |
| 注意 | `systemctl --user daemon-reload` / `enable --now` がエラーになる CI 環境では、ファイル書き込みステップのみをヘルパー関数で切り出して検証する方法を検討すること |

#### TC-IT-123: `SystemdBackend::uninstall()` — unit ファイルの削除（冪等）

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 + エッジケース（冪等） |
| 前提 | TC-IT-122 と同じ環境設定 |
| 操作 | (A) unit ファイルを手動作成後に uninstall() を実行する → ファイル不在を確認する。(B) ファイル不在状態から uninstall() のみを実行する |
| 期待 | (A) unit ファイルが削除されていること。(B) `Ok(())` が返ること（冪等） |
| 検証方法 | (A) `assert!(!unit_path.exists())` / (B) `assert!(result.is_ok())` |

---

### 5.3 Launchd Backend（macOS）

配置: `crates/shikomi-cli/tests/it_autostart.rs`
CI 条件: `#[cfg(target_os = "macos")]`

#### TC-IT-124: `LaunchdBackend::install()` — plist ファイルが `~/Library/LaunchAgents/` 代替に作成されること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tempfile::TempDir` を `HOME` にオーバーライドする。`launchctl bootstrap` は CI ログインセッション依存のため `#[ignore]` を付与する（ファイル書き込みのみ検証） |
| 操作 | 1. `HOME` を tempdir パスに設定する / 2. `LaunchdBackend::new().install()` のファイル書き込み部分（`plist_path` への `write`）を検証する / 3. `{tempdir}/Library/LaunchAgents/dev.shikomi.daemon.plist` の存在を確認する |
| 期待 | plist ファイルが作成されること。plist 内容が XML 形式で `<key>Label</key>` + `<string>dev.shikomi.daemon</string>` を含むこと。`{daemon_path}` / `{log_dir}` プレースホルダが残っていないこと |
| 検証方法 | `assert!(plist_path.exists())` / ファイル内容の文字列検証 |

#### TC-IT-125: `LaunchdBackend::uninstall()` — plist ファイルの削除（冪等）

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 + エッジケース（冪等） |
| 前提 | TC-IT-124 と同じ環境設定 |
| 操作 | (A) plist を手動作成後に uninstall() を実行する → ファイル不在を確認する。(B) ファイル不在状態から uninstall() のみを実行する |
| 期待 | (A) plist ファイルが削除されていること。(B) `Ok(())` が返ること（`NotFound` を透過して冪等が成立する） |
| 検証方法 | (A) `assert!(!plist_path.exists())` / (B) `assert!(result.is_ok())` |

---

### 5.4 Windows Task Scheduler Backend

配置: `crates/shikomi-cli/tests/it_autostart.rs`
CI 条件: `#[cfg(target_os = "windows")]`

#### TC-IT-126W: `WindowsTaskSchedulerBackend::install()` + `uninstall()` — タスクの登録・削除（Windows CI のみ）

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| CI 条件 | `#[cfg(target_os = "windows")]` — Windows CI ランナーのみ実行 |
| 操作 | 1. `WindowsTaskSchedulerBackend::new().install()` を実行する / 2. `schtasks /Query /TN "shikomi\shikomi-daemon"` の exit 0 でタスク登録を確認する / 3. `uninstall()` を実行する / 4. `schtasks /Query` の非 0 exit でタスク削除を確認する |
| 期待 | install() は `Ok(())` を返すこと。uninstall() は `Ok(())` を返すこと。タスク登録・削除がそれぞれ確認できること |
| 検証方法 | `schtasks /Query /TN "shikomi\shikomi-daemon"` の exit code で確認 |

---

### 5.5 CLI サブコマンド統合（OS 横断）

配置: `crates/shikomi-cli/tests/it_autostart_cli.rs`

#### TC-IT-127: `shikomi daemon install` — exit 0 + stdout メッセージ + OS 固有 hint + ファイル存在確認

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tempfile::TempDir` を `HOME` にオーバーライドする。`shikomi-daemon` バイナリが `target/debug/` に存在すること（`cargo build` 済み） |
| 操作 | 1. `assert_cmd::cargo::cargo_bin("shikomi")` を取得する / 2. `.env("HOME", tempdir.path())` を設定して `["daemon", "install"]` を実行する |
| 期待 | exit code = 0。stdout に `"shikomi-daemon autostart enabled"` が含まれること。stdout に OS 固有 hint（macOS: `"launchctl kickstart"` / Linux systemd: `"systemctl --user status"` / Linux XDG: `"XDG Autostart"` / Windows: `"schtasks /Run"`）が含まれること。OS 固有の設定ファイルが tempdir 内に作成されていること |
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("autostart enabled"))` + `Path::exists()` でファイル確認 |

#### TC-IT-128: `shikomi daemon uninstall` — exit 0 + stdout メッセージ + ファイル削除確認

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | TC-IT-127 後の状態（install 済み）。または未登録状態（冪等確認） |
| 操作 | 1. `["daemon", "uninstall"]` を実行する / 2. OS 固有の設定ファイルが削除されていることを確認する |
| 期待 | exit code = 0。stdout に `"shikomi-daemon autostart disabled"` が含まれること。OS 固有の設定ファイルが削除されていること（`!Path::exists()`） |
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("autostart disabled"))` + `assert!(!file_path.exists())` |

#### TC-IT-129: `shikomi daemon status --no-ipc` → `"daemon: unknown (--no-ipc)"` + exit 0

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | daemon の起動状態は問わない |
| 操作 | `["--no-ipc", "daemon", "status"]` を実行する |
| 期待 | exit code = 0（REQ-DDM-012: status は常に exit 0）。stdout の 1 行目が `"daemon: unknown (--no-ipc)"` であること。stdout の 2 行目が `"autostart: enabled"` または `"autostart: disabled"` のいずれかであること。IPC 接続試行が発生しないこと（タイムアウト待ちなし） |
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("daemon: unknown (--no-ipc)"))` |

#### TC-IT-130: `shikomi daemon install` 冪等性 — 2 回連続実行で 2 回目も exit 0

| 項目 | 内容 |
|------|------|
| 種別 | エッジケース（冪等性） |
| 前提 | TC-IT-127 の前提と同じ |
| 操作 | 1. `["daemon", "install"]` を 1 回目実行する / 2. 同じコマンドを 2 回目実行する |
| 期待 | 1 回目: exit code = 0。2 回目: exit code = 0。2 回目も stdout に `"shikomi-daemon autostart enabled"` が含まれること（重複登録エラーにならない） |
| 検証方法 | 2 回目の `assert_cmd::assert().success().stdout(predicates::str::contains("autostart enabled"))` |
| 設計根拠 | REQ-DDM-010 §冪等性。`basic-design.md §REQ-DDM-010` の「登録済みの場合は冪等（再登録せず成功扱い）」|

#### TC-IT-131: install / uninstall 後の `status --no-ipc` が autostart 状態を正しく反映すること

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 操作 | (A) install 実行後に `["--no-ipc", "daemon", "status"]` を実行する。(B) uninstall 実行後に同コマンドを実行する |
| 期待 | (A) stdout の 2 行目に `"autostart: enabled"` が含まれること。(B) stdout の 2 行目に `"autostart: disabled"` が含まれること |
| 検証方法 | `predicates::str::contains("autostart: enabled")` / `predicates::str::contains("autostart: disabled")` |

#### TC-IT-132: `shikomi daemon install` 失敗時に MSG-CLI-120 + exit 1 を出力すること

| 項目 | 内容 |
|------|------|
| 種別 | 異常系（権限不足） |
| 前提 | `HOME` を書き込み権限なしのディレクトリ（`chmod 000` 適用）に設定し、設定ファイルの書き込みを故意に失敗させる |
| 操作 | `["daemon", "install"]` を実行する。ファイル書き込み失敗 → `AutostartError::IoError` が発生する |
| 期待 | exit code = 1。stderr に `"error: failed to enable autostart:"` が含まれること（MSG-CLI-120）。stdout が空であること |
| 検証方法 | `assert_cmd::assert().code(1).stderr(predicates::str::contains("failed to enable autostart"))` |
| セキュリティ観点 | stderr に credential / token / secret が含まれないことを確認すること |

---

## 6. テスト配置と dev-dependencies

### 新規テストファイル

| ファイル | 対象 TC-ID |
|---------|-----------|
| `crates/shikomi-cli/tests/it_autostart.rs` | TC-IT-120〜126W（Backend 直接呼び出し） |
| `crates/shikomi-cli/tests/it_autostart_cli.rs` | TC-IT-127〜132（`assert_cmd` 経由 CLI 呼び出し） |

### `shikomi-cli/Cargo.toml` に追加する dev-dependencies

| crate | 用途 | 既存 / 新規 |
|-------|------|------------|
| `assert_cmd` | CLI バイナリの subprocess 起動・アサーション | 既存（確認要） |
| `tempfile` | `TempDir` による一時ディレクトリ生成 | 既存（確認要） |
| `serial_test` | `HOME` / `DBUS_SESSION_BUS_ADDRESS` 環境変数操作の競合防止 | 新規追加 |
| `predicates` | stdout / stderr アサーション | 既存（確認要） |

---

## 7. CI 監査ゲート

| チェック | コマンド / 手段 | 失敗条件 |
|---------|--------------|---------|
| IT 全件通過 | `just test-cli` または `cargo test -p shikomi-cli --test it_autostart --test it_autostart_cli` | 1 件でも FAILED |
| `no_ipc` 参照が `lib.rs` で 3 件（vault dispatch + `build_handle` + daemon status IPC probe 分岐） | `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` | 3 件以外 |
| `DaemonSubcommand` が `cli.rs` にのみ定義されていること | `grep -rn "DaemonSubcommand" crates/shikomi-cli/src/` | `cli.rs` 以外にヒット |
| `autostart::detect` が `lib.rs` から呼ばれていること | `grep -n "autostart::" crates/shikomi-cli/src/lib.rs` | 0 件 |
| IT timeout（`shikomi daemon status --no-ipc` でタイムアウト発生なし）| TC-IT-129 | 数秒以上の待機が発生 |

該当なし — CI 除外ゲート項目はない。全項目が IT または grep 静的検査の対象である。
