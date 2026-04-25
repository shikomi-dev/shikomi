# 基本設計書 — flows（処理フロー / シーケンス図）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 (Phase 1: list) / Issue #30 (Phase 1.5: add/edit/remove) -->
<!-- 配置先: docs/features/daemon-ipc/basic-design/flows.md -->
<!-- 兄弟: ./index.md, ./security.md, ./error.md, ./ipc-protocol.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。処理フローは番号付き箇条書き、シーケンスは Mermaid `sequenceDiagram` で表現する。

## 処理フロー

本 feature の主要フローは「daemon 起動 → IPC 接続受付 → ハンドシェイク → vault 操作 → graceful shutdown」と、CLI クライアント側「`--ipc` 指定 → 接続 → ハンドシェイク → 操作 → 出力」の 2 系統。

### 共通: `shikomi-daemon` 起動〜listener 開始

1. `main()` が `#[tokio::main]` で `tokio` 多重スレッドランタイム起動 → `shikomi_daemon::run().await` を呼ぶ
2. `run()` 内で最初に `std::panic::set_hook(Box::new(panic_hook_fn))` を登録（`./security.md §panic hook` 参照、CLI と同型 fixed-message）
3. `tracing-subscriber` を `EnvFilter::try_from_env("SHIKOMI_DAEMON_LOG").unwrap_or_else(|_| EnvFilter::new("info"))` で初期化
4. `SingleInstanceLock::acquire(&socket_dir)?` で **3 段階先取り**:
   - **Unix**: `flock(LOCK_EX|LOCK_NB)` 獲得 → 既存 socket `unlink` → `UnixListener::bind`（順序厳守、`./security.md §シングルインスタンスの race-safe 保証`）
   - **Windows**: `NamedPipeServer::new` を `first_pipe_instance(true)` 付きで作成
   - 失敗時は `tracing::error!` + `ExitCode::SingleInstanceUnavailable (2)` で early return
5. `SqliteVaultRepository::from_directory(&resolve_vault_dir())?` で repo 構築（既存 `cli-vault-commands` で追加された API を再利用、`SHIKOMI_VAULT_DIR` env / OS デフォルトの解決は infra 内部）
6. `repo.load()?` で `Vault` ロード → `vault.protection_mode()` が `Encrypted` なら `tracing::error!("vault is encrypted; daemon does not support encrypted vaults yet")` + `ExitCode::EncryptionUnsupported (3)` で early return
7. `Arc<Mutex<Vault>>` 構築 → `Arc<R>` で repo を共有
8. `IpcServer::new(listener, repo, vault).start().await?` で listen ループ起動
9. 並行で `lifecycle::shutdown::wait_for_signal()` を spawn し、`SIGTERM` / `SIGINT` / `CTRL_CLOSE_EVENT` 受信を待機
10. シグナル受信 → server に `shutdown.notify()` → server が in-flight 完了待機 → `SingleInstanceLock` Drop（ソケット削除 + flock 解放）→ `ExitCode::Success (0)`

### 共通: 接続受付〜ハンドシェイク

1. `listener.accept().await` で接続待機（loop）
2. 接続受付時に `peer_credential::verify(&stream)?`:
   - **Unix**: `getsockopt(fd, SOL_SOCKET, SO_PEERCRED, ...)`（Linux）/ `getsockopt(fd, SOL_LOCAL, LOCAL_PEERCRED, ...)`（macOS）で UID 取得 → `geteuid()` と比較
   - **Windows**: `GetNamedPipeClientProcessId` → `OpenProcessToken` → `GetTokenInformation(TokenUser)` で SID 取得 → 自プロセス SID と比較
   - 不一致 → 即切断 + `tracing::warn!("peer credential mismatch...")` → loop 先頭へ
3. `Framed::new(stream, LengthDelimitedCodec::builder().max_frame_length(16 MiB).little_endian().length_field_length(4).new_codec())` で stream をフレーム化
4. `tokio::spawn` でこの接続用ハンドラタスクを spawn（並行接続対応）
5. ハンドラタスクの最初: `framed.next().await` で初回フレーム受信 → タイムアウト 5 秒（`tokio::time::timeout`）
6. `rmp_serde::from_slice::<IpcRequest>(&frame)` でデコード → `IpcRequest::Handshake { client_version }` でなければ即切断 + `tracing::warn!("first frame must be Handshake")`
7. `client_version == IpcProtocolVersion::V1` 判定:
   - 一致 → `framed.send(rmp_serde::to_vec(&IpcResponse::Handshake { server_version: V1 }).into()).await?` → 通常リクエスト受付ループへ
   - 不一致 → `framed.send(rmp_serde::to_vec(&IpcResponse::ProtocolVersionMismatch { server: V1, client: <受信値> }).into()).await?` → 切断 + `tracing::warn!`

### 共通: 通常リクエスト受付ループ（ハンドシェイク後）

1. `framed.next().await` で次フレーム受信
2. `rmp_serde::from_slice::<IpcRequest>(&frame)` でデコード:
   - 失敗 → 該当接続のみ切断 + `tracing::warn!("MessagePack decode failed: {}")` + バッファ即解放（daemon プロセスはクラッシュさせない）
   - フレーム長 16 MiB 超過 → `Codec` がエラー返却 → 該当接続のみ切断 + `tracing::warn!("frame length exceeds 16 MiB")`
3. `vault_mutex.lock().await` でロック取得
4. `handler::handle_request(&*repo, &mut vault, request)` で `IpcResponse` を pure 写像で得る
5. `vault_mutex` 解放
6. `framed.send(rmp_serde::to_vec(&response).into()).await?` で応答
7. loop 先頭に戻る（接続切断は client 側または shutdown 側で発生、サーバ側からは常時応答可能）

### REQ-DAEMON-007: List 応答フロー

1. `IpcRequest::ListRecords` 受信
2. `vault_mutex.lock().await`
3. `vault.records().iter().map(RecordSummary::from_record).collect::<Vec<_>>()` で投影:
   - `RecordSummary { id, kind, label, value_preview: record.text_preview(40), value_masked: record.kind() == Secret }`
   - `text_preview` は `shikomi-core::Record` の既存メソッド（`cli-vault-commands` で追加済み）
4. `IpcResponse::Records(summaries)` を返す

### REQ-DAEMON-008: Add 応答フロー

1. `IpcRequest::AddRecord { kind, label, value: SerializableSecretBytes(secret_bytes), now }` 受信
2. `vault_mutex.lock().await`
3. `let payload = RecordPayload::Plaintext(SecretString::from_bytes(secret_bytes))`（`shikomi-core::SecretString::from_bytes` API、`expose_secret` を呼ばずバイト列から直接構築）
4. `let record_id = RecordId::new(uuid::Uuid::now_v7())?`
5. `let record = Record::new(record_id, kind, label, payload, now)?`
6. `vault.add_record(record)?` で集約に追加 → 失敗時は `IpcResponse::Error(IpcErrorCode::Domain { reason })`
7. `repo.save(&vault)?` → 失敗時は `IpcResponse::Error(IpcErrorCode::Persistence { reason })`
8. `IpcResponse::Added { id: record_id }` を返す

### REQ-DAEMON-009: Edit 応答フロー

1. `IpcRequest::EditRecord { id, label, value, now }` 受信
2. `vault_mutex.lock().await`
3. `vault.find_record(&id)` → `None` なら `IpcResponse::Error(NotFound { id })`
4. `vault.update_record(&id, |old| { ... })` のクロージャ内で:
   - `label.is_some()` → `record = old.with_updated_label(label.unwrap(), now)?`
   - `value.is_some()` → `record = record.with_updated_payload(RecordPayload::Plaintext(SecretString::from_bytes(value.unwrap().0)), now)?`
5. `repo.save(&vault)?` → `IpcResponse::Edited { id }` を返す

### REQ-DAEMON-010: Remove 応答フロー

1. `IpcRequest::RemoveRecord { id }` 受信
2. `vault_mutex.lock().await`
3. `vault.find_record(&id)` → `None` なら `IpcResponse::Error(NotFound { id })`
4. `vault.remove_record(&id)?` → 失敗時は `IpcResponse::Error(Domain { reason })`
5. `repo.save(&vault)?` → `IpcResponse::Removed { id }` を返す

### REQ-DAEMON-014: graceful shutdown フロー

1. `tokio::signal::unix::signal(SignalKind::terminate())` / `tokio::signal::ctrl_c` / `tokio::signal::windows::ctrl_close` で受信
2. `IpcServer::shutdown_notify()` を発火 → `accept` ループが新規接続受付を停止
3. 既存接続のハンドラタスクが in-flight リクエストの応答送信完了するまで `JoinSet::join_all` で待機（タイムアウト 30 秒、実装時調整可）
4. `Mutex<Vault>` Drop → `repo` Drop → `VaultLock` 解放（既存 `shikomi-infra::persistence::lock::VaultLock` の `Drop` 実装）
5. `SingleInstanceLock` Drop → ソケットファイル `unlink`（Unix のみ、Windows はカーネルが解放）+ lock ファイル close（`flock` 解放）
6. `tracing::info!("graceful shutdown complete")` → `ExitCode::Success (0)` を return

### CLI 側: `shikomi --ipc list` フロー

1. `shikomi-cli` の `main()` → `shikomi_cli::run()` を呼ぶ
2. `run()` で panic hook 登録（既存）→ Locale 決定 → tracing 初期化 → clap パース
3. パース結果 `args.ipc == true` → `IpcVaultRepository::connect(default_socket_path()?).await?` で daemon 接続:
   - `default_socket_path()` は `dirs` を使い OS 規約に従ってソケットパス解決（Unix: `$XDG_RUNTIME_DIR/shikomi/daemon.sock`、Windows: `\\.\pipe\shikomi-daemon-{user-sid}`）
   - `connect` は内部で `UnixStream::connect` / `NamedPipeClient::connect` → `Framed` 構築 → `IpcRequest::Handshake { V1 }` 送信 → `IpcResponse::Handshake { V1 }` or `ProtocolVersionMismatch` 受信
   - 接続失敗 → `PersistenceError::DaemonNotRunning(socket_path)` → `CliError::DaemonNotRunning` 経由で stderr に `MSG-CLI-110` + 終了コード 1
   - プロトコル不一致 → `CliError::ProtocolVersionMismatch { server, client }` → stderr に `MSG-CLI-111` + 終了コード 1
4. `args.ipc == false` → 既存 `SqliteVaultRepository::from_directory(...)` 経路（変更なし）
5. 構築した `Box<dyn VaultRepository>` を `usecase::list::list_records(&*repo)` に渡す（`UseCase` は trait 越しのアクセスのみで、IPC か SQLite かを意識しない = Clean Arch 原則の体現）
6. `presenter::list::render_list(&views, locale)` で整形 → stdout 出力 → `ExitCode::Success`

### CLI 側: `shikomi --ipc list` 内部の IPC 往復（Issue #30 で経路確定）

1. `IpcVaultRepository::list_summaries()` 呼出（**専用メソッド**、`VaultRepository::load` ではない）
2. 内部で `IpcRequest::ListRecords` を MessagePack シリアライズ → `framed.send(...).await?`
3. `framed.next().await?` で `IpcResponse::Records(summaries)` 受信 → デコード
4. `Vec<RecordSummary>` を CLI 側 `run_list` に返却
5. `run_list` は `RecordSummary` を `RecordView::from_summary` で射影し、`presenter::list::render_list` に渡して整形

**設計方針の確定（Issue #30）**: `IpcVaultRepository` は **`VaultRepository` trait を実装しない**。旧設計の案 A（`load` で投影 `Vault` 再構築）/ 案 B（trait 分割）/ 案 C（シャドウ差分 + 偽実装注入）はいずれも構造的破綻が判明したため**全て廃案**。新設計では:

- `IpcVaultRepository` の公開メソッド: `list_summaries` / `add_record` / `edit_record` / `remove_record` の 4 専用メソッド
- CLI 側 Composition Root が `enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }` で経路保持し、各サブコマンドハンドラが `match handle` で 2 アーム分岐
- 詳細は `../detailed-design/ipc-vault-repository.md §設計方針の確定` を単一真実源とする

### CLI 側: `shikomi --ipc add` フロー（**Issue #30 で新設**）

1. `shikomi-cli` の `main()` → `shikomi_cli::run()` を呼ぶ
2. `run()` で panic hook 登録 → Locale 決定 → tracing 初期化 → clap パース
3. `args.ipc == true` → `IpcVaultRepository::connect(default_socket_path()?)?` で daemon 接続 + ハンドシェイク（PR #29 既存）→ `RepositoryHandle::Ipc(ipc)` で保持
4. clap パース結果 `Subcommand::Add(a)` → `run_add(&handle, a, locale, args.quiet)`
5. `run_add` 関数の内部:
   1. `--value` / `--stdin` 衝突検出（既存）
   2. shell 履歴警告（既存）
   3. `RecordLabel::try_new(args.label.clone())?`
   4. `value` 取得（`--value` または `--stdin`、`SecretString` でラップ）
   5. `let input = AddInput { kind, label, value };`
   6. `let now = OffsetDateTime::now_utc();`
   7. `match handle`:
      - `RepositoryHandle::Sqlite(repo)` → 既存経路（変更なし）
      - `RepositoryHandle::Ipc(ipc)` → `let id = ipc.add_record(input.kind, input.label, input.value, now)?;`
   8. `println!("{}", render_added(&id, locale))`
6. `IpcVaultRepository::add_record` 内部:
   1. `SerializableSecretBytes::from_secret_string(value)` で secret ラップ（`expose_secret` 不使用経路）
   2. `IpcRequest::AddRecord { kind, label, value, now }` 構築 → `IpcClient::round_trip(&request)` 発行
   3. `IpcResponse::Added { id }` 受信 → `Ok(id)`
   4. `IpcResponse::Error(IpcErrorCode::Persistence { reason })` → `PersistenceError` 写像 → CLI 側で `MSG-CLI-107` 経路（vault 未作成 / lock 競合等）

### CLI 側: `shikomi --ipc edit` フロー（**Issue #30 で新設**）

1〜4 は `--ipc add` と同様（接続確立後、`Subcommand::Edit(a)` 分岐）
5. `run_edit` 関数の内部:
   1. `--value` / `--stdin` 衝突検出（既存）
   2. 「最低 1 つの更新フラグ必須」検証（既存）
   3. `RecordId::try_from_str(&args.id)?` → `CliError::InvalidId`
   4. `RecordLabel::try_new(...)?`（`Option`）
   5. `value` 取得（`Option<SecretString>`）
   6. `let input = EditInput { id, label, value };`
   7. `let now = OffsetDateTime::now_utc();`
   8. `match handle`:
      - `RepositoryHandle::Sqlite(repo)` → 既存経路
      - `RepositoryHandle::Ipc(ipc)` → `let id = ipc.edit_record(input.id, input.label, input.value, now)?;`
   9. `println!("{}", render_updated(&id, locale))`
6. `IpcVaultRepository::edit_record` 内部:
   1. `IpcRequest::EditRecord { id, label, value: value_opt_bytes, now }` 構築
   2. `IpcClient::round_trip(&request)` 発行
   3. `IpcResponse::Edited { id }` → `Ok(id)`
   4. `IpcResponse::Error(IpcErrorCode::NotFound { id })` → `PersistenceError::RecordNotFound(id)` → CLI 側で `MSG-CLI-106` 経路

### CLI 側: `shikomi --ipc remove` フロー（**Issue #30 で新設**）

1〜4 は同様（`Subcommand::Remove(a)` 分岐）
5. `run_remove` 関数の内部:
   1. `RecordId::try_from_str(&args.id)?`
   2. **id 存在確認 + プロンプト用 label 取得（`--yes` でも常に実行、Fail Fast 経路の統一）**:
      - `match handle`:
        - `RepositoryHandle::Sqlite(repo)` → `repo.load()?` → `find_record(&id)` で label 取得
        - `RepositoryHandle::Ipc(ipc)` → `ipc.list_summaries()?` → `iter().find(|s| s.id == id)` で `RecordSummary.label` 取得
      - id 非存在なら `CliError::RecordNotFound(id)` で early return（プロンプト前 Fail Fast、**`RemoveRecord` リクエストを発行しない**）
      - **設計理由**: `--yes` 経路でも label 取得を行うことで、id 非存在時の Fail Fast 経路が `--yes` / 非 `--yes` で**完全一致**する。「`--yes` だから daemon に投げてから NotFound 受信する」という余計な往復を回避し、TC-IT-093 の意図（`--yes` でも `RemoveRecord` 未発行）を満たす
   3. **確認判定**:
      - `args.yes == true` → プロンプトをスキップ、`ConfirmedRemoveInput::new(id)` 直接構築（ステップ 2 で label を取得済みだが**画面表示しない**、id 存在確認のために取得しているのみ）
      - `args.yes == false` かつ `is_stdin_tty() == true` → ステップ 2 で取得した label をプロンプトに表示 → y/Y で `ConfirmedRemoveInput::new(id)` 構築、それ以外は `render_cancelled` で early return
      - `args.yes == false` かつ `is_stdin_tty() == false` → `CliError::NonInteractiveRemove`
   4. `match handle`:
      - `RepositoryHandle::Sqlite(repo)` → 既存経路
      - `RepositoryHandle::Ipc(ipc)` → `let id = ipc.remove_record(input.id().clone())?;`
   5. `println!("{}", render_removed(&id, locale))`

**`list_summaries` 全件取得の境界（YAGNI 注記）**: `run_remove` の IPC 経路はプロンプト前段で `ipc.list_summaries()` を 1 往復発行し全レコード summary を取得する。`MAX_FRAME_LENGTH = 16 MiB` 上限を超えない範囲で動作する想定（vault 100k レコード × 平均 200 byte = 約 20 MB のため、約 80k レコードで境界に達する）。境界突破時は **`IpcRequest::FindRecord { id }` 等のピンポイント取得バリアントを後続 Issue で追加**する余地を残す（YAGNI、Phase 1.5 では full-list 戦略で十分）。フレーム長超過時は `LengthDelimitedCodec` がエラーを返し、CLI 側は `PersistenceError::IpcIo` 経由で fail fast（vault 破損・データロスは発生しない、`./security.md §フレーム超過時の挙動`）。
6. `IpcVaultRepository::remove_record` 内部:
   1. `IpcRequest::RemoveRecord { id }` 構築
   2. `IpcClient::round_trip(&request)` 発行
   3. `IpcResponse::Removed { id }` → `Ok(id)`
   4. `IpcResponse::Error(IpcErrorCode::NotFound { id })` → `PersistenceError::RecordNotFound(id)`（前段で label 取得時に検出済みのため通常到達しないが、TOCTOU 競合で別接続が同時 remove した時の防御的経路）

## シーケンス図

### 代表シーケンス: `shikomi --ipc list`（正常系）

```mermaid
sequenceDiagram
    actor User
    participant Shell
    participant Cli as shikomi-cli (run)
    participant IpcRepo as IpcVaultRepository
    participant Stream as UDS / Named Pipe
    participant Server as shikomi-daemon (IpcServer)
    participant Verifier as PeerCredentialVerifier
    participant Handler as ipc::handler
    participant VaultMutex as Mutex Vault
    participant Repo as SqliteVaultRepository
    participant Stdout

    User->>Shell: shikomi --ipc list
    Shell->>Cli: spawn process -> run()
    Cli->>Cli: clap parse, args.ipc = true
    Cli->>IpcRepo: connect default_socket_path()
    IpcRepo->>Stream: UnixStream::connect / NamedPipeClient::connect
    Stream-->>IpcRepo: connected
    IpcRepo->>Stream: Framed::send(Handshake V1)
    Stream->>Server: frame received
    Server->>Verifier: verify peer uid/sid
    Verifier-->>Server: ok (uid match)
    Server->>Server: framed.next() -> Handshake V1 OK
    Server->>Stream: Framed::send(Handshake server V1)
    Stream-->>IpcRepo: Handshake response received
    IpcRepo-->>Cli: Ok(IpcVaultRepository)

    Cli->>IpcRepo: load() (via VaultRepository trait)
    IpcRepo->>Stream: Framed::send(ListRecords)
    Stream->>Server: frame received
    Server->>VaultMutex: lock
    VaultMutex-->>Server: vault ref
    Server->>Handler: handle_request(repo, vault, ListRecords)
    Handler->>Handler: vault.records().map(RecordSummary::from_record)
    Handler-->>Server: IpcResponse::Records(Vec)
    Server->>VaultMutex: unlock
    Server->>Stream: Framed::send(Records)
    Stream-->>IpcRepo: Records frame received
    IpcRepo-->>Cli: Vault projection
    Cli->>Cli: presenter::list::render_list
    Cli->>Stdout: write
    Cli->>Shell: exit code 0
```

### 代表シーケンス: ピア検証失敗（不正なクライアントから接続）

```mermaid
sequenceDiagram
    actor Attacker as 別ユーザの悪意あるプロセス
    participant Stream as UDS
    participant Server as shikomi-daemon (IpcServer)
    participant Verifier as PeerCredentialVerifier
    participant Tracing

    Attacker->>Stream: UnixStream::connect
    Stream->>Server: accept
    Server->>Verifier: verify peer uid
    Verifier->>Verifier: SO_PEERCRED -> uid_attacker
    Verifier->>Verifier: compare with daemon_uid -> mismatch
    Verifier-->>Server: Err(PeerVerificationError::UidMismatch)
    Server->>Tracing: warn(peer credential mismatch)
    Server->>Stream: drop (close immediately)
    Note over Server,Stream: hand-shake never reached, no protocol leak
```

**注記**: UDS `0600` / Named Pipe owner-only DACL で OS レイヤが先に拒否するため、このケースは**通常発生しない**。検証は多層防御として動作する（OS 拒否を回避された場合のバックアップ）。

### 代表シーケンス: プロトコルバージョン不一致

```mermaid
sequenceDiagram
    actor User
    participant Cli as shikomi-cli v0.2 (V2 client)
    participant IpcRepo as IpcVaultRepository
    participant Stream as UDS
    participant Server as shikomi-daemon v0.1 (V1 server)

    User->>Cli: shikomi --ipc list
    Cli->>IpcRepo: connect
    IpcRepo->>Stream: connect
    Stream-->>IpcRepo: ok
    IpcRepo->>Stream: send(Handshake { client_version: V2 })
    Stream->>Server: frame
    Server->>Server: V2 != V1 -> mismatch
    Server->>Stream: send(ProtocolVersionMismatch { server: V1, client: V2 })
    Server->>Stream: close connection
    Stream-->>IpcRepo: ProtocolVersionMismatch frame
    IpcRepo-->>Cli: Err(PersistenceError::ProtocolVersionMismatch { server: V1, client: V2 })
    Cli->>Cli: render_error(MSG-CLI-111)
    Cli->>User: stderr + exit code 1
```

**Phase 2 完了後**: `--ipc` を既定経路にする際は別 Issue で扱う。本 feature は `--ipc` オプトインのみ。Phase 1.5（Issue #30）完了時点では、**4 サブコマンド全て**（`list` / `add` / `edit` / `remove`）が `--ipc` 経路で透過動作する。

### 代表シーケンス: `shikomi --ipc add`（正常系、Issue #30 新規）

```mermaid
sequenceDiagram
    actor User
    participant Cli as shikomi-cli (run_add)
    participant Handle as RepositoryHandle::Ipc
    participant IpcRepo as IpcVaultRepository
    participant Stream as UDS / Named Pipe
    participant Server as shikomi-daemon (IpcServer)
    participant Handler as ipc::handler::add
    participant VaultMutex as Mutex Vault
    participant Repo as SqliteVaultRepository (atomic save)

    User->>Cli: shikomi --ipc add --kind text --label L --value V
    Cli->>Cli: clap parse, build AddInput
    Cli->>Handle: match Ipc(ipc) - dispatch
    Handle->>IpcRepo: ipc.add_record(kind, label, value, now)
    IpcRepo->>IpcRepo: SerializableSecretBytes::from_secret_string
    IpcRepo->>Stream: Framed::send(IpcRequest::AddRecord)
    Stream->>Server: frame received
    Server->>VaultMutex: lock
    Server->>Handler: handle_add(&repo, &mut vault, kind, label, value, now)
    Handler->>Handler: Uuid::now_v7 -> RecordId, build Record
    Handler->>VaultMutex: vault.add_record(record)
    Handler->>Repo: repo.save(&vault) - atomic write
    Repo-->>Handler: Ok
    Handler-->>Server: IpcResponse::Added { id }
    Server->>VaultMutex: unlock
    Server->>Stream: Framed::send(Added { id })
    Stream-->>IpcRepo: Added frame received
    IpcRepo-->>Handle: Ok(id)
    Handle-->>Cli: id
    Cli->>Cli: render_added(id, locale)
    Cli->>User: stdout: added: {id} + exit code 0
    Note over Cli,IpcRepo: id is daemon-generated, NOT cli-generated (no double-issuance)
```

### 代表シーケンス: `shikomi --ipc edit --id <id> --label NEW`（正常系、Issue #30 新規）

```mermaid
sequenceDiagram
    actor User
    participant Cli as shikomi-cli (run_edit)
    participant IpcRepo as IpcVaultRepository
    participant Stream as UDS
    participant Server as shikomi-daemon
    participant Handler as ipc::handler::edit
    participant VaultMutex as Mutex Vault
    participant Repo as SqliteVaultRepository

    User->>Cli: shikomi --ipc edit --id <id> --label NEW
    Cli->>Cli: clap parse, build EditInput { id, label: Some(NEW), value: None }
    Cli->>IpcRepo: ipc.edit_record(id, Some(label), None, now)
    IpcRepo->>Stream: Framed::send(IpcRequest::EditRecord)
    Stream->>Server: frame
    Server->>VaultMutex: lock
    Server->>Handler: handle_edit(&repo, &mut vault, id, Some(label), None, now)
    Handler->>VaultMutex: vault.find_record(&id)
    alt id found
        Handler->>VaultMutex: vault.update_record(id, |old| old.with_updated_label(label, now))
        Handler->>Repo: repo.save(&vault)
        Handler-->>Server: IpcResponse::Edited { id }
    else id not found
        Handler-->>Server: IpcResponse::Error(IpcErrorCode::NotFound { id })
    end
    Server->>VaultMutex: unlock
    Server->>Stream: Framed::send(response)
    Stream-->>IpcRepo: response frame
    alt Edited
        IpcRepo-->>Cli: Ok(id)
        Cli->>User: stdout: updated: {id} + exit code 0
    else Error(NotFound)
        IpcRepo-->>Cli: Err(PersistenceError::RecordNotFound(id))
        Cli->>User: stderr: MSG-CLI-106 + exit code 1
    end
```

### 代表シーケンス: `shikomi --ipc remove --id <id> --yes`（正常系、Issue #30 新規）

```mermaid
sequenceDiagram
    actor User
    participant Cli as shikomi-cli (run_remove)
    participant IpcRepo as IpcVaultRepository
    participant Server as shikomi-daemon
    participant Handler as ipc::handler::remove
    participant VaultMutex as Mutex Vault
    participant Repo as SqliteVaultRepository

    User->>Cli: shikomi --ipc remove --id <id> --yes
    Cli->>Cli: clap parse, RecordId::try_from_str
    Cli->>IpcRepo: ipc.list_summaries() (id existence check + label preview)
    IpcRepo-->>Cli: Vec RecordSummary
    Note over Cli: id found in summaries, label retrieved (not displayed because --yes)
    Note over Cli: --yes path: skip prompt only, build ConfirmedRemoveInput
    Cli->>IpcRepo: ipc.remove_record(id)
    IpcRepo->>Server: Framed::send(IpcRequest::RemoveRecord { id })
    Server->>VaultMutex: lock
    Server->>Handler: handle_remove(&repo, &mut vault, id)
    Handler->>VaultMutex: vault.find_record(&id) - check existence
    alt id found
        Handler->>VaultMutex: vault.remove_record(&id)
        Handler->>Repo: repo.save(&vault)
        Handler-->>Server: IpcResponse::Removed { id }
    else id not found
        Handler-->>Server: IpcResponse::Error(IpcErrorCode::NotFound { id })
    end
    Server->>VaultMutex: unlock
    Server-->>IpcRepo: response
    alt Removed
        IpcRepo-->>Cli: Ok(id)
        Cli->>User: stdout: removed: {id} + exit code 0
    else Error(NotFound)
        IpcRepo-->>Cli: Err(PersistenceError::RecordNotFound(id))
        Cli->>User: stderr: MSG-CLI-106 + exit code 1
    end
```

### 代表シーケンス: graceful shutdown

```mermaid
sequenceDiagram
    actor User
    participant Daemon as shikomi-daemon
    participant Server as IpcServer
    participant ConnA as 既存接続A (in-flight Add)
    participant ConnB as 既存接続B (idle)
    participant SingleLock as SingleInstanceLock
    participant Repo as SqliteVaultRepository (VaultLock)

    User->>Daemon: SIGTERM
    Daemon->>Server: shutdown_notify
    Server->>Server: stop accept loop (no new connections)
    Server->>ConnA: continue in-flight task
    ConnA->>ConnA: handle Add -> save(&vault)
    ConnA-->>Server: response sent, task complete
    Server->>ConnB: idle, close gracefully
    Server->>Server: JoinSet::join_all complete
    Server-->>Daemon: shutdown complete
    Daemon->>Repo: drop -> VaultLock::drop (release flock)
    Daemon->>SingleLock: drop -> unlink socket + flock release
    Daemon->>User: exit code 0
```

### 代表シーケンス: 暗号化 vault 検出（daemon 起動失敗）

```mermaid
sequenceDiagram
    actor User
    participant Daemon as shikomi-daemon
    participant Repo as SqliteVaultRepository
    participant Tracing
    participant Stderr

    User->>Daemon: shikomi-daemon
    Daemon->>Daemon: panic_hook set, tracing init, single_instance acquire ok
    Daemon->>Repo: from_directory + load
    Repo-->>Daemon: Vault { protection_mode: Encrypted }
    Daemon->>Tracing: error!("vault is encrypted; daemon does not support encrypted vaults yet")
    Daemon->>Stderr: tracing fmt layer writes
    Daemon->>User: exit code 3 (EncryptionUnsupported)
    Note over Daemon: socket NOT created, single_instance lock dropped (flock release)
```

### 代表シーケンス: daemon 二重起動失敗

```mermaid
sequenceDiagram
    participant DaemonA as 既存 daemon (running)
    participant DaemonB as 新しい daemon プロセス
    participant Lock as daemon.lock (flock)
    participant Tracing
    participant Stderr

    DaemonA->>Lock: holds flock LOCK_EX
    DaemonB->>DaemonB: start
    DaemonB->>Lock: open + flock(LOCK_EX | LOCK_NB)
    Lock-->>DaemonB: Err(EWOULDBLOCK)
    DaemonB->>Tracing: error!("another daemon is running; flock acquisition failed")
    DaemonB->>Stderr: tracing fmt
    DaemonB->>DaemonB: exit code 2 (SingleInstanceUnavailable)
    Note over DaemonA: continues normally
    Note over DaemonB: socket NEVER touched (race-safe ordering)
```

**Phase 2（daemon 経由）が既定になった後の差分**: `--ipc` フラグは廃止または `--no-ipc` 反転フラグへ。本 feature の範囲外（後続 Issue）。
