# 詳細設計書 — index（クラス設計 / 分割概要）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/index.md -->
<!-- 兄弟: ./protocol-types.md, ./daemon-runtime.md, ./ipc-vault-repository.md, ./composition-root.md, ./lifecycle.md, ./future-extensions.md -->

## 記述ルール（必ず守ること）

詳細設計に**疑似コード・サンプル実装（python/ts/go 等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

本書では Rust の関数シグネチャは**プレーンテキスト（インライン `code`）**で示し、実装本体は一切書かない。Mermaid 図 + 表 + 箇条書きで設計判断を記述する。

## 分割構成（500 行超え回避）

`detailed-design.md` 単一ファイルの 500 行超えを避けるため、本詳細設計は次の 7 ファイルに分割する（`cli-vault-commands` の 7 ファイル分割を踏襲）:

| ファイル | 担当領域 |
|---------|---------|
| `index.md` | クラス設計（全体像 Mermaid）/ 設計判断の補足 / ファイル分割の根拠 |
| `protocol-types.md` | `shikomi-core::ipc` 配下の型定義詳細（`IpcRequest` / `IpcResponse` / `RecordSummary` / `IpcErrorCode` / `SerializableSecretBytes` のフィールド・serde attribute）|
| `daemon-runtime.md` | `shikomi-daemon` の tokio ランタイム / `IpcServer` / `ipc::handler` / `permission::peer_credential` 詳細 |
| `ipc-vault-repository.md` | `shikomi-cli::io::IpcVaultRepository` の `VaultRepository` trait 実装 / `IpcClient` の Framed 保持 / `connect` のハンドシェイク手順 |
| `composition-root.md` | `shikomi-daemon::run()` の処理順序 / `shikomi-cli::run()` の `--ipc` 分岐 / panic_hook の詳細（CLI と同型）|
| `lifecycle.md` | `SingleInstanceLock` RAII / OS 別シングルインスタンス先取り / graceful shutdown / signal handling |
| `future-extensions.md` | 将来拡張のための設計フック / 実装担当への引き継ぎ事項 / 運用注記 |

各ファイルは独立して読めるよう、内部参照は `./xxx.md §...` 形式で記す。

## クラス設計（詳細）— 全体像

```mermaid
classDiagram
    direction TB

    class IpcProtocolVersion {
        <<enum non_exhaustive serde>>
        +V1
    }

    class IpcRequest {
        <<enum non_exhaustive Serialize Deserialize>>
        +Handshake
        +ListRecords
        +AddRecord
        +EditRecord
        +RemoveRecord
    }

    class IpcResponse {
        <<enum non_exhaustive Serialize Deserialize>>
        +Handshake
        +ProtocolVersionMismatch
        +Records
        +Added
        +Edited
        +Removed
        +Error
    }

    class RecordSummary {
        <<struct serde>>
        +id RecordId
        +kind RecordKind
        +label RecordLabel
        +value_preview Option String
        +value_masked bool
        +from_record record RecordSummary
    }

    class IpcErrorCode {
        <<enum non_exhaustive Serialize Deserialize>>
        +EncryptionUnsupported
        +NotFound
        +InvalidLabel
        +Persistence
        +Domain
        +Internal
    }

    class SerializableSecretBytes {
        <<struct manual Serialize Deserialize>>
        +inner SecretBytes
        +Debug REDACTED
    }

    class SecretBytes {
        <<from shikomi-core::secret>>
        +Debug REDACTED
        +Drop zeroize
        +pub_crate as_serialize_slice
        +from_vec
    }

    class DaemonRun {
        <<lib.rs async fn>>
        +run async ExitCode
    }

    class DaemonMain {
        <<bin entry tokio::main>>
        +main ExitCode
    }

    class SingleInstanceLock {
        <<RAII struct>>
        +acquire socket_dir Result
        -lock_file File
        -socket_path PathBuf
    }

    class IpcServer {
        +new listener vault repo Self
        +start async Result
        +shutdown_notify
    }

    class FramingCodec {
        +configure LengthDelimitedCodec
        +max_frame_length 16MiB
    }

    class HandshakeNegotiator {
        +negotiate framed timeout Result
    }

    class IpcRequestHandler {
        +handle_request repo vault req IpcResponse
    }

    class PeerCredentialVerifier {
        <<unix windows split>>
        +verify_unix stream Result
        +verify_windows stream Result
    }

    class PanicHookDaemon {
        <<module>>
        +install
        -hook_fn fixed_message
    }

    class IpcVaultRepository {
        <<shikomi-cli::io>>
        +connect socket_path Result
        +load Result Vault
        +save vault Result
        +exists bool
        -client IpcClient
    }

    class IpcClient {
        +new framed Self
        +send_request req Result
        +recv_response Result IpcResponse
        -framed Framed UnixStream LengthDelimitedCodec
    }

    class VaultRepository {
        <<trait re-used from shikomi-infra>>
        +load Result
        +save Result
        +exists bool
    }

    class SqliteVaultRepository {
        <<concrete re-used from shikomi-infra>>
        +from_directory
    }

    class CliRun {
        <<shikomi-cli::lib.rs fn edited>>
        +run ExitCode
    }

    class CliArgs {
        <<clap Parser edited>>
        +ipc bool global
    }

    class CliError {
        <<enum edited>>
        +DaemonNotRunning
        +ProtocolVersionMismatch
    }

    class PersistenceError {
        <<enum edited from shikomi-infra>>
        +DaemonNotRunning
        +ProtocolVersionMismatch
        +IpcDecode
        +IpcEncode
        +IpcIo
    }

    IpcRequest --> IpcProtocolVersion
    IpcRequest --> SerializableSecretBytes
    IpcResponse --> IpcProtocolVersion
    IpcResponse --> RecordSummary
    IpcResponse --> IpcErrorCode
    SerializableSecretBytes --> SecretBytes : wraps with manual Serialize

    DaemonMain --> DaemonRun : awaits
    DaemonRun --> SingleInstanceLock : RAII acquire
    DaemonRun --> SqliteVaultRepository : direct construct
    DaemonRun --> IpcServer : starts
    DaemonRun --> PanicHookDaemon : install
    IpcServer --> FramingCodec : per stream
    IpcServer --> HandshakeNegotiator : per accept
    IpcServer --> PeerCredentialVerifier : per accept
    IpcServer --> IpcRequestHandler : per request
    HandshakeNegotiator --> IpcRequest
    HandshakeNegotiator --> IpcResponse
    IpcRequestHandler --> IpcRequest
    IpcRequestHandler --> IpcResponse
    IpcRequestHandler ..> VaultRepository

    IpcVaultRepository ..|> VaultRepository : implements
    IpcVaultRepository --> IpcClient
    IpcClient --> FramingCodec
    IpcClient --> IpcRequest
    IpcClient --> IpcResponse
    IpcClient ..> PersistenceError : maps IpcResponse::Error

    CliRun --> CliArgs : parse with --ipc
    CliRun --> SqliteVaultRepository : when args.ipc false
    CliRun --> IpcVaultRepository : when args.ipc true
    CliRun --> CliError : maps
    CliError --> PersistenceError : From
    SqliteVaultRepository ..|> VaultRepository : implements
```

## 設計判断の補足

**1. なぜ `shikomi-core::ipc` を I/O なしのモジュールにするか**:
- `shikomi-core` は pure Rust / no-I/O ドメイン crate（`tech-stack.md` §4.5）。`tokio` / `rmp-serde` / `tokio-util` を依存に入れると、純粋ドメインの方針が崩れる
- `shikomi-core::ipc` は型定義と `serde` 派生のみを背負う。MessagePack シリアライズの実体（`rmp_serde::to_vec` / `from_slice`）は呼び出し側 crate（`shikomi-daemon` / `shikomi-cli`）の責務
- これにより `shikomi-core` の users（`shikomi-gui` など）が IPC 不要なら `tokio` / `rmp-serde` を引き込まない

**2. なぜ `IpcRequest` / `IpcResponse` を `#[non_exhaustive] enum` にするか**:
- 後続 feature（ホットキー登録 / 暗号化操作 / セッショントークン）のバリアント追加が**全て非破壊変更**として扱える
- `#[non_exhaustive]` により外部 crate での `match` が wildcard 強制となり、新バリアント追加で既存コードが壊れない
- `VaultVersion`（`shikomi-core::vault::version`）の前例を踏襲

**3. なぜ `SerializableSecretBytes` を別型にするか**:
- `shikomi-core::SecretBytes` 自体は `Serialize` / `Deserialize` を**意図的に持たない**（永続化フォーマット側で誤って `serde_json` 等に流れることを型で防ぐ、`tech-stack.md` §4.5「`secrecy` の `serde` 連携は使わない」と整合）
- IPC 経路でのみ秘密を運搬する文脈を newtype で明示化し、その型に **`expose_secret` を呼ばないシリアライズ実装**を用意（`../basic-design/security.md §SecretBytes のシリアライズ契約`）
- 永続化からの誤流入は `crates/shikomi-infra/src/persistence/` 配下で `SerializableSecretBytes` の import を 0 件とする CI grep で監査

**4. なぜ `IpcRequestHandler::handle_request` を pure 関数にするか**:
- `&dyn VaultRepository` と `&mut Vault` を引数で受ける pure 写像にすることで、ハンドラ単独で結合テストが容易（モック `VaultRepository` + 固定 `Vault` で UseCase 単位の検証）
- I/O（`Mutex` ロック / `Framed::send` / タスク spawn）は `IpcServer` 層の責務。責務分離が型境界で表現される
- `cli-vault-commands` の UseCase pure 関数（`list_records` / `add_record` / `edit_record` / `remove_record`）と同じ思想

**5. なぜ `SingleInstanceLock` を RAII 構造体にするか**:
- `flock` 取得 / `unlink` / `bind` の 3 段階を構造体の `acquire` メソッドで完結させ、`Drop` で「`flock` 解放 + ソケット削除」を行う
- ライフサイクル制御を型で表現（Tell, Don't Ask）。`acquire` を呼び `lock` 値を保持する間は単一インスタンスが保証され、値が drop した瞬間にリソースが解放される
- daemon の `run()` 関数の最後で `lock` がスコープを抜けることで graceful shutdown のクリーンアップと統合

**6. なぜ `IpcServer::handle_request` を `Mutex<Vault>` の外で呼ぶか**:
- ハンドラ自体は I/O をしない pure 関数にしたいため、`Mutex` ロック取得 → ハンドラ呼び出し → ロック解放のパターンを `IpcServer` 層に置く
- ロックを保持したまま `framed.send` を呼ぶと、応答送信中に他接続の vault 操作がブロックされて並行性が低下する。**送信前にロック解放**してから `framed.send` するのが望ましい
- 注記: ハンドラ pure 化のため `IpcResponse` を返すが、その値の生成には `&mut Vault` が必要。`vault_mutex.lock().await; let response = handler::handle_request(&*repo, &mut vault, req); drop(vault); framed.send(response).await?` の流れ

**7. なぜ `IpcVaultRepository` が `VaultRepository` trait を実装するか（Phase 2 移行契約の実体化）**:
- `cli-vault-commands` の Phase 2 移行契約「`run()` の 1 行差し替えで Repository 構築のみ変更」を実体化する
- `IpcVaultRepository::load` / `save` / `exists` は IPC 越しに daemon に問い合わせる実装。CLI の `usecase` / `presenter` / `input` / `view` / `error` レイヤは無変更
- `args.ipc` 分岐は `lib.rs::run()` の Repository 構築箇所 1 箇所のみ

**8. なぜ `IpcVaultRepository::load` で完全な `Vault` を返すか（List 専用最小実装）**:
- `VaultRepository` trait の `load` は `Result<Vault, PersistenceError>` を返す既存契約
- 本 feature では `load` 実装を「`IpcRequest::ListRecords` 送信 → `RecordSummary` 受信 → Secret 値非含有な `Vault` 集約を再構築」とする
- **trade-off**: Secret 値が必要な操作（`add` / `edit` の既存 vault 取得）は `load` 経由で得られない。本 feature では **`add` / `edit` / `remove` UseCase の Phase 2 経路は別途設計**:
  - **案 A**: `IpcVaultRepository` が `VaultRepository` trait を「list 用途で部分実装、`save` 経由の add/edit/remove は `IpcRequest::AddRecord` 等を直接送信」する複合実装（本 feature 採用）
  - **案 B**: `VaultRepository` trait を分割して `LoadVault` / `MutateVault` の 2 trait にする（既存型に大改修、却下）
- 案 A の具体: `IpcVaultRepository::save(&self, vault: &Vault) -> Result<(), PersistenceError>` は **trait 互換のため受けるが、内部で no-op or panic**（`load` 直後の vault を save しても意味がないため）。実際の add/edit/remove は **CLI 側 UseCase が `IpcVaultRepository::add_record` / `edit_record` / `remove_record` メソッドを直接呼ぶ専用拡張 trait `IpcVaultRepositoryExt`** を `IpcVaultRepository` に実装し、UseCase 側で `if let Some(ipc_repo) = repo.downcast_ref::<IpcVaultRepository>() { ipc_repo.add_record_via_ipc(...) }` の形にする
- **案 A の問題点**: UseCase が IPC 専用経路を意識する必要があり Clean Arch 違反
- **案 A の修正案 = 案 C（本 feature 最終採用）**: `IpcVaultRepository::save` は受け取った `Vault` の差分を計算して `IpcRequest::AddRecord` / `EditRecord` / `RemoveRecord` を発行する**実装的に妥当な経路**を取る。詳細は `./ipc-vault-repository.md §load/save 実装方針` で確定する

**9. なぜ daemon の panic hook を CLI と同型 fixed-message にするか**:
- daemon のクラッシュは secret 露出経路を増やす可能性がある（`PanicHookInfo::Debug` での raw 文字列展開）
- CLI で確立した「`info.payload()` を参照しない」「`tracing` マクロを呼ばない」「固定文言のみ stderr」の 3 規約を daemon にも適用
- 詳細は `./composition-root.md §panic hook` および `../basic-design/security.md §panic hook と secret 漏洩経路の遮断`

**10. なぜ `--ipc` をオプトインフラグにするか（Phase 2 全面切替を本 Issue でやらない）**:
- daemon の自動起動 / GUI 起動経路は本 Issue のスコープ外（後続 feature）
- `--ipc` を既定にすると「daemon 未起動 → CLI 失敗」が日常動作になり、daemon の手動起動を意識する必要が出る
- 既定は SQLite 直結を維持し、開発者・テスト目的で `--ipc` を opt-in する
- Phase 2 全面切替（`--ipc` 廃止 or `--no-ipc` 反転）は「daemon 自動起動 + ホットキー / 暗号化が揃う」段階で別 Issue（`process-model.md` §4.1.1）

**11. なぜ `[lib] + [[bin]]` の 2 ターゲット構成にするか（daemon 側）**:
- `cli-vault-commands` の `shikomi-cli` と同じ思想: `[lib]` で `shikomi_daemon::run()` を提供し、結合テスト（`tokio::test`）から呼び出し可能にする
- `[[bin]]` の `main.rs` は `#[tokio::main] async fn main() -> ExitCode { shikomi_daemon::run().await }` の 3 行ラッパ
- 全 `pub` 項目に `#[doc(hidden)]` を付け、`cargo doc` で隠す（外部公開 API 契約化を避ける）

**12. なぜ `shikomi-core::ipc::tests` を core 内に置くか / 置かないか**:
- 案 A（core 内）: `crates/shikomi-core/src/ipc/tests.rs` で MessagePack round-trip テスト → ただし `rmp-serde` を `dev-dependencies` に追加する必要があり、`shikomi-core` の純粋性が揺らぐ
- 案 B（呼び出し側 crate の integration test）: `crates/shikomi-daemon/tests/ipc_protocol.rs` で round-trip テスト → `rmp-serde` は daemon の通常 dependency
- **本 feature 採用: 案 B**。`shikomi-core` の `dev-dependencies` 純粋性を維持し、daemon / cli の integration test で round-trip を検証

以降の詳細は以下のファイルを参照:

- **IPC 型定義詳細**: `./protocol-types.md`
- **daemon ランタイム実装**: `./daemon-runtime.md`
- **CLI 側 IPC クライアント**: `./ipc-vault-repository.md`
- **コンポジションルート**: `./composition-root.md`
- **シングルインスタンス + シャットダウン**: `./lifecycle.md`
- **将来拡張 + 実装注意**: `./future-extensions.md`
