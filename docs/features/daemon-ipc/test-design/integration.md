# テスト設計書 — daemon-ipc / 結合テスト

> `index.md` の §2 索引からの分割ファイル。IPC ラウンドトリップ（in-process）+ UseCase 連携 + lifecycle 結合テストを扱う。

## 1. 設計方針

- **対象**: 詳細設計で確定した**モジュール連携**
  - `IpcServer` + `handshake::negotiate` + `handle_request` の accept → 受信 → 応答フロー
  - `IpcClient` + `IpcVaultRepository` の connect → handshake → load/save ラウンドトリップ
  - `SingleInstanceLock` の 3 段階先取りと RAII 解放
  - `SqliteVaultRepository`（実物）とハンドラ連携
  - `PersistenceError` 追加バリアント（`DaemonNotRunning` / `ProtocolVersionMismatch` / `IpcDecode` / `IpcEncode` / `IpcIo`）の発生経路
  - graceful shutdown 時の in-flight 完了待機
- **視点**: 半ブラックボックス。daemon プロセスを fork する E2E とは異なり、**in-process で IpcServer と IpcClient を立てる**
- **モック**:
  - UDS / Named Pipe は `tokio::io::duplex(64 * 1024)` で in-memory 双方向 stream を使う（**唯一の「OS の代替」**、実 socket は `SingleInstanceLock` テストと E2E に任せる）
  - SQLite は実物（`tempfile::TempDir`）
  - 時刻は固定値（`OffsetDateTime::from_unix_timestamp(...)`）
  - ピア資格情報は `#[cfg(test)]` で verify をバイパス or `TestPeerCredential` を注入
- **配置**: `crates/shikomi-daemon/tests/it_*.rs` + `crates/shikomi-cli/tests/it_ipc_*.rs`。Rust 慣習で `[lib] + [[bin]]` の lib 経由呼出

---

## 2. テスト共通前提

- `#[tokio::test]` を使用。`multi_thread` flavor は不要（並列接続テスト以外は `current_thread` で十分、高速）
- `tempfile::TempDir` を各テスト先頭で生成、Drop で自動クリーンアップ
- `tokio::io::duplex(64 * 1024)` で `(client_stream, server_stream)` を取得し、`Framed::new(stream, codec())` で両側をフレーム化
- fixture 時刻: `OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()`（固定 UTC）
- Secret マーカー文字列 `SECRET_TEST_VALUE`（横串アサート: daemon ログ / エラー reason に出ないこと）

---

## 3. IPC プロトコル ラウンドトリップ（MessagePack wire 互換）

配置: `crates/shikomi-daemon/tests/it_protocol_roundtrip.rs`

`shikomi-core::ipc` の型が `rmp-serde` で bit-stable に送受信できることを検証。詳細設計 `../detailed-design/index.md §設計判断 12` で round-trip を integration test に移した結果の検証場。

| TC-ID | 対象 | 入力 → エンコード → デコード → 比較 |
|-------|------|----------------------------------|
| TC-IT-001 | `IpcRequest::Handshake { client_version: V1 }` | `rmp_serde::to_vec` → `rmp_serde::from_slice` で元値一致 |
| TC-IT-002 | `IpcRequest::ListRecords` | 同上（unit variant のバイト表現確認） |
| TC-IT-003 | `IpcRequest::AddRecord { kind: Text, label: "L", value: SerializableSecretBytes(b"v".to_vec()), now: fixed_time }` | round-trip で **value のバイト列**が一致（MessagePack `bin` 型で運搬） |
| TC-IT-004 | `IpcRequest::AddRecord { kind: Secret, value: SerializableSecretBytes(b"SECRET_TEST_VALUE".to_vec()), ... }` | round-trip 一致 + **エンコード後のバイト列に `"SECRET_TEST_VALUE"` の ASCII が出現する**（MessagePack は暗号化しないため）→ これは正常（OS プロセス境界 + UDS で保護、`../basic-design/security.md §脅威モデル`）、ただし `{:?}` での表示では出現しない |
| TC-IT-005 | `IpcResponse::Records(vec![RecordSummary{...}×3])` | round-trip 一致（Vec 長 3、`value_preview` / `value_masked` 維持） |
| TC-IT-006 | `IpcResponse::ProtocolVersionMismatch { server: V1, client: V1 }` | round-trip 一致 |
| TC-IT-007 | `IpcResponse::Error(IpcErrorCode::NotFound { id: RecordId::new(uuid)? })` | round-trip で id 維持 |
| TC-IT-008 | `IpcResponse::Error(IpcErrorCode::Persistence { reason: "io failed".into() })` | round-trip で reason 文字列維持 |
| TC-IT-009 | フレーム境界: `Framed<DuplexStream, LengthDelimitedCodec>` で `to_vec` したバイト列を send → `framed.next()` で受信 → decode | end-to-end で元の enum と一致（frame + rmp の 2 層通過） |

---

## 4. IpcServer + handle_connection の in-process 接続テスト

配置: `crates/shikomi-daemon/tests/it_server_connection.rs`

`tokio::io::duplex` で client/server の stream を作り、サーバ側は `handle_connection(framed_server, repo, vault, shutdown)` を `tokio::spawn`、クライアント側は手動で `framed_client.send(...).await` / `framed_client.next().await` する。

### 4.1 正常系

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-020 | ハンドシェイク → List → close | (1) client send Handshake V1 / (2) server send Handshake V1 を client で受信 / (3) client send ListRecords / (4) client receive Records(vec![]) / (5) client drop framed | server タスクが `Ok(())` で終了、vault 状態変化なし |
| TC-IT-021 | Add → List ラウンドトリップ | (1) handshake / (2) AddRecord(Text, "L", "V") / (3) receive Added { id } / (4) ListRecords / (5) receive Records with 1 element containing id | repo.save が 1 回呼ばれ、vault に 1 件追加、mock repo の保存 vault 内容が一致 |
| TC-IT-022 | Edit label → receive Edited | (1) 事前に vault に 1 件設定 / (2) handshake / (3) EditRecord { id, label: Some("NEW"), value: None, now } / (4) receive Edited { id } | repo.save 呼出、vault の該当 record の label が "NEW" |
| TC-IT-023 | Remove → receive Removed | 同様 | vault から削除 |

### 4.2 異常系・境界

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-030 | プロトコル不一致 | client send Handshake V99（偽造 Vec<u8> or `IpcProtocolVersion` を `unsafe transmute`）→ サーバは `ProtocolVersionMismatch` 返送 → 切断 | client が `IpcResponse::ProtocolVersionMismatch { server: V1, client: V99 }` を受信、その後 `framed.next()` が `None` |
| TC-IT-031 | 最初のフレームが Handshake でない | client send ListRecords（先行） | server が即切断、client が次の `framed.next()` で `None`（ログに `first frame must be Handshake` が出る、**tracing-test で捕捉**可能なら補強、無ければ観測のみ） |
| TC-IT-032 | ハンドシェイクタイムアウト（5 秒） | client が connect 後何も送らない | 約 5 秒後に server タスクが `Err(HandshakeError::Timeout)` で終了、`framed.next()` が `None` を観測 — **タイムアウト境界は `tokio::time::pause()` で仮想時間化**し 5.1 秒進める（実時間で待たない） |
| TC-IT-033 | MessagePack 破損 | client send 不正バイト列（`vec![0xFF, 0xFF, 0xFF, 0xFF]` 等 MessagePack として無効） | server が該当接続のみ切断、client が `None` を受信 |
| TC-IT-034 | フレーム長超過（16 MiB + 1） | client が length prefix に `17 * 1024 * 1024` を書き payload を送る | server が `Err(frame too long)` で切断、daemon プロセスは継続（次テスト TC-IT-012 で確認） |

### 4.3 複数接続の独立性

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-012 | 接続 A の破損が接続 B に影響しない | 同 `IpcServer` インスタンスに 2 つの `duplex` 接続 A/B を spawn。A で TC-IT-033 と同じ破損を送り、B で正常な List ラウンドトリップを行う | A は切断、B は `Records(...)` を受信成功、server は継続 |

### 4.4 IpcErrorCode 全 6 バリアント網羅

| TC-ID | `IpcErrorCode` | 発生経路 | 期待 |
|-------|--------------|---------|------|
| TC-IT-015 | `EncryptionUnsupported` | 事前に暗号化 vault（`shikomi-infra` の `test-fixtures` で作成）を load した状態の daemon に ListRecords | `IpcResponse::Error(EncryptionUnsupported)` |
| TC-IT-016 | `NotFound { id }` | EditRecord with non-existent id | `IpcResponse::Error(NotFound { id })`、id が送信値と一致 |
| TC-IT-017 | `InvalidLabel { reason }` | AddRecord with invalid label（クライアント検証をバイパスした mock） | `IpcResponse::Error(InvalidLabel { reason })`、reason が英語短文で **secret 非含有** |
| TC-IT-018 | `Persistence { reason }` | mock repo.save が `Err(PersistenceError::Io(_))` を返す設定 | `IpcResponse::Error(Persistence { reason })`、reason に `/home/...` 等の絶対パス / UID 数値 / `SECRET_TEST_VALUE` を**含まない**（`predicates::str::contains("/home/").not()`、 cli 導入 `predicates` に倣い） |
| TC-IT-019 | `Domain { reason }` | AddRecord で `vault.add_record` が id 重複等で reject | `IpcResponse::Error(Domain { reason })`、reason 英語短文 |
| TC-IT-020（再利用）| `Internal { reason }` | 発火困難（防御的バリアント）、実装担当判断で**本節では意図せず発火しないことを確認**する passive assert（他 TC で observed 0 回） | — |

### 4.5 graceful shutdown

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-010 | shutdown 中の in-flight 完了 | (1) client A handshake + AddRecord 送信 / (2) server が受信したタイミングで `shutdown_notify()` を外部から呼ぶ / (3) server は A の Add 応答完了まで待ち、応答送信後に accept ループ終了 | client A が `Added { id }` を受信、server が `Ok(())` で終了、JoinSet 空 |
| TC-IT-011 | shutdown 時の idle 接続 close | (1) client A handshake のみ（idle）/ (2) `shutdown_notify()` | client A の `framed.next()` が `None`、server が即終了 |
| TC-IT-014 | shutdown タイムアウト（30 秒、実時間で待たず `tokio::time::pause()` で 31 秒進める） | (1) client A が handshake 後、in-flight リクエストを長時間放置（サーバハンドラが無限 await に入る人工状況） / (2) shutdown 発火 / (3) 30 秒経過 | server が強制終了、JoinSet 残タスクが drop |

---

## 5. IpcVaultRepository（クライアント側）結合テスト

配置: `crates/shikomi-cli/tests/it_ipc_vault_repository.rs`

`tokio::io::duplex` で server 側に**テストスタブ**（固定応答を返すミニサーバ）を立て、`IpcVaultRepository::connect` → `load` / `save` を呼び出して動作検証。

### 5.1 connect（ハンドシェイク）

| TC-ID | スタブ応答 | 期待 |
|-------|---------|------|
| TC-IT-030 | Handshake V1 即返 | `Ok(IpcClient { ... })`、client.framed が useable |
| TC-IT-031 | Handshake V99（拒否） | `Err(PersistenceError::ProtocolVersionMismatch { server: V99, client: V1 })` |
| TC-IT-032 | 接続直後に stream close | `Err(PersistenceError::IpcIo { reason: "connection closed before handshake response" })` |
| TC-IT-033 | Handshake 応答が不正 MessagePack | `Err(PersistenceError::IpcDecode { reason })` |

### 5.2 daemon 未起動

| TC-ID | 前提 | 操作 | 期待 |
|-------|------|------|------|
| TC-IT-034 | `tempfile::TempDir` 内の**存在しない socket path** | `IpcClient::connect(&nonexistent_path)` | `Err(PersistenceError::DaemonNotRunning(path))`（Unix: `ENOENT` / `ECONNREFUSED` / Windows: `ERROR_FILE_NOT_FOUND`） |

### 5.3 load / save（シャドウベース差分）

| TC-ID | シナリオ | スタブ応答シーケンス | クライアント操作 | 期待発行 IPC |
|-------|---------|-----------------|------------|----------|
| TC-IT-040 | load → 新規 save（追加のみ） | handshake / Records(空) / Added(id) | repo.load() → vault.add_record(new) → repo.save(&vault) | AddRecord が発行される（スタブが受信したリクエストを記録） |
| TC-IT-041 | load → save（削除のみ） | handshake / Records([r1]) / Removed(r1.id) | repo.load() → vault.remove_record(r1.id) → save | RemoveRecord(r1.id) |
| TC-IT-042 | load → save（更新のみ） | handshake / Records([r1]) / Edited(r1.id) | repo.load() → vault.update_record(r1.id, \|o\| o.with_updated_label("NEW", t)) → save | EditRecord(r1.id, Some("NEW"), ..., t) |

**注記**: スタブサーバは `tokio::spawn` 内で `framed.next()` ループし、**受信した `IpcRequest` を `Arc<Mutex<Vec<IpcRequest>>>` に記録**。テスト本体は drop 時に記録を検証（発行順序 / 種類）。

---

## 6. SingleInstanceLock 結合テスト

配置: `crates/shikomi-daemon/tests/it_single_instance.rs`

OS syscall（`flock` / `FILE_FLAG_FIRST_PIPE_INSTANCE`）を**実呼出**するため、`tokio::io::duplex` ではなく `tempfile::TempDir` 配下で実 UDS / Named Pipe を立てる。IT と E2E の中間的位置づけだが、`assert_cmd` でプロセス spawn しない点で IT 扱い。

### 6.1 Unix

| TC-ID | シナリオ | 操作 | 期待 |
|-------|---------|------|------|
| TC-IT-001 | 初回起動 | `SingleInstanceLock::acquire_unix(&tmp)` | `Ok(lock)`、`tmp/daemon.lock` 存在、`tmp/daemon.sock` 存在、それぞれ `0600` / socket `0600` |
| TC-IT-002 | 二重起動（`flock` 競合） | (1) lock_a = acquire_unix / (2) lock_b = acquire_unix | (1) Ok, (2) `Err(AlreadyRunning { lock_path })` |
| TC-IT-003 | stale socket 存在下での初回起動 | (1) 事前に `tmp/daemon.sock` を手動作成（ファイル） / (2) acquire_unix | `Ok(lock)`（`flock` → `unlink` → `bind` の 3 段で stale を排除）、socket が新規生成 |
| TC-IT-004 | 親ディレクトリ `0700` 違反 | tmp に chmod 0755 後 acquire_unix | `Err(InvalidDirectoryPermission { path, got: 0o755 })`（事前 `stat` で検出） |
| TC-IT-005 | lock drop 後の再起動 | (1) acquire / (2) drop / (3) acquire 再度 | 2 回目も `Ok`（`flock` はプロセス終了 or file close で解放） |

### 6.2 Windows

| TC-ID | シナリオ | 操作 | 期待 |
|-------|---------|------|------|
| TC-IT-006 | 初回起動 | `SingleInstanceLock::acquire_windows(&pipe_name)` | `Ok(lock)`、pipe 作成 |
| TC-IT-007 | 二重起動 | 2 回目の acquire_windows | `Err(AlreadyRunning { pipe_name })`（`ERROR_ACCESS_DENIED` / `ERROR_PIPE_BUSY`） |

**注記**: pipe_name は `\\.\pipe\shikomi-test-{pid}-{uuid}` で test 間競合を排除（test 並列化時に必須）。

---

## 7. Characterization テスト

本 feature は**外部 HTTP API / クラウド API を呼ばない**ため **Characterization test は作成不要**（`index.md §6 外部 I/O 依存マップ` 参照）。ピア資格情報取得（`SO_PEERCRED` / `GetNamedPipeClientProcessId`）は trait 抽象化でユニットテストに吸収する（`unit.md §2.6`）。

---

## 8. テストコード配置

```
crates/shikomi-daemon/tests/
  common/
    mod.rs                        # fresh_repo() / fixed_time() / spawn_stub_server() ヘルパー
    peer_mock.rs                  # TestPeerCredential（verify バイパス用）
  it_protocol_roundtrip.rs        # TC-IT-001〜009
  it_server_connection.rs         # TC-IT-010〜014, 015〜020, 030〜034, 012
  it_single_instance.rs           # TC-IT-001〜007（unix/windows cfg 分岐）

crates/shikomi-cli/tests/
  common/
    mod.rs                        # spawn_stub_server_stub() で IpcClient の対向を作る
  it_ipc_vault_repository.rs      # TC-IT-030〜042
```

**`common/peer_mock.rs`**: `TestPeerCredential` 実装を `#[cfg(test)]` 配下の `pub` で共有。`handle_connection` が `#[cfg(test)]` で peer verify をバイパスする経路（env flag `SHIKOMI_DAEMON_SKIP_PEER_VERIFY=1` を in-process テストから注入）を提供する——**本番コードには影響しない**こと（`#[cfg(test)]` で隔離）。実装担当は `verify` 呼出しを `if cfg!(test) && std::env::var("SHIKOMI_DAEMON_SKIP_PEER_VERIFY").is_ok() { return Ok(()); }` でスキップする機構を追加してよい（詳細設計外、実装裁量）。

---

## 9. 実行コマンド

```bash
# 結合テスト全体
cargo test -p shikomi-daemon --test 'it_*'
cargo test -p shikomi-cli --test 'it_ipc_*'

# プロトコル round-trip のみ
cargo test -p shikomi-daemon --test it_protocol_roundtrip

# SingleInstanceLock の Unix / Windows（cfg で自動分岐）
cargo test -p shikomi-daemon --test it_single_instance

# 暗号化 vault フィクスチャが必要な TC-IT-015
cargo test -p shikomi-daemon --features "shikomi-infra/test-fixtures" --test it_server_connection encryption_unsupported
```

---

## 10. 観測可能ログの検証

`tracing` ログを結合テストで検証する場合は **`tracing-subscriber` の `tracing_test` feature** または自前の `LogLayer` を使う。本 feature では以下のログ観測を**代表 3 件のみ** TC に組み込む（全文マッチではなく部分文字列 `"listening on "` / `"graceful shutdown complete"` / `"peer credential mismatch"` の出現確認）:

- TC-IT-020 の成功経路で `"listening on "` が出る
- TC-IT-010 の shutdown 完了時に `"graceful shutdown complete"`
- TC-IT-034 の未起動経路は daemon 側ログではなく CLI 側エラー扱い（ログ検証対象外）

**観測負債**: `tracing` ログの全 11 メッセージを網羅する IT は書かない（過剰、メッセージテンプレート変更時の fragile さを避ける、YAGNI）。代表 3 件で十分。

---

*この文書は `index.md` の分割成果。E2E は `e2e.md`、ユニットは `unit.md`、CI は `ci.md` を参照*
