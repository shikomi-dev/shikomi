# システムテスト戦略 — daemon-ipc

<!-- feature: daemon-ipc / Issue #26 (Phase 1) / Issue #30 (Phase 1.5) / Issue #80 (Bug-F-008) -->
<!-- 配置先: docs/features/daemon-ipc/system-test-design.md -->
<!-- Vモデル対応: システムテスト（feature 横断の業務シナリオ、E2E より細粒度 / IT より粗粒度）-->

## 1. 概要

本書は `daemon-ipc` feature のシステムテスト（ST）戦略を定義する。sub-feature `test-design/*.md` が扱う IT / UT とは異なり、**daemon プロセス全体を実際に起動 / 停止する通し動作**を検証する。

| 項目 | 内容 |
|------|------|
| 対象 | `shikomi-daemon` プロセス全体（lift & shutdown サイクル）+ `shikomi-cli --ipc` E2E 連携 |
| テストレベル | システムテスト（E2E 一歩手前。subprocess 起動だが全プロセスを対象）|
| 実行環境 | CI 3 OS matrix（Linux / macOS / Windows）|
| テストフレームワーク | `assert_cmd` + `predicates`（`shikomi-daemon`、`shikomi-cli` 共に）|
| 配置 | `crates/shikomi-daemon/tests/` の `it_*.rs`（結合）+ E2E スクリプト |

## 2. 対応受入基準

| 受入基準 | 対応 ST-ID | 対応シナリオ |
|---------|-----------|------------|
| AC-001（初回起動、vault.db 不在）| ST-DAEMON-010 | `SC-DAEMON-001` |
| AC-003（初回起動ログ検証）| ST-DAEMON-011 | `SC-DAEMON-001` |
| AC-004（2 回目起動で再生成しない）| ST-DAEMON-012 | `SC-DAEMON-001` |

## 3. 既存システムテスト

`daemon-ipc` の既存 E2E テストは `test-design/e2e.md` に記述済み:

- `TC-E2E-001`: 実 daemon 起動 + `--ipc list` round-trip
- `TC-E2E-002`: SIGKILL 後 stale socket での次回起動成功
- `TC-E2E-003`: SIGTERM graceful shutdown → exit 0
- `TC-E2E-004`: 二重起動 → exit 2（シングルインスタンス保証）
- `TC-E2E-005`: 暗号化 vault 検出 → exit 3

## 4. 観察戦略

システムテストでは以下の観測手段を組み合わせる:

| 観測対象 | 観測手段 |
|---------|---------|
| 終了コード | `assert_cmd::assert().code(N)` |
| stdout / stderr ログ | `assert_cmd::assert().stderr(predicates::str::contains("..."))` |
| ファイルシステム状態 | `std::fs::metadata(path).is_ok()` / mtime 比較 |
| IPC round-trip | `shikomi-cli` サブプロセスから `--ipc list` 発行 |

## 5. Issue #80 追加テスト（ST-DAEMON-010〜012）

### ST-DAEMON-010: vault.db 不在での daemon 起動成功

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-001 |
| 前提 | `tempfile::TempDir` の空ディレクトリを `SHIKOMI_VAULT_DIR` に設定（vault.db なし）|
| 操作 | `assert_cmd::Command::cargo_bin("shikomi-daemon")` で daemon を spawn し、`100ms` 待機後に SIGTERM を送信 |
| 期待結果 | (1) daemon が exit 0 で終了すること / (2) `$SHIKOMI_VAULT_DIR/vault.db` が生成されていること |
| 配置 | `crates/shikomi-daemon/tests/it_vault_init.rs`（IT として §11 と共存）|

### ST-DAEMON-011: 初回起動ログ検証

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-003 |
| 前提 | ST-DAEMON-010 と同条件。`SHIKOMI_DAEMON_LOG=info` を env に設定 |
| 操作 | daemon 起動後 stderr ログをキャプチャ |
| 期待結果 | stderr に `"vault not found; created new plaintext vault at "` が含まれること / `"hint: to enable encryption"` が含まれること |
| 注意 | `tracing_subscriber` の出力先は `stderr`（`composition-root.md §エラーメッセージ出力先の統一` で確定）|

### ST-DAEMON-012: 2 回目起動で vault.db を再生成しない

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-004 |
| 前提 | ST-DAEMON-010 を先行実施し vault.db 生成済み |
| 操作 | vault.db の mtime を記録後、daemon を再起動して SIGTERM で停止 |
| 期待結果 | (1) daemon が exit 0 で終了すること / (2) vault.db の mtime が変化しないこと / (3) stderr に `"vault not found"` が出現しないこと |

## 6. テスト除外事項

| 項目 | 理由 |
|------|------|
| 暗号化 vault での初回起動 | `daemon-vault-encryption` feature スコープ（未起票）|
| マルチユーザ接続拒否 | CI 環境に `sudo -u nobody` 等が必要（Linux 専用、環境依存）|
| ネットワーク越し IPC | UDS / Named Pipe はローカルのみ（設計上不要）|
