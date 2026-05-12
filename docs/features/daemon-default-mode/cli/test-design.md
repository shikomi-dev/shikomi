# テスト設計書 — daemon-default-mode / cli

<!-- feature: daemon-default-mode / sub-feature: cli / Issue #126 -->
<!-- 配置先: docs/features/daemon-default-mode/cli/test-design.md -->
<!-- Vモデル対応: ユニットテスト（階層 3 詳細設計）+ 結合テスト（階層 3 モジュール契約）+ 受入テスト（階層 1/2 E2E）-->
<!-- 親: ../feature-spec.md / 兄弟: ./basic-design.md / ./detailed-design.md -->

## 1. 設計方針

| 項目 | 内容 |
|------|------|
| 対象 | `shikomi-cli`: `CliArgs::no_ipc` 追加・`ipc` 廃止（REQ-DDM-001）/ `build_handle` 分岐反転（REQ-DDM-002）/ `MSG-CLI-051` 廃止（REQ-DDM-003）/ `MSG-CLI-110` hint 更新（REQ-DDM-004）/ vault IPC 強制保護（REQ-DDM-005）|
| テストレベル | UT（ホワイトボックス）+ IT（半ブラックボックス、assert_cmd 経由）+ E2E（完全ブラックボックス、受入基準 AC-DDM-01〜06）|
| 言語 / フレームワーク | Rust / `assert_cmd`・`predicates`・`tempfile`・`tokio::test`（IT / UT）、`std::process::Command`（E2E）|
| 配置（UT）| `crates/shikomi-cli/src/` 各ファイル内 `#[cfg(test)] mod tests`（Rust 慣習）|
| 配置（IT）| `crates/shikomi-cli/tests/it_cli_default_mode.rs` |
| 配置（E2E）| `crates/shikomi-cli/tests/e2e_sc_ddm_001.rs` |
| 実行レシピ | `just test-cli`（`--all-targets -p shikomi-cli`）|
| 採番方針 | UT: TC-UT-150〜158 / IT: TC-IT-110〜115 / E2E: TC-E2E-120〜125 |

---

## 2. 外部 I/O 依存マップ

本 sub-feature が依存する外部 I/O を全列挙する。「要起票」のまま実装着手は禁止。

| 外部 I/O | 利用箇所 | raw fixture | factory | characterization 状態 |
|---------|---------|------------|---------|----------------------|
| clap 引数パース（`CliArgs`）| `cli.rs` | N/A（OSS ライブラリ、実物使用）| N/A | 不要 |
| UDS ソケット（IPC 接続）| `build_handle` → `IpcVaultRepository::connect` | N/A（`tempfile::TempDir` + 実 UDS）| N/A | 不要（in-process IpcServer スタブで代替。詳細は §3 モック方針）|
| SQLite（vault.db）| `build_handle` → `SqliteVaultRepository::from_directory` | N/A（`tempfile::TempDir` 実接続）| N/A | 不要（実 DB 使用）|
| 環境変数（`SHIKOMI_VAULT_DIR` / `XDG_RUNTIME_DIR`）| `default_socket_path` / `vault_dir` 解決 | N/A（test で `env()` 注入）| N/A | 不要 |
| stderr / stdout（MSG-CLI-110 等）| `render_error` | N/A（assert_cmd / predicates で直接観測）| N/A | 不要 |

**判定**: 本 sub-feature は外部 API・クラウドサービス等の非同期 I/O に依存しない。Characterization fixture（raw / schema / factory）の起票は不要。

---

## 3. モック方針

| テストレベル | IPC 経路のモック方法 | SQLite 経路 | 時刻 |
|------------|-------------------|-----------|------|
| UT | `clap::Parser::parse_from(...)` で引数パースのみ検証（接続不要）/ `build_handle` の IPC 接続テストは daemon 未起動状態で `DaemonNotRunning` を誘発（モック不要）| `tempfile::TempDir` 実接続 | 不問（時刻依存なし）|
| IT | `IpcServer` + `HotkeyManager::new_null()` + `SingleInstanceLock::acquire_unix` で in-process mock daemon（`shikomi-daemon/test-fixtures` feature 必要）。assert_cmd で `shikomi` サブプロセスを起動し、UDS ソケット経路のみ切り替え | `tempfile::TempDir` 実接続 | 不問 |
| E2E | 実 `shikomi-daemon` バイナリを `std::process::Command` でスポーン（完全ブラックボックス）| 実 DB | 不問 |

**assumed mock 禁止**: IT の IpcServer スタブは `shikomi-daemon` 既存インフラ（`IpcServer::new` + `watch::channel`）を流用する。インライン辞書リテラルによる仮定の返却値は禁止。

---

## 4. テストマトリクス（トレーサビリティ）

| 要件 / 受入基準 ID | テスト ID | テストレベル | 種別 | 対象関数 / 動作 |
|-----------------|----------|------------|------|--------------|
| REQ-DDM-001 | TC-UT-150 | UT | 正常 | `CliArgs` parse: `--no-ipc` → `no_ipc=true` |
| REQ-DDM-001 | TC-UT-151 | UT | 正常 | `CliArgs` parse: 引数なし → `no_ipc=false`（既定）|
| REQ-DDM-001 | TC-UT-152 | UT | 異常 | `CliArgs` parse: `--ipc` → clap error exit 2 |
| REQ-DDM-002 | TC-UT-153 | UT | 正常 | `build_handle(no_ipc=false)` → `RepositoryHandle::Ipc(_)` |
| REQ-DDM-002 | TC-UT-154 | UT | 正常 | `build_handle(no_ipc=true)` → `RepositoryHandle::Sqlite(_)` |
| REQ-DDM-002 | TC-UT-155 | UT | 異常 | `build_handle(no_ipc=false)` + daemon 未起動 → `CliError::DaemonNotRunning` |
| REQ-DDM-003 | TC-UT-156 | UT | 契約 | `render_ipc_opt_in_notice` 関数が存在しないこと（コンパイル確認）|
| REQ-DDM-003 | TC-UT-157 | UT | セキュリティ | IPC 経路 + `quiet=false` でも MSG-CLI-051 が出力されないこと |
| REQ-DDM-004 | TC-UT-158 | UT | 正常 | MSG-CLI-110 hint 文面に `--ipc` 言及が含まれないこと |
| REQ-DDM-005 | TC-UT-159 | UT | 正常 | vault サブコマンドは `no_ipc=true` でも IPC 強制（分岐しない）|
| REQ-DDM-001/002 | TC-IT-110 | IT | 正常 | `shikomi list` (daemon 起動中) → IPC 経路で成功 |
| REQ-DDM-002 | TC-IT-111 | IT | 正常 | `shikomi --no-ipc list` (daemon 不要) → SQLite 直結で成功 |
| REQ-DDM-002 | TC-IT-112 | IT | 異常 | daemon 未起動 `shikomi list` → MSG-CLI-110 + exit 1 |
| REQ-DDM-003 | TC-IT-113 | IT | セキュリティ | `shikomi list` stderr に MSG-CLI-051 が含まれないこと |
| REQ-DDM-005 | TC-IT-114 | IT | 正常 | `shikomi --no-ipc vault encrypt` → daemon IPC 経路 + 結果確認 |
| AC-DDM-01 | TC-E2E-120 | E2E | 正常 | daemon 起動 + `shikomi list` → IPC 経由でレコード一覧返却 |
| AC-DDM-02 | TC-E2E-121 | E2E | 正常 | `shikomi --no-ipc list` → daemon 未起動でも SQLite 直結で成功 |
| AC-DDM-03 | TC-E2E-122 | E2E | 異常 | daemon 未起動 `shikomi list` → MSG-CLI-110 + exit 1 |
| AC-DDM-04 | TC-E2E-123 | E2E | 異常 | `shikomi --ipc list` → clap error + exit 2（廃止確認）|
| AC-DDM-05 | TC-E2E-124 | E2E | セキュリティ | daemon 起動 + `shikomi list` stderr に MSG-CLI-051 非出力 |
| AC-DDM-06 | TC-E2E-125 | E2E | 正常 | `shikomi --no-ipc vault encrypt` → IPC 強制（vault サブコマンド保護）|

---

## 5. ユニットテスト設計（TC-UT-150〜159）

配置: 各 `crates/shikomi-cli/src/*.rs` の `#[cfg(test)] mod tests`。

### 5.1 CliArgs パース（REQ-DDM-001）

配置: `crates/shikomi-cli/src/cli.rs` `#[cfg(test)] mod tests`

#### TC-UT-150: `--no-ipc` フラグが `no_ipc = true` に変換される

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-001 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "--no-ipc", "list"])` |
| 期待結果 | `args.no_ipc == true` |

#### TC-UT-151: 引数なし（既定）で `no_ipc = false`

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-001 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | `CliArgs::parse_from(["shikomi", "list"])` |
| 期待結果 | `args.no_ipc == false` |

#### TC-UT-152: `--ipc` フラグが clap error（exit 2）を返す（廃止確認）

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-001（廃止・Fail Fast）|
| 種別 | 異常系 |
| 前提条件 | なし |
| 操作 | `CliArgs::try_parse_from(["shikomi", "--ipc", "list"])` |
| 期待結果 | `Err(_)`（clap の `ErrorKind::UnknownArgument` 相当）。error メッセージに `"--ipc"` が含まれること。exit code 2 相当 |

---

### 5.2 `build_handle` 分岐反転（REQ-DDM-002）

配置: `crates/shikomi-cli/src/lib.rs` `#[cfg(test)] mod tests`

#### TC-UT-153: `no_ipc = false`（既定）→ `RepositoryHandle::Ipc(_)` を返す

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-002 |
| 種別 | 正常系 |
| 前提条件 | `tempfile::TempDir` に実 UDS ソケットを持つ in-process daemon スタブが起動済み（`IpcServer` + `HotkeyManager::new_null()`）|
| 操作 | `build_handle(&CliArgs { no_ipc: false, vault_dir: None, .. }, locale, quiet)` |
| 期待結果 | `Ok(RepositoryHandle::Ipc(_))`（`RepositoryHandle::Sqlite` ではない）|

#### TC-UT-154: `no_ipc = true` → `RepositoryHandle::Sqlite(_)` を返す

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-002 |
| 種別 | 正常系 |
| 前提条件 | `tempfile::TempDir` に空の vault dir |
| 操作 | `build_handle(&CliArgs { no_ipc: true, vault_dir: Some(tmp_path), .. }, locale, quiet)` |
| 期待結果 | `Ok(RepositoryHandle::Sqlite(_))` |

#### TC-UT-155: IPC 接続失敗 → `CliError::DaemonNotRunning`

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-002 |
| 種別 | 異常系 |
| 前提条件 | daemon 未起動（ソケットファイル不在）|
| 操作 | `build_handle(&CliArgs { no_ipc: false, .. }, locale, quiet)` |
| 期待結果 | `Err(CliError::DaemonNotRunning)` |

---

### 5.3 MSG-CLI-051 廃止（REQ-DDM-003）

配置: `crates/shikomi-cli/src/presenter/warning.rs` `#[cfg(test)] mod tests`（または `lib.rs`）

#### TC-UT-156: `render_ipc_opt_in_notice` 関数が存在しないこと

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-003 |
| 種別 | 契約（コンパイル保証）|
| 前提条件 | なし |
| 操作 | `cargo check -p shikomi-cli` が成功すること。`warning.rs` に `render_ipc_opt_in_notice` を呼ぶコードがないこと（`grep` ゼロ件で確認）|
| 期待結果 | コンパイルエラーなし / `grep -rn "render_ipc_opt_in\|MSG-CLI-051" crates/shikomi-cli/src/` が 0 件 |
| 実装メモ | CI の `tc_ci_msg_cli_051_abolished` として `audit-secret-paths.sh` 相当のパターンで静的検査する方法も可 |

#### TC-UT-157: IPC 既定経路で MSG-CLI-051 が出力されない

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-003 |
| 種別 | セキュリティ / 正常系 |
| 前提条件 | in-process daemon スタブ起動済み |
| 操作 | `build_handle(no_ipc=false, quiet=false, ...)` を呼び、`eprint_stderr` が `MSG-CLI-051` の文言（`"IPC mode"` / `"--ipc"` 等）を出力しないことを `tracing_test::traced_test` または stderr キャプチャで確認 |
| 期待結果 | stderr に `"MSG-CLI-051"` / `"IPC mode"` / `"opt-in"` が含まれない |

---

### 5.4 MSG-CLI-110 hint 文面更新（REQ-DDM-004）

配置: `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)] mod tests`

#### TC-UT-158: MSG-CLI-110 hint 文面に `--ipc` が含まれない

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-004 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | `render_error(&CliError::DaemonNotRunning { socket_path: "/tmp/x.sock".into() }, Locale::En)` / `Locale::Ja` を呼ぶ |
| 期待結果 | (1) 返り値文字列に `"--ipc"` が含まれないこと (2) `"shikomi-daemon"` を含む起動案内 hint が含まれること (3) `"socket"` + path 文字列が含まれること（情報の欠落なし）|

---

### 5.5 vault サブコマンド IPC 強制保護（REQ-DDM-005）

配置: `crates/shikomi-cli/src/lib.rs` `#[cfg(test)] mod tests`

#### TC-UT-159: `--no-ipc` + vault サブコマンド → `no_ipc` は vault 経路に影響しない

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-005 |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | `CliArgs { no_ipc: true, subcommand: Subcommand::Vault(_), .. }` のとき `run_vault` / `connect_vault_ipc` が `no_ipc` を参照しないことを確認。`grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` で `build_handle` 以外の参照が 0 件 |
| 期待結果 | `no_ipc` の参照は `build_handle` 関数内の 1 箇所のみ |

---

## 6. 結合テスト設計（TC-IT-110〜114）

配置: `crates/shikomi-cli/tests/it_cli_default_mode.rs`

テストバイナリ実行コマンド: `just test-cli`（`cargo test --all-targets -p shikomi-cli --features "shikomi-infra/test-fixtures"`）

**共通前提**:
- `#[cfg(unix)]` — UDS を使用するため Unix 限定
- `tempfile::TempDir` で XDG_RUNTIME_DIR / SHIKOMI_VAULT_DIR を隔離
- in-process daemon スタブ: IT の場合は `shikomi-daemon` パッケージの `IpcServer` + `HotkeyManager::new_null()` + `SingleInstanceLock::acquire_unix` を使用（`shikomi-daemon/test-fixtures` feature が必要なため、`just test-daemon` レシピでの実行が前提）
- assert_cmd: `Command::cargo_bin("shikomi")` でサブプロセス起動

**重要**: `IpcServer` を in-process で立てる場合、`shikomi-daemon/test-fixtures` が必要。`shikomi-cli` パッケージ単独のテストでは daemon スタブを立てられないため、TC-IT-110 / TC-IT-113 / TC-IT-114 は `--features "shikomi-daemon/test-fixtures,shikomi-infra/test-fixtures"` を有効にした `just test-daemon` レシピでの実行を要する（`just test-cli` でスキップ可）。**実行レシピは `just test-daemon`**。

---

### TC-IT-110: `shikomi list`（daemon 起動中・`--no-ipc` なし）→ IPC 経路で成功

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-001 / REQ-DDM-002 |
| 種別 | 正常系 |
| 前提条件 | in-process daemon スタブ起動済み（ソケット + vault 0 件）|
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", xdg).args(["list"]).assert()` |
| 期待結果 | `success()` / stdout に レコード一覧（0 件時は空行またはヘッダのみ）/ exit 0 |
| 注意 | `--ipc` フラグなしで IPC 経路が使われることが主眼。`--no-ipc` を明示しないこと |

---

### TC-IT-111: `shikomi --no-ipc list`（daemon 不要）→ SQLite 直結で成功

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-002 |
| 種別 | 正常系 |
| 前提条件 | daemon 未起動 / `SHIKOMI_VAULT_DIR` に空 vault.db（`SqliteVaultRepository::from_directory` + `save` で事前作成）|
| 操作 | `Command::cargo_bin("shikomi").env("SHIKOMI_VAULT_DIR", vault_dir).args(["--no-ipc", "list"]).assert()` |
| 期待結果 | `success()` / exit 0 / stdout にレコード一覧（0 件）/ daemon を起動しなくても動作する |

---

### TC-IT-112: daemon 未起動 + `shikomi list` → MSG-CLI-110 + exit 1

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-002 |
| 種別 | 異常系 |
| 前提条件 | daemon 未起動（ソケット不在）|
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", empty_xdg).args(["list"]).assert()` |
| 期待結果 | `failure()` / exit 1 / stderr に `"shikomi-daemon"` または `"not running"` を含む（MSG-CLI-110）|
| 注意 | `--ipc` フラグなしで失敗することを確認する。エラーメッセージに socket path が含まれること（hint 文面の一部）|

---

### TC-IT-113: `shikomi list` の stderr に MSG-CLI-051 が含まれない

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-003 |
| 種別 | セキュリティ / 正常系 |
| 前提条件 | in-process daemon スタブ起動済み |
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", xdg).args(["list"]).assert()` |
| 期待結果 | `success()` / **stderr に `"IPC mode"` / `"--ipc"` / `"opt-in"` / `"MSG-CLI-051"` の文言が含まれない** |

---

### TC-IT-114: `shikomi --no-ipc vault encrypt` → vault サブコマンドが IPC 強制（`--no-ipc` 無視）

| 項目 | 内容 |
|------|------|
| 対応要件 | REQ-DDM-005 |
| 種別 | 正常系 |
| 前提条件 | daemon 未起動または起動済み（IPC 強制のため daemon 未起動の場合は MSG-CLI-110 + exit 1 が期待動作）|
| 操作 | `Command::cargo_bin("shikomi").env("XDG_RUNTIME_DIR", xdg).args(["--no-ipc", "vault", "encrypt"]).assert()` |
| 期待結果 | **daemon 未起動時**: `failure()` / exit 1 / stderr に MSG-CLI-110（daemon 未起動エラー）— `--no-ipc` が vault 経路に影響しないことを証明 / **daemon 起動時**: vault 操作が IPC 経由で実行される |
| 注意 | `--no-ipc` を指定しても vault サブコマンドは SQLite 直結にフォールバックしてはならない |

---

## 7. 受入テスト設計 / E2E（TC-E2E-120〜125）

配置: `crates/shikomi-cli/tests/e2e_sc_ddm_001.rs`

**完全ブラックボックス方針**: `std::process::Command` で `shikomi-daemon` / `shikomi` バイナリを spawn し、stdout / stderr / exit code とファイルシステム観測のみで判定する。DB 直接確認・内部状態参照・テスト用裏口・内部関数呼び出しは一切行わない。

実行レシピ: `just test-daemon`（`-p shikomi-daemon -p shikomi-cli` の両方をビルドするため）

**ペルソナ対応**: ペルソナ B（山田 美咲 / CLI 主体）の日常操作シナリオ。daemon を意識せず `shikomi list` が動くことが Phase 2 の本質的価値。

---

### TC-E2E-120 ← AC-DDM-01: daemon 起動 + `shikomi list` → IPC 経由で成功

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-01 |
| 対応要件 | R1-DDM-01 |
| 種別 | 正常系 |
| 前提条件 | `XDG_RUNTIME_DIR` / `SHIKOMI_VAULT_DIR` を isolate した `tight_tempdir` / `shikomi-daemon` バイナリが起動済みかつ sock ファイルが存在する |
| 操作 | (1) `shikomi-daemon` を spawn（sock 生成まで最大 8 秒待機）(2) `shikomi list` を `--ipc` フラグなしで実行 |
| 期待結果 | (1) `shikomi list` が exit 0 で成功 (2) stdout にレコード一覧（0 件でも成功） (3) stderr に `"MSG-CLI-051"` / `"--ipc"` 等の opt-in 警告が含まれない |
| cleanup | daemon に SIGTERM → exit 0 確認 |

---

### TC-E2E-121 ← AC-DDM-02: `shikomi --no-ipc list` → SQLite 直結で成功（daemon 不要）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-02 |
| 対応要件 | R1-DDM-02 |
| 種別 | 正常系 |
| 前提条件 | daemon 未起動 / `SHIKOMI_VAULT_DIR` に vault.db が存在する（事前に `shikomi-daemon` で 1 回起動してから停止、または直接 SQLite で作成）|
| 操作 | `shikomi --no-ipc list`（`XDG_RUNTIME_DIR` に sock なし）|
| 期待結果 | exit 0 / stdout にレコード一覧（0 件）/ daemon が起動していなくても動作する |

---

### TC-E2E-122 ← AC-DDM-03: daemon 未起動 + `shikomi list` → MSG-CLI-110 + exit 1

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-03 |
| 対応要件 | R1-DDM-01 / R1-DDM-05 |
| 種別 | 異常系 |
| 前提条件 | daemon 未起動（sock ファイル不在）/ `XDG_RUNTIME_DIR` に shikomi サブディレクトリのみ作成されていても sock なし |
| 操作 | `shikomi list`（`--ipc` フラグなし / daemon 未起動）|
| 期待結果 | exit 1 / stderr に MSG-CLI-110 文面（`"not running"` または `"shikomi-daemon"` を含む）/ stderr に `"hint:"` + daemon 起動コマンド案内 / **stderr に `"--ipc"` が含まれない**（Phase 2 移行後は `--ipc` を案内しない）|

---

### TC-E2E-123 ← AC-DDM-04: `shikomi --ipc list` → exit 2（廃止確認）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-04 |
| 対応要件 | R1-DDM-03（Fail Fast）|
| 種別 | 異常系 |
| 前提条件 | なし（daemon 起動不要）|
| 操作 | `shikomi --ipc list` |
| 期待結果 | exit 2 / stderr に clap のエラーメッセージ（`"unexpected argument '--ipc'"` 等）/ 正常処理が行われないこと |
| 注意 | clap の exit code は 2（使用法エラー）。exit 1（アプリケーションエラー）と混同しないこと |

---

### TC-E2E-124 ← AC-DDM-05: daemon 起動中 `shikomi list` → MSG-CLI-051 非出力（廃止確認）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-05 |
| 対応要件 | R1-DDM-04 |
| 種別 | セキュリティ / 正常系 |
| 前提条件 | `shikomi-daemon` 起動済み |
| 操作 | `shikomi list`（`--ipc` なし）を実行し stderr を収集 |
| 期待結果 | exit 0 / stderr に以下が**含まれない**: `"IPC mode"` / `"--ipc"` / `"opt-in"` / `"MSG-CLI-051"` の文言（Phase 1 の警告が廃止されていることを確認）|

---

### TC-E2E-125 ← AC-DDM-06: `shikomi --no-ipc vault encrypt` → IPC 強制（vault サブコマンド保護）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | AC-DDM-06 |
| 対応要件 | R1-DDM-06 |
| 種別 | 正常系 |
| 前提条件 | daemon 未起動（IPC 強制の証明のため）|
| 操作 | `shikomi --no-ipc vault encrypt`（daemon 未起動状態）|
| 期待結果 | exit 1 / stderr に MSG-CLI-110（daemon 未起動エラー）— **`--no-ipc` を指定しても vault サブコマンドは SQLite 直結にフォールバックしない**ことを証明 / SQLite への直接アクセスが行われないこと（vault.db が変更されないこと）|
| 補足 | `vault encrypt` が `--no-ipc` を無視して IPC を試みる → daemon 未起動 → MSG-CLI-110 の 3 段論法 |

---

## 8. テスト配置まとめ

| ファイル | 種別 | TC-ID | 実行レシピ |
|---------|------|-------|----------|
| `crates/shikomi-cli/src/cli.rs` `#[cfg(test)]` | UT | TC-UT-150〜152 | `just test-cli` |
| `crates/shikomi-cli/src/lib.rs` `#[cfg(test)]` | UT | TC-UT-153〜155, TC-UT-159 | `just test-daemon`（`test-fixtures` 必要）|
| `crates/shikomi-cli/src/presenter/warning.rs` `#[cfg(test)]` | UT | TC-UT-156〜157 | `just test-cli` |
| `crates/shikomi-cli/src/presenter/error.rs` `#[cfg(test)]` | UT | TC-UT-158 | `just test-cli` |
| `crates/shikomi-cli/tests/it_cli_default_mode.rs` | IT | TC-IT-110〜114 | `just test-daemon` |
| `crates/shikomi-cli/tests/e2e_sc_ddm_001.rs` | E2E | TC-E2E-120〜125 | `just test-daemon` |

---

## 9. CI ゲート

| チェック | コマンド | 失敗条件 |
|---------|---------|---------|
| UT 全件通過 | `cargo test -p shikomi-cli` | 1 件でも FAILED |
| IT / E2E 全件通過 | `just test-daemon` | 1 件でも FAILED |
| MSG-CLI-051 残存なし | `grep -rn "MSG-CLI-051\|render_ipc_opt_in" crates/shikomi-cli/src/` が 0 件 | 1 件でも HIT |
| `--ipc` フラグ参照なし | `grep -rn 'args\.ipc\b\|"--ipc"' crates/shikomi-cli/src/` が 0 件（テストコード除く）| 1 件でも HIT |
| `--no-ipc` が vault 経路に影響しない | `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` が `build_handle` 内 1 件のみ | 2 件以上 HIT |
