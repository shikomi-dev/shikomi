# 結合テスト設計書 — daemon-default-mode / autostart

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/test-design/integration.md -->
<!-- Vモデル対応: 階層 3（IT — モジュール契約・モジュール間連携）-->
<!-- 親: ../basic-design.md / ../detailed-design.md -->

## 目的

`autostart` サブシステムのモジュール間連携（`AutostartBackend` 実装 ↔ OS ファイルシステム /
外部コマンド）と、CLI サブコマンド（`shikomi daemon install/uninstall/status`）の
エンドツーエンド挙動を「プロセス起動なし（in-process / assert_cmd 経由）」で検証する。

`tempfile::TempDir` を `HOME` 環境変数でオーバーライドし、実システムの設定ディレクトリ
（`~/Library/LaunchAgents/` / `~/.config/systemd/user/` / `~/.config/autostart/`）への
副作用を排除する。外部コマンド（`launchctl` / `systemctl`）を実際に呼び出す場合は
CI ランナーが対応している OS でのみ実行し、非対応 CI では `#[ignore]` を付与する。

---

## テストマトリクス（トレーサビリティ）

| TC-ID | 対象 REQ / 基本設計箇所 | 検証内容 | 実装先 |
|-------|----------------------|---------|--------|
| TC-IT-120 | REQ-DDM-016 / `XdgAutostartBackend::install()` | .desktop ファイルが tempdir に作成される | `crates/shikomi-cli/tests/it_autostart.rs` |
| TC-IT-121 | REQ-DDM-016 / `XdgAutostartBackend::uninstall()` | .desktop ファイルが削除される（冪等）| 同上 |
| TC-IT-122 | REQ-DDM-016 / `XdgAutostartBackend::is_registered()` | ファイル存在 → `true` / 不在 → `false` | 同上 |
| TC-IT-123 | REQ-DDM-015 / `SystemdBackend::install()` | unit ファイルが tempdir に作成される（Linux）| 同上 |
| TC-IT-124 | REQ-DDM-015 / `SystemdBackend::uninstall()` | unit ファイルが削除される（冪等、Linux）| 同上 |
| TC-IT-125 | REQ-DDM-014 / `LaunchdBackend::install()` | plist ファイルが tempdir に作成される（macOS）| 同上 |
| TC-IT-126 | REQ-DDM-014 / `LaunchdBackend::uninstall()` | plist ファイルが削除される（冪等、macOS）| 同上 |
| TC-IT-127 | REQ-DDM-010 / `run_daemon_subcommand(Install)` | `shikomi daemon install` → exit 0 + stdout メッセージ + hint | `crates/shikomi-cli/tests/it_autostart_cli.rs` |
| TC-IT-128 | REQ-DDM-011 / `run_daemon_subcommand(Uninstall)` | `shikomi daemon uninstall` → exit 0 + stdout メッセージ | 同上 |
| TC-IT-129 | REQ-DDM-012 / `run_daemon_subcommand(Status, no_ipc=true)` | `shikomi daemon status --no-ipc` → `"daemon: unknown (--no-ipc)"` | 同上 |
| TC-IT-130 | REQ-DDM-010 / MSG-CLI-120 | `shikomi daemon install` 失敗 → stderr に MSG-CLI-120 + exit 1 | 同上 |
| TC-IT-131 | REQ-DDM-010 §冪等性 | `shikomi daemon install` 2 回連続 → 2 回目も exit 0 | 同上 |
| TC-IT-132 | REQ-DDM-012 §is_registered | install 後 → `autostart: enabled` / uninstall 後 → `autostart: disabled` | 同上 |

---

## 外部 I/O 依存マップ

| 依存先 | 依存する TC | 隔離方法 | fixture 状態 |
|--------|-----------|---------|-------------|
| `dirs::home_dir()` / `HOME` 環境変数 | TC-IT-120〜132 | テスト内で `std::env::set_var("HOME", tempdir.path())` し、テスト後に復元。`#[serial_test::serial]` で競合防止 | raw fixture 不要（env var 操作） |
| ファイルシステム (`fs::write` / `fs::remove_file` / `Path::exists`) | TC-IT-120〜126 | `tempfile::TempDir` を使用（テスト終了時に自動削除）| raw fixture 不要 |
| `shikomi` CLI バイナリ | TC-IT-127〜132 | `assert_cmd::cargo::cargo_bin("shikomi")` でビルド済みバイナリを使用 | ビルド済み artifact（`cargo build` 済み前提）|
| `launchctl` コマンド | TC-IT-125〜126 | macOS CI ランナーのみで実行。`launchctl bootstrap` はファイル作成後の手順（ファイル書き込みのみ検証し `launchctl` 呼出は `#[ignore]` で CI 分離）| CI macOS ランナーで検証要 |
| `systemctl --user` コマンド | TC-IT-123〜124 | Linux CI で `DBUS_SESSION_BUS_ADDRESS` が設定されていない場合は `systemctl` 呼出が `CommandFailed` になる。ファイル書き込みまでを検証し、コマンド呼出以降は `#[ignore]` で分離 | CI Linux ランナーで検証要 |
| `schtasks` コマンド | Windows TC-IT-125W | `#[cfg(target_os = "windows")]` でスコープ。Windows CI のみ | CI Windows ランナーで検証要 |
| `std::env::current_exe()` + `resolve_daemon_path()` | TC-IT-127〜131 | `assert_cmd` 経由の CLI 呼び出しでは `current_exe()` が `shikomi` バイナリを返す。`shikomi-daemon` が同ディレクトリにあることを前提（`cargo test --test it_autostart_cli` 実行時は `target/debug/` に両バイナリが存在する）| ビルド artifact 依存 |

> **注記**: `launchctl` / `systemctl` の実コマンド呼び出しを伴うテストは
> CI マトリクス（OS 別ランナー）で実行する。ローカル Linux 環境で
> `DBUS_SESSION_BUS_ADDRESS` が未設定の場合は `XdgAutostartBackend` 経由になる
> （`detect()` のフォールバックロジック）。

---

## テストケース詳細

### TC-IT-120: `XdgAutostartBackend::install()` — .desktop ファイル作成

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 前提 | `HOME` を `tempfile::TempDir` にオーバーライド |
| 手順 | (1) `HOME` を tempdir に設定 / (2) `XdgAutostartBackend::new().install()` を呼び出す / (3) `tempdir/.config/autostart/shikomi-daemon.desktop` の存在を確認 |
| 期待 | (1) `Ok(())` が返る / (2) `.desktop` ファイルが作成されている / (3) ファイルに `ExecStart=` ではなく `Exec=` が含まれる（XDG desktop 仕様）/ (4) `{daemon_path}` プレースホルダが残っていない |
| 正常 / 異常 | 正常系 |

### TC-IT-121: `XdgAutostartBackend::uninstall()` — .desktop ファイル削除（冪等）

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 前提 | `HOME` を tempdir にオーバーライド。install() 済み状態と未 install 状態の両方でテスト |
| 手順 | (A) install 済み: `install()` → `uninstall()` → ファイル不在を確認 / (B) 未登録: `uninstall()` のみ → `Ok(())` が返る（冪等）|
| 期待 | (A) ファイルが削除されている / (B) `Ok(())` — `NotFound` は透過 |
| 正常 / 異常 | 正常系（A）/ エッジケース（B: 冪等）|

### TC-IT-122: `XdgAutostartBackend::is_registered()` — ファイル存在確認

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 手順 | (A) install() 後 → `is_registered()` / (B) uninstall() 後 → `is_registered()` |
| 期待 | (A) `true` / (B) `false` |
| 正常 / 異常 | 正常系 |

---

### TC-IT-123: `SystemdBackend::install()` — unit ファイル作成

```
#[cfg(target_os = "linux")]
#[ignore = "requires DBUS_SESSION_BUS_ADDRESS for systemctl; file I/O portion only"]
```

| 項目 | 内容 |
|------|------|
| 前提 | `HOME` を tempdir にオーバーライド |
| 手順 | `SystemdBackend::new().install()` を呼び出す（`systemctl` 呼出は失敗する場合あり）|
| 期待 | `tempdir/.config/systemd/user/shikomi-daemon.service` が作成されていること。ファイル内容に `ExecStart=` + 絶対パスが含まれること |
| 補足 | `systemctl` が `CommandFailed` を返しても、ファイル作成ステップ（手順 1〜4）が先に完了している場合は部分的に成功。IT ではファイル書き込みまでを個別 helper で切り出すか、`systemctl` 呼び出しより前の `write()` ステップだけを検証する方法を検討すること |
| 正常 / 異常 | 正常系（ファイル書き込み部分） |

### TC-IT-124: `SystemdBackend::uninstall()` — unit ファイル削除（冪等）

```
#[cfg(target_os = "linux")]
```

| 項目 | 内容 |
|------|------|
| 手順 | (A) unit ファイルを手動作成後 → `uninstall()` → ファイル不在を確認 / (B) ファイル不在状態で `uninstall()` → `Ok(())` |
| 期待 | (A) ファイルが削除される / (B) `Ok(())` — 冪等 |
| 正常 / 異常 | 正常系（A）/ エッジケース（B）|

---

### TC-IT-125: `LaunchdBackend::install()` — plist ファイル作成

```
#[cfg(target_os = "macos")]
#[ignore = "launchctl bootstrap requires login session; file I/O only"]
```

| 項目 | 内容 |
|------|------|
| 前提 | `HOME` を tempdir にオーバーライド |
| 手順 | `LaunchdBackend::new().install()` を呼び出す |
| 期待 | `tempdir/Library/LaunchAgents/dev.shikomi.daemon.plist` が作成されていること。plist 内容が XML 形式で `<key>Label</key> <string>dev.shikomi.daemon</string>` を含むこと |
| 正常 / 異常 | 正常系（ファイル書き込み部分）|

### TC-IT-126: `LaunchdBackend::uninstall()` — plist ファイル削除（冪等）

```
#[cfg(target_os = "macos")]
```

| 項目 | 内容 |
|------|------|
| 手順 | (A) plist を手動作成後 → `uninstall()` → ファイル不在を確認 / (B) ファイル不在状態で `uninstall()` → `Ok(())` |
| 期待 | (A) ファイルが削除される / (B) `Ok(())` — 冪等 |
| 正常 / 異常 | 正常系（A）/ エッジケース（B）|

---

### TC-IT-127: `shikomi daemon install` — exit 0 + stdout メッセージ + hint

| 項目 | 内容 |
|------|------|
| 対象 | `shikomi daemon install` CLI サブコマンド（`assert_cmd::Command` 経由）|
| 前提 | `HOME` を tempdir にオーバーライドして CLI を実行。`shikomi-daemon` バイナリが `target/debug/` に存在する（`cargo build` 済み）|
| 手順 | (1) `assert_cmd::cargo::cargo_bin("shikomi")` を取得 / (2) `.env("HOME", tempdir.path())` を設定して `["daemon", "install"]` を実行 |
| 期待 | (1) exit code = 0 / (2) stdout が `"shikomi-daemon autostart enabled"` を含む / (3) stdout が OS 固有の hint（`"hint: to start immediately:"` または `"hint: this uses XDG Autostart"` 等）を含む / (4) stderr が空（エラーなし）|
| セキュリティ観点 | stdout / stderr に `"password"` / `"secret"` / `"token"` が含まれないことを確認 |
| 正常 / 異常 | 正常系 |
| 実装先 | `crates/shikomi-cli/tests/it_autostart_cli.rs fn tc_it_127_daemon_install_success()` |

### TC-IT-128: `shikomi daemon uninstall` — exit 0 + stdout メッセージ

| 項目 | 内容 |
|------|------|
| 手順 | (1) install 済み状態から `["daemon", "uninstall"]` を実行 / (2) 未登録状態からも実行（冪等確認）|
| 期待 | (1)(2) ともに: exit code = 0 / stdout が `"shikomi-daemon autostart disabled"` を含む / stderr が空 |
| 正常 / 異常 | 正常系 + エッジケース（冪等）|
| 実装先 | `crates/shikomi-cli/tests/it_autostart_cli.rs fn tc_it_128_daemon_uninstall_success()` |

### TC-IT-129: `shikomi daemon status --no-ipc` — `"daemon: unknown (--no-ipc)"` 出力

| 項目 | 内容 |
|------|------|
| 手順 | `["daemon", "status", "--no-ipc"]` を実行（daemon が起動していない環境）|
| 期待 | (1) exit code = 0（REQ-DDM-012: status は常に exit 0）/ (2) stdout の 1 行目が `"daemon: unknown (--no-ipc)"` / (3) stdout の 2 行目が `"autostart: enabled"` または `"autostart: disabled"` のいずれか |
| 正常 / 異常 | 正常系（--no-ipc フラグ使用）|
| 実装先 | `crates/shikomi-cli/tests/it_autostart_cli.rs fn tc_it_129_daemon_status_no_ipc()` |

### TC-IT-130: `shikomi daemon install` 失敗 — MSG-CLI-120 + exit 1

| 項目 | 内容 |
|------|------|
| 前提 | `HOME` を書き込み権限なしのディレクトリ（`chmod 000 tmpdir`）に設定し、ファイル書き込みを故意に失敗させる |
| 手順 | `["daemon", "install"]` を実行。ファイル書き込み失敗 → `AutostartError::IoError` が発生 |
| 期待 | (1) exit code = 1 / (2) stderr に `"error: failed to enable autostart:"` を含む（MSG-CLI-120）/ (3) stdout が空 |
| セキュリティ観点 | stderr に絶対パス以外の情報（credential / token 等）が含まれないことを確認 |
| 正常 / 異常 | 異常系（権限不足）|
| 実装先 | `crates/shikomi-cli/tests/it_autostart_cli.rs fn tc_it_130_daemon_install_failure_msg_cli_120()` |

### TC-IT-131: `shikomi daemon install` 冪等性 — 2 回連続で exit 0

| 項目 | 内容 |
|------|------|
| 手順 | `["daemon", "install"]` を 2 回連続実行 |
| 期待 | 1 回目 / 2 回目ともに exit code = 0。2 回目も stdout に `"shikomi-daemon autostart enabled"` を含む |
| 正常 / 異常 | エッジケース（冪等性）|
| 実装先 | `crates/shikomi-cli/tests/it_autostart_cli.rs fn tc_it_131_daemon_install_idempotent()` |

### TC-IT-132: install / uninstall 後の `shikomi daemon status` 出力確認

| 項目 | 内容 |
|------|------|
| 手順 | (A) install 後に `["daemon", "status", "--no-ipc"]` / (B) uninstall 後に `["daemon", "status", "--no-ipc"]` |
| 期待 | (A) stdout 2 行目が `"autostart: enabled"` / (B) stdout 2 行目が `"autostart: disabled"` |
| 正常 / 異常 | 正常系 |
| 実装先 | `crates/shikomi-cli/tests/it_autostart_cli.rs fn tc_it_132_status_reflects_install_state()` |

---

## ファイル配置と `Cargo.toml` 変更

### 新規テストファイル

| ファイル | 内容 |
|---------|------|
| `crates/shikomi-cli/tests/it_autostart.rs` | TC-IT-120〜126（Backend 直接呼び出し）|
| `crates/shikomi-cli/tests/it_autostart_cli.rs` | TC-IT-127〜132（`assert_cmd` 経由 CLI 呼び出し）|

### `shikomi-cli/Cargo.toml` に追加する dev-dependencies

```toml
[dev-dependencies]
assert_cmd = "2"          # 既存（確認要）
tempfile = "3"            # 既存（確認要）
serial_test = "3"         # DBUS_SESSION_BUS_ADDRESS 環境変数操作の競合防止
predicates = "3"          # stdout/stderr アサーション
```

---

## モック方針

| 外部依存 | IT での扱い |
|---------|-----------|
| `HOME` 環境変数 | `std::env::set_var("HOME", tempdir.path())` + `#[serial]` で実ディレクトリへの副作用を排除 |
| `launchctl` / `systemctl` | 実コマンド呼び出し。CI が対応 OS のランナーを持つ場合のみ。非対応 CI は `#[ignore]` |
| `schtasks` | Windows CI のみ。`#[cfg(target_os = "windows")]` でスコープ |
| `shikomi-daemon` バイナリ | `target/debug/` の artifact。`CARGO_BIN_EXE_shikomi-daemon` 環境変数または同ディレクトリ検索 |

---

## 実行方法

```sh
# autostart IT のみ実行
cargo test -p shikomi-cli --test it_autostart
cargo test -p shikomi-cli --test it_autostart_cli

# justfile レシピ（shikomi-cli 全テスト）
just test-cli

# ignore 付きも含めて実行（CI macOS ランナー等）
cargo test -p shikomi-cli --test it_autostart -- --include-ignored
```

---

*百年後まで御機嫌よう。*
