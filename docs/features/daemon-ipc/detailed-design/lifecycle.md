# 詳細設計書 — lifecycle（SingleInstanceLock / シングルインスタンス先取り / graceful shutdown）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/lifecycle.md -->
<!-- 兄弟: ./index.md, ./protocol-types.md, ./daemon-runtime.md, ./ipc-vault-repository.md, ./composition-root.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。リソースのライフサイクルは**取得 → 使用 → 解放**の 3 段階で記述する。

## モジュール配置

```
crates/shikomi-daemon/src/lifecycle/
  mod.rs                      # 再エクスポート
  single_instance.rs          # SingleInstanceLock RAII
  shutdown.rs                 # シグナル受信 + in-flight 待機
```

## `SingleInstanceLock`

- 配置: `crates/shikomi-daemon/src/lifecycle/single_instance.rs`
- 型: `pub struct SingleInstanceLock`
- フィールド（private）:
  - **Unix**: `lock_file: std::fs::File`（`flock` 保持中）+ `socket_path: PathBuf`（解放時 unlink 用）+ `listener: Option<tokio::net::UnixListener>`（移譲用、`take_listener()` で消費）
  - **Windows**: `pipe_server: Option<tokio::net::windows::named_pipe::NamedPipeServer>`（`take_listener()` で消費）+ `pipe_name: String`（ログ用）

### `acquire` メソッド（Unix）

- シグネチャ: `pub fn acquire_unix(socket_dir: &Path) -> Result<Self, SingleInstanceError>`
- `cfg(unix)` 配下のみコンパイル

**処理順序（3 段階厳守）**:

1. **親ディレクトリ確保**:
   - `std::fs::create_dir_all(socket_dir)?` で `0700` で作成（既存なら no-op）
   - `std::fs::set_permissions(socket_dir, Permissions::from_mode(0o700))?` で明示的に `0700` 適用（umask 影響を避ける）
   - `std::fs::metadata(socket_dir)?` で `mode & 0o777 == 0o700` を再検証（不一致なら `Err(SingleInstanceError::InvalidDirectoryPermission { path, got })`）

2. **lock ファイル取得 + flock**:
   - `let lock_path = socket_dir.join("daemon.lock");`
   - `let lock_file = OpenOptions::new().create(true).read(true).write(true).mode(0o600).open(&lock_path)?;`
   - `nix::fcntl::flock(lock_file.as_raw_fd(), FlockArg::LockExclusiveNonblock)` で `flock(LOCK_EX | LOCK_NB)` 獲得試行
     - `Err(nix::errno::Errno::EWOULDBLOCK)` → `Err(SingleInstanceError::AlreadyRunning { lock_path })`
     - その他 → `Err(SingleInstanceError::Lock(err))`

3. **stale socket の除去**:
   - `let socket_path = socket_dir.join("daemon.sock");`
   - `std::fs::remove_file(&socket_path)` の `Result`:
     - `Ok(())` → 続行（stale socket を削除）
     - `Err(err)` で `err.kind() == ErrorKind::NotFound` → 続行（既存なし）
     - その他のエラー → `Err(SingleInstanceError::UnlinkFailed { path: socket_path, err })`

4. **ソケット bind**:
   - `let listener = tokio::net::UnixListener::bind(&socket_path)?;`
   - `std::fs::set_permissions(&socket_path, Permissions::from_mode(0o600))?` でソケットファイル `0600` 適用（`UnixListener::bind` のデフォルトは umask 依存のため明示適用）
   - 失敗時: `Err(SingleInstanceError::Bind(err))`

5. **return `Ok(SingleInstanceLock { lock_file, socket_path, listener: Some(listener) })`**

**race-safe の根拠**（`process-model.md` §4.1 ルール 2 / `../basic-design/security.md §シングルインスタンスの race-safe 保証`）:

- ステップ 2（`flock`）を**先に**取ることで「ロック取得済 = 自分が単一インスタンス確定」が保証される
- ステップ 3（`unlink`）はロック取得後に行うため、**他 daemon のソケットを誤って消さない**
- `flock` は POSIX advisory lock。プロセス終了時にカーネルが自動 release するため、SIGKILL された daemon が残した lock ファイルでも次の起動は必ず獲得できる（stale PID エッジケース不発生）

### `acquire` メソッド（Windows）

- シグネチャ: `pub fn acquire_windows(pipe_name: &str) -> Result<Self, SingleInstanceError>`
- `cfg(windows)` 配下のみコンパイル

**処理順序**:

1. **Named Pipe 作成**:
   - `tokio::net::windows::named_pipe::ServerOptions::new().first_pipe_instance(true).pipe_mode(PipeMode::Byte).max_instances(1).create(pipe_name)`
   - `first_pipe_instance(true)` により `CreateNamedPipeW` に `FILE_FLAG_FIRST_PIPE_INSTANCE` フラグが付与される
   - 既存インスタンスがある場合 `ERROR_ACCESS_DENIED` または `ERROR_PIPE_BUSY` で失敗 → `Err(SingleInstanceError::AlreadyRunning { pipe_name: pipe_name.to_string() })`

2. **SDDL 適用（owner-only）**:
   - 別途 `permission::windows::apply_owner_only_sddl(handle)?` で SDDL 文字列 `"D:P(A;;GA;;;<owner-sid>)"` を適用
   - SDDL 設定失敗 → `Err(SingleInstanceError::SddlSetup(err))`

3. **return `Ok(SingleInstanceLock { pipe_server: Some(server), pipe_name: pipe_name.to_string() })`**

### `take_listener` メソッド

- シグネチャ:
  - Unix: `pub fn take_unix_listener(&mut self) -> Option<tokio::net::UnixListener>`
  - Windows: `pub fn take_windows_pipe_server(&mut self) -> Option<NamedPipeServer>`
- 内部: `self.listener.take()` で所有権を移譲
- 設計理由: `IpcServer` に listener を渡すため、`SingleInstanceLock` から listener を取り出す。`SingleInstanceLock` 自身は `flock` / pipe handle のみ保持し続ける（解放責務は `SingleInstanceLock` Drop に残す）

### `Drop` 実装

- `impl Drop for SingleInstanceLock`
- 処理（Unix）:
  1. `if self.listener.is_some() { drop(self.listener.take()); }` （明示的 drop、listener が close される）
  2. `let _ = std::fs::remove_file(&self.socket_path);` （ソケットファイル unlink、エラーは無視 = ベストエフォート）
  3. `lock_file` フィールドが drop される際にカーネルが `flock` を自動解放（明示呼び出し不要）
- 処理（Windows）:
  1. `if self.pipe_server.is_some() { drop(self.pipe_server.take()); }` （pipe close、カーネルが pipe 実体を release）
  2. `lock_file` 等の追加リソースなし（Named Pipe が排他制御の本体）

**設計判断**:
- `Drop` 内では `?` を使わない（`Drop::drop` の戻り値は `()`）。エラーは `tracing::warn!` で記録 or 無視
- `tracing::warn!` を `Drop` 内で呼ぶと、subscriber が既に drop されている場合に panic する可能性があるため、**`Drop` 内では `tracing` を呼ばず `eprintln!` か無視**を採用（fail-secure）

### エラー型

- `pub enum SingleInstanceError`
- `#[derive(Debug, thiserror::Error)]`
- バリアント:
  - `AlreadyRunning { lock_path: PathBuf }` (Unix) / `AlreadyRunning { pipe_name: String }` (Windows)
  - `InvalidDirectoryPermission { path: PathBuf, got: u32 }` (Unix のみ)
  - `Lock(nix::errno::Errno)` (Unix のみ)
  - `UnlinkFailed { path: PathBuf, err: std::io::Error }` (Unix のみ)
  - `Bind(std::io::Error)` (Unix) / `PipeCreateFailed(std::io::Error)` (Windows)
  - `SddlSetup(std::io::Error)` (Windows のみ)

## graceful shutdown

### `lifecycle::shutdown::wait_for_signal` 関数

- 配置: `crates/shikomi-daemon/src/lifecycle/shutdown.rs`
- シグネチャ: `pub async fn wait_for_signal(notify: Arc<Notify>)`
- 処理:
  - **Unix**:
    1. `let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())?;`
    2. `let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())?;`
    3. `tokio::select! { _ = sigterm.recv() => {}, _ = sigint.recv() => {} }`
    4. `tracing::info!(target: "shikomi_daemon::lifecycle", "shutdown signal received");`
    5. `notify.notify_waiters();`
  - **Windows**:
    1. `let mut ctrl_close = tokio::signal::windows::ctrl_close()?;`
    2. `let mut ctrl_c = tokio::signal::windows::ctrl_c()?;`
    3. `tokio::select! { _ = ctrl_close.recv() => {}, _ = ctrl_c.recv() => {} }`
    4. 以下同上

### `IpcServer::start_with_shutdown` の処理

- シグネチャ: `pub async fn start_with_shutdown(&mut self, shutdown: Arc<Notify>) -> Result<(), DaemonError>`
- 処理:
  1. `let mut connections = JoinSet::new();`
  2. loop:
     - `tokio::select!` で:
       - `accept_branch`: `self.listener.accept()`
       - `shutdown_branch`: `shutdown.notified()`
     - `accept_branch`:
       - ピア検証 → 成功なら `connections.spawn(handle_connection(...))` で spawn
     - `shutdown_branch`:
       - listener を drop（新規接続受付停止）
       - **in-flight 待機（タイムアウト 30 秒）**:
         - `tokio::time::timeout(Duration::from_secs(30), connections.join_all())` で待機
         - タイムアウト時: `tracing::warn!("in-flight requests did not complete within 30s; forcing shutdown");` + connections を drop（タスク強制中断）
       - `tracing::info!("server shutdown complete");`
       - return `Ok(())`

**設計判断**:
- 30 秒の根拠: vault `save` 操作（atomic write + fsync）は数十 ms で完了するため、複数 in-flight でも 30 秒で十分。`shikomi` の vault サイズ想定（〜数 MB）では 1 リクエストの完了は秒未満
- タイムアウト時の強制中断は **fail-secure**: 長時間 in-flight は異常状態（panic / deadlock）の可能性が高く、graceful 待機を打ち切って次の起動を許す

### `run()` のシャットダウン経路（`./composition-root.md §処理順序` 詳細化）

`shikomi_daemon::run()` の終盤:

1. `let server_result = server.start_with_shutdown(shutdown.clone()).await;`
2. shutdown 完了 → スコープを抜ける順序で:
   - `vault: Arc<Mutex<Vault>>` の最後の `Arc` 参照消失 → `Mutex<Vault>` Drop → `Vault` Drop（domain 集約は I/O なし、ただ drop される）
   - `repo: Arc<SqliteVaultRepository>` の最後の参照消失 → `SqliteVaultRepository` Drop → 内部の `VaultLock` の `Drop` が呼ばれて `flock` 解放（既存 `shikomi-infra::persistence::lock::VaultLock` の RAII）
   - `single_instance: SingleInstanceLock` Drop → ソケット unlink + `flock` 解放
3. `tracing::info!("graceful shutdown complete");`
4. `match server_result`:
   - `Ok(())` → return `ExitCode::from(0)`
   - `Err(_)` → `tracing::error!("server error: {}", err);` → return `ExitCode::from(1)`

**RAII の効能**:
- 各リソースの解放順序が型システムで保証される（`SingleInstanceLock` が最後に drop される、`Drop` 順は宣言の逆順）
- 部分的な解放失敗（ソケット unlink エラー等）はベストエフォート（次の daemon 起動時に stale 検出されるため）

## CLI 側のシャットダウン

`shikomi-cli` 短命プロセスのため、CLI 側のシグナル handling は最小:

- `Ctrl+C`（`SIGINT`）→ Rust ランタイムが panic 経由で exit（panic_hook で fixed-message + exit 101）
- 通常終了 → `run()` の戻り値に従う
- 本 feature では CLI 側に追加の signal handler を設けない（`tokio::runtime` は `current_thread` 1 回限り使用、I/O 待ちで block する経路でも `Ctrl+C` で OS 強制終了されて問題なし）

## ソケットパス解決ロジック

- 配置: `crates/shikomi-daemon/src/lifecycle/socket_path.rs`（新規）
- シグネチャ:
  - Unix: `pub fn resolve_socket_dir() -> Result<PathBuf, SocketPathError>`
  - Windows: `pub fn resolve_pipe_name() -> Result<String, SocketPathError>`

**Unix の解決**:

1. `std::env::var("XDG_RUNTIME_DIR")` を優先（Linux 標準）
2. `Some(d)` → `PathBuf::from(d).join("shikomi")` → return
3. 未設定または空 → fallback:
   - **Linux**: `dirs::runtime_dir()` で `/run/user/$UID/shikomi`（`XDG_RUNTIME_DIR` 由来の OS 標準位置）
   - **macOS**: `dirs::cache_dir()` で `~/Library/Caches/shikomi`（XDG 規約は適用されない、Apple 規約に従う）
4. fallback も失敗 → `Err(SocketPathError::CannotResolve)`

**Windows の解決**:

1. **自プロセスの SID 取得は `permission::windows::resolve_self_user_sid()` に委譲**（`unsafe` を `permission/windows.rs` に集約、服部平次指摘への対応）:
   - 配置: `crates/shikomi-daemon/src/permission/windows.rs`（既存 `#![allow(unsafe_code)]` モジュール内）
   - シグネチャ: `pub fn resolve_self_user_sid() -> Result<String, std::io::Error>`
   - 内部実装: `GetCurrentProcessToken` → `GetTokenInformation(TokenUser)` → `ConvertSidToStringSidW` を `unsafe` ブロックでラップし、セーフな `String` として返す
2. `lifecycle::socket_path::resolve_pipe_name` は `permission::windows::resolve_self_user_sid()` を呼び、戻り値の SID 文字列から `format!(r"\\.\pipe\shikomi-daemon-{}", sid_string)` を組み立てるのみ（**`unsafe` を `lifecycle/` 配下に置かない**）
3. SID 取得失敗 → `resolve_self_user_sid` が返す `std::io::Error` を `SocketPathError::SidLookup(err)` でラップ

**unsafe の局所化原則（再掲）**:

- `crates/shikomi-daemon/src/permission/{unix,windows}.rs` **以外**で `unsafe` ブロックを書かない（CI grep で監査、**TC-CI-019** 対象）
- `lifecycle/socket_path.rs` は **unsafe を含まない** — `permission/windows` 経由でセーフな `String` / `u32` を取得するのみ
- `../basic-design/security.md §unsafe_code の扱い` の表と完全整合

**設計判断**:
- 環境変数による上書きは本 feature では対応しない（YAGNI、将来 `--socket-dir` フラグ追加余地）
- Windows の SID は CLI / daemon で同じ User SID を使うため、同期不要（同一ユーザ実行が前提、ピア検証で別ユーザを排除）
- `permission::windows::resolve_self_user_sid` は **daemon 専用**（CLI 側は後述の `crates/shikomi-cli/src/io/windows_sid.rs` で同型関数を用意、両者は**独立実装**で crate 境界を尊重）

### CLI 側の対応する解決

`shikomi-cli::io::ipc_vault_repository::default_socket_path()` も同じ解決ロジックを使う。**daemon と CLI で同じ関数を共有するか、別実装か**:

- 案 A: `shikomi-core::ipc` に `default_socket_dir()` / `default_pipe_name()` を pub fn として追加 → daemon / CLI で共有
- 案 B: 各 crate で独立記述（DRY 違反）

**本 feature 採用: 案 A**。理由: ソケットパス解決は protocol 一致と同じく「daemon と CLI が同じ場所を見る」契約のため、`shikomi-core::ipc` で単一真実源化するのが妥当。`dirs` / `windows-sys` への依存は `shikomi-core` の純粋性と衝突するため、案 A を限定実装:

- `shikomi-core::ipc::SocketPath` 等の **論理ロケーション enum**（`UnixRuntimeDir { dir: &str }` / `WindowsNamedPipe { sid: &str }` 等）を pub で公開
- 実際の `XDG_RUNTIME_DIR` 解決 / SID 取得は daemon / CLI 各 crate 側で行う
- これにより `shikomi-core` は I/O / OS 依存を持たない

**確定**: 本 feature では**案 B**（各 crate 独立記述）を採用し、DRY 違反は受容する。理由: ソケットパス文字列の組立は数行で済み、`dirs` / `windows-sys` への依存を `shikomi-core` に持ち込むコストが見合わない。共通化は Phase 2 全面切替時の別 Issue で検討（YAGNI）。

## テスト観点（テスト設計担当向け）

**ユニットテスト**:
- `SingleInstanceLock::acquire_unix` の各エラー経路（親ディレクトリ作成失敗 / flock 失敗 / unlink 失敗 / bind 失敗）の `SingleInstanceError` バリアント生成
- `Drop` 実装が `flock` 解放 + ソケット unlink を行うこと（`tempfile::TempDir` で独立ソケットディレクトリを使い検証）
- `resolve_socket_dir` / `resolve_pipe_name` の OS 別解決（cfg 分岐）

**結合テスト**:
- 同じソケットディレクトリで 2 つの `SingleInstanceLock::acquire` を呼び、2 つ目が `AlreadyRunning` で失敗すること
- 1 つ目が drop された後、2 つ目の acquire が成功すること（再起動シナリオ）
- `wait_for_signal` のシグナル受信検証（`tokio::signal::unix::signal` のモック化）

**E2E**:
- daemon 起動 → `kill -9 <pid>` でプロセス強制終了 → 新規 daemon 起動成功（stale socket でも `flock` 自動解放のため）
- daemon A 起動中に daemon B 起動 → daemon B が exit 2（`AlreadyRunning`）
- daemon に SIGTERM 送信 → in-flight リクエスト完了後 graceful shutdown → ソケットファイル unlink 確認
- 30 秒 in-flight 強制中断シナリオ（テスト難度高、優先度低）

テストケース番号の割当は `test-design/` が担当する。
