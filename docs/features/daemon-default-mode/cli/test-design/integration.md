# テスト設計書 — daemon-default-mode / cli / 結合テスト

<!-- feature: daemon-default-mode / sub-feature: cli / Issue #126 -->
<!-- 配置先: docs/features/daemon-default-mode/cli/test-design/integration.md -->
<!-- Vモデル対応: 階層 3（basic-design.md §モジュール契約 → 結合テスト）-->
<!-- 兄弟: unit.md / 親: ../basic-design.md / ../detailed-design.md -->

## 1. 設計方針

- **対象**: `shikomi` CLI バイナリの IPC / SQLite 経路選択（コンポジションルート水準）
  - `shikomi list`（デフォルト IPC 経路）の結合動作
  - `shikomi --no-ipc list`（SQLite 直結経路）の結合動作
  - daemon 未起動時の失敗動作と MSG-CLI-110 出力
  - `MSG-CLI-051` 廃止後の stderr 非出力確認
  - vault サブコマンドの `--no-ipc` 無視（IPC 強制）
- **視点**: 半ブラックボックス。`assert_cmd::Command::cargo_bin("shikomi")` でサブプロセス起動し、stdout / stderr / exit code で検証
- **モック**: in-process `IpcServer`（`HotkeyManager::new_null()` + `SingleInstanceLock::acquire_unix`）を UDS ソケット上に立て、`shikomi` サブプロセスがそれに接続する
- **DB**: `tempfile::TempDir` 実接続（SQLite）
- **配置**: `crates/shikomi-cli/tests/it_cli_default_mode.rs`
- **実行レシピ**: `just test-daemon`（`shikomi-daemon/test-fixtures,shikomi-infra/test-fixtures` feature 必要）

---

## 2. 外部 I/O 依存マップ

| 外部 I/O | 利用箇所 | characterization 状態 |
|---------|---------|----------------------|
| UDS ソケット（IPC 接続）| `IpcServer` + `shikomi` サブプロセス | 不要（in-process スタブ・実 UDS 使用）|
| SQLite（vault.db）| `shikomi --no-ipc list` 経路 | 不要（`tempfile::TempDir` 実接続）|
| `shikomi-daemon` バイナリ | IT では不使用（in-process スタブで代替）| 不要 |
| 環境変数（`XDG_RUNTIME_DIR` / `SHIKOMI_VAULT_DIR`）| `Command::cargo_bin("shikomi").env(...)` | 不要（test 内で直接注入）|

外部 API・クラウドサービスへの依存なし。Characterization fixture 不要。

---

## 3. モック方針（IT）

| 依存先 | モック方法 |
|--------|-----------|
| IpcServer（daemon 代替）| `IpcServer::new` + `HotkeyManager::new_null()` + `SingleInstanceLock::acquire_unix` + `watch::channel` で in-process 起動。`shikomi` サブプロセスはこの UDS ソケットに接続 |
| SQLite | `tempfile::TempDir` + `SqliteVaultRepository::from_directory` 実接続（`--no-ipc` 経路）|
| 時刻 | 不問（IT は時刻依存なし）|

**起動待機**: IpcServer を `tokio::spawn` 後、`tokio::time::sleep(Duration::from_millis(50))` でソケット accept 開始を待つ（`it_vault_init.rs` の既存パターンを踏襲）。

---

## 4. 共通前提（テスト関数冒頭）

```
#![cfg(unix)]
// test-fixtures feature が必要: just test-daemon で実行
use std::time::Duration;
use tempfile::TempDir;
// tight_tempdir() : 0o700 の TempDir を生成（セキュリティ規約に従う）
fn tight_tempdir() -> TempDir { ... }
```

---

## 5. トレーサビリティマトリクス

| TC-ID | 対応要件 | 対応受入基準 | 種別 | 検証観点 |
|-------|---------|------------|------|---------|
| TC-IT-110 | REQ-DDM-001 / REQ-DDM-002 | AC-DDM-01 / AC-DDM-05 | 正常 | `shikomi list` → IPC 経路 + MSG-CLI-051 非出力 |
| TC-IT-111 | REQ-DDM-002 | AC-DDM-02 | 正常 | `shikomi --no-ipc list` → SQLite 直結 |
| TC-IT-112 | REQ-DDM-002 | AC-DDM-03 | 異常 | daemon 未起動 → MSG-CLI-110 + exit 1 |
| TC-IT-113 | REQ-DDM-003 | AC-DDM-05 | セキュリティ | MSG-CLI-051 文言が stderr に含まれない |
| TC-IT-114 | REQ-DDM-005 | AC-DDM-06 | 正常 | `--no-ipc vault encrypt` → vault IPC 強制 |

上位トレーサビリティ: `TC-IT-110〜114` → `ST-DDM-001〜006`（system-test-design.md）→ `SC-DDM-001`（acceptance-tests/scenarios/SC-DDM-001-ipc-default-mode.md）→ `AC-DDM-01〜06`（feature-spec.md §5）

---

## 6. テストケース一覧

### TC-IT-110: `shikomi list`（daemon 起動中 / `--no-ipc` なし）→ IPC 経路で成功 + MSG-CLI-051 非出力

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tight_tempdir()` + in-process IpcServer スタブ起動済み（vault 0 件）|
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", xdg).args(["list"]).assert()` |
| 期待（exit）| `success()` / exit 0 |
| 期待（stdout）| レコード一覧（0 件含む）|
| 期待（stderr）| `"IPC mode"` / `"--ipc"` / `"opt-in"` が含まれない（MSG-CLI-051 廃止確認）|
| 注意 | `--ipc` フラグを明示しないこと。IPC 既定を検証するテストのため |

---

### TC-IT-111: `shikomi --no-ipc list`（daemon 不要）→ SQLite 直結で成功

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | daemon 未起動 / `SHIKOMI_VAULT_DIR` に空 vault.db（事前に `SqliteVaultRepository::from_directory` + `save` で作成）|
| 操作 | `Command::cargo_bin("shikomi").env("SHIKOMI_VAULT_DIR", vault_dir).args(["--no-ipc", "list"]).assert()` |
| 期待（exit）| `success()` / exit 0 |
| 期待（stdout）| レコード一覧（0 件）|
| 期待（挙動）| daemon が起動していなくても動作すること（UDS ソケット不在でも exit 0）|

---

### TC-IT-112: daemon 未起動 + `shikomi list` → MSG-CLI-110 + exit 1

| 項目 | 内容 |
|------|------|
| 種別 | 異常系 |
| 前提 | daemon 未起動（ソケット不在） / `XDG_RUNTIME_DIR` に shikomi ディレクトリのみ存在（sock なし）|
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", empty_xdg).args(["list"]).assert()` |
| 期待（exit）| `failure()` / exit 1 |
| 期待（stderr）| `"shikomi-daemon"` または `"not running"` を含む（MSG-CLI-110 原因文） |
| 期待（stderr）| `"hint:"` + daemon 起動コマンドを含む（案内行）|
| 期待（stderr）| `"--ipc"` が**含まれない**（Phase 2 廃止フラグを案内しない）|

---

### TC-IT-113: daemon 起動中 + `shikomi list` → stderr に MSG-CLI-051 文言が含まれない

| 項目 | 内容 |
|------|------|
| 種別 | セキュリティ / 正常系 |
| 前提 | in-process IpcServer スタブ起動済み |
| 操作 | TC-IT-110 と同じコマンドで stderr のみを精査 |
| 期待（stderr）| 以下が全て含まれない: `"IPC mode"` / `"--ipc"` / `"opt-in"` / `"MSG-CLI-051"` |
| 注意 | TC-IT-110 と操作が同一のため、同一テスト関数内でアサーションを追加する形でよい（1 テスト多アサーションを例外的に許容） |

---

### TC-IT-114: `shikomi --no-ipc vault encrypt` → vault IPC 強制（`--no-ipc` 無視）

| 項目 | 内容 |
|------|------|
| 種別 | 正常系（異常ケースとして daemon 未起動で検証）|
| 前提 | daemon 未起動（IPC 強制の証明に最適: `--no-ipc` が無視されれば IPC 試行 → daemon 未起動 → MSG-CLI-110）|
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", empty_xdg).args(["--no-ipc", "vault", "encrypt"]).assert()` |
| 期待（exit）| `failure()` / exit 1 |
| 期待（stderr）| MSG-CLI-110（`"not running"` 等）— `--no-ipc` が vault 経路に影響しないことを証明 |
| 期待（挙動）| vault.db が**変更されていない**こと（SQLite 直結フォールバックが起きていない）|
| 補足 | `--no-ipc` 無視 → IPC 試行 → daemon 未起動 → MSG-CLI-110 の 3 段論法 |

---

## 7. CI ゲート

| チェック | コマンド / 手段 | 失敗条件 |
|---------|--------------|---------|
| IT 全件通過 | `just test-daemon` | 1 件でも FAILED |
| MSG-CLI-051 残存なし | `grep -rn "MSG-CLI-051\|render_ipc_opt_in" crates/shikomi-cli/src/` が 0 件 | 1 件でも HIT |
| `--ipc` フラグ参照なし | `grep -rn 'args\.ipc\b' crates/shikomi-cli/src/` が 0 件 | 1 件でも HIT |
| `no_ipc` が build_handle 外に漏れない | `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` が `build_handle` 内 1 件のみ | 2 件以上 HIT |
