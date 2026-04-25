# 要求分析書

<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/requirements-analysis.md -->

## 人間の要求

> Issue #26（feat(shikomi-daemon): daemon プロセス骨格 + IPC プロトコル (daemon-ipc Phase 1)）:
>
> 「`develop @ 43c7392` 時点で `shikomi-daemon/src/main.rs` は `fn main() {}` の 3 行に過ぎず、**MVP 看板「任意のグローバルホットキーを押下すると登録文字列がフォアグラウンドアプリへ投入される」の常駐プロセス実体が存在しない**。後続 Issue（グローバルホットキー層・クリップボード投入層・暗号化モード・GUI）はいずれも daemon 骨格と IPC プロトコルの上に載る設計であり、本 Issue が全ての土台となる。」
>
> キャプテン決定: **Sub-issue 分割なし、本 1 Issue で推進**。

依頼主（まこちゃん）は「Issue #26 を土台として後続 feature が積めるよう、daemon プロセス骨格と IPC プロトコルを最小スコープで実装する」順序を開発チームに委ねており、チーム内で「`process-model.md` §4.1.1 で確定済みの Phase 2 移行パスを実体化する」「初期 IPC 操作は CLI Phase 1 と 1:1 対応の 4 本に限る（YAGNI）」「ホットキー / 暗号化 / GUI は本 Issue のスコープ外」の 3 軸でこの順序に合意した。

## 背景・目的

- **現状: daemon プロセスが存在しない**。`crates/shikomi-daemon/src/main.rs` は `fn main() {}` の 3 行スケルトン。`docs/architecture/context/process-model.md` §4.1 ルール 1「CLI/GUI は直接 vault を開かない。IPC 経由でのみ daemon に依頼する」は最終形態（Phase 2）の規定だが、その実体が一行も存在しない。
- **`cli-vault-commands` feature は Phase 2 移行契約を既に提示済み**。`docs/features/cli-vault-commands/detailed-design/future-extensions.md` の将来拡張テーブル先頭行で「`run()` で `SqliteVaultRepository::from_directory(&path)` を `IpcVaultRepository::connect(socket_path)` に差し替えるだけ。`UseCase` / `Presenter` は無変更」を契約として宣言済み。本 feature はこの契約の**実体化**である。
- **後続 feature の土台**: グローバルホットキー層（X11/Wayland/macOS/Windows の `HotkeyBackend`）、クリップボード投入層（`arboard` + sensitive hint）、暗号化モード（`vault encrypt`/`decrypt`/`rekey` + keyring）、`shikomi-gui`（Tauri v2）はいずれも「daemon が常駐し、CLI/GUI が IPC 経由で daemon の vault 真実源にアクセスする」前提で設計されている。daemon 骨格と IPC プロトコルがなければ、いずれの後続 feature も着手できない。
- **Phase 2 全面切替は本 Issue のスコープ外**。本 Issue では `--ipc` オプトインフラグ経由の限定導入に留め、CLI 既定経路は SQLite 直結（Phase 1）を維持する。フラグ廃止と既定切替はホットキー / 暗号化モードが揃って「daemon 常駐が MVP の通常動線」となった時点で別 Issue として扱う（`process-model.md` §4.1.1）。
- **ビジネス価値**: 本 feature 完成後、`shikomi --ipc list` で daemon 経由の vault 操作が成立する最初のバージョンとなる。後続 feature が「daemon に乗る」前提で実装できるため、開発速度が層構造に沿って加速する。

## 議論結果

工程 0（アーキテクチャ更新 PR #27）で以下が**確定済み**（外部レビュー人間承認 + 内部レビュー 3 担当合格）:

- `IpcVaultRepository` の配置先: **`shikomi-cli::io::IpcVaultRepository`**（CLI クライアント層）と **`shikomi-daemon` 内 IPC サーバハンドラ**の 2 実装で同じ `VaultRepository` trait を満たす。`shikomi-infra` には IPC クライアントを置かない（Clean Architecture の依存方向保持、`shikomi-infra` を `tokio` UDS/Named Pipe まで抱え込ませない）
- IPC スキーマの単一真実源: **`shikomi-core::ipc`** に `IpcRequest` / `IpcResponse` / `IpcProtocolVersion` の型定義のみ配置（I/O を持たない、§2.1 の方針に整合）
- シングルインスタンス保証（Unix）: **`flock(LOCK_EX | LOCK_NB)` → `unlink` → `UnixListener::bind`** の 3 段階。PID ファイルは作成しない（`flock` 自体が「プロセス生きている間だけ保持」の排他性を OS カーネルが保証、stale PID エッジケースが原理的に発生しない）
- シングルインスタンス保証（Windows）: Named Pipe を `FILE_FLAG_FIRST_PIPE_INSTANCE` で排他作成
- ピア認証: `SO_PEERCRED`（Linux）/ `LOCAL_PEERCRED`（macOS）/ `GetNamedPipeClientProcessId` + `OpenProcessToken`（Windows）で接続元 UID/SID を取得し検証——**Issue #26 スコープ内、必須**
- セッショントークン認証: **Issue #26 スコープ外**。後続 Issue で `IpcProtocolVersion::V2` 追加と同時に `Handshake { client_version, session_token }` への非破壊拡張として実装（`#[non_exhaustive] enum` の効能）
- プロトコルバージョニング: `IpcProtocolVersion` を `#[non_exhaustive] enum` で定義し、破壊的変更時にバリアント追加で明示（`VaultVersion` の前例踏襲）
- 初期サポート IPC 操作: **vault 操作 4 本（`List` / `Add` / `Edit` / `Remove`）+ プロトコル制御 1 本（`Handshake`）の計 5 バリアント**。ホットキー登録 API・暗号化操作 API は後続 Issue で variant 追加（YAGNI）
- 通信プロトコル: MessagePack（`rmp-serde` `^1.3`）over length-delimited framed stream（`tokio-util::codec::LengthDelimitedCodec`、最大フレーム長 16 MiB）
- バージョンピン厳密化: tokio `^1.44.2`（RUSTSEC-2025-0023 patched 範囲完全内包）、rmp-serde `^1.3`（`Raw`/`RawRef` 不使用契約を `shikomi-core::ipc` に明文化、RUSTSEC-2022-0092 への再接触を構造的に遮断）

工程 1（タスク分析）で以下を合意:

- 設計担当（セル）提案: 新規 feature `daemon-ipc` 作成 + `cli-vault-commands` 既存編集の 2 軸で進める
- **キャプテン決定: 承認、本 1 Issue で推進**
- スコープ絞り込み: **vault 操作は CLI Phase 1 と 1:1 対応の 4 本のみ**。daemon が将来抱えるホットキー登録 / クリップボード投入 / 暗号化 / 自動アンロックは別 feature として後続 Issue で起票
- 対応する vault モード: **平文モードのみ**（暗号化モード対応は cli-vault-commands と同様に `EncryptionUnsupported` を返す）。暗号化対応は将来 feature `daemon-vault-encryption`（仮称、未起票）
- Sub-issue 分割: **不要**。本 1 Issue で daemon 骨格 + IPC スキーマ + クライアント骨格をまとめて完成させる（部分実装で develop に落とすと Phase 2 移行契約が宙に浮く）

## ペルソナ

本 feature のプライマリ体験者は**開発貢献者**（後続 feature を daemon の上に積む Rust 実装担当）であり、エンドユーザーは `--ipc` オプトインフラグを明示指定した時のみ間接的に体験する。エンドユーザーペルソナ（田中 / 山田 / 佐々木）は `docs/architecture/context/overview.md` §3.2 を参照。

| ペルソナ名 | 役割 | 技術レベル | 利用文脈 | 達成したいゴール |
|-----------|------|-----------|---------|----------------|
| 野木 拓海 — **プライマリ** | 後続 feature の Rust 実装担当 | Rust 中級（`tokio` / `serde` / `rmp-serde` / Unix IPC 実務経験） | ホットキー feature / 暗号化モード / GUI feature を着手する際、daemon 骨格と IPC スキーマを参照して「`IpcRequest` バリアント追加 + ハンドラ実装」のみで機能拡張ができることを確認する | 本 feature の `IpcRequest` / `IpcResponse` 型と daemon ライフサイクル（起動 / 単一インスタンス確保 / graceful shutdown）が見えており、`#[non_exhaustive]` 拡張ポイントから後続 feature が無理なく積める |
| 志摩 凛 — **セカンダリ** | OSS コントリビュータ | Rust 初級 | GitHub Issue を拾い、`shikomi --ipc list` 経路の小バグ修正 PR や IPC ハンドラのテストケース追加 PR を提案 | クライアント / サーバ両層の責務分離が明瞭で、シリアライズ層を触る勇気が出るドキュメント |
| 山田 美咲 — **セカンダリ** | 日常ユーザー兼 OSS コントリビュータ | Wayland / X11 / Named Pipe を区別できる | Phase 2 移行前の `--ipc` オプトインを使い、daemon 経由経路と SQLite 直結経路の挙動が同一であることを確認する | `shikomi --ipc list` と `shikomi list` で**bit 同一の結果**が得られる |
| 田中 俊介 | 日常ユーザー（GUI 主、CLI 非常用） | ChatGPT / Slack は使えるが PowerShell は触れない | 本 feature の対象外（`--ipc` を明示指定しない限り Phase 1 経路のまま、daemon の存在を意識しない） | 対応なし。daemon 自動起動 / GUI 経路は後続 feature で扱う |

**ペルソナへの設計インパクト**:

- 野木: `shikomi-core::ipc` が型定義のみで I/O を持たないため、後続 feature は `IpcRequest::HotkeyRegister { ... }` のような variant 追加と daemon ハンドラ実装のみで拡張可能。`#[non_exhaustive] enum` により非破壊変更で扱える
- 志摩: `shikomi-daemon` のディレクトリ構造を `ipc::server` / `ipc::framing` / `ipc::handshake` / `lifecycle` / `permission` で責務分離し、レビュー容易性を確保
- 山田: `--ipc` オプトインで daemon 経由の経路を試せ、SQLite 直結との同一性を E2E テストでも検証可能
- 田中: 既定動作（`--ipc` なし）が無変更のため、本 feature が田中の体験を悪化させない

## 前提条件・制約

- **Issue #1 / Issue #4 / Issue #10 / Issue #21 完了済み**: `shikomi-core` vault ドメイン型、`shikomi-infra::VaultRepository` trait、`SqliteVaultRepository` 実装、Unix パーミッション `0600` / Windows owner-only DACL 強制、`cli-vault-commands` の 4 サブコマンド全て merge 済み。本 feature は**既存の `VaultRepository` trait を再利用**し、新規 trait は導入しない。
- **PR #27 マージ済み**: `process-model.md` §4.1 / §4.1.1 / §4.2、`tech-stack.md` §2.1（IPC 4 行）、`threat-model.md` §S 行 / §A07 行で本 feature のスコープ・トランスポート・認証・プロトコルバージョニング・依存ピンが既に確定済み。本 feature の設計はこれらの規定と**完全整合**することが必須。
- **暗号化モード未対応**: `shikomi-infra` の永続化層は平文モードのみ対応。本 feature の daemon も平文モードのみを `IpcResponse` で返す。暗号化 vault を検出したら **Fail Fast で `IpcResponse::Error(EncryptionUnsupported)`** を返す。`vault encrypt` / `decrypt` / `rekey` の IPC 操作は**本 feature のスコープ外**（後続 feature `daemon-vault-encryption` で導入）。
- **MVP Phase 1（CLI 直結）併存**: 本 feature 完成後も、CLI の既定経路は `SqliteVaultRepository` 直結を維持する。`--ipc` オプトインフラグ経由でのみ daemon 経由経路が有効化される。Phase 2 全面切替（フラグ廃止 / 既定が IPC 経由）は本 Issue のスコープ外。
- **ホットキー / クリップボード非対象**: 本 feature はホットキー購読・クリップボード投入を扱わない。`HotkeyBackend` / `ClipboardBackend` は後続 feature で daemon に追加する。
- **自動起動 / GUI 起動経路非対象**: `process-model.md` §4.1 ルール 3〜4 の「OS 自動起動」「GUI 起動時の daemon 起動」は本 Issue の受入基準に含まない。daemon は手動起動（`cargo run -p shikomi-daemon` / `shikomi-daemon` バイナリ直接起動）のみ対応する。OS 自動起動は GUI feature 着手時に統合的に設計する。
- **`[workspace.dependencies]` 経由の依存追加**: `tokio` `^1.44.2`、`tokio-util` `^0.7`、`rmp-serde` `^1.3`、`serde` 既存（`derive` feature）、`time` 既存。各 crate 側 `Cargo.toml` は `{ workspace = true }` のみ書く（`tech-stack.md` §4.4）。
- **3 OS matrix CI**: 本 feature は Linux / macOS / Windows 3 OS で動作する必要があり、既存の `.github/workflows/test-core.yml` / `test-infra.yml` と同型の matrix で daemon 統合テストを CI 必須とする。

## 機能一覧

| 機能ID | 機能名 | 概要 | 優先度 |
|--------|-------|------|--------|
| REQ-DAEMON-001 | daemon プロセス起動とランタイム初期化 | `tokio` 多重スレッドランタイム起動、`tracing-subscriber` 初期化、`SHIKOMI_DAEMON_LOG` 環境変数でログレベル制御 | 必須 |
| REQ-DAEMON-002 | シングルインスタンス保証（Unix） | `flock(LOCK_EX \| LOCK_NB)` → `unlink` → `UnixListener::bind` の 3 段階で起動。二重起動は exit 非 0 で拒否 | 必須 |
| REQ-DAEMON-003 | シングルインスタンス保証（Windows） | Named Pipe を `FILE_FLAG_FIRST_PIPE_INSTANCE` で排他作成。既存インスタンスがある場合は exit 非 0 で拒否 | 必須 |
| REQ-DAEMON-004 | IPC エンドポイント作成 | UDS（`$XDG_RUNTIME_DIR/shikomi/daemon.sock` または `~/Library/Caches/shikomi/daemon.sock`、ソケット `0600` / 親ディレクトリ `0700`）/ Named Pipe（`\\.\pipe\shikomi-daemon-{user-sid}`、SDDL で owner-only） | 必須 |
| REQ-DAEMON-005 | ピア資格情報検証 | Unix: `SO_PEERCRED`（Linux）/ `LOCAL_PEERCRED`（macOS）で UID 取得し daemon 所有者 UID と一致確認。Windows: `GetNamedPipeClientProcessId` → `OpenProcessToken` で SID 取得し一致確認 | 必須 |
| REQ-DAEMON-006 | プロトコルハンドシェイク | クライアント接続直後に `IpcRequest::Handshake { client_version }` を受信し `IpcProtocolVersion` の一致判定。不一致なら `IpcResponse::ProtocolVersionMismatch { server, client }` 返送 → 切断（Fail Fast） | 必須 |
| REQ-DAEMON-007 | vault 操作 IPC ハンドラ（List） | `IpcRequest::ListRecords` を受け、daemon が保持する `Vault` から `Vec<RecordSummary>` を返す | 必須 |
| REQ-DAEMON-008 | vault 操作 IPC ハンドラ（Add） | `IpcRequest::AddRecord { kind, label, value, ... }` を受け、`Vault::add_record` → `repo.save` → `IpcResponse::Added { id }` | 必須 |
| REQ-DAEMON-009 | vault 操作 IPC ハンドラ（Edit） | `IpcRequest::EditRecord { id, label_opt, value_opt, ... }` を受け、`Vault::update_record` → `repo.save` → `IpcResponse::Edited` | 必須 |
| REQ-DAEMON-010 | vault 操作 IPC ハンドラ（Remove） | `IpcRequest::RemoveRecord { id }` を受け、`Vault::remove_record` → `repo.save` → `IpcResponse::Removed` | 必須 |
| REQ-DAEMON-011 | フレーミングと最大長制限 | `tokio_util::codec::LengthDelimitedCodec` で 4 バイト LE prefix + payload。最大フレーム長 16 MiB を daemon 側で強制（DoS 対策、超過は切断） | 必須 |
| REQ-DAEMON-012 | MessagePack シリアライズ / デコード | `rmp-serde` で `IpcRequest` / `IpcResponse` を `Vec<u8>` ↔ struct 変換。デコード失敗は該当接続のみ切断、daemon プロセスはクラッシュしない（graceful degradation） | 必須 |
| REQ-DAEMON-013 | 暗号化 vault 拒否（Fail Fast） | daemon 起動時に `repo.load()` で得た `Vault::protection_mode() == Encrypted` なら全 IPC 操作で `IpcResponse::Error(EncryptionUnsupported)` を返す | 必須 |
| REQ-DAEMON-014 | graceful shutdown | `SIGTERM` / `SIGINT`（Unix）/ `CTRL_CLOSE_EVENT`（Windows）受信で in-flight リクエスト完了待機 → `repo` 解放（`VaultLock` 解除）→ ソケット `unlink` / Named Pipe close → exit 0 | 必須 |
| REQ-DAEMON-015 | `IpcVaultRepository` クライアント | `shikomi-cli::io::IpcVaultRepository` を新設（`VaultRepository` trait 実装）。`connect(socket_path)` で daemon 接続、`load` / `save` / `exists` を IPC 越しに実行 | 必須 |
| REQ-DAEMON-016 | CLI `--ipc` オプトインフラグ | `shikomi-cli` のグローバルフラグ `--ipc` を追加。指定時のみコンポジションルートで `IpcVaultRepository::connect` を構築（既定は `SqliteVaultRepository::from_directory`） | 必須 |
| REQ-DAEMON-017 | daemon 未起動時の Fail Fast | `--ipc` 指定で daemon が起動していない（接続不可）なら `CliError::DaemonNotRunning` を返し、`shikomi daemon start` を案内するヒントを stderr に出力（終了コード 1） | 必須 |
| REQ-DAEMON-018 | スキーマ単一真実源（DRY） | `IpcRequest` / `IpcResponse` / `IpcProtocolVersion` / `RecordSummary` を `shikomi-core::ipc` のみに定義。daemon / cli / gui で再定義しない | 必須 |
| REQ-DAEMON-019 | プロトコルバージョン管理 | `IpcProtocolVersion::V1` を初期バージョンとして `#[non_exhaustive] enum` で定義。破壊的変更時に `V2` 等のバリアント追加で明示（VaultVersion 前例踏襲） | 必須 |
| REQ-DAEMON-020 | secret マスキング（IPC 経路） | `IpcRequest::AddRecord` / `IpcResponse::Records` 等の secret フィールドはサーバ・クライアント両側で `expose_secret()` を呼ばずに `SecretBytes` として運搬。`Debug` 経由でログに secret が出ない | 必須 |
| REQ-DAEMON-021 | エラー応答の型化 | daemon は `IpcResponse::Error(IpcErrorCode)` で構造化エラーを返す（`NotFound` / `EncryptionUnsupported` / `Persistence` / `Domain` / `Internal`）。クライアント側で `match` 可能 | 必須 |

## Sub-issue分割計画

該当なし — 理由: 本 feature は daemon プロセス骨格 + IPC スキーマ + クライアント骨格の 3 点が**1 つの Phase 2 移行契約を構成**しており、別 PR に分割すると中途半端な状態（例: スキーマだけ merge、daemon が `fn main() {}` のまま）の develop が生じる。`cli-vault-commands` 完了時の Phase 2 移行契約（`run()` 1 行差し替え）を**実体化する単一 PR** で扱う。

| Sub-issue名 | スコープ | 依存関係 |
|------------|---------|---------|
| — | — | — |

## 非機能要求

| 区分 | 指標 | 目標 |
|-----|------|------|
| 応答時間（IPC 単発、平文モード） | `shikomi --ipc list`（100 レコード）の総実行時間（CLI 起動 → IPC 接続 → ハンドシェイク → ListRecords 応答 → 出力 → 終了） | p95 200 ms 以下（直結 Phase 1 の 100 ms に対し、CLI 起動 + IPC 接続 / シリアライズ / フレーミングのオーバーヘッドを 2 倍程度のバジェットで許容） |
| ホットキー応答時間バジェット先取り | daemon が常駐状態で受ける IPC リクエスト 1 本の処理時間（接続済み接続 1 本に対する `ListRecords` 応答） | p95 50 ms 以下（`nfr.md` §9 の平文モード p95 50 ms に整合）。IPC 経路上のオーバーヘッドを定量化し、後続のホットキー feature が「daemon 内 vault 操作 50 ms + クリップボード操作 50 ms = 合計 100 ms」を達成可能とする |
| シリアライズ最大フレーム長 | `LengthDelimitedCodec::max_frame_length` 設定値 | 16 MiB（DoS 対策、`process-model.md` §4.2 規定） |
| 同時接続数 | daemon が同時にハンドル可能な IPC 接続数 | 32 以上（後続のホットキー feature が daemon 経由で複数操作を並行投入する余地を確保。MVP では実用上 2〜4 程度を想定） |
| バイナリサイズ | `shikomi-daemon` 単独リリースビルドサイズ（LTO + strip 後） | 8 MB 以下（`tokio` フル + `rmp-serde` + `shikomi-infra` 依存） |
| ビルド時間 | `cargo build -p shikomi-daemon` 初回 | 120 秒以下（`tokio` フル feature + `tokio-util::codec` 由来のコンパイル時間を許容） |
| ユニット / 結合テスト実行時間 | `cargo test -p shikomi-daemon` 全テスト | 30 秒以下（`tokio::test` で in-process IPC テストを `tempfile` ソケットパスで並列実行） |
| 秘密値リーク耐性（IPC 経路） | `IpcRequest::AddRecord` 経路で投入した secret 値の `tracing` ログ / panic メッセージへの露出 | 0 件（`SecretBytes` を `Debug` 経由でログに出さない、daemon の panic hook は CLI と同型 fixed-message） |
| `expose_secret` 呼び出し経路 | `crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/` 配下 | 0 件（serde 経由で `SecretBytes` のシリアライズが完結する設計、CI grep で監査） |
| プロトコル互換性 | `IpcProtocolVersion::V1` のバイナリレイアウト | 後続 Issue で `V2` 追加時、`V1` クライアントが daemon `V2` に接続したらハンドシェイクで `ProtocolVersionMismatch` で Fail Fast（接続継続させない） |
| RustSec advisory ゼロ | `tokio` `^1.44.2` / `tokio-util` `^0.7` / `rmp-serde` `^1.3` の active advisory | 0 件（PR #27 で確認済み証跡を `tech-stack.md` §2.1 に記載済み、cargo-deny で継続監査） |

## 受入基準

| # | 基準 | 検証方法 |
|---|------|---------|
| 1 | daemon が Linux / macOS / Windows 3 OS でそれぞれの IPC エンドポイントに listen できる | E2E テスト（`tokio::test` で daemon を spawn し、`UnixStream::connect` / `NamedPipeClient::connect` で接続） |
| 2 | `shikomi --ipc list` が daemon 経由で既存 vault の list を返し、**SQLite 直結版（`shikomi list`）と bit 同一の結果**を得る | E2E テスト（`tempfile::TempDir` で同じ vault を 2 経路で読み、stdout の正規化比較） |
| 3 | daemon の二重起動が exit 非 0 で拒否される | E2E テスト（同じソケットパスで 2 つ目の daemon プロセスを spawn し終了コード非 0 を確認） |
| 4 | Unix で SIGKILL された daemon の残留 socket ファイルがあっても、次の daemon 起動が成功する | E2E テスト（手動で stale socket を作成 → daemon 起動 → bind 成功） |
| 5 | `SIGTERM` / `Ctrl+C`（Windows は `CTRL_CLOSE_EVENT`）で graceful shutdown する | E2E テスト（in-flight リクエストの応答完了を確認後、ソケット / Named Pipe が削除されていることを確認） |
| 6 | プロトコルバージョン不一致で daemon が `ProtocolVersionMismatch` を返し、クライアントが終了コード非 0 で終わる | UT + E2E（`IpcProtocolVersion` を `unsafe` ハックで偽造したクライアントスタブを作成） |
| 7 | MessagePack ペイロード破損で daemon が当該接続のみ切断、プロセスはクラッシュしない | UT（`tokio::io::duplex` で破損バイト列を流し込み、daemon が他接続にレスポンスを返し続けることを確認） |
| 8 | UDS パーミッションが `0600`、親ディレクトリが `0700` | E2E テスト（`std::fs::metadata` で確認） |
| 9 | Named Pipe DACL が owner-only（`GetNamedSecurityInfoW` で所有者 SID のみ ACE 1 個） | E2E テスト（既存 `vault-persistence` の DACL 検証手法を踏襲） |
| 10 | ピア UID / SID 不一致の接続が即切断される | E2E テスト（root から daemon への接続を試みる Linux ケース、別ユーザでの Windows ケース） |
| 11 | 暗号化 vault に対し全 IPC 操作が `IpcResponse::Error(EncryptionUnsupported)` を返し、CLI 側で終了コード 3 になる | E2E テスト（暗号化ヘッダのみ持つフィクスチャ vault を `--ipc` 経由で操作） |
| 12 | `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo deny check` すべて pass | CI |
| 13 | 3 OS matrix CI（既存 `test-core.yml` / `test-infra.yml` と同型）で daemon 統合テストが PASS | CI |
| 14 | `process-model.md` / `tech-stack.md` / `threat-model.md` への変更が PR #27（工程 0）で完了済みで、本 Issue では追加変更が**発生しない** | 設計レビュー（差分確認） |
| 15 | `shikomi-core::ipc` モジュールが I/O を持たず、純粋な型定義のみ（`tokio` / `rmp-serde` を `shikomi-core` の依存に追加しない） | コード grep（`crates/shikomi-core/src/ipc/` 配下に `tokio::` / `rmp_serde::` の参照が無いこと、ただし `serde::{Serialize, Deserialize}` derive のみ許可） |
| 16 | `shikomi-cli/src/io/ipc_vault_repository.rs`（仮）が `VaultRepository` trait を実装し、CLI の `run()` から差し替え 1 行で接続経路を切り替えられる | 設計レビュー + コード grep（`SqliteVaultRepository` / `IpcVaultRepository` の参照が `lib.rs::run()` 1 関数内に閉じる） |
| 17 | `expose_secret` が `crates/shikomi-core/src/ipc/` / `crates/shikomi-cli/src/io/` / `crates/shikomi-daemon/src/` で 0 件 | CI grep（`scripts/ci/audit-secret-paths.sh` 拡張） |
| 18 | `rmp_serde::Raw` / `RawRef` が `crates/shikomi-core/src/ipc/` で 0 件（`tech-stack.md` §2.1 で明文化された不使用契約の遵守） | CI grep |

## 扱うデータと機密レベル

本 feature は IPC 層であり、**vault 操作の入力 / 出力が IPC 経路で流れる**ため、各データの機密レベルと運搬経路を明示する。`cli-vault-commands` の同名セクションを踏襲しつつ、IPC 境界での扱いを追加する。

| データ | 機密レベル | 本 feature での扱い |
|-------|----------|-----------------|
| レコードラベル（`AddRecord.label` / `EditRecord.label_opt`） | 低〜中 | `RecordLabel` ドメイン型でシリアライズ（`String` 内部表現を `serde` で運搬）。IPC 経路上は平文で流れる（UDS / Named Pipe の OS プロセス境界保護に依存） |
| Text レコードの値（`AddRecord.value` / `EditRecord.value_opt` の Text kind 由来） | 低〜高（ユーザ判断） | `SecretBytes`（`shikomi-core` で導入予定の `Vec<u8>` 系 secret ラッパ）として送受信。`Debug` 経路で `[REDACTED]` 固定 |
| Secret レコードの値（同上、Secret kind 由来） | 最高 | 同上。**`expose_secret()` を `shikomi-core::ipc` / `shikomi-cli::io` / `shikomi-daemon` のいずれでも呼ばない**（CI grep で監査）。serde シリアライズ経路上で `SecretBytes` のバイト列が直接 MessagePack バイナリに変換される |
| RecordId（`EditRecord.id` / `RemoveRecord.id` / `IpcResponse::Added.id`） | 低 | `RecordId`（`Uuid` 内部表現）でシリアライズ |
| ピア UID / SID（`SO_PEERCRED` / `GetNamedPipeClientProcessId` 取得値） | 低 | daemon ローカル変数で扱い、ログ出力時は数値のみ。secret に該当しない（`tracing::warn!` で接続失敗時に記録可能） |
| プロトコルバージョン（`IpcProtocolVersion::V1` 等） | 低 | enum バリアント名としてログ出力可。`Debug` 経路で問題なし |
| エラーコード（`IpcErrorCode::NotFound` 等） | 低 | enum バリアント名として運搬 |
| マスターパスワード / BIP-39 リカバリコード | 最高 | **本 feature では扱わない**（暗号化モード未対応のため）。将来の `daemon-vault-encryption` feature で `IpcRequest::Unlock { master_password: SecretBytes }` 等の variant 追加で導入 |
| セッショントークン | 最高 | **本 feature では扱わない**（Issue #26 スコープ外、`process-model.md` §4.2 認証 (2) で scope-out 確定）。後続 Issue で `Handshake { client_version, session_token: SecretBytes }` 拡張時に同じ `SecretBytes` 経路で運搬 |

設計担当・実装担当は、**Secret 値は `SecretBytes` 型で daemon の serde デシリアライズ → `RecordPayload::Plaintext(SecretString::from_bytes(...))` まで剥ぎ落とさずに運ぶ**こと。`Vec<u8>` / `&[u8]` に剥ぎ落とす経路を作った瞬間、`Debug` 経由の漏洩リスクが生じる。**`shikomi-core::SecretBytes` のシリアライズ実装は本 feature で `shikomi-core::ipc` モジュール内に配置する `SerializableSecretBytes` ラッパで隔離する**（暗号化モード時に MessagePack 経路へ秘密が直接流れることを設計レベルで意識化、詳細は基本設計 `basic-design/security.md §SecretBytes のシリアライズ契約` 参照）。
