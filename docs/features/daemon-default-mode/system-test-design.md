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

本 feature の受入基準（`feature-spec.md §5`）と ST-ID の対応を示す。受入基準の詳細は `docs/acceptance-tests/scenarios/SC-DDM-001-ipc-default-mode.md` を単一真実源とする。

| 受入基準 | 対応 ST-ID | 対応シナリオ |
|---------|-----------|------------|
| AC-DDM-01（daemon 起動中に `shikomi list` → IPC 経由成功）| ST-DDM-010 | `SC-DDM-001` |
| AC-DDM-02（`shikomi --no-ipc list` → SQLite 直結成功）| ST-DDM-011 | `SC-DDM-001` |
| AC-DDM-03（daemon 未起動 + `shikomi list` → MSG-CLI-110 + exit 1）| ST-DDM-012 | `SC-DDM-001` |
| AC-DDM-04（`shikomi --ipc list` → clap error exit 2）| ST-DDM-013 | `SC-DDM-001` |
| AC-DDM-05（daemon 起動中 `shikomi list` の stderr に MSG-CLI-051 なし）| ST-DDM-014 | `SC-DDM-001` |
| AC-DDM-06（`shikomi --no-ipc vault encrypt` → vault IPC 強制確認）| ST-DDM-015 | `SC-DDM-001` |

## 3. システムテスト定義（ST-DDM-010〜015）

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
| daemon OS 自動起動（launchd / systemd / Windows Task Scheduler）| Sub-B（Issue #127）スコープ。本書では対象外 |
| `--no-ipc` と暗号化 vault の組み合わせ | `daemon-vault-encryption` feature スコープ（未起票）|
| Windows Named Pipe + `--no-ipc` 同時接続 | CI 環境で `\\.\pipe\` 経路のテスト設定が複雑。Phase 2 過渡期は Linux / macOS のみ重点確認 |

## 6. 受入テストシナリオとの対応

詳細な受入テスト（AC-DDM-01〜06 の Given / When / Then）は `docs/acceptance-tests/scenarios/SC-DDM-001-ipc-default-mode.md` を単一真実源とする。本書の ST-DDM-010〜015 は同シナリオの**技術的実装**（subprocess + `assert_cmd` による自動化）を担当する。
