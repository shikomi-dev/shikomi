# 詳細設計書 — future-extensions（将来拡張フック / 実装担当への引き継ぎ）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/future-extensions.md -->
<!-- 兄弟: ./index.md, ./protocol-types.md, ./daemon-runtime.md, ./ipc-vault-repository.md, ./composition-root.md, ./lifecycle.md -->

## 記述ルール

疑似コード禁止。将来拡張は「何を」「なぜ今やらないか」「移行時のインパクト」の 3 点で示す。

## バイナリ正規形仕様 / 外部プロトコル互換性の契約

本 feature は IPC プロトコルを新規定義するため、**外部プロトコル互換性契約**を明示する:

- **`IpcProtocolVersion::V1` のレイアウトは凍結**。バリアント追加・フィールド追加・改名はすべて `V2` 等の新バリアント追加で扱う（`./protocol-types.md` / `../basic-design/ipc-protocol.md §バージョニングルール`）
- バリアント名（`add_record` / `list_records` 等）は wire 表現の安定性に直結するため**改名禁止**（`serde(rename_all = "snake_case")` の規則変更も禁止）
- `IpcProtocolVersion` のバリアント名（`v1`）は `Display` で出力されるため、log / エラーメッセージにも影響する。**`v1` 表記の変更禁止**
- `IpcErrorCode` のバリアント追加は **`#[non_exhaustive]` で非破壊**として扱える（既存クライアントが `_ => MSG-CLI-109 (Internal)` で wildcard 写像する設計のため）
- `RecordSummary` の `value_preview: Option<String>` は将来 grapheme 単位 truncate 等の改良があり得るが、wire 上の型（`Option<String>`）は変更しない

CLI の外部 I/F は `cli-vault-commands` の `future-extensions.md` で既に確定済み:

- サブコマンド名・フラグ名・終了コードの再割当禁止
- `MSG-CLI-xxx` ID の意味変更禁止（本 feature で `MSG-CLI-110` / `MSG-CLI-111` が追加、再割当禁止対象）
- **新規追加**: `MSG-CLI-051`（warning: `--ipc` opt-in 通知）、`MSG-CLI-110`（daemon not running）、`MSG-CLI-111`（protocol mismatch）の固定。文言改善は可、ID は固定

## 将来拡張のための設計フック

本 feature の `#[non_exhaustive] enum` 設計と Phase 2 移行契約により、以下の後続 feature が**最小コストで積める**。

| 将来機能 | 本 feature の設計フック | 変更インパクト | 関連 Issue |
|---------|------------------|-------------|---------|
| **ホットキー登録 IPC**（`daemon-hotkey`、未起票） | `IpcRequest` の `#[non_exhaustive] enum` に `RegisterHotkey { spec: HotkeySpec, target_record: RecordId }` バリアント追加 + daemon 側ハンドラ実装 | `IpcRequest` バリアント 1 + `IpcResponse::HotkeyRegistered { handle }` バリアント 1 + daemon 内 `HotkeyManager` モジュール新設 | 後続 |
| **クリップボード投入 IPC**（`daemon-clipboard`、未起票） | `IpcRequest::InjectClipboard { record_id, sensitive_hint: bool, auto_clear_seconds: u32 }` バリアント追加 | `IpcRequest` バリアント 1 + daemon 内 `ClipboardBackend` モジュール（OS 別実装、`arboard` 連携）+ クリア タイマー | 後続 |
| **暗号化 vault 操作**（`daemon-vault-encryption`、未起票） | `IpcRequest` に `Unlock { master_password: SerializableSecretBytes }` / `Lock` / `ChangeMasterPassword { old, new }` / `Rekey` 等を追加 | `IpcRequest` バリアント 4+ + daemon 内 `VaultUnlocker` ステート管理（`secrecy::SecretBox<[u8; 32]>` で VEK キャッシュ）+ アイドルタイムアウト + スクリーンロック連動。`IpcResponse::Error(EncryptionUnsupported)` の写像対象が**実機能に変わる** | 後続 |
| **セッショントークン認証**（`daemon-session-token`、未起票） | `IpcProtocolVersion::V2` 追加 + `IpcRequest::Handshake { client_version, session_token: SerializableSecretBytes }` への非破壊拡張。`#[non_exhaustive] enum` の効能 | プロトコル `V1` ↔ `V2` の互換性検証必須。daemon 起動時に 32 byte CSPRNG 生成 → `0700` ファイルに保存 → CLI 側 `IpcVaultRepository::connect` で読み込んで送信 | 後続 |
| **GUI クライアント**（`shikomi-gui`、未起票） | `IpcVaultRepository` を `shikomi-gui` から再利用、または別 crate（`shikomi-ipc-client` 仮）へ切出 | `shikomi-cli::io::ipc_vault_repository` が `pub` で公開済みのため、`shikomi-gui` の `Cargo.toml` で `shikomi-cli = { workspace = true }` 経由で利用可能（ただし bin crate 依存の clean さに留意、別 crate 切出が長期的に妥当） | 後続 |
| **`--ipc` 既定化（Phase 2 全面切替）** | `shikomi-cli` の clap で `args.ipc` のデフォルトを `true` に変更、または `--no-ipc` フラグ反転 | OS 自動起動（`process-model.md` §4.1 ルール 3）/ ホットキー / 暗号化が揃った時点で実施。本 feature の Phase 1 経路（`SqliteVaultRepository` 直結）はバックアップとして残す | 後続 |
| **daemon 内 vault 自動初期化**（`add` 初回時に空 vault 作成、後続） | 現状 daemon は既存 vault のみ扱う。CLI の `--ipc add` が daemon 側で `IpcResponse::Error(Persistence)` を返す挙動を改善 | `IpcRequest::CreateVault { protection_mode }` バリアント追加 or daemon 起動時 `--init` フラグでオプトイン | 後続 |
| **List フィルタリング** | `IpcRequest::ListRecords` に `Option<ListFilter>` フィールドを追加（`#[serde(default)]` で `Option` の場合は非破壊変更扱い、`./basic-design/ipc-protocol.md §バージョニングルール` の例外運用） | フィールド追加が `Option<T>` + `serde(default)` なら旧クライアントが受信 `None` で動作可能。本 feature の保守的扱いでは `V2` バリアントとして安全側で実装 | 後続 |
| **ログ レベル動的変更** | `IpcRequest::SetLogLevel { level }` バリアント追加 + `tracing_subscriber::reload` で hot reload | reload handle を IpcServer 構築時に注入 | 後続 |
| **daemon 自動起動経路**（OS launchd / systemd / Task Scheduler） | 本 feature では手動起動のみ対応。後続 feature で `tauri-plugin-autostart` または OS API 直接呼出を実装 | `process-model.md` §4.1 ルール 3 の規定を実装。daemon に `--register-autostart` / `--unregister-autostart` フラグ追加 | 後続 |

## 実装担当（坂田銀時）への引き継ぎメモ

### shikomi-core 側の変更

- **`SecretBytes` の存在確認**: `shikomi-core::secret` モジュールに `SecretBytes` が既存かを確認。**未存在の場合は本 feature で新規追加**（`SecretString` との対称性、`Vec<u8>` ベース、`zeroize::Zeroize` 実装、`Drop` で zeroize、`Debug` で `[REDACTED]` 固定）
- **`SecretBytes::as_serialize_slice` の追加**: `pub(crate)` メソッドとして `crates/shikomi-core/src/secret/bytes.rs` に追加。内部で `expose_secret` を呼ぶが、その呼出は `crates/shikomi-core/src/secret/` 内に閉じる（CI grep 監査範囲外、`./protocol-types.md §SerializableSecretBytes`）
- **`shikomi-core::ipc` モジュールの I/O 純粋性**: `Cargo.toml` に `tokio` / `rmp-serde` / `tokio-util` を追加しない。`serde::{Serialize, Deserialize}` の derive のみで完結
- **`MAX_FRAME_LENGTH` 定数**: `shikomi-core::ipc::MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024;` を pub const で公開（daemon / cli が共有）

### shikomi-daemon 側の実装

- **`#[tokio::main(flavor = "multi_thread")]` の採用**: bin の `main.rs` で multi-thread runtime。`current_thread` よりオーバーヘッドが少しだけ大きいが、複数接続の並行処理を見越して multi-thread を採用
- **panic hook は `run()` の最初の行**で登録（tokio runtime 初期化前）。hook 内で `info.payload()` / `info.message()` / `info.location()` を**参照しない**、`tracing` マクロを**呼ばない**
- **`unsafe` ブロック**: `crates/shikomi-daemon/src/permission/{unix,windows}.rs` 以外に書かない。両ファイル冒頭に `#![allow(unsafe_code)]` を明示
- **`SingleInstanceLock` の RAII**: 取得 → 使用（IpcServer に listener 移譲）→ Drop（解放）のライフサイクルを `shikomi_daemon::run()` のスコープで完結させる。`Box::leak` 等で長寿命化しない
- **`tokio::sync::Mutex` の採用**: `std::sync::Mutex` だと `await` を跨いで保持できない。ハンドラの `vault.lock().await` は `tokio::sync::Mutex` 必須
- **ロック解放のタイミング**: ハンドラ呼び出し → `drop(vault_guard)` → `framed.send` の順序。応答送信中に他接続の vault 操作をブロックしない
- **`tracing` マクロの `target` 引数**: `target: "shikomi_daemon::ipc::handler"` 等で領域分類（`./daemon-runtime.md §tracing target 規約`）。エンドユーザは `SHIKOMI_DAEMON_LOG=shikomi_daemon::ipc::handler=debug` で細粒度制御可能
- **`expose_secret()` の禁止**: `crates/shikomi-daemon/src/` 配下で 0 件（CI grep）。`SecretString::from_bytes(value.0.as_serialize_slice_for_construct(...))` 等の API は `shikomi-core` 側に閉じる
- **`rmp_serde::Raw` / `RawRef` の禁止**: `crates/shikomi-core/src/ipc/` 配下で 0 件（CI grep）。実装時に誤って使ったら CI で fail
- **graceful shutdown のタイムアウト 30 秒**: `tokio::time::timeout(Duration::from_secs(30), connections.join_all())`。タイムアウト時は `JoinSet` を drop して強制中断
- **`#[doc(hidden)]` の適用**: `lib.rs` の全 `pub` 項目（`pub async fn run`, `pub mod ipc`, `pub mod lifecycle`, `pub mod permission`, `pub mod panic_hook`）に `#[doc(hidden)]` を付ける（`shikomi-cli` と同方針）
- **`[lib] + [[bin]]` の Cargo.toml**: `[lib]` に `name = "shikomi_daemon"` / `path = "src/lib.rs"` を明示、`[[bin]]` に `name = "shikomi-daemon"` / `path = "src/main.rs"` を明示

### shikomi-cli 側の編集

- **`--ipc` フラグの clap 設定**: `#[arg(long, global = true)]` でグローバル化。`--vault-dir` / `--quiet` / `--verbose` と同列の扱い
- **`Box<dyn VaultRepository>` 化**: `args.ipc` 分岐で異なる具体型を保持するため、`Box<dyn VaultRepository>` で抽象化。既存 `VaultRepository` trait の `dyn` 互換性を確認（`Sized` 制約 / generic メソッド非含有）
- **tokio runtime の起動**: `args.ipc == true` 時のみ `tokio::runtime::Builder::new_current_thread().enable_all().build()?` で内部ランタイム起動。既定経路（`SqliteVaultRepository` 直結）は同期のままでオーバーヘッドゼロ維持
- **`block_on` の二重起動回避**: `IpcVaultRepository` の同期 trait メソッドが内部で `block_on` を呼ぶため、外側に既存ランタイムがあると panic する。CLI の `run()` は同期関数なので外側ランタイムは存在しない（安全）
- **既存 `cli-vault-commands` テストへの影響**: `--ipc` フラグのデフォルト値 `false` で既存挙動が完全維持される。既存 E2E テストは無変更で pass する想定
- **`MSG-CLI-051` の表示**: `args.ipc == true` かつ `args.quiet == false` の時のみ stderr に出力。`--quiet` 時は抑止（既存規約に整合）
- **`presenter::error::render_error` の編集**: `MSG-CLI-110` / `MSG-CLI-111` の英日併記を追加（既存 `MSG-CLI-100〜109` と同パターン）
- **`From<PersistenceError> for CliError`**: 新バリアント `DaemonNotRunning` / `ProtocolVersionMismatch` への写像を追加。既存の `Persistence(PersistenceError)` 経路は維持（`IpcDecode` / `IpcEncode` / `IpcIo` は `Persistence(...)` でラップ）

### IPC 通信実装の注意

- **`Framed::next` の戻り値**: `Option<Result<Bytes, std::io::Error>>` の構造。`None` はクライアント切断、`Some(Err)` はフレーム破損。両方を区別してエラー処理
- **MessagePack デコード失敗時のバッファ解放**: `rmp_serde::from_slice` 自体はバッファをコピーしないため、`Bytes` を drop すれば即解放。`Bytes` の参照カウントに注意（`framed.next()` で `Bytes` を取得した時点で参照カウント 1）
- **`tokio::time::timeout` のタイムアウト解像度**: ms 単位で十分。秒オーダのタイムアウト（5 秒 / 30 秒）に問題なし
- **`Arc<Notify>` の `notify_waiters`**: 1 度呼び出して全 waiter を起こす設計。`notify_one` ではなく `notify_waiters` を使う（複数接続タスクが同時に shutdown を観測する必要がある）

### `VaultRepository` trait の `dyn` 互換性確認

実装着手前に `crates/shikomi-infra/src/persistence/repository.rs` の trait 定義を確認:

- メソッドが `&self` / `&mut self` を取る → OK
- メソッドが generic（`fn xx<T>(&self, ...)`）を持たない → OK（持つ場合は `dyn` 互換でない）
- `where Self: Sized` がない → OK
- 互換性に問題があれば本 feature の実装着手前に**別 PR でリファクタ**を起こす（`./index.md §設計判断 8` の前提条件）

### CI への追加項目（実装 PR で対応）

- 本設計 PR では `.github/workflows/` を編集しない（`infra-changes.md` 同方針、`cli-vault-commands` 踏襲）
- 実装 PR で以下を追加:
  - 3 OS matrix CI（Linux / macOS / Windows）で daemon 統合テストを実行
  - `expose_secret` 0 件 grep（`crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/`）
  - `Raw` / `RawRef` 0 件 grep（`crates/shikomi-core/src/ipc/`）
  - `unsafe` ブロック `permission/` 限定確認 grep（`crates/shikomi-daemon/src/`）
  - daemon panic hook の `info.payload()` / `tracing::*!` 0 件 grep
  - `tokio::` / `rmp_serde::` import 0 件 grep（`crates/shikomi-core/src/ipc/`、純粋性監査）

### `Cargo.toml` への追加項目（実装 PR で対応）

`crates/{shikomi-core,shikomi-cli,shikomi-daemon}/Cargo.toml` の編集と、ルート `Cargo.toml` の `[workspace.dependencies]` への追加:

- `tokio-util = { version = "0.7", features = ["codec"] }` → workspace
- `rmp-serde = { version = "1.3" }` → workspace
- `bytes = { version = "1" }` → workspace
- `nix = { version = "0.29", default-features = false, features = ["socket", "fs", "user"] }` → workspace（unix のみ使用、`[target.'cfg(unix)'.dependencies]`）
- `windows-sys = { version = "0.59", features = ["Win32_System_Pipes", "Win32_System_Threading", "Win32_Security", "Win32_Foundation"] }` → workspace（windows のみ使用、`[target.'cfg(windows)'.dependencies]`）

`tokio` の features 拡張:

- 既存 `tokio` ピンを `^1.44.2` に維持（PR #27 で確定済み）
- `shikomi-cli` / `shikomi-daemon` の `Cargo.toml` で features = `["rt-multi-thread", "net", "io-util", "sync", "signal", "time", "macros"]` を有効化

## 運用注記

### `RUST_BACKTRACE` について

- daemon 側でも CLI 側と同じく **CI 環境では `RUST_BACKTRACE=0` を推奨**（panic hook の固定文言のみ出力、バックトレース不要、ログ肥大化と secret 漏洩リスクの両方を抑止）
- ローカル開発で詳細デバッグが必要な場合は個別に `RUST_BACKTRACE=1` を設定
- `cli-vault-commands` の `future-extensions.md` と同方針

### `SHIKOMI_DAEMON_LOG` について

- daemon の tracing レベルを制御する環境変数
- 値: `error` / `warn` / `info`（デフォルト）/ `debug` / `trace`、または target 別細粒度（`shikomi_daemon::ipc::handler=debug,info`）
- `EnvFilter::try_from_env("SHIKOMI_DAEMON_LOG")` で読込、未設定時は `EnvFilter::new("info")` フォールバック
- secret 露出は CI grep + 設計規約で監査（`tracing` マクロに `IpcRequest` 全体を渡さない、variant 名のみ）

### daemon の手動起動方法

本 feature 完成時点では daemon は手動起動のみ対応。**`MSG-CLI-110` hint の文面と完全一致**させる（`basic-design/error.md §MSG-CLI-110 確定文面`、ペガサス指摘 ②）:

- **Linux / macOS（基本）**: `shikomi-daemon &` でバックグラウンド起動 / `nohup shikomi-daemon &` で nohup 化
- **Windows（基本）**: PowerShell で `Start-Process -NoNewWindow shikomi-daemon`
- **systemd 環境（Linux）**: `~/.config/systemd/user/shikomi-daemon.service` を手動配置 → `systemctl --user start shikomi-daemon`（後続 feature で自動配置）
- **launchd 環境（macOS）**: `~/Library/LaunchAgents/dev.shikomi.daemon.plist` を手動配置 → `launchctl kickstart gui/$(id -u)/dev.shikomi.daemon`（後続 feature で自動配置）

**`shikomi daemon start` 等の CLI サブコマンドは追加しない**（`process-model.md` §4.1 規定で daemon は独立バイナリ扱い、ペガサス指摘 ①）。ユーザ向けドキュメント / hint 文面 / エラーメッセージの全てで**実バイナリ名 `shikomi-daemon` のみ**を案内する。

OS 自動起動の実装は後続 feature の責務（`process-model.md` §4.1 ルール 3）。

### Phase 1 と Phase 2 の併存運用

本 feature 完成後の運用パターン:

- **既定（Phase 1）**: `shikomi list` 等で SQLite 直結。daemon 起動不要
- **オプトイン（Phase 2）**: `shikomi --ipc list` 等で daemon 経由。daemon を事前に手動起動
- **同時実行**: CLI 直結と daemon が同一 vault.db に同時アクセスする可能性あり → `VaultLock`（既存 `shikomi-infra::persistence::lock`）で一方が `PersistenceError::Locked` で Fail Fast（設計通り、データ破損なし）

ユーザは特定の vault.db を「daemon 経由でのみ操作する」または「CLI 直結のみ」のいずれかを意識的に選択する。混在は避ける運用が望ましい（後続 feature で UI 上で明示化）。

## 残タスク（本 PR では扱わない）

- **実装 PR**: `shikomi-core::ipc` / `shikomi-daemon` 本体 / `shikomi-cli::io::ipc_*` のコード追加（別 PR、本設計 PR のマージ後）
- **`tech-stack.md` 更新**: `[workspace.dependencies]` 追加に伴う §4.5 / §4.6 反映（実装 PR で同時実施、本設計 PR には含めない、`cli-vault-commands` 踏襲）
- **CI ジョブ追加**: 3 OS matrix daemon 統合テスト、TC-CI-016〜023 の grep チェック（実装 PR で対応）
- **`docs/features/cli-vault-commands/` のテスト設計編集**: `--ipc` 経路の E2E テストケース追加はテスト設計担当（涅マユリ）の責務（本 PR では設計書のみ、テスト設計は別工程）
- **OS 自動起動実装**（後続 feature）: `process-model.md` §4.1 ルール 3 / 4 の実装は本 Issue のスコープ外
- **ホットキー / クリップボード / 暗号化 / GUI**（後続 feature）: 本 feature の `#[non_exhaustive] enum` 設計を活用して各 Issue で追加

## 結語

本 feature の設計は、Clean Architecture の Phase 2 移行契約を**実体化**する。`cli-vault-commands` で確立した「`run()` の 1 行差し替えで Repository 構築のみ変更」が、`IpcVaultRepository::connect` の追加と `--ipc` 分岐により完了する。

`shikomi-core::ipc` の I/O 不在性、`#[non_exhaustive] enum` による非破壊拡張、`SerializableSecretBytes` の `expose_secret` 不使用契約、`SingleInstanceLock` の race-safe RAII——いずれも**型・責務・依存方向・Fail Fast の契約**として設計書に残した。

実装担当（坂田銀時）は、本設計書の契約を満たす限りにおいて実装詳細を自由に決定できる。後続 feature 担当は `IpcRequest` / `IpcResponse` の `#[non_exhaustive]` 拡張ポイントを使って、ホットキー / クリップボード / 暗号化 / GUI を**1 バリアント追加 + 1 ハンドラ実装**で積める。

疑似コード・実装サンプルを書かずに、**型・契約・責務分離**のみを設計書に残した。これが daemon 骨格 + IPC プロトコル feature の最大の価値である。
