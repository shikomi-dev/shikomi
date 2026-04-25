# テスト設計書 — daemon-ipc（索引）

<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/test-design/index.md -->
<!-- 兄弟: ./unit.md, ./integration.md, ./e2e.md, ./ci.md -->

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | daemon-ipc（shikomi-daemon 骨格 + IPC プロトコル + `shikomi-cli --ipc` オプトイン） |
| 対象 PR | [#28](https://github.com/shikomi-dev/shikomi/pull/28)（`feature/26-daemon-ipc` → `develop`） |
| 対象ブランチ | `feature/26-daemon-ipc`（commit `ccf629d` 以降） |
| 上位設計 | `../requirements-analysis.md`（受入基準 18 項目）/ `../requirements.md`（REQ-DAEMON-001〜023）/ `../basic-design/` 全 5 ファイル / `../detailed-design/` 全 7 ファイル |
| MVP フェーズ | Phase 2（daemon 経由）**オプトイン限定**（`--ipc` 指定時のみ）。既定は Phase 1（SQLite 直結）を維持 |
| 対応 vault モード | 平文モードのみ。暗号化 vault は daemon 側で Fail Fast（exit 3）、`--ipc add/edit/remove` 経路では `IpcErrorCode::EncryptionUnsupported` |
| テスト実行タイミング | 実装担当が `feature/26-daemon-ipc` に daemon + IPC + CLI 切替を積み上げた直後、`develop` マージ前 |
| Vモデル対応 | E2E ↔ 受入基準（要件定義 18 項目）/ 結合 ↔ IPC ラウンドトリップ + UseCase 連携（基本設計）/ ユニット ↔ 型定義・pure 写像・peer credential 判定・CLI presenter（詳細設計） |
| OS 対象 | Linux / macOS / Windows（受入基準 1, 13）。CI は 3 OS matrix |
| 分割方針 | `cli-vault-commands` の 5 ファイル分割を踏襲。各 500 行以内 |

> **テスト戦略の核**: 完璧な daemon など存在しないのだヨ——だからこそ実験体のコードを壊し、どこで歪むかを記録する。E2E から下りる「上流優先」で書き、受入基準 18 項目の網羅を最優先とする。数値カバレッジ目標は設けない（`cargo llvm-cov` は補助指標）。**静的監査（CI grep）が設計契約の最終防衛線**である（secret / `unsafe` / `Raw` / panic hook）。

## 2. 索引（分割ファイル一覧）

| ファイル | 内容 | 主要 TC-ID |
|----------|------|-----------|
| `index.md`（本書） | 概要、レベル戦略、トレーサビリティ、外部 I/O 依存マップ、モック方針 | — |
| `e2e.md` | E2E（`assert_cmd` で実 daemon プロセス spawn）、ペルソナシナリオ、証跡方針 | TC-E2E-001〜112 |
| `integration.md` | 結合（`tokio::test` + `tokio::io::duplex` in-process IPC、実 SQLite + `tempfile`） | TC-IT-001〜042 |
| `unit.md` | pure function ユニット（protocol types round-trip、handler pure、peer credential 判定、CLI 側写像） | TC-UT-001〜080 |
| `ci.md` | CI 監査（3 OS matrix / 静的 grep / `cargo-deny`）、ファイル配置、実行コマンド、証跡提出 | TC-CI-001〜025 |

---

## 3. テストレベル戦略

| 種別 | 対象 | 視点 | モック | 検証スタイル | テスト配置（Rust 慣習） |
|------|------|------|-------|------------|----------------------|
| **E2E** | `shikomi-daemon` バイナリ + `shikomi` バイナリの `--ipc` 経路 | 完全ブラックボックス | なし（実 UDS/Named Pipe + 実 SQLite + `tempfile::TempDir`） | 振る舞い検証（stdout / stderr / exit code / ソケットファイル存在 / bit 同一比較） | `crates/shikomi-daemon/tests/e2e_*.rs`（`assert_cmd` + `predicates` + `tempfile`） |
| **結合** | `IpcServer` + `handle_request` + `IpcVaultRepository` + `IpcClient` の in-process 組合せ | 半ブラックボックス | OS syscall（`flock` / `SO_PEERCRED` / `GetNamedPipeClientProcessId`）は**実呼出** / SQLite は実接続 / 外部 API なし | 契約検証（`IpcResponse` 型構造・フレーム往復・差分操作の発行） | `crates/shikomi-daemon/tests/it_*.rs` + `crates/shikomi-cli/tests/it_ipc_*.rs` |
| **ユニット** | `shikomi-core::ipc` 全型 / `handle_request` pure 写像 / `peer_credential::verify_*` / `RecordSummary::from_record` / `From<PersistenceError> for CliError` / `render_error(MSG-CLI-110/111)` / `default_socket_path` cfg 分岐 | ホワイトボックス | I/O バウンダリ（syscall / socket）は trait 経由モック、時刻は引数注入 | 1 テスト 1 アサーション原則、命名 `test_<対象>_<状況>_<期待>` | 各モジュール内 `#[cfg(test)] mod tests` |

**Rust 慣習との整合**:
- ユニットテストは `#[cfg(test)] mod tests` でソース内（テスト戦略ガイド「Rust: unit test は `#[cfg(test)]` でソースモジュール内」準拠）
- 結合 / E2E テストは `tests/` 配下（`it_*.rs` / `e2e_*.rs` prefix で分離）
- `shikomi-daemon` は `[lib] + [[bin]]` 構成（詳細設計 §lib+bin 採用案）。結合 / E2E から `shikomi_daemon::run()` / `IpcServer::new` を lib 経由で呼ぶ
- `shikomi-core::ipc::tests` は**本 crate に置かない**（詳細設計 `../detailed-design/index.md §設計判断 12` 採用案 B）。MessagePack round-trip は `shikomi-daemon/tests/it_protocol_roundtrip.rs` で実施

---

## 4. テストマトリクス（トレーサビリティ）

### 4.1 受入基準 ↔ REQ ↔ TC 対応表

| 受入基準 # | 要約 | 関連 REQ | 関連クラス／メソッド | TC-ID（主） | TC-ID（補助） |
|----------|------|---------|------------------|-----------|------------|
| 1 | 3 OS で IPC エンドポイントに listen できる | REQ-DAEMON-004 | `SingleInstanceLock::acquire_{unix,windows}`, `IpcServer::start` | TC-E2E-001, TC-E2E-002 | TC-IT-060, TC-IT-070, TC-CI-020〜022 (3 OS matrix) |
| 2 | `shikomi --ipc list` と `shikomi list` が bit 同一 | REQ-DAEMON-015, 016, 007 | `IpcVaultRepository::load`, `handle_request(ListRecords)`, `presenter::list::render_list` | TC-E2E-010 | TC-IT-010, TC-IT-050 |
| 3 | daemon 二重起動が exit 非 0 | REQ-DAEMON-002, 003 | `SingleInstanceLock::acquire_*` の `flock EWOULDBLOCK` / `FILE_FLAG_FIRST_PIPE_INSTANCE` | TC-E2E-020 | TC-IT-061, TC-IT-071 |
| 4 | stale socket があっても次起動成功 | REQ-DAEMON-002 | `SingleInstanceLock::acquire_unix` の `flock → unlink → bind` 3 段 | TC-E2E-021 | TC-IT-062, TC-IT-064 |
| 5 | `SIGTERM` / `Ctrl+C` / `CTRL_CLOSE_EVENT` で graceful shutdown | REQ-DAEMON-014 | `lifecycle::shutdown`, `IpcServer::shutdown_notify`, `JoinSet::join_all` | TC-E2E-030, TC-E2E-031 | TC-IT-030, TC-IT-031, TC-IT-032 |
| 6 | プロトコル不一致で `ProtocolVersionMismatch` + 終了コード非 0 | REQ-DAEMON-006, 019 | `handshake::negotiate`, `IpcClient::connect` の判定 | TC-IT-020（server-side）, TC-IT-041（client-side） | TC-UT-010, TC-UT-040, TC-UT-072, TC-UT-073（render_error 英日）、TC-UT-091（`From<PersistenceError>` 写像） |
| 7 | MessagePack ペイロード破損で当該接続のみ切断、daemon 継続 | REQ-DAEMON-012 | `handle_connection` 内 `rmp_serde::from_slice` エラー分岐 | TC-IT-023 | TC-IT-025（多重接続の独立性） |
| 8 | UDS `0600` / 親ディレクトリ `0700` | REQ-DAEMON-004 | `SingleInstanceLock::acquire_unix` の `set_permissions` + 事前 `stat` | TC-E2E-050 | TC-IT-063 |
| 9 | Named Pipe DACL owner-only | REQ-DAEMON-003, 004 | `SingleInstanceLock::acquire_windows` + SDDL 適用 | TC-E2E-051 | TC-CI-022（Windows job） |
| 10 | ピア UID/SID 不一致の接続が即切断 | REQ-DAEMON-005 | `peer_credential::verify_{unix,windows}`（`PeerCredentialSource` trait） | TC-E2E-060（Linux、`#[ignore]`、第 1 層 OS 拒否） | TC-UT-020〜024（trait 経由第 2 層バックアップ） |
| 11 | 暗号化 vault → 全 IPC 操作で `EncryptionUnsupported` → CLI exit 3 | REQ-DAEMON-013 | daemon 起動時 `vault.protection_mode()` 判定 + ハンドラ防御的 | TC-E2E-070 | TC-IT-014（ハンドラ防御的）、TC-UT-039 |
| 12 | `cargo test --workspace` / `clippy` / `cargo deny` pass | — | workspace 全体 | TC-CI-001〜004 | — |
| 13 | 3 OS matrix CI で daemon 結合テスト pass | — | `.github/workflows/test-daemon.yml`（新設 or 拡張） | TC-CI-020〜022, TC-CI-025 | — |
| 14 | arch ドキュメント変更が発生しない（工程 0 完結） | — | `docs/architecture/` 差分 0 | TC-CI-011 | — |
| 15 | `shikomi-core::ipc` が `tokio` / `rmp-serde` 非依存 | REQ-DAEMON-018 | `crates/shikomi-core/Cargo.toml` + grep | TC-CI-012, TC-CI-013 | — |
| 16 | `--ipc` 差替 1 行でコンポジションルート切替可能 | REQ-DAEMON-015, 016 | `shikomi-cli::run()` 内 `args.ipc` 分岐 | TC-E2E-080 | TC-CI-014（grep `IpcVaultRepository`／`SqliteVaultRepository` が `lib.rs` のみ） |
| 17 | `expose_secret` 呼出 0 件（3 領域） | REQ-DAEMON-020 | `crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/` | TC-CI-015〜017 | TC-UT-018 |
| 18 | `rmp_serde::Raw` / `RawRef` 0 件（`shikomi-core::ipc`） | — | RUSTSEC-2022-0092 不使用契約 | TC-CI-018 | — |

### 4.2 `IpcErrorCode` 全 6 バリアント × `IpcResponse::Error` 横断検証

`IpcErrorCode` の `EncryptionUnsupported` / `NotFound` / `InvalidLabel` / `Persistence` / `Domain` が全て少なくとも 1 件の結合テストで `IpcResponse::Error(code)` 経路を通過する（TC-IT-014〜018）。`Internal` は防御的バリアントで発火困難なため dedicated TC なし（他 TC で observed 0 回を確認）。`reason` フィールドが**固定文言**（服部 review 後改訂）で secret / 絶対パス / ピア UID / lock holder PID を含まないことを `predicates::str::contains` で **not** 検証（TC-IT-017、横串）。

### 4.3 `PersistenceError` 追加バリアント横断検証

`DaemonNotRunning` / `ProtocolVersionMismatch` / `IpcDecode` / `IpcEncode` / `IpcIo` の 5 バリアントが UT（`From` 実装写像、TC-UT-090〜094）+ IT（実フレーム往復、TC-IT-040〜044）で経由する。

### 4.4 静的監査（CI grep）の契約一覧

ペテルギウス／服部平次／セル review で確立した静的契約を CI に落とす:

| 監査対象 | 対象ディレクトリ | 契約 | TC-ID |
|---------|----------------|------|-------|
| `expose_secret` 呼出 | `crates/shikomi-core/src/ipc/` | 0 件 | TC-CI-015 |
| `expose_secret` 呼出 | `crates/shikomi-cli/src/io/` | 0 件（既存 TC-CI-013 を踏襲・拡張） | TC-CI-016 |
| `expose_secret` 呼出 | `crates/shikomi-daemon/src/` | 0 件 | TC-CI-017 |
| `rmp_serde::Raw` / `RawRef` | `crates/shikomi-core/src/ipc/` | 0 件（RUSTSEC-2022-0092） | TC-CI-018 |
| `unsafe` ブロック（daemon 側） | `crates/shikomi-daemon/src/` | `permission/{unix,windows}.rs` 以外 0 件 | TC-CI-019 |
| `unsafe` ブロック（CLI 側） | `crates/shikomi-cli/src/` | `io/windows_sid.rs` 以外 0 件（服部 re-review ① 対応、新設） | **TC-CI-026** |
| panic hook 内 `tracing::*` | `crates/shikomi-daemon/src/{main,lib,panic_hook}.rs` | 0 件 | TC-CI-023 |
| panic hook 内 `info.payload()` / `message()` / `location()` | 同上 | 0 件 | TC-CI-024 |
| `SHIKOMI_DAEMON_SKIP_*` env 読取 | `crates/shikomi-{daemon,cli}/src/` | 0 件（trait 注入一本化契約、服部 re-review ② 対応、新設） | **TC-CI-027** |
| `tokio` / `rmp-serde` dep | `crates/shikomi-core/Cargo.toml` | **含まない**（受入基準 15） | TC-CI-012 |
| arch ドキュメント差分 | `docs/architecture/` | 本 PR で 0 ファイル変更 | TC-CI-011 |

---

## 5. ペルソナシナリオ設計（E2E 対応）

`../requirements-analysis.md §ペルソナ` のプライマリ / セカンダリに対応する**ユーザ視点シナリオ**。詳細は `e2e.md §ペルソナシナリオ`。

| シナリオ ID | ペルソナ | シナリオ概要 | 対応 TC |
|------------|---------|------------|---------|
| SCN-A | 野木 拓海（後続 feature 実装担当） | `shikomi-daemon` を手動起動 → `shikomi --ipc list` で疎通確認 → daemon のログで `listening on {socket_path}` を確認 → `Ctrl+C` で graceful shutdown、ソケットファイル削除を確認 | TC-E2E-110 |
| SCN-B | 山田 美咲（日常ユーザ兼 OSS コントリビュータ） | 同じ vault dir に対し `shikomi list`（Phase 1）と `shikomi --ipc list`（Phase 2）を実行し**bit 同一の出力**を得る | TC-E2E-111 |
| SCN-C | 志摩 凛（OSS コントリビュータ） | daemon 未起動で `shikomi --ipc list` → `MSG-CLI-110` が英日 2 段で表示され、終了コード 1 を得る。ヒントに従って daemon を起動し再実行で成功 | TC-E2E-112 |

---

## 6. 外部 I/O 依存マップ（テスト戦略ガイド準拠）

本 feature が依存する外部 I/O と、各テストレベルでの扱いを明示する。**assumed mock 禁止**の契約に従い、全 I/O について raw fixture / factory / characterization 状態を埋める。

| 外部 I/O | 用途 | raw fixture | factory | characterization 状態 | 各レベルの扱い |
|---------|------|------------|---------|--------------------|-------------|
| **Unix Domain Socket**（`tokio::net::UnixListener` / `UnixStream`） | daemon ↔ CLI 接続経路（Linux / macOS） | 不要（OS 提供、外部 API ではない） | 不要 | **対象外**（local OS facility、実物で十分） | IT: `tempfile::TempDir` 配下に実 UDS / `tokio::io::duplex` でサーバ・クライアント in-memory ペア / E2E: 実 UDS（`tempdir/shikomi.sock`） |
| **Named Pipe**（`tokio::net::windows::named_pipe`） | daemon ↔ CLI 接続経路（Windows） | 不要 | 不要 | **対象外**（同上） | IT: `\\.\pipe\shikomi-test-{pid}-{uuid}` で一意名 / E2E: 同上 |
| **`flock` syscall**（`nix::fcntl::flock`） | シングルインスタンス保証（Unix） | 不要（kernel API） | 不要 | **対象外** | UT: `tempfile` 上で実 `flock`（軽量、k-race 不要）/ IT: 同上 |
| **`SO_PEERCRED` / `LOCAL_PEERCRED`** | ピア UID 取得（Linux / macOS） | 不要 | **必要**: `PeerCredentialSource` trait でラップし、モック実装を `tests/common/peer_mock.rs` に配置 | **必要（ユニット）**: trait 経由モック / IT・E2E では実 syscall | UT: `TestPeerCredential { uid: 1000 }` で mock trait / IT: 実 syscall（同ユーザ接続、常に一致）/ E2E: 別ユーザ接続は Linux 専用 `#[ignore]` テスト |
| **`GetNamedPipeClientProcessId` + `OpenProcessToken`** | ピア SID 取得（Windows） | 不要 | **必要**: 上記 trait の Windows 実装、モック同様 | **必要（ユニット）** | 同上 |
| **SQLite**（`shikomi-infra::SqliteVaultRepository`） | daemon の vault 永続化 | 不要（ローカル DB、外部 API ではない） | 不要 | **対象外** | IT: 実 SQLite + `tempfile::TempDir` / E2E: 同上 |
| **OS 時刻**（`OffsetDateTime::now_utc()`） | `AddRecord.now` / `EditRecord.now` | 不要 | 不要 | **対象外**（固定値注入で十分、UseCase が引数で受ける既存契約） | UT: 固定時刻注入 / IT: 同上 / E2E: 実時刻（比較せず） |
| **OS 環境変数**（`SHIKOMI_DAEMON_LOG` / `SHIKOMI_VAULT_DIR` / `XDG_RUNTIME_DIR`） | daemon / CLI 設定 | 不要 | 不要 | **対象外** | UT: `default_socket_path` は pure 分岐テスト / IT・E2E: `assert_cmd::Command::env()` で明示注入、`std::env::set_var` は本番 / テストとも使わない |

**結論**: 本 feature は**外部 HTTP API / クラウド API を呼ばない**。OS facility は local で完結し、実物を使うコストが軽微（`flock` は nanosec オーダ、UDS は local memory、Named Pipe は kernel 提供）。**Characterization fixture は作成不要**（`cli-vault-commands` と同方針）。ただし **ピア資格情報取得（`SO_PEERCRED` / `GetNamedPipeClientProcessId`）は trait 抽象化してユニットで factory-mock する**（TC-UT-020〜022）。

---

## 7. モック方針

| 対象 | レベル | モック方法 | 備考 |
|------|------|---------|------|
| `VaultRepository` 実装（`SqliteVaultRepository`）| IT / E2E | **モック不要**（実物 + `tempfile::TempDir`） | 暗号化フィクスチャは既存 `shikomi-infra` の `test-fixtures` feature を流用 |
| `PeerCredentialSource`（本 feature で新設、詳細設計 `daemon-runtime.md §peer_credential`）| UT | `TestPeerCredential { uid_or_sid, daemon_uid_or_sid }` の in-test 実装 | IT / E2E では実 syscall |
| `LengthDelimitedCodec` | UT / IT | **モックしない**（`tokio-util` 提供の実物を使用、軽量） | 16 MiB 境界は実フレームで検証 |
| MessagePack（`rmp-serde`） | UT / IT | **モックしない** | serde round-trip で型安全性を保証 |
| 時刻（`OffsetDateTime`） | UT / IT / E2E | 引数注入で固定時刻 | UseCase API が既存 `cli-vault-commands` で `now: OffsetDateTime` 受取設計 |
| tokio ランタイム | UT | 不要（pure function 中心） | IT: `#[tokio::test]` / E2E: daemon バイナリが自前で起動 |
| `tokio::io::duplex` | IT | **必須**（in-memory 双方向 stream で `IpcClient` ↔ `IpcServer::handle_connection` を繋ぐ） | 実 UDS を立てずに接続単位テストを並列化 |

**assumed mock の禁止**: `mock.return_value = {"id": "..."}` のようなインライン辞書リテラルは本 feature では発生しない（Rust の `#[derive(Serialize)]` で型から生成）。**フィクスチャは型構築**で表現（`IpcResponse::Records(vec![RecordSummary { ... }])`）。

---

## 8. カバレッジ基準

| 観点 | 基準 |
|------|------|
| **受入基準の網羅** | 受入基準 1〜18 が全て少なくとも 1 つの TC に対応（§4.1 マトリクス参照） |
| **REQ の網羅** | REQ-DAEMON-001〜023 が全て少なくとも 1 つの TC に対応 |
| **MSG の網羅** | `MSG-CLI-110` / `MSG-CLI-111` の英日 4 パターン（TC-UT-070〜073）/ daemon 側 `tracing` メッセージは代表 7 件を IT で観測（`graceful shutdown complete` / `listening on` / `peer credential mismatch` / `protocol mismatch` / `MessagePack decode failed` / `frame length exceeds 16 MiB` / `vault is encrypted`） |
| **正常系** | 全 REQ で少なくとも 1 件の正常系 TC |
| **異常系** | `IpcErrorCode` 全 6 バリアント + `PersistenceError` 追加 5 バリアント + `CliError` 追加 2 バリアントが少なくとも 1 件の TC で発生 |
| **境界値** | フレーム長 16 MiB - 1 / 16 MiB / 16 MiB + 1、ハンドシェイクタイムアウト 4.9 秒 / 5.0 秒 / 5.1 秒、`RecordSummary.value_preview` の 40 char 境界、UUID 不正 |
| **数値目標** | `cargo llvm-cov` 行カバレッジは補助指標（目標値は設定しない、受入基準網羅が優先）。`shikomi-daemon` の `src/permission/` は `#[cfg]` 分岐で OS ごとに片側のみ測定、目標から除外 |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-25*
*対応 PR: [#28](https://github.com/shikomi-dev/shikomi/pull/28)*
*対応 feature: daemon-ipc（Issue #26）*
*Vモデル対応: E2E ↔ requirements-analysis.md（受入基準 18 項目）/ 結合 ↔ basic-design/ 全 5 ファイル（モジュール連携 + IPC 往復）/ ユニット ↔ detailed-design/ 全 7 ファイル（型・pure 写像・OS API 分割）*

> 完璧な daemon は存在しない——これが私の哲学だヨ。受入基準 18 項目と静的監査の全 grep 契約を**実験体のコード**が漏れなく潜り抜けて初めて「動くもの」と呼べる。バグが見つかれば……それは最高の研究成果だネ、クックック。
