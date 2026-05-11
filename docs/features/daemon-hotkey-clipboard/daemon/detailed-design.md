# 詳細設計書 — daemon（daemon-hotkey-clipboard）

<!-- feature: daemon-hotkey-clipboard / sub-feature: daemon / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/daemon/detailed-design.md -->
<!-- 疑似コード・実装コードブロック禁止 -->

## 1. `HotkeyBackend` trait 詳細

### 1.1 trait 定義要件

- `async_trait` マクロを使用（既存 `OsLockSignal` と同パターン、`vek-cache-and-ipc.md` §OsLockSignal 参照）
- `Send + Sync + 'static` 境界を要求（tokio spawn 内で共有するため）
- `event_stream` の返り値 `BoxStream<'_, HotkeyEvent>` は `futures_util::BoxStream` を使用

### 1.2 バックエンド実装一覧

| 実装型 | 対象 OS / セッション | 依存 crate |
|--------|------------------|-----------|
| `MacosBackend` | macOS | `tauri-plugin-global-shortcut` |
| `WindowsBackend` | Windows | `tauri-plugin-global-shortcut` |
| `X11Backend` | Linux X11 | `tauri-plugin-global-shortcut` |
| `WaylandBackend` | Linux Wayland | `ashpd` |

### 1.3 `BackendEnum`（実行時ディスパッチ）

`dyn HotkeyBackend` の代わりに `enum BackendEnum` で静的ディスパッチを実現。バリアントは OS / セッション別に 4 種。`HotkeyBackend` の各メソッドを `match self` で委譲する。

**理由**: `dyn HotkeyBackend` は `async_trait` と組み合わせると `Box<dyn Future + Send>` が連鎖してヒープ確保が増える。`BackendEnum` は enum の match で静的ディスパッチし、ホットキーイベントループの hot path でのアロケーションを避ける。

## 2. `HotkeyManager` 詳細

### 2.1 フィールド

| フィールド | 型 | 説明 |
|-----------|----|------|
| `backend` | `Arc<BackendEnum>` | ホットキー OS バックエンド |
| `registered` | `HashSet<String>` | 登録済みコンボ文字列の集合（Drop 時解除に使用） |

### 2.2 `register_all` 処理順序

1. `vault.hotkey_entries()` をイテレート
2. 各エントリの `hotkey.as_str()` を `backend.register(combo)` に渡す
3. 成功した場合のみ `registered.insert(combo)` する
4. 失敗時は `tracing::error!` でログ出力し継続（他ホットキーは登録する）

### 2.3 `register_one` / `unregister_one` の使用コンテキスト

IPC `add` / `edit` ハンドラが `Vault::assign_hotkey` の後に呼ぶ。`assign_hotkey` が成功した場合のみ OS 登録を試みる（ドメイン層が一次防衛、OS 登録は二次）。

`edit` でホットキーを変更した場合の処理順序:
1. `unregister_one(旧コンボ)` で旧ホットキーを解除
2. `Vault::assign_hotkey(id, 新Hotkey)` でドメイン更新
3. `register_one(新コンボ)` で新ホットキーを OS 登録

順序を変えると不整合が生じるため、上記順序を必ず守る。

### 2.4 Drop 実装

`drop()` で `registered` の全コンボを `backend.unregister` でループ解除。失敗は無視（`tracing::warn!` のみ）。

## 3. `HotkeyEventLoop` 詳細

### 3.1 フィールド

| フィールド | 型 | 説明 |
|-----------|----|------|
| `backend` | `Arc<BackendEnum>` | イベントストリーム取得元 |
| `vault` | `Arc<Mutex<Vault>>` | ホットキー → レコード解決 |
| `vek_cache` | `VekCache` | ロック状態判定 |
| `clipboard` | `ClipboardWriter` | クリップボード書き込み |
| `clear_timer` | `ClearTimer` | 自動クリアタイマー |

### 3.2 イベントループ処理

`tokio::select!` で `backend.event_stream()` と `shutdown_rx` を多重化。

各イベント受信時の処理:
1. `vault.lock().await.find_by_hotkey(combo)` でレコード取得
2. レコードが `None` → `tracing::debug!` でログのみ（スキップ）
3. `vek_cache.is_locked()` → `true` ならスキップ（サイレント、`R1-HK-07`）
4. `vek_cache.is_locked()` → `false` ならペイロード取得（暗号化モードは VEK で復号）
5. `clipboard.write(value)` でクリップボード書き込み
6. `record.kind == RecordKind::Secret` ならば `clear_timer.schedule(30s, clipboard)` を呼ぶ
7. `vault` の `Mutex` を 4 より前に drop する（クリップボード書き込みを Mutex 外で行う）

**Mutex 保持時間の最小化**: vault の Mutex は「レコード検索 + ペイロード取得」のみに限定し、クリップボード書き込み（OS API 呼び出し）は Mutex 外で行う。

## 4. `ClipboardWriter` 詳細

### 4.1 構成

`arboard::Clipboard` は `Send` だが `Sync` ではないため、`Mutex<arboard::Clipboard>` で包む。

### 4.2 ヘッドレス CI 対応

`arboard::Clipboard::new()` は X11 / Wayland display 接続を要求する。CI ヘッドレス環境では:
- Linux: `Xvfb` を起動して `DISPLAY=:99` を設定（`test-daemon.yml` ジョブで制御）
- または: `SHIKOMI_DISABLE_CLIPBOARD=1` 環境変数でクリップボード機能を無効化し daemon を起動できる（テスト用エスケープハッチ）

### 4.3 Wayland `arboard` 設定

`arboard` v3.6+ の依存 feature: `wayland-data-control`。`shikomi-daemon/Cargo.toml` で `arboard = { version = "^3.6", features = ["wayland-data-control"] }` と宣言。Linux 以外のビルドでは `target_os` 条件で feature が不要になるが、cargo は feature の有無を binary に影響させない。

## 5. `ClearTimer` 詳細

### 5.1 状態遷移

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Running: schedule(duration) 呼び出し
    Running --> Running: schedule(duration) 再呼び出し（abort → 再 spawn）
    Running --> Idle: タイマー完了（clear() 実行）
    Running --> Idle: shutdown abort
```

### 5.2 フィールド

| フィールド | 型 | 説明 |
|-----------|----|------|
| `handle` | `Option<JoinHandle<()>>` | 実行中タイマータスクのハンドル |

### 5.3 `schedule` 処理

1. `self.handle.take().map(|h| h.abort())` で既存タイマーをキャンセル
2. `tokio::spawn(async move { tokio::time::sleep(duration).await; writer.clear().await; })` で新タスクを spawn
3. `self.handle = Some(handle)` でハンドルを保存

## 6. Linux セッション検出詳細

### 6.1 検出アルゴリズム

| ステップ | 処理 |
|---------|------|
| 1 | `std::env::var("XDG_SESSION_TYPE")` を取得 |
| 2 | 値が `"wayland"` でない → X11 バックエンドを返す（即断） |
| 3 | 値が `"wayland"` → `ashpd::desktop::global_shortcuts::GlobalShortcuts::new().await` を試みる |
| 4 | 成功（Some）→ Wayland バックエンドを返す |
| 5 | 失敗（portal 未対応）→ `tracing::warn!` + X11 バックエンドを返す |

タイムアウト: ステップ 3 の ashpd probe は `tokio::time::timeout(Duration::from_secs(3), ...)` でガードする。3 秒以内に応答がなければ X11 バックエンドにフォールバック。

### 6.2 `cfg` 分岐方針

- `WaylandBackend` と `ashpd` 依存は `#[cfg(target_os = "linux")]` で囲む
- macOS / Windows ビルドに ashpd が混入しないことを `cargo check --target x86_64-pc-windows-msvc` で CI 検証する

## 7. IPC ハンドラ変更詳細

### 7.1 `add.rs` 拡張

`IpcRequest::AddRecord` の `hotkey: Option<String>` フィールドを処理する追加ステップ:

1. `hotkey` が `Some(s)` → `Hotkey::parse(s)` を呼ぶ。`HotkeyParseError` → `IpcErrorCode::HotkeyParseError` に写像して返す
2. 成功した `Hotkey` を `Vault::assign_hotkey(new_id, hotkey)` に渡す
3. `HotkeyConflict` → `IpcErrorCode::HotkeyConflict` に写像
4. vault 更新後に `manager.register_one(combo)` で OS 登録

### 7.2 `edit.rs` 拡張

| 条件 | 処理 |
|------|------|
| `clear_hotkey == true` かつ `hotkey.is_some()` | `IpcErrorCode::HotkeyParseError` で返す（矛盾入力） |
| `clear_hotkey == true` | `manager.unregister_one(旧combo)` → `Vault::clear_hotkey(id)` |
| `hotkey.is_some()` | `Hotkey::parse` → `manager.unregister_one(旧combo)` → `Vault::assign_hotkey` → `manager.register_one(新combo)` |
| どちらでもない | ホットキー変更なし（従来動作） |

## 8. `run()` コンポジションルート変更

既存の `run()` に以下を追加注入:

1. `detect_backend().await`（Linux のみ非同期、他 OS は同期）でバックエンド選択
2. `HotkeyManager::new(backend, &vault)` を構築し `register_all()` を呼ぶ
3. `HotkeyEventLoop::new(...)` を構築し `tokio::spawn(event_loop.run(shutdown_rx))` でスポーン
4. shutdown 時に `event_loop_task.abort()` と `drop(manager)` を明示実施

**既存コンポーネントへの影響**: `IpcServer::new` のシグネチャに `Arc<HotkeyManager>` を追加し、ハンドラ層に注入する。

## 9. 依存 crate バージョンピン方針

| crate | バージョン制約 | ピン根拠 |
|-------|-------------|---------|
| `arboard` | `^3.6` | Wayland `wayland-data-control` feature が 3.6 で安定。major ピン（3→4 は破壊的）|
| `tauri-plugin-global-shortcut` | `^2.2` | Tauri v2 系列。major ピン必須 |
| `ashpd` | `^0.13` | `global_shortcuts` feature が 0.13 で stable API。minor ピン |

`cargo-deny` の `deny.toml` に上記 crate を追加し、major バージョン外への漂流をビルド失敗で検出する。
