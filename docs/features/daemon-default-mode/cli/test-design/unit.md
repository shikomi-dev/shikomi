# テスト設計書 — daemon-default-mode / cli / ユニットテスト

<!-- feature: daemon-default-mode / sub-feature: cli / Issue #126 -->
<!-- 配置先: docs/features/daemon-default-mode/cli/test-design/unit.md -->
<!-- Vモデル対応: 階層 3（詳細設計 → ユニットテスト）-->
<!-- 兄弟: integration.md / 親: ../basic-design.md / ../detailed-design.md -->

## 1. 設計方針

- **対象**: `crates/shikomi-cli/src/` 内の public / crate-internal 関数・フィールド
  - `CliArgs::no_ipc` フィールドの clap パース（REQ-DDM-001）
  - `build_handle` 関数の分岐反転（REQ-DDM-002）
  - `render_ipc_opt_in_notice` 関数の廃止確認（REQ-DDM-003）
  - `MSG-CLI-110` hint 文面の `--ipc` 言及削除（REQ-DDM-004）
  - vault サブコマンドが `no_ipc` を参照しない構造的保証（REQ-DDM-005）
- **粒度**: 1 テスト 1 アサーション。命名 `test_何をした時_どうなるべきか`
- **配置**: Rust 慣習、`#[cfg(test)] mod tests` でソースモジュール内（詳細は §6 テスト配置）
- **モック**: clap パースには不要。`build_handle` の IPC 経路テストは daemon 未起動状態で `DaemonNotRunning` を誘発（モック不要）、または in-process `IpcServer` スタブ使用（§3 参照）

---

## 2. 外部 I/O 依存マップ

| 外部 I/O | 利用箇所 | characterization 状態 |
|---------|---------|----------------------|
| clap 引数パース | `cli.rs` | 不要（OSS ライブラリ・実物使用）|
| UDS ソケット（IPC 接続）| `build_handle` → `IpcVaultRepository::connect` | 不要（daemon 未起動で自然に `DaemonNotRunning` 誘発 / in-process スタブ）|
| SQLite（vault.db）| `build_handle` → `SqliteVaultRepository` | 不要（`tempfile::TempDir` 実接続）|
| 環境変数 | `default_socket_path` | 不要（test で `env()` 注入）|

外部 API・クラウドサービス等への依存なし。Characterization fixture（raw / schema / factory）の起票不要。

---

## 3. モック方針（UT）

| 対象 | モック方法 |
|------|-----------|
| IPC 接続（`build_handle(no_ipc=false)`）| daemon 未起動状態で `connect` を呼ぶ → `CliError::DaemonNotRunning` が自然発生（assumed mock 不要）|
| IPC 接続（正常系 TC-UT-153）| in-process `IpcServer` スタブ（`HotkeyManager::new_null()` + `SingleInstanceLock::acquire_unix`）— `shikomi-daemon/test-fixtures` feature 必要 |
| SQLite（`build_handle(no_ipc=true)`）| `tempfile::TempDir` 実接続 |
| `render_error` の文面確認 | 実関数を直接呼び出し（引数注入のみ）|

---

## 4. トレーサビリティマトリクス

| TC-ID | 対応要件 | 対応受入基準 | 種別 | 対象関数 |
|-------|---------|------------|------|---------|
| TC-UT-150 | REQ-DDM-001 | AC-DDM-01 / AC-DDM-04 | 正常 | `CliArgs` parse: `--no-ipc` |
| TC-UT-151 | REQ-DDM-001 | AC-DDM-01 / AC-DDM-05 | 正常 | `CliArgs` parse: 引数なし（既定）|
| TC-UT-152 | REQ-DDM-001 | AC-DDM-04 | 異常 | `CliArgs` parse: `--ipc` → clap error |
| TC-UT-153 | REQ-DDM-002 | AC-DDM-01 | 正常 | `build_handle(no_ipc=false)` → Ipc |
| TC-UT-154 | REQ-DDM-002 | AC-DDM-02 | 正常 | `build_handle(no_ipc=true)` → Sqlite |
| TC-UT-155 | REQ-DDM-002 | AC-DDM-03 | 異常 | `build_handle` + daemon 未起動 → DaemonNotRunning |
| TC-UT-156 | REQ-DDM-003 | AC-DDM-05 | 契約 | `render_ipc_opt_in_notice` 廃止（grep ゼロ件）|
| TC-UT-157 | REQ-DDM-003 | AC-DDM-05 | セキュリティ | IPC 経路で MSG-CLI-051 非出力 |
| TC-UT-158 | REQ-DDM-004 | AC-DDM-03 | 正常 | MSG-CLI-110 hint に `--ipc` 非含有 |
| TC-UT-159 | REQ-DDM-005 | AC-DDM-06 | 正常 | vault 経路は `no_ipc` を参照しない（grep 2 件のみ: vault dispatch + build_handle）|

上位トレーサビリティ: `TC-UT-150〜159` → `TC-IT-110〜114`（integration.md）→ `ST-DDM-010〜015`（system-test-design.md）→ `SC-DDM-001`（acceptance-tests/scenarios/）→ `AC-DDM-01〜06`（feature-spec.md §5）

---

## 5. テストケース一覧

### 5.1 CliArgs パース（REQ-DDM-001）

配置: `crates/shikomi-cli/src/cli.rs` `#[cfg(test)] mod tests`

#### TC-UT-150: `--no-ipc` → `args.no_ipc == true`

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "--no-ipc", "list"])` |
| 期待 | `args.no_ipc == true` |

#### TC-UT-151: 引数なし → `args.no_ipc == false`（既定）

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "list"])` |
| 期待 | `args.no_ipc == false` |

#### TC-UT-152: `--ipc` → clap error（廃止・Fail Fast）

| 項目 | 内容 |
|------|------|
| 種別 | 異常系 |
| 前提 | なし |
| 操作 | `CliArgs::try_parse_from(["shikomi", "--ipc", "list"])` |
| 期待 | `Err(_)`（`ErrorKind::UnknownArgument` 相当）/ error に `"--ipc"` が含まれる / exit 2 相当 |

---

### 5.2 `build_handle` 分岐反転（REQ-DDM-002）

配置: `crates/shikomi-cli/src/lib.rs` `#[cfg(test)] mod tests`

#### TC-UT-153: `no_ipc=false`（既定）→ `RepositoryHandle::Ipc(_)`

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | in-process IpcServer スタブ起動済み（`HotkeyManager::new_null()`）|
| 操作 | `build_handle(&CliArgs { no_ipc: false, .. }, locale, quiet)` |
| 期待 | `Ok(RepositoryHandle::Ipc(_))`（Sqlite でないこと）|
| 実行レシピ | `just test-daemon`（`shikomi-daemon/test-fixtures` feature 必要）|

#### TC-UT-154: `no_ipc=true` → `RepositoryHandle::Sqlite(_)`

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | `tempfile::TempDir` に空 vault dir |
| 操作 | `build_handle(&CliArgs { no_ipc: true, vault_dir: Some(tmp), .. }, locale, quiet)` |
| 期待 | `Ok(RepositoryHandle::Sqlite(_))` |

#### TC-UT-155: `no_ipc=false` + daemon 未起動 → `CliError::DaemonNotRunning`

| 項目 | 内容 |
|------|------|
| 種別 | 異常系 |
| 前提 | daemon 未起動（ソケット不在）|
| 操作 | `build_handle(&CliArgs { no_ipc: false, .. }, locale, quiet)` |
| 期待 | `Err(CliError::DaemonNotRunning)` |

---

### 5.3 MSG-CLI-051 廃止（REQ-DDM-003）

配置: `crates/shikomi-cli/src/presenter/warning.rs` `#[cfg(test)] mod tests`

#### TC-UT-156: `render_ipc_opt_in_notice` が存在しない（コンパイル保証）

| 項目 | 内容 |
|------|------|
| 種別 | 契約（静的検査）|
| 前提 | なし |
| 操作 | `grep -rn "render_ipc_opt_in\|MSG-CLI-051" crates/shikomi-cli/src/` |
| 期待 | 0 件（コンパイルエラーなし）|
| 実装メモ | CI ゲートとして `#[test]` + `assert!` でも実装可（`include_str!` + `contains` パターン）|

#### TC-UT-157: IPC 経路（`quiet=false`）で MSG-CLI-051 が出力されない

| 項目 | 内容 |
|------|------|
| 種別 | セキュリティ / 正常系 |
| 前提 | in-process IpcServer スタブ起動済み |
| 操作 | `build_handle(no_ipc=false, quiet=false, ...)` + stderr キャプチャ |
| 期待 | stderr に `"IPC mode"` / `"--ipc"` / `"opt-in"` が含まれない |

---

### 5.4 MSG-CLI-110 hint 更新（REQ-DDM-004）

配置: `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)] mod tests`

#### TC-UT-158: MSG-CLI-110 hint 文面に `--ipc` が含まれない

| 項目 | 内容 |
|------|------|
| 種別 | 正常系 |
| 前提 | なし |
| 操作 | `render_error(&CliError::DaemonNotRunning { socket_path: "/tmp/x.sock".into() }, Locale::En)` / `Locale::Ja` |
| 期待 | (1) `"--ipc"` が含まれない (2) `"shikomi-daemon"` を含む起動案内がある (3) `"socket"` + path が含まれる |

---

### 5.5 vault IPC 強制（REQ-DDM-005）

配置: `crates/shikomi-cli/src/lib.rs` `#[cfg(test)] mod tests`

#### TC-UT-159: `no_ipc` が vault 経路（`build_handle` 以外）に影響しない

| 項目 | 内容 |
|------|------|
| 種別 | 正常系（静的検査）|
| 前提 | なし |
| 操作 | `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` |
| 期待 | **2 件のみ**（① `build_handle` 内の IPC/SQLite 分岐 / ② vault サブコマンド dispatch の `args.no_ipc && !quiet` による MSG-CLI-052 出力判定）。3 件以上は `no_ipc` が vault 経路に漏れた証拠。1 件以下は MSG-CLI-052 出力が消えた証拠 |

---

## 6. テスト配置

| ファイル | TC-ID | 実行レシピ |
|---------|-------|----------|
| `crates/shikomi-cli/src/cli.rs` `#[cfg(test)]` | TC-UT-150〜152 | `just test-cli` |
| `crates/shikomi-cli/src/lib.rs` `#[cfg(test)]` | TC-UT-153〜155, TC-UT-159 | `just test-daemon` |
| `crates/shikomi-cli/src/presenter/warning.rs` `#[cfg(test)]` | TC-UT-156〜157 | `just test-cli` |
| `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)]` | TC-UT-158 | `just test-cli` |
