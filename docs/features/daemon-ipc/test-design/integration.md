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

**採番方針（ペテルギウス review 指摘 ① 対応、2026-04-25 renumber）**: §3 round-trip（001〜009）、§4 server（010〜032）、§5 client（040〜052）、§6 single-instance（060〜071）で**範囲非重複**に再設計。

### 4.1 正常系

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-010 | ハンドシェイク → List → close | (1) client send Handshake V1 / (2) server send Handshake V1 を client で受信 / (3) client send ListRecords / (4) client receive Records(vec![]) / (5) client drop framed | server タスクが `Ok(())` で終了、vault 状態変化なし |
| TC-IT-011 | Add → List ラウンドトリップ | (1) handshake / (2) AddRecord(Text, "L", "V") / (3) receive Added { id } / (4) ListRecords / (5) receive Records with 1 element containing id | repo.save が 1 回呼ばれ、vault に 1 件追加、mock repo の保存 vault 内容が一致 |
| TC-IT-012 | Edit label → receive Edited | (1) 事前に vault に 1 件設定 / (2) handshake / (3) EditRecord { id, label: Some("NEW"), value: None, now } / (4) receive Edited { id } | repo.save 呼出、vault の該当 record の label が "NEW" |
| TC-IT-013 | Remove → receive Removed | 同様 | vault から削除 |

### 4.2 IpcErrorCode 全 6 バリアント網羅

| TC-ID | `IpcErrorCode` | 発生経路 | 期待 |
|-------|--------------|---------|------|
| TC-IT-014 | `EncryptionUnsupported` | 事前に暗号化 vault（`shikomi-infra` の `test-fixtures` で作成）を load した状態の daemon に ListRecords | `IpcResponse::Error(EncryptionUnsupported)` |
| TC-IT-015 | `NotFound { id }` | EditRecord with non-existent id | `IpcResponse::Error(NotFound { id })`、id が送信値と一致 |
| TC-IT-016 | `InvalidLabel { reason }` | AddRecord with invalid label（クライアント検証をバイパスした mock） | `IpcResponse::Error(InvalidLabel { reason })`、reason が **固定文言**（詳細設計 `../detailed-design/daemon-runtime.md §Result→IpcErrorCode 写像` 服部 review 後改訂、`"invalid label"` 等）、secret / 絶対パス / UID 非含有 |
| TC-IT-017 | `Persistence { reason }` | mock repo.save が `Err(PersistenceError::Io(_))` を返す設定 | `IpcResponse::Error(Persistence { reason })`、reason が **固定文言**（`"persistence error"` 等）、`/home/...` 等の絶対パス / `SECRET_TEST_VALUE` / lock holder PID を**含まない**（`predicates::str::contains("/home/").not()` + `contains("pid").not()`） |
| TC-IT-018 | `Domain { reason }` | AddRecord で `vault.add_record` が id 重複等で reject | `IpcResponse::Error(Domain { reason })`、reason 固定文言 |
| （`Internal` はpassive）| — | 発火困難な防御的バリアント | 他 TC で observed 0 回を確認（dedicated TC なし） |

### 4.3 異常系・境界

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-020 | プロトコル不一致（server-side 判定） | client が事前に用意した MessagePack バイト列（`{"handshake": {"client_version": "v99"}}` 相当）を `framed.send` → サーバは `ProtocolVersionMismatch` 返送 → 切断 | client が `IpcResponse::ProtocolVersionMismatch { server: V1, client: V99 }` を受信、その後 `framed.next()` が `None`。 **前 PR の TC-E2E-040 からの再分類先（ペテルギウス指摘 ② 対応）** |
| TC-IT-021 | 最初のフレームが Handshake でない | client send ListRecords（先行） | server が即切断、client が次の `framed.next()` で `None`（ログに `first frame must be Handshake` が出る、**tracing-test で捕捉**可能なら補強、無ければ観測のみ） |
| TC-IT-022 | ハンドシェイクタイムアウト（5 秒） | client が connect 後何も送らない | 約 5 秒後に server タスクが `Err(HandshakeError::Timeout)` で終了、`framed.next()` が `None` を観測 — **タイムアウト境界は `tokio::time::pause()` で仮想時間化**し 5.1 秒進める（実時間で待たない） |
| TC-IT-023 | MessagePack 破損 | client send 不正バイト列（`vec![0xFF, 0xFF, 0xFF, 0xFF]` 等 MessagePack として無効） | server が該当接続のみ切断、client が `None` を受信 |
| TC-IT-024 | フレーム長超過（16 MiB + 1） | client が length prefix に `17 * 1024 * 1024` を書き payload を送る | server が `Err(frame too long)` で切断、daemon プロセスは継続（次テスト TC-IT-025 で確認） |

### 4.4 複数接続の独立性

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-025 | 接続 A の破損が接続 B に影響しない | 同 `IpcServer` インスタンスに 2 つの `duplex` 接続 A/B を spawn。A で TC-IT-023 と同じ破損を送り、B で正常な List ラウンドトリップを行う | A は切断、B は `Records(...)` を受信成功、server は継続 |

### 4.5 graceful shutdown

| TC-ID | シナリオ | 操作 | 期待結果 |
|-------|---------|------|--------|
| TC-IT-030 | shutdown 中の in-flight 完了 | (1) client A handshake + AddRecord 送信 / (2) server が受信したタイミングで `shutdown_notify()` を外部から呼ぶ / (3) server は A の Add 応答完了まで待ち、応答送信後に accept ループ終了 | client A が `Added { id }` を受信、server が `Ok(())` で終了、JoinSet 空 |
| TC-IT-031 | shutdown 時の idle 接続 close | (1) client A handshake のみ（idle）/ (2) `shutdown_notify()` | client A の `framed.next()` が `None`、server が即終了 |
| TC-IT-032 | shutdown タイムアウト（30 秒、実時間で待たず `tokio::time::pause()` で 31 秒進める） | (1) client A が handshake 後、in-flight リクエストを長時間放置（サーバハンドラが無限 await に入る人工状況） / (2) shutdown 発火 / (3) 30 秒経過 | server が強制終了、JoinSet 残タスクが drop |

---

## 5. IpcVaultRepository（クライアント側）結合テスト

配置: `crates/shikomi-cli/tests/it_ipc_vault_repository.rs`

`tokio::io::duplex` で server 側に**テストスタブ**（固定応答を返すミニサーバ）を立て、`IpcVaultRepository::connect` → `load` / `save` を呼び出して動作検証。

### 5.1 connect（ハンドシェイク）

| TC-ID | スタブ応答 | 期待 |
|-------|---------|------|
| TC-IT-040 | Handshake V1 即返 | `Ok(IpcClient { ... })`、client.framed が useable |
| TC-IT-041 | Handshake V2（将来 V2 daemon vs V1 client の相当、拒否） | `Err(PersistenceError::ProtocolVersionMismatch { server: V2, client: V1 })`。 **前 PR の TC-E2E-041 からの再分類先（ペテルギウス指摘 ② 対応、偽 daemon スタブは IT 境界）** |
| TC-IT-042 | 接続直後に stream close | `Err(PersistenceError::IpcIo { reason: "connection closed before handshake response" })` |
| TC-IT-043 | Handshake 応答が不正 MessagePack | `Err(PersistenceError::IpcDecode { reason })` |

### 5.2 daemon 未起動

| TC-ID | 前提 | 操作 | 期待 |
|-------|------|------|------|
| TC-IT-044 | `tempfile::TempDir` 内の**存在しない socket path** | `IpcClient::connect(&nonexistent_path)` | `Err(PersistenceError::DaemonNotRunning(path))`（Unix: `ENOENT` / `ECONNREFUSED` / Windows: `ERROR_FILE_NOT_FOUND`） |

### 5.3 load / save（シャドウベース差分）

| TC-ID | シナリオ | スタブ応答シーケンス | クライアント操作 | 期待発行 IPC |
|-------|---------|-----------------|------------|----------|
| TC-IT-050 | load → 新規 save（追加のみ） | handshake / Records(空) / Added(id) | repo.load() → vault.add_record(new) → repo.save(&vault) | AddRecord が発行される（スタブが受信したリクエストを記録） |
| TC-IT-051 | load → save（削除のみ） | handshake / Records([r1]) / Removed(r1.id) | repo.load() → vault.remove_record(r1.id) → save | RemoveRecord(r1.id) |
| TC-IT-052 | load → save（更新のみ） | handshake / Records([r1]) / Edited(r1.id) | repo.load() → vault.update_record(r1.id, \|o\| o.with_updated_label("NEW", t)) → save | EditRecord(r1.id, Some("NEW"), ..., t) |

**注記**: スタブサーバは `tokio::spawn` 内で `framed.next()` ループし、**受信した `IpcRequest` を `Arc<Mutex<Vec<IpcRequest>>>` に記録**。テスト本体は drop 時に記録を検証（発行順序 / 種類）。

---

## 6. SingleInstanceLock 結合テスト

配置: `crates/shikomi-daemon/tests/it_single_instance.rs`

OS syscall（`flock` / `FILE_FLAG_FIRST_PIPE_INSTANCE`）を**実呼出**するため、`tokio::io::duplex` ではなく `tempfile::TempDir` 配下で実 UDS / Named Pipe を立てる。IT と E2E の中間的位置づけだが、`assert_cmd` でプロセス spawn しない点で IT 扱い。

### 6.1 Unix

| TC-ID | シナリオ | 操作 | 期待 |
|-------|---------|------|------|
| TC-IT-060 | 初回起動 | `SingleInstanceLock::acquire_unix(&tmp)` | `Ok(lock)`、`tmp/daemon.lock` 存在、`tmp/daemon.sock` 存在、それぞれ `0600` / socket `0600` |
| TC-IT-061 | 二重起動（`flock` 競合） | (1) lock_a = acquire_unix / (2) lock_b = acquire_unix | (1) Ok, (2) `Err(AlreadyRunning { lock_path })` |
| TC-IT-062 | stale socket 存在下での初回起動 | (1) 事前に `tmp/daemon.sock` を手動作成（ファイル） / (2) acquire_unix | `Ok(lock)`（`flock` → `unlink` → `bind` の 3 段で stale を排除）、socket が新規生成 |
| TC-IT-063 | 親ディレクトリ `0700` 違反 | tmp に chmod 0755 後 acquire_unix | `Err(InvalidDirectoryPermission { path, got: 0o755 })`（事前 `stat` で検出） |
| TC-IT-064 | lock drop 後の再起動 | (1) acquire / (2) drop / (3) acquire 再度 | 2 回目も `Ok`（`flock` はプロセス終了 or file close で解放） |

### 6.2 Windows

| TC-ID | シナリオ | 操作 | 期待 |
|-------|---------|------|------|
| TC-IT-070 | 初回起動 | `SingleInstanceLock::acquire_windows(&pipe_name)` | `Ok(lock)`、pipe 作成 |
| TC-IT-071 | 二重起動 | 2 回目の acquire_windows | `Err(AlreadyRunning { pipe_name })`（`ERROR_ACCESS_DENIED` / `ERROR_PIPE_BUSY`） |

**注記**: pipe_name は `\\.\pipe\shikomi-test-{pid}-{uuid}` で test 間競合を排除（test 並列化時に必須、pipe_name はテスト引数として渡す——本番コードから env で上書きする経路は設けない、ペテルギウス指摘 ③ 対応）。

---

## 7. Characterization テスト

本 feature は**外部 HTTP API / クラウド API を呼ばない**ため **Characterization test は作成不要**（`index.md §6 外部 I/O 依存マップ` 参照）。ピア資格情報取得（`SO_PEERCRED` / `GetNamedPipeClientProcessId`）は trait 抽象化でユニットテストに吸収する（`unit.md §2.6`）。

---

## 8. テストコード配置

```
crates/shikomi-daemon/tests/
  common/
    mod.rs                        # fresh_repo() / fixed_time() / spawn_stub_server() ヘルパー
    peer_mock.rs                  # TestPeerCredential: PeerCredentialSource trait 実装
  it_protocol_roundtrip.rs        # TC-IT-001〜009
  it_server_connection.rs         # TC-IT-010〜032
  it_single_instance.rs           # TC-IT-060〜071（unix/windows cfg 分岐）

crates/shikomi-cli/tests/
  common/
    mod.rs                        # spawn_stub_server() で IpcClient の対向を作る
  it_ipc_vault_repository.rs      # TC-IT-040〜052
```

### 8.1 ピア検証のテスト時 バイパス経路（env 裏口 禁止、trait 一本化）

**ペテルギウス review 指摘 ③ 対応（2026-04-25）**: 前版で言及した `SHIKOMI_DAEMON_SKIP_PEER_VERIFY` env 裏口を**全面削除**する。理由: 本番バイナリに env 読取コードが残ると多層防御が構造的に崩れる（セキュリティ境界に env 分岐を置く時点で契約違反）。

**採用案: `PeerCredentialSource` trait 注入一本化**（`unit.md §2.6` / `§3.1 引き継ぎ` と完全同型）:

- `pub(crate) trait PeerCredentialSource { fn peer_uid(&self) -> Result<u32, PeerVerificationError>; fn self_uid(&self) -> u32; }` を `crates/shikomi-daemon/src/permission/peer_credential/mod.rs` に定義
- 本番実装: `impl PeerCredentialSource for tokio::net::UnixStream`（cfg unix）/ `impl PeerCredentialSource for tokio::net::windows::named_pipe::NamedPipeServer`（cfg windows）
- `IpcServer::handle_connection` は `PeerCredentialSource` を trait object として受け取る構造にする（**`#[cfg(test)]` 分岐なし**、本番コードは trait 経由のみ）
- テスト用: `crates/shikomi-daemon/tests/common/peer_mock.rs` で `pub struct TestPeerCredential { peer: u32, slf: u32 }` を `impl PeerCredentialSource` → IT から `IpcServer::new_for_test(listener_enum, repo, vault, Box::new(TestPeerCredential { peer: slf, slf }))` で注入
- `IpcServer::new_for_test` は `#[cfg(test)]` で提供し、本番 `IpcServer::new` は実 stream の `impl PeerCredentialSource` を自動使用する

**本番コード側の規約**:
- `std::env::var("SHIKOMI_DAEMON_SKIP_*")` 系の読取コードを**一切書かない**（CI grep で監査、`ci.md` で追加契約化）
- `#[cfg(test)]` ブロックは `common/` 内のヘルパーに閉じる。`src/` 配下の `#[cfg(test)]` は `mod tests` の UT のみ

**利点**:
- 本番バイナリの攻撃面が増えない（env 読取コード不在）
- テストは trait 注入で決定論的（env 読取時点の競合・unset漏れリスク回避）
- ピア検証自体は UT（TC-UT-020〜024）と trait 差替 IT（§4 全体）で網羅

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

# 暗号化 vault フィクスチャが必要な TC-IT-014
cargo test -p shikomi-daemon --features "shikomi-infra/test-fixtures" --test it_server_connection encryption_unsupported
```

---

## 10. 観測可能ログの検証

`tracing` ログを結合テストで検証する場合は **`tracing-subscriber` の `tracing_test` feature** または自前の `LogLayer` を使う。本 feature では以下のログ観測を**代表 3 件のみ** TC に組み込む（全文マッチではなく部分文字列 `"listening on "` / `"graceful shutdown complete"` / `"peer credential mismatch"` の出現確認）:

- TC-IT-010 の成功経路で `"listening on "` が出る
- TC-IT-030 の shutdown 完了時に `"graceful shutdown complete"`
- TC-IT-044 の未起動経路は daemon 側ログではなく CLI 側エラー扱い（ログ検証対象外）

**観測負債**: `tracing` ログの全 11 メッセージを網羅する IT は書かない（過剰、メッセージテンプレート変更時の fragile さを避ける、YAGNI）。代表 3 件で十分。

---

*この文書は `index.md` の分割成果。E2E は `e2e.md`、ユニットは `unit.md`、CI は `ci.md` を参照*
