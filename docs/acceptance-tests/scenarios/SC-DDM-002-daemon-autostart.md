# 受入テストシナリオ — SC-DDM-002: daemon OS 自動起動（Sub-B）

<!-- 配置先: docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md -->
<!-- 対応要件: REQ-DDM-010〜017（daemon-default-mode/autostart/basic-design.md §モジュール契約）/ Issue #127 -->
<!-- Vモデル対応: 受入テスト（最上位、業務シナリオ横断）-->
<!-- 対応 AC: AC-DDM-07 / AC-DDM-08 / AC-DDM-09 / AC-DDM-10 -->

## シナリオ概要

| 項目 | 内容 |
|------|------|
| シナリオ ID | SC-DDM-002 |
| タイトル | `shikomi daemon install/uninstall/status` が OS 自動起動を正しく管理し、受入基準 AC-DDM-07〜10 を満たす |
| 対象ペルソナ | ペルソナ B（山田 美咲 — エンジニア / CLI 主体）|
| 優先度 | High（Sub-B マージの受入必須）|
| 前提条件 | shikomi Phase 2 バイナリがインストール済み（`shikomi` / `shikomi-daemon` が同一ディレクトリに存在）。OS に対応する自動起動機構が利用可能（macOS: launchd / Linux: systemd または XDG Autostart / Windows: Task Scheduler）|
| 関連 Issue | #125（Phase 2 全体）/ #127（Sub-B: OS 自動起動）|
| 関連シナリオ | SC-DDM-001（IPC 既定化）/ SC-DAEMON-001（daemon 初回起動）|

---

## 受入基準

### AC-DDM-07: `shikomi daemon install` が成功し、OS 固有の自動起動ファイルが配置される

**Given**: `shikomi` / `shikomi-daemon` バイナリが同一ディレクトリに存在する  
**When**: `shikomi daemon install` を実行する  
**Then**:
- コマンドが exit 0 で成功すること
- stdout に `"shikomi-daemon autostart enabled"` が出力されること
- stdout に OS 固有の hint が出力されること:
  - macOS: `"hint: to start immediately: launchctl kickstart gui/{uid}/dev.shikomi.daemon"`
  - Linux (systemd): `"hint: to check status: systemctl --user status shikomi-daemon"`
  - Linux (XDG): `"hint: this uses XDG Autostart; shikomi-daemon will start on next login"`
  - Windows: `"hint: to start immediately: schtasks /Run /TN \"shikomi\\shikomi-daemon\""`
- OS 固有の自動起動ファイルが配置されていること:
  - macOS: `~/Library/LaunchAgents/dev.shikomi.daemon.plist` が存在する
  - Linux (systemd): `~/.config/systemd/user/shikomi-daemon.service` が存在する
  - Linux (XDG): `~/.config/autostart/shikomi-daemon.desktop` が存在する
  - Windows: `schtasks /Query /TN "shikomi\shikomi-daemon"` が exit 0 で成功する
- stderr が空であること（エラーメッセージなし）

---

### AC-DDM-08: `shikomi daemon uninstall` が成功し、自動起動ファイルが削除される

**Given**: `shikomi daemon install` が実行済みで自動起動ファイルが存在する  
**When**: `shikomi daemon uninstall` を実行する  
**Then**:
- コマンドが exit 0 で成功すること
- stdout に `"shikomi-daemon autostart disabled"` が出力されること
- OS 固有の自動起動ファイルが削除されていること:
  - macOS: `~/Library/LaunchAgents/dev.shikomi.daemon.plist` が存在しない
  - Linux (systemd): `~/.config/systemd/user/shikomi-daemon.service` が存在しない
  - Linux (XDG): `~/.config/autostart/shikomi-daemon.desktop` が存在しない
  - Windows: `schtasks /Query /TN "shikomi\shikomi-daemon"` が失敗する（タスクなし）
- stderr が空であること

---

### AC-DDM-09: `shikomi daemon status` が常に exit 0 で稼働状態と登録状態を 2 行で出力する

**Given**: daemon が起動済み / 未起動のいずれかの状態。自動起動登録済み / 未登録のいずれかの状態  
**When**: `shikomi daemon status` を実行する  
**Then**:
- コマンドが常に exit 0 で終了すること（REQ-DDM-012 §設計原則: 情報提供のみ、副作用なし）
- stdout の 1 行目が以下のいずれか:
  - `"daemon: running"` — daemon プロセスが稼働中（IPC 接続成功）
  - `"daemon: not running"` — daemon プロセスが未起動（IPC 接続失敗）
  - `"daemon: unknown (--no-ipc)"` — `--no-ipc` フラグ指定時
- stdout の 2 行目が以下のいずれか:
  - `"autostart: enabled"` — 自動起動ファイルが存在する
  - `"autostart: disabled"` — 自動起動ファイルが存在しない
- stderr が空であること

**検証シナリオ（状態の組み合わせ）**:

| 状態 | 1 行目 | 2 行目 |
|------|--------|--------|
| daemon 起動中 / autostart 登録済み | `daemon: running` | `autostart: enabled` |
| daemon 未起動 / autostart 登録済み | `daemon: not running` | `autostart: enabled` |
| daemon 未起動 / autostart 未登録 | `daemon: not running` | `autostart: disabled` |
| `--no-ipc` 指定 / autostart 登録済み | `daemon: unknown (--no-ipc)` | `autostart: enabled` |

---

### AC-DDM-10: `shikomi daemon install` を 2 回連続実行しても 2 回目も exit 0（冪等性）

**Given**: `shikomi daemon install` を 1 回実行済みで自動起動ファイルが存在する  
**When**: `shikomi daemon install` を再度実行する  
**Then**:
- 2 回目のコマンドが exit 0 で成功すること（エラーにならない）
- stdout に `"shikomi-daemon autostart enabled"` が出力されること
- OS 固有の自動起動ファイルが依然として存在すること（上書き = 内容は同一）
- stderr が空であること

---

## 自動化テストケース

以下のテストケースを `crates/shikomi-cli/tests/e2e_sc_ddm_002.rs` に実装する。

**ブラックボックス方針**: `std::process::Command` / `assert_cmd::Command` で `shikomi` バイナリを
起動し、stdout / stderr / exit code とファイルシステム観測のみで判定する。
内部関数呼び出し・DB 直接確認・テスト用裏口は一切行わない。
`HOME` 環境変数を `tempfile::TempDir` にオーバーライドし、実システムへの副作用を排除する。

---

### SC-DDM-002-TC-001: AC-DDM-07 自動化（install 成功）

```
#[cfg(unix)]  // macOS + Linux。Windows は SC-DDM-002-TC-001W
```

| 項目 | 内容 |
|------|------|
| 対応 AC | AC-DDM-07 |
| 前提 | `HOME` を `tempfile::TempDir` にオーバーライド。`shikomi-daemon` が `target/debug/` に存在する |
| 手順 | (1) `CARGO_BIN_EXE_shikomi` バイナリを取得 / (2) `HOME=tempdir shikomi daemon install` を実行 |
| 期待 | (1) exit code = 0 / (2) stdout に `"shikomi-daemon autostart enabled"` を含む / (3) stdout に `"hint:"` を含む / (4) 自動起動ファイルが `tempdir` 配下に作成されている / (5) stderr が空 |
| セキュリティ観点 | stdout / stderr に `"password"` / `"secret"` / `"token"` が含まれないことを確認 |
| 関数名 | `sc_ddm_002_tc001_ac07_daemon_install_creates_autostart_file()` |

---

### SC-DDM-002-TC-002: AC-DDM-08 自動化（uninstall 成功）

```
#[cfg(unix)]
```

| 項目 | 内容 |
|------|------|
| 対応 AC | AC-DDM-08 |
| 前提 | TC-001 と同じ `HOME=tempdir` 環境で install 済み状態 |
| 手順 | (1) install 実行（前提確立）/ (2) `shikomi daemon uninstall` を実行 / (3) ファイル不在を確認 |
| 期待 | (1) exit code = 0 / (2) stdout に `"shikomi-daemon autostart disabled"` を含む / (3) 自動起動ファイルが `tempdir` 配下に存在しない / (4) stderr が空 |
| 関数名 | `sc_ddm_002_tc002_ac08_daemon_uninstall_removes_autostart_file()` |

---

### SC-DDM-002-TC-003: AC-DDM-09 自動化（status 出力 + 常に exit 0）

```
#[cfg(unix)]
```

| 項目 | 内容 |
|------|------|
| 対応 AC | AC-DDM-09 |
| 手順 | (A) install 後: `shikomi daemon status --no-ipc` を実行 / (B) uninstall 後: `shikomi daemon status --no-ipc` を実行 |
| 期待 | (A) exit code = 0 / stdout 1 行目 = `"daemon: unknown (--no-ipc)"` / stdout 2 行目 = `"autostart: enabled"` / (B) exit code = 0 / stdout 1 行目 = `"daemon: unknown (--no-ipc)"` / stdout 2 行目 = `"autostart: disabled"` |
| 補足 | `--no-ipc` を使用して daemon プロセスの有無によるテスト不安定性を排除 |
| 関数名 | `sc_ddm_002_tc003_ac09_daemon_status_always_exit_0()` |

---

### SC-DDM-002-TC-004: AC-DDM-10 自動化（install 冪等性）

```
#[cfg(unix)]
```

| 項目 | 内容 |
|------|------|
| 対応 AC | AC-DDM-10 |
| 手順 | (1) `shikomi daemon install` を実行 / (2) 再度 `shikomi daemon install` を実行 |
| 期待 | (1)(2) ともに exit code = 0 / stdout に `"shikomi-daemon autostart enabled"` を含む / stderr が空 |
| 関数名 | `sc_ddm_002_tc004_ac10_daemon_install_idempotent()` |

---

## 手動テスト観点（自動化困難 / OS 実環境が必要）

以下の受入観点は自動化テストでカバーが困難なため、手動テストとして実施する。

| 観点 | 手順 | 期待 |
|------|------|------|
| 実 launchctl 登録確認（macOS） | `shikomi daemon install` 後に `launchctl list \| grep shikomi` | `dev.shikomi.daemon` がリストに表示される |
| 次回ログイン時自動起動確認（macOS） | `shikomi daemon install` 後にログアウト → 再ログイン | `shikomi-daemon` が自動起動していること（ソケットが存在する）|
| 実 systemctl 登録確認（Linux / systemd） | `shikomi daemon install` 後に `systemctl --user status shikomi-daemon` | unit が `enabled` 状態 |
| 実 schtasks 登録確認（Windows） | `shikomi daemon install` 後に `schtasks /Query /TN "shikomi\shikomi-daemon"` | タスクが表示される |
| install 権限不足エラー（MSG-CLI-120） | `HOME` を書き込み不可ディレクトリに設定して `shikomi daemon install` | stderr に `"error: failed to enable autostart:"` + 詳細。exit 1 |
| uninstall 権限不足エラー（MSG-CLI-121） | `HOME` を書き込み不可ディレクトリに設定して `shikomi daemon uninstall` | stderr に `"error: failed to disable autostart:"` + 詳細。exit 1 |

---

## テスト実装ファイル配置

| ファイル | 内容 |
|---------|------|
| `crates/shikomi-cli/tests/e2e_sc_ddm_002.rs` | SC-DDM-002-TC-001〜004 の実装 |

## 実行方法

```sh
# E2E 受入テスト（SC-DDM-002）のみ実行
cargo test -p shikomi-cli --test e2e_sc_ddm_002

# justfile レシピ
just test-cli
```

---

*百年後まで御機嫌よう。*
