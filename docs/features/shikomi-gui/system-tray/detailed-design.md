# 詳細設計書 — system-tray（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: system-tray / Issue #97 -->
<!-- 配置先: docs/features/shikomi-gui/system-tray/detailed-design.md -->
<!-- 疑似コード・実装コードブロック禁止 -->
<!-- 参照: docs/features/shikomi-gui/system-tray/basic-design.md -->
<!-- 参照: docs/features/shikomi-gui/ipc-client/basic-design.md（Sub-B 凍結済み）-->
<!-- 参照: docs/features/shikomi-gui/ipc-client/detailed-design.md §1〜2 -->
<!-- 参照: docs/features/shikomi-gui/feature-spec.md（凍結済み）-->

---

## 1. `system_tray::setup()` フロー

```mermaid
sequenceDiagram
    participant LibRS as lib.rs setup()
    participant SetupFn as system_tray::setup()
    participant TrayBuilder as TrayIconBuilder
    participant Window as WebviewWindow("main")
    participant CountdownTask as countdown::run()

    LibRS->>SetupFn: setup(app)
    SetupFn->>TrayBuilder: new().icon(...).tooltip("shikomi").menu(build_menu(app)).build(app)
    TrayBuilder-->>SetupFn: TrayIcon
    SetupFn->>SetupFn: tray.on_tray_icon_event(右クリック → popup)
    SetupFn->>SetupFn: menu.on_menu_event(id 分岐)
    SetupFn->>Window: on_window_event(CloseRequested → prevent_default + hide)
    SetupFn->>CountdownTask: tauri::async_runtime::spawn(countdown::run(app_handle, tray_id))
    CountdownTask-->>SetupFn: JoinHandle（drop 時 abort）
```

`setup()` は `tauri::Result<()>` を返す。`TrayIconBuilder::build` 失敗は Fail Fast（`setup()` から即エラーを返し Tauri ランタイムを停止させる）。

---

## 2. close-to-tray ハンドラ詳細（REQ-TRAY-02）

```mermaid
sequenceDiagram
    actor ユーザー
    participant OS
    participant Window as WebviewWindow("main")
    participant App as AppHandle

    ユーザー->>OS: ×ボタンクリック
    OS->>Window: WindowEvent::CloseRequested { api }
    Window->>Window: api.prevent_default()
    Window->>Window: window.hide()
    Note over Window: プロセス継続 / トレイ常駐
    Note over Window: ウィンドウは非表示になるが破棄されない

    ユーザー->>App: トレイメニュー「終了」
    App->>App: app.exit(0)
    Note over App: 唯一のプロセス終了経路
```

**注意事項**:

- `window.hide()` はウィンドウを非表示にするが `WebviewWindow` は破棄されない。再表示は `window.show()` + `window.set_focus()` で即時可能（再初期化コストなし）
- macOS の Dock アイコンクリックによる再表示は `WindowEvent::Focused` または `app_handle` 経由の `show()` で対応する（macOS 固有実装、詳細は §6 プラットフォーム差異を参照）
- `CloseRequested` ハンドラ内で **`app.exit(0)` を呼ばないこと**。close-to-tray と明示終了の経路が交差して意図しない終了が発生する

---

## 3. トレイメニュー項目詳細（REQ-TRAY-03）

### 3.1 `build_menu(app)` の構成

| 順序 | MenuItem ID | ラベル | 種別 |
|------|------------|--------|------|
| 1 | `"open_window"` | 「ウィンドウを開く」 | `MenuItem` |
| 2 | — | （セパレータ） | `PredefinedMenuItem::separator` |
| 3 | `"restart_daemon"` | 「shikomi のサービスを再起動する」 | `MenuItem` |
| 4 | — | （セパレータ） | `PredefinedMenuItem::separator` |
| 5 | `"quit"` | 「終了」 | `MenuItem` |

### 3.2 各 MenuEvent ハンドラ詳細

#### `"open_window"` ハンドラ

```mermaid
sequenceDiagram
    participant MenuEvent
    participant App as AppHandle
    participant Window as WebviewWindow("main")

    MenuEvent->>App: id == "open_window"
    App->>Window: get_webview_window("main")
    alt window 取得成功
        Window->>Window: show()
        Window->>Window: set_focus()
    else window 取得失敗（None）
        App->>App: tracing::warn!("main window not found")
    end
```

#### `"restart_daemon"` ハンドラ

```mermaid
sequenceDiagram
    participant MenuEvent
    participant App as AppHandle
    participant AppState
    participant Shell as tauri_plugin_shell
    participant Daemon as shikomi-daemon

    MenuEvent->>App: id == "restart_daemon"
    App->>AppState: lock().await = None（既存接続を切断）
    App->>Shell: Command::new("shikomi").args(["start"]).spawn()
    Note over Shell: 非同期 spawn（完了を待たない）
    Shell->>Daemon: shikomi start
    App->>App: 既存 lib.rs::reconnect_task() を再呼び出し
    Note over App: 再接続成功で AppState = Some(...)
```

**`tauri-plugin-shell` スコープ許可**: `tauri.conf.json` の `plugins.shell.scope` に `{ "name": "shikomi", "cmd": "shikomi", "args": [{ "validator": "^start$" }] }` を追加する。`validator` は完全一致アンカー（`^start$`）を使用し部分一致・コマンドインジェクションを防止する。daemon 再起動で使用するコマンドは `start` のみのため `stop` は scope に含めない（最小権限）。セキュリティ根拠の詳細は `basic-design.md §6.1` を参照。

#### `"quit"` ハンドラ

1. `AppHandle::exit(0)` を呼び出す
2. Tauri ランタイムが各ウィンドウを `destroy` し、`AppState` 内 `GuiIpcClient` の `Drop` が実行されてソケットを閉じる（RAII）

---

## 4. countdown タスク詳細（REQ-TRAY-04, 05）

### 4.1 ポーリングループ

```mermaid
sequenceDiagram
    participant Task as countdown::run(app_handle, tray_id)
    participant Cmd as poll_remaining()
    participant Daemon as shikomi-daemon
    participant App as AppHandle
    participant Tray as TrayIcon

    loop 1 秒ごと
        Task->>Task: tokio::time::sleep(1s)
        Task->>Cmd: poll_remaining(&app_handle)
        Cmd->>Daemon: IpcRequest::GetClipboardStatus
        Daemon-->>Cmd: ClipboardStatus { remaining_secs }
        Cmd-->>Task: Option<u64>

        Task->>App: tray_by_id(&tray_id)
        alt TrayIcon 取得成功
            App-->>Task: Some(tray)
            Task->>Tray: tray.set_tooltip(tooltip_text(remaining))
        else TrayIcon 消失（アプリ終了中）
            App-->>Task: None
            Task->>Task: break（ループ終了）
        end
    end
```

### 4.2 エラーハンドリング

| ケース | 処理 |
|--------|------|
| `AppState` が `None`（daemon 未接続） | `poll_remaining()` が `None` を返す（エラー伝搬なし）。ツールチップは「shikomi」非アクティブ表示 |
| IPC 通信エラー | `poll_remaining()` が `tracing::debug!` でログのみ、`None` を返す。ツールチップは「shikomi」に更新（過剰 reset のリスクより一貫性を優先）|
| 予期しない IPC レスポンス variant | `tracing::debug!` でバリアント名をログ、`None` 返却 |
| `set_tooltip` 失敗 | `tracing::warn!` でログしループ継続（best-effort。Linux Wayland 等の環境差異を吸収）|
| `tray_by_id()` が `None`（トレイ消失） | ループを `break` で終了。アプリ終了中を示す。panic しない |
| タスク panic | Tokio の spawn タスクは panic 発生時に当該タスクのみ終了し、アプリ全体を止めない（Tokio ランタイム保証）。`catch_unwind` は不要 |

### 4.3 タスクライフサイクル

`countdown::run(app_handle, tray_id)` は `setup()` から `tauri::async_runtime::spawn` で起動する。`AppHandle` の strong 参照を保持し、毎ループで `app_handle.tray_by_id(&tray_id)` を呼ぶ。`None` が返った場合（トレイ破棄＝アプリ終了途中）は `break` でループを自然終了する。weak 参照を使わない理由: Tauri v2 の `tray_by_id()` が `Option` を返すことで生存確認が完結するため、weak 参照による二重管理は不要（KISS）。

---

## 5. `get_clipboard_countdown` Tauri Command 詳細（REQ-TRAY-04）

### 5.1 Command シグネチャと型

| フィールド | 型 | 説明 |
|-----------|-----|------|
| `ClipboardCountdownResult.remaining_secs` | `Option<u64>` | `Some(n)`: 残 n 秒 / `None`: 非アクティブ |

**シリアライズ**: `#[derive(Serialize)]` で `{ "remaining_secs": 15 }` または `{ "remaining_secs": null }` として SolidJS に渡る。`GUIError` は返さない（§4.2 理由）。

### 5.2 `AppState` 未接続時の挙動

daemon 未接続（`AppState == None`）の場合、IPC 呼び出しをスキップして即 `{ remaining_secs: null }` を返す。これは `not_connected` エラーを UI に見せないためのサイレントフォールバック。countdown 用の軽量ポーリングがエラーパネルを誘発しないよう設計する。

---

## 6. daemon 側 IPC 拡張詳細（Sub-D スコープ）

### 6.1 共有カウントダウン状態

`crates/shikomi-daemon/src/hotkey/event_loop.rs` の `HotkeyEventLoop` に `countdown_started_at: Arc<Mutex<Option<Instant>>>` フィールドを追加する。

| イベント | 操作 |
|---------|------|
| `RecordKind::Secret` のクリップボード投入成功 | `*countdown_started_at.lock().await = Some(Instant::now())` |
| `ClearTimer` による clipboard clear 完了 | `*countdown_started_at.lock().await = None` |
| `ClearTimer::abort()` （shutdown 時） | `*countdown_started_at.lock().await = None` |

この `Arc<Mutex<Option<Instant>>>` を `IpcServer` に渡し、`V2Context` 経由で `GetClipboardStatus` ハンドラが参照する。

### 6.2 `GetClipboardStatus` ハンドラロジック

| 条件 | `remaining_secs` |
|------|-----------------|
| `countdown_started_at == None` | `None` |
| `elapsed >= CLEAR_TIMEOUT_SECS` | `None`（タイマーが既に発火済み扱い）|
| `elapsed < CLEAR_TIMEOUT_SECS` | `Some(CLEAR_TIMEOUT_SECS - elapsed)` |

`CLEAR_TIMEOUT_SECS` は `shikomi_core::CLEAR_TIMEOUT_SECS`（= 30）を使用する（DRY）。

### 6.3 `IpcRequest` / `IpcResponse` 拡張

`shikomi-core/src/ipc/request.rs` と `response.rs` は `#[non_exhaustive]` 済みのため、新 variant 追加は非破壊変更。

---

## 7. lib.rs 変更点

`run()` 関数の `.setup()` フック内に以下を追加する:

| 追加処理 | 順序 |
|---------|------|
| `app.manage::<AppState>(...)` の既存処理の後に `system_tray::setup(app)?` を呼び出す | daemon 接続の前または後（どちらでも可。countdown タスクは daemon 未接続を `None` で扱うため） |
| `.invoke_handler` に `get_clipboard_countdown` を追加 | 既存 10 コマンドの末尾に追記 |
| `tauri_plugin_shell::init()` を `.plugin()` に追加 | daemon 再起動機能のため |

---

## 8. プラットフォーム差異

| OS | 差異 | 対応 |
|----|------|------|
| **macOS** | Dock アイコンを残すと Dock から「終了」が可能になる。Dock クリックでウィンドウを再表示したいが `AppEvent::Reopen` が必要 | `tauri::RunEvent::Reopen { ... }` を `.run(|_, event|)` クロージャで捕捉し `window.show()` を呼ぶ |
| **macOS** | トレイ右クリックではなく左クリックでメニューが開く挙動が一般的 | `TrayIconEvent::LeftButtonUp` でも `popup()` するハンドラを追加 |
| **Windows** | タスクバー通知領域アイコン。右クリックのみメニュー表示 | `RightButtonUp` のみ対応（左クリックはウィンドウ open とする）|
| **Linux** | X11 / Wayland でのトレイサポートは `libappindicator` 依存 | `tauri` v2 の `tray-icon` feature が `libappindicator` を自動リンク。`Cargo.toml` の `tray-icon` feature は Sub-A 時点で既に有効化済み |
| **Linux（Wayland）** | ツールチップ未サポートの環境がある | `set_tooltip` 呼び出しは best-effort とし、失敗時は `tracing::warn!` のみ（アプリ動作を止めない）|

---

## 9. 内部ヘルパー関数の可視性

| 関数名 | 可視性 | 役割 |
|--------|--------|------|
| `tooltip_text(remaining: Option<u64>) -> String` | `fn`（モジュールプライベート） | ツールチップ文字列を生成する純粋関数。`countdown.rs` 内でのみ呼ぶ |


この関数も `pub fn` にしない。呼び出し元は `countdown::run()` のみ。

---

## 10. 定数・ツールチップ文言一覧

| 状態 | ツールチップ文字列 |
|------|-----------------|
| カウントダウン非アクティブ | `"shikomi"` |
| カウントダウン中（残 n 秒） | `"shikomi — クリップボードを自動消去まで {n} 秒"` |

