# システムテスト戦略 — daemon-default-mode

<!-- feature: daemon-default-mode / Issue #125（親）/ Sub-A Issue #126（CLI 既定化）/ Sub-B Issue #127（OS 自動起動）-->
<!-- 配置先: docs/features/daemon-default-mode/system-test-design.md -->
<!-- Vモデル対応: システムテスト（feature 横断の業務シナリオ、E2E より細粒度 / IT より粗粒度）-->

## 1. 概要

本書は `daemon-default-mode` feature のシステムテスト（ST）戦略を定義する。sub-feature `cli/test-design/` が扱う IT / UT とは異なり、**`shikomi-cli` と `shikomi-daemon` を実際にプロセス起動して Phase 2 既定化の通し動作を検証**する。

| 項目 | 内容 |
|------|------|
| 対象 | `shikomi-cli`（IPC 既定 + `--no-ipc`）+ `shikomi-daemon`（起動 / 停止サイクル）の連携 |
| テストレベル | システムテスト（subprocess 起動によるブラックボックス。E2E の一歩手前。IT / UT より粗粒度）|
| 実行環境 | CI 3 OS matrix（Linux / macOS / Windows）|
| テストフレームワーク | `assert_cmd` + `predicates`（`shikomi-cli` / `shikomi-daemon` 共に）|
| 配置 | `crates/shikomi-cli/tests/st_default_mode*.rs`（Sub-A）/ Sub-B は別 PR で追加 |

## 2. 対応受入基準

本 feature の受入基準（`feature-spec.md §5`）と ST-ID の対応を示す。受入基準の詳細は各シナリオドキュメントを単一真実源とする。

### Sub-A（Issue #126）: CLI IPC 既定化

受入基準詳細: `docs/acceptance-tests/scenarios/SC-DDM-001-ipc-default-mode.md`

| 受入基準 | 対応 ST-ID | 対応シナリオ |
|---------|-----------|------------|
| AC-DDM-01（daemon 起動中に `shikomi list` → IPC 経由成功）| ST-DDM-010 | `SC-DDM-001` |
| AC-DDM-02（`shikomi --no-ipc list` → SQLite 直結成功）| ST-DDM-011 | `SC-DDM-001` |
| AC-DDM-03（daemon 未起動 + `shikomi list` → MSG-CLI-110 + exit 1）| ST-DDM-012 | `SC-DDM-001` |
| AC-DDM-04（`shikomi --ipc list` → clap error exit 2）| ST-DDM-013 | `SC-DDM-001` |
| AC-DDM-05（daemon 起動中 `shikomi list` の stderr に MSG-CLI-051 なし）| ST-DDM-014 | `SC-DDM-001` |
| AC-DDM-06（`shikomi --no-ipc vault encrypt` → vault IPC 強制確認）| ST-DDM-015 | `SC-DDM-001` |

### Sub-B（Issue #127）: daemon OS 自動起動

受入基準詳細: `docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md`

| 受入基準 | 対応 ST-ID | 対応シナリオ |
|---------|-----------|------------|
| AC-DDM-07（`shikomi daemon install` → exit 0 + autostart ファイル配置確認）| ST-DDM-020 | `SC-DDM-002` |
| AC-DDM-08（`shikomi daemon uninstall` → exit 0 + autostart ファイル削除確認）| ST-DDM-021 | `SC-DDM-002` |
| AC-DDM-09（`shikomi daemon status` daemon 起動中 → `"daemon: running"` + exit 0）| ST-DDM-022 | `SC-DDM-002` |
| AC-DDM-09（`shikomi daemon status` daemon 未起動 → `"daemon: not running"` + exit 0）| ST-DDM-023 | `SC-DDM-002` |
| AC-DDM-10（`shikomi daemon install` 2 回実行 → 2 回目も exit 0）| ST-DDM-024 | `SC-DDM-002` |
| 追加（`shikomi daemon status --no-ipc` → `"daemon: unknown (--no-ipc)"` + exit 0）| ST-DDM-025 | `SC-DDM-002` |

## 3. システムテスト定義（ST-DDM-010〜015 / 020〜025）

### ST-DDM-010: daemon 起動中に `shikomi list` が IPC 経由で成功すること

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-01 |
| 前提 | `tempfile::TempDir` に空 vault.db を配置（`SHIKOMI_VAULT_DIR` 設定）。`shikomi-daemon` を `assert_cmd::Command::cargo_bin("shikomi-daemon")` で起動し、`200ms` 待機（IPC socket 確立待ち）|
| 操作 | `shikomi list`（`--ipc` フラグなし）を実行 |
| 期待結果 | (1) exit 0 / (2) stdout に空リスト出力 / (3) stderr に `MSG-CLI-051` 文字列が**含まれない**|
| 検証方法 | `assert_cmd::assert().success().stdout(...).stderr(predicates::str::contains("--ipc routes operations").not())` |
| 後処理 | daemon に SIGTERM を送信して終了 |

### ST-DDM-011: `shikomi --no-ipc list` が daemon 未起動でも SQLite 直結で成功すること

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-02 |
| 前提 | `tempfile::TempDir` に空 vault.db を配置（`SHIKOMI_VAULT_DIR` 設定）。daemon は**起動しない** |
| 操作 | `shikomi --no-ipc list` を実行 |
| 期待結果 | (1) exit 0 / (2) stdout に空リスト出力 / (3) stderr に `MSG-CLI-110` が**含まれない** |
| 検証方法 | `assert_cmd::assert().success()` |
| 注意 | daemon 未起動であることが前提。`SHIKOMI_VAULT_DIR` を `tempfile::TempDir` に限定することで OS デフォルト vault への意図せぬアクセスを防ぐ |

### ST-DDM-012: daemon 未起動で `shikomi list` が `MSG-CLI-110` を出力して exit 1 になること

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-03 |
| 前提 | daemon は**起動しない**。`SHIKOMI_VAULT_DIR` を `tempfile::TempDir` に設定 |
| 操作 | `shikomi list`（引数なし）を実行 |
| 期待結果 | (1) exit 1 / (2) stderr に `"shikomi-daemon is not running"` が含まれる / (3) stderr の hint 行に `"--ipc"` 文字列が**含まれない** |
| 検証方法 | `assert_cmd::assert().code(1).stderr(predicates::str::contains("not running")).stderr(predicates::str::contains("--ipc").not())` |

### ST-DDM-013: `shikomi --ipc list` が clap error で exit 2 になること（`--ipc` 廃止確認）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-04 |
| 前提 | daemon の起動状態は問わない（clap パース段階で失敗する）|
| 操作 | `shikomi --ipc list` を実行 |
| 期待結果 | (1) exit 2（clap の usage error）/ (2) stderr に `"unexpected argument '--ipc'"` が含まれる |
| 検証方法 | `assert_cmd::assert().code(2).stderr(predicates::str::contains("--ipc"))` |

### ST-DDM-014: daemon 起動中の `shikomi list` stderr に `MSG-CLI-051` が出力されないこと

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-05 |
| 前提 | ST-DDM-010 と同条件。daemon 起動済み |
| 操作 | `shikomi list` を実行（`--ipc` フラグなし、`--quiet` なし）|
| 期待結果 | (1) exit 0 / (2) stderr に `"routes operations through shikomi-daemon"` が**含まれない**（MSG-CLI-051 廃止の確認）|
| 検証方法 | `assert_cmd::assert().success().stderr(predicates::str::contains("routes operations").not())` |
| 注意 | ST-DDM-010 と同テスト実行でまとめて検証可能。別 TC として独立させることで AC-DDM-05 のトレーサビリティを明示 |

### ST-DDM-015: `shikomi --no-ipc vault encrypt` が vault IPC 強制を維持すること（REQ-DDM-005）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-06 |
| 前提 | daemon は**起動しない**（daemon 接続失敗で MSG-CLI-110 が出ることで IPC 強制を確認）。`SHIKOMI_VAULT_DIR` を `tempfile::TempDir` に設定 |
| 操作 | `shikomi --no-ipc vault encrypt` を実行 |
| 期待結果 | (1) exit 1 / (2) stderr に `"note: vault commands always use IPC; --no-ipc does not apply"` (MSG-CLI-052) が含まれる / (3) stderr に `"shikomi-daemon is not running"` (MSG-CLI-110) が含まれる |
| 検証方法 | `assert_cmd::assert().code(1).stderr(predicates::str::contains("vault commands always use IPC")).stderr(predicates::str::contains("not running"))` |
| 根拠 | `--no-ipc` が無視されて IPC 接続を試みた結果 `MSG-CLI-110` が出ることで、vault 経路の IPC 強制（REQ-DDM-005）が機能していることを証明する |

## Sub-B システムテスト定義（ST-DDM-020〜025）

### ST-DDM-020: `shikomi daemon install` が exit 0 で成功し autostart ファイルが配置されること（AC-DDM-07）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-07 |
| 前提 | `tempfile::TempDir` を `HOME` 代替に設定（`dirs::home_dir()` が返すディレクトリを上書き）。`shikomi-daemon` バイナリが `current_exe()` と同ディレクトリに存在すること |
| 操作 | `shikomi daemon install` を実行 |
| 期待結果 | (1) exit 0 / (2) stdout に `"shikomi-daemon autostart enabled"` が含まれる / (3) stdout に OS 固有 hint が含まれる（macOS: `"launchctl kickstart"` / Linux systemd: `"systemctl --user status"` / Linux XDG: `"XDG Autostart"` / Windows: `"schtasks /Run"`）/ (4) OS 固有ファイルが配置されていること（macOS: plist ファイル存在 / Linux: unit または .desktop ファイル存在 / Windows: schtasks query 成功）|
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("autostart enabled"))` + ファイル存在確認 `Path::exists()` |
| CI 条件 | 3 OS matrix（Linux / macOS / Windows）でそれぞれ対応 Backend が選択されること |

### ST-DDM-021: `shikomi daemon uninstall` が exit 0 で成功し autostart ファイルが削除されること（AC-DDM-08）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-08 |
| 前提 | ST-DDM-020 後の状態（install 済み）|
| 操作 | `shikomi daemon uninstall` を実行 |
| 期待結果 | (1) exit 0 / (2) stdout に `"shikomi-daemon autostart disabled"` が含まれる / (3) OS 固有ファイルが削除されていること |
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("autostart disabled"))` + `!Path::exists()` |

### ST-DDM-022: `shikomi daemon status` が daemon 起動中に正しい 2 行を出力すること（AC-DDM-09）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-09 |
| 前提 | `shikomi-daemon` を起動済み（`200ms` 待機）+ `shikomi daemon install` 実行済み |
| 操作 | `shikomi daemon status` を実行 |
| 期待結果 | (1) exit 0 / (2) stdout の 1 行目に `"daemon: running"` / (3) stdout の 2 行目に `"autostart: enabled"` |
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("daemon: running")).stdout(predicates::str::contains("autostart: enabled"))` |

### ST-DDM-023: `shikomi daemon status` が daemon 未起動でも exit 0 を返すこと（AC-DDM-09）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-09 |
| 前提 | daemon は**起動しない**。autostart は未登録 |
| 操作 | `shikomi daemon status` を実行 |
| 期待結果 | (1) **exit 0**（status は常に成功）/ (2) stdout に `"daemon: not running"` / (3) stdout に `"autostart: disabled"` |
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("not running")).stdout(predicates::str::contains("autostart: disabled"))` |
| 注意 | exit 1 で終了してはならない（REQ-DDM-012: status は情報提供のみ）|

### ST-DDM-024: `shikomi daemon install` の 2 回実行が冪等であること（AC-DDM-10）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-10 |
| 前提 | 1 回目の `shikomi daemon install` 実行済み |
| 操作 | `shikomi daemon install` を再度実行（2 回目）|
| 期待結果 | (1) exit 0 / (2) stdout に `"shikomi-daemon autostart enabled"` が含まれる（重複登録エラーにならない）|
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("autostart enabled"))` |

### ST-DDM-025: `shikomi daemon status` に `--no-ipc` を指定した場合、IPC probe を省略して exit 0 を返すこと

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-09（`--no-ipc` 分岐）/ basic-design.md §`--no-ipc` との関係 |
| 前提 | daemon の起動状態は問わない |
| 操作 | `shikomi --no-ipc daemon status` を実行 |
| 期待結果 | (1) exit 0 / (2) stdout に `"daemon: unknown (--no-ipc)"` が含まれる / (3) IPC 接続試行なし（タイムアウト待ちが発生しない）|
| 検証方法 | `assert_cmd::assert().success().stdout(predicates::str::contains("daemon: unknown (--no-ipc)"))` |

## 4. 観察戦略

| 観測対象 | 観測手段 |
|---------|---------|
| 終了コード | `assert_cmd::assert().code(N)` |
| stdout 内容 | `assert_cmd::assert().stdout(predicates::str::contains("..."))` |
| stderr 内容（存在確認）| `assert_cmd::assert().stderr(predicates::str::contains("..."))` |
| stderr 内容（非存在確認）| `assert_cmd::assert().stderr(predicates::str::contains("...").not())` |
| daemon の起動確認 | `200ms` sleep 後に `shikomi-cli` で IPC 接続確認（接続成功 = daemon 起動済み）|

## 5. テスト除外事項

| 項目 | 理由 |
|------|------|
| OS 再起動後の daemon 自動起動実証 | CI 環境での OS 再起動は不可。`SC-DDM-002 §手動確認事項` として記録 |
| `--no-ipc` と暗号化 vault の組み合わせ | `daemon-vault-encryption` feature スコープ（未起票）|
| Windows Named Pipe + `--no-ipc` 同時接続 | CI 環境で `\\.\pipe\` 経路のテスト設定が複雑。Phase 2 過渡期は Linux / macOS のみ重点確認 |
| MSG-CLI-110 hint への `shikomi daemon install` 誘導 | Sub-B 完了後の別 PR（`autostart/basic-design.md §Sub-B 完了後に更新するメッセージ`）|

## 6. 受入テストシナリオとの対応

| シナリオ | 対象 AC | 対応 ST-ID |
|---------|---------|-----------|
| `SC-DDM-001-ipc-default-mode.md` | AC-DDM-01〜06（Sub-A） | ST-DDM-010〜015 |
| `SC-DDM-002-daemon-autostart.md` | AC-DDM-07〜10（Sub-B） | ST-DDM-020〜025 |

各シナリオの Given / When / Then は `docs/acceptance-tests/scenarios/` を単一真実源とする。本書の ST-DDM-NNN は各シナリオの**技術的実装**（subprocess + `assert_cmd` による自動化）を担当する。
