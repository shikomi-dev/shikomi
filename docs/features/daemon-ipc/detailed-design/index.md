# 詳細設計書 — index（クラス設計 / 分割概要）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 (Phase 1: list) / Issue #30 (Phase 1.5: add/edit/remove) -->
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
        <<shikomi-cli::io - VaultRepository NON-impl, Issue #30>>
        +connect socket_path Result
        +list_summaries Result Vec
        +add_record Result RecordId
        +edit_record Result RecordId
        +remove_record Result RecordId
        -client IpcClient
        -runtime tokio Runtime
    }

    class RepositoryHandle {
        <<shikomi-cli::lib non-public enum, Issue #30>>
        +Sqlite SqliteVaultRepository
        +Ipc IpcVaultRepository
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

    IpcVaultRepository --> IpcClient
    IpcVaultRepository --> IpcRequest : sends AddRecord/EditRecord/RemoveRecord/ListRecords
    IpcClient --> FramingCodec
    IpcClient --> IpcRequest
    IpcClient --> IpcResponse
    IpcClient ..> PersistenceError : maps IpcResponse::Error

    CliRun --> CliArgs : parse with --ipc
    CliRun --> RepositoryHandle : constructs by args.ipc
    RepositoryHandle --> SqliteVaultRepository : Sqlite arm
    RepositoryHandle --> IpcVaultRepository : Ipc arm
    CliRun --> CliError : maps
    CliError --> PersistenceError : From
    SqliteVaultRepository ..|> VaultRepository : implements
```

**注記（Issue #30 で確定）**: `IpcVaultRepository` は `VaultRepository` trait を**実装しない**（破線 `..|>` を引かない）。CLI Composition Root は `Box<dyn VaultRepository>` ではなく `RepositoryHandle` enum で経路を保持し、`run_*` 各サブコマンドハンドラが `match handle` で 2 アーム分岐する。詳細は `./ipc-vault-repository.md §設計方針の確定` を単一真実源とする。

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

**7. なぜ `IpcVaultRepository` が `VaultRepository` trait を実装しないか（Issue #30 で確定、案 D）**:
- 旧 PR #29（Phase 1）では「list 専用最小実装」として `VaultRepository` trait を**部分実装**する案 A → 「`save` 内で差分計算して IPC 操作を発行する」案 C と段階的に検討された
- PR #29 で実装してみた結果、案 C は以下の構造的破綻を起こすことが判明:
  - `load()` で受け取る `Vec<RecordSummary>` から `Vault` を再構築する際、`RecordPayload::Plaintext(SecretString::from_str(""))` のような**「嘘の Plaintext(empty)」を注入**しなければ `Record` を構築できない
  - `add_record` で daemon が生成する真実 ID と、CLI 側 `Uuid::now_v7()` 生成 ID が**二重発行**される（`save` 時に「嘘 ID で上書き」が起こる）
  - `exists()` が IPC 経路では本質的に**常に `true`**（接続成功 = vault 存在）となり、Phase 1 直結経路で意味があった `VaultNotInitialized` Fail Fast が崩壊
  - シャドウと新 `Vault` の `updated_at` timestamp 差分による差分検出は「同 timestamp で payload 変更」のエッジケースで検出漏れする
- PR #29 ではこれを「`--ipc add/edit/remove` を runtime reject」して先送りすることで Phase 1 を成立させた
- **Issue #30 で案 D を採用**: `IpcVaultRepository` は `VaultRepository` trait を**実装しない**。代わりに専用メソッド（`list_summaries` / `add_record` / `edit_record` / `remove_record`）を公開する。CLI Composition Root は `enum RepositoryHandle { Sqlite, Ipc }` で経路保持し、各サブコマンドハンドラが `match handle` で 2 アーム分岐する
- **案 D の利点**: 「嘘の Plaintext(empty) / 嘘 ID / 常時 true な exists()」を**型として持てなくする**（構造的排除）。Tell, Don't Ask / Fail Fast / DRY のいずれにも整合
- **案 D で失う性質**: 「`run()` の 1 行差し替えで Phase 2 移行」契約は、新設計では「`RepositoryHandle::Sqlite` バリアント削除 + 各 `run_*` の `match` 1 アーム削除」に置き換わる。enum の網羅性検査が修正漏れをコンパイル時に検出するため、保守性は失われない

**8. なぜ `args.ipc` 分岐を `Box<dyn VaultRepository>` でなく `enum RepositoryHandle` で表現するか（Issue #30 で確定）**:
- 旧 PR #29 では `Box<dyn VaultRepository>` を採用予定だったが、Issue #30 で `IpcVaultRepository` の trait 非実装が確定したため**構造的に表現不能**となった
- `enum RepositoryHandle { Sqlite(SqliteVaultRepository), Ipc(IpcVaultRepository) }` で表現することで:
  - `match handle` の網羅性検査がコンパイル時に経路追加時の修正漏れを検出（dyn 経由ではこれが効かない）
  - スタック配置でヒープ確保不要（`Box` の間接参照ペナルティ排除）
  - `IpcVaultRepository` の専用メソッドが**経路毎に異なるシグネチャ**（`add_record(kind, label, value, now) -> Result<RecordId, _>` vs `usecase::add::add_record(repo, input, now) -> Result<RecordId, _>`）を持つことを enum で明示できる
- 配置: `crates/shikomi-cli/src/lib.rs` 内 non-public enum（外部 crate からの参照は構造的に不可能、CLI 内部実装の隠蔽を維持）

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
