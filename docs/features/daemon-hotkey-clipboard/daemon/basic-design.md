# 基本設計書 — daemon（daemon-hotkey-clipboard）

<!-- feature: daemon-hotkey-clipboard / sub-feature: daemon / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/daemon/basic-design.md -->
<!-- 疑似コード・実装コードブロック禁止 -->

## §モジュール契約（機能要件マッピング）

| 要件 ID | 契約 |
|---------|------|
| R1-HK-01 | daemon 起動時に `HotkeyManager::register_all` が vault の全ホットキーエントリを OS に登録する |
| R1-HK-04 | `ClipboardWriter::write` が `arboard::Clipboard` 経由で OS クリップボードに値を書き込む |
| R1-HK-05 | `ClearTimer::schedule` が 30 秒後に `ClipboardWriter::clear` を実行するタスクを spawn する |
| R1-HK-06 | `HotkeyBackend` trait を実装する `X11Backend` と `WaylandBackend` を Linux 起動時に動的選択する |
| R1-HK-07 | vault がロック中の場合、`HotkeyEventLoop` は OS 通知でユーザーに「ロック中」を伝えクリップボード書き込みをスキップする（R1-HK-13 準拠）|
| R1-HK-08 | IPC ハンドラ `add.rs` / `edit.rs` が `hotkey` フィールドを処理し `Vault::assign_hotkey` を呼ぶ |
| R1-HK-09 | IPC ハンドラ `edit.rs` が `clear_hotkey` フラグを処理し `Vault::clear_hotkey` を呼ぶ |

## 1. モジュール構成

変更対象 crate: **`shikomi-daemon`**（主）/ **`shikomi-infra`**（`arboard` 依存追加）/ **`shikomi-cli`**（`--hotkey` CLI オプション追加）

```
crates/shikomi-daemon/src/
  hotkey/
    mod.rs              ← HotkeyManager, HotkeyBackend trait, session detection
    backend/
      mod.rs            ← BackendEnum（実行時ディスパッチ）
      macos.rs          ← macOS バックエンド (tauri-plugin-global-shortcut)
      windows.rs        ← Windows バックエンド (tauri-plugin-global-shortcut)
      linux_x11.rs      ← Linux X11 バックエンド
      linux_wayland.rs  ← Linux Wayland バックエンド (ashpd)
    event_loop.rs       ← HotkeyEventLoop（ホットキー → クリップボード投入ループ）
    clipboard.rs        ← ClipboardWriter（arboard ラッパ）
    clear_timer.rs      ← ClearTimer（secret エントリ 30 秒クリア）
  ipc/
    handler/
      add.rs            ← hotkey フィールド処理追加（既存ファイル更新）
      edit.rs           ← hotkey / clear_hotkey 処理追加（既存ファイル更新）
  lib.rs                ← run() に HotkeyManager + HotkeyEventLoop を注入

crates/shikomi-cli/src/
  cli.rs                ← --hotkey / --clear-hotkey オプション追加（既存ファイル更新）
  input/
    hotkey.rs           ← ホットキー文字列入力パーサ（新設）
  usecase/
    add.rs              ← hotkey フィールドを IPC リクエストに渡す（既存更新）
    edit.rs             ← hotkey / clear_hotkey を IPC リクエストに渡す（既存更新）
```

## 2. コンポーネント設計

```mermaid
flowchart TB
    subgraph Daemon["shikomi-daemon"]
        direction TB
        RunFn["run() — コンポジションルート"]
        HKM["HotkeyManager\n全ホットキー登録・解除"]
        EL["HotkeyEventLoop\nホットキーイベント受信ループ"]
        CW["ClipboardWriter\narboard ラッパ"]
        CT["ClearTimer\n30 秒自動クリアタスク"]
        BE["HotkeyBackend trait\nmacOS / Windows / X11 / Wayland"]
        VekC["VekCache\n既存: vault ロック状態管理"]
        IPCAdd["IPC handler: add"]
        IPCEdit["IPC handler: edit"]
    end

    RunFn --> HKM
    RunFn --> EL
    HKM --> BE
    EL --> CW
    EL --> CT
    EL --> VekC
    IPCAdd --> HKM
    IPCEdit --> HKM
```

### 2.1 `HotkeyBackend` trait

OS ホットキー登録・解除の抽象インターフェース。3 OS / 4 バックエンドの実装を統一する。

| メソッド | シグネチャ（プレーンテキスト） | 説明 |
|---------|---------------------------|------|
| `register` | `fn register(&self, combo: &str) -> Result<(), HotkeyError>` | 指定コンボを OS に登録 |
| `unregister` | `fn unregister(&self, combo: &str) -> Result<(), HotkeyError>` | 指定コンボの OS 登録を解除 |
| `event_stream` | `fn event_stream(&self) -> BoxStream<HotkeyEvent>` | 登録済みホットキーのイベントストリーム |

`BoxStream<HotkeyEvent>` は `futures_util::Stream` の動的ディスパッチ型。`async_trait` を使用（既存 `OsLockSignal` と同パターン）。

### 2.2 `HotkeyEvent` 型

| フィールド | 型 | 説明 |
|-----------|----|------|
| `combo` | `String` | 発火したホットキー正規化文字列 |

### 2.3 `HotkeyManager`

vault 内の全ホットキーを OS バックエンドに登録・管理する RAII オブジェクト。

| メソッド | 説明 |
|---------|------|
| `new(backend, vault)` | コンストラクタ |
| `register_all()` | vault の全ホットキー登録済みエントリを `backend.register` で一括登録 |
| `register_one(combo)` | 単一ホットキーを OS 登録（IPC add/edit ハンドラから呼ばれる） |
| `unregister_one(combo)` | 単一ホットキーを OS 解除（IPC edit ハンドラから呼ばれる） |

Drop 時に全登録済みホットキーを `backend.unregister` で解除（RAII）。

### 2.4 `HotkeyEventLoop`

daemon のホットキーイベント受信ループ。`HotkeyBackend::event_stream` から `HotkeyEvent` を受信し、クリップボード投入を行う。

```mermaid
sequenceDiagram
    participant OS as OS ホットキーサブシステム
    participant EL as HotkeyEventLoop
    participant Vault as Vault (Mutex)
    participant VEK as VekCache
    participant CW as ClipboardWriter
    participant CT as ClearTimer

    OS->>EL: HotkeyEvent { combo: "ctrl+alt+1" }
    EL->>Vault: lock() → find_by_hotkey("ctrl+alt+1")
    Vault-->>EL: Some(&Record)
    EL->>VEK: is_locked()
    alt vault がロック中
        VEK-->>EL: true
        EL->>EL: OS通知「vault がロック中」を送信（R1-HK-13）+ スキップ
    else vault がアンロック
        VEK-->>EL: false
        EL->>CW: write(payload_value)
        CW-->>EL: Ok(())
        alt record.kind == Secret
            EL->>CT: schedule(30s, clear)
        end
    end
```

### 2.5 `ClipboardWriter` trait と実装

**`ClipboardWriter` を trait として定義する**（テスト時の `MockClipboardWriter` 差し替えを可能にするため）。

```
trait ClipboardWriter: Send + 'static {
    fn write(&mut self, value: &[u8]) -> Result<(), ClipboardError>;
    fn clear(&mut self) -> Result<(), ClipboardError>;
}
```

（上記は Rust 関数シグネチャのプレーンテキスト表記）

| 実装型 | 説明 |
|-------|------|
| `ArboardClipboardWriter` | `arboard::Clipboard` を内部で保持する本番実装 |
| `MockClipboardWriter` (テスト用) | `Vec<String>` で操作履歴を保持。`daemon/test-design.md §2` 参照 |

`ArboardClipboardWriter::new()` 失敗時（ヘッドレス環境 / クリップボード未対応）は `NullClipboardWriter`（全操作が noop / 警告ログのみ）にフォールバックする。daemon 起動は継続。

**Wayland**: `arboard` v3.6+ の `wayland-data-control` feature を有効化することで Wayland プロトコルに対応。

### 2.6 `ClearTimer`

secret エントリの自動クリアタスク管理。

| 状態 | 説明 |
|------|------|
| Idle | タイマー未設定 |
| Running(JoinHandle) | 30 秒カウントダウン中 |

動作:
1. `schedule(duration, writer)` を呼ぶと既存 Running タイマーを `abort()` し新しいタスクを spawn
2. タスク内: `tokio::time::sleep(duration)` → `writer.clear()`
3. shutdown シグナル受信時: タイマータスクを `abort()`

### 2.7 `Notifier` trait と OS 通知設計（R1-HK-13 / R1-HK-14）

**`Notifier` を trait として定義する**（テスト時の `MockNotifier` 差し替えを可能にするため。`ClipboardWriter §2.5` と同一パターン）。

```
trait Notifier: Send + Sync + 'static {
    fn notify(&self, level: NotifyLevel, title: &str, body: &str) -> Result<(), NotifyError>;
}
```

（上記は Rust 関数シグネチャのプレーンテキスト表記）

| 実装型 | 説明 |
|-------|------|
| `NotifyRustNotifier` | `notify-rust` crate を使用する本番実装 |
| `MockNotifier` (テスト用) | `Vec<(NotifyLevel, String, String)>` で送信履歴を保持。`daemon/test-design.md §3` 参照 |

`HotkeyEventLoop` は `notifier: Arc<dyn Notifier>` フィールドで保持する（詳細は `daemon/detailed-design.md §4.1`）。

**通知シナリオ一覧**:

| 状況 | title | body | level |
|------|-------|------|-------|
| vault ロック中のホットキー押下（R1-HK-13）| `"shikomi"` | `"vault がロック中です。shikomi vault unlock を実行してください"` | Low / Info |
| クリップボード書き込み失敗（R1-HK-14）| `"shikomi"` | `"クリップボードへの書き込みに失敗しました"` | Normal |
| OS ホットキー登録失敗（起動時）| `"shikomi"` | `"ホットキー {combo} の登録に失敗しました。他のアプリと競合している可能性があります"` | Normal |

**通知の非ブロック性**: `notify()` 呼び出しは非同期で行い、失敗時は `tracing::warn!` でログのみ（通知システムの不在がアプリ動作を止めない）。

**`Sync` 要件**: `HotkeyEventLoop` が `Arc<dyn Notifier>` を `tokio::spawn` タスク内で共有するため `Sync` 境界が必要。`MockNotifier` は `Mutex<Vec<...>>` で内部可変性を実現し `Sync` を満たす。

### 2.8 Linux バックエンド選択（セッション検出）

```mermaid
flowchart LR
    Start["daemon 起動"]
    Probe["XDG_SESSION_TYPE 取得\n+ ashpd GlobalShortcuts portal probe"]
    X11["X11Backend\ntauri-plugin-global-shortcut"]
    Wayland["WaylandBackend\nashpd GlobalShortcuts"]

    Start --> Probe
    Probe -->|"wayland\nかつ portal 応答あり"| Wayland
    Probe -->|"x11\nまたは portal 無応答"| X11
```

- `XDG_SESSION_TYPE=wayland` かつ `ashpd::desktop::global_shortcuts::GlobalShortcuts::new()` が成功した場合のみ Wayland バックエンドを選択
- portal 応答がない（GNOME 43 以前 / KDE Plasma 5 以前）場合は X11 バックエンドにフォールバック
- フォールバック時は `tracing::warn!` でセッション種別と理由を記録

### 2.9 監査ログ設計（R1-HK-12）

ホットキー発火イベントは `tracing::info!(target: "shikomi::audit")` で記録する。

| フィールド | 内容 | 例 |
|-----------|------|-----|
| `event` | イベント種別 | `"hotkey_triggered"` |
| `record_id` | 発火したエントリの RecordId | `"uuid-xxxx"` |
| `combo` | ホットキー組み合わせ | `"alt+ctrl+1"` |
| `result` | 結果 | `"injected"` / `"skipped:vault_locked"` / `"skipped:not_found"` / `"error:clipboard"` |
| `secret` | secret フラグ | `true` / `false` |

**記録しない情報**: ペイロード値（平文・暗号文問わず）、マスターパスワード、VEK。
**ログレベル**: `info`（デフォルトで記録）。`SHIKOMI_DAEMON_LOG` 環境変数で制御可能。

## 3. CLI 変更（`shikomi-cli`）

### 3.1 `add` サブコマンド拡張

| 追加オプション | 型 | 説明 |
|--------------|-----|------|
| `--hotkey <COMBO>` | `Option<String>` | 例: `--hotkey "ctrl+alt+1"` |

### 3.2 `edit` サブコマンド拡張

| 追加オプション | 型 | 説明 |
|--------------|-----|------|
| `--hotkey <COMBO>` | `Option<String>` | ホットキーを変更 |
| `--clear-hotkey` | `bool` (flag) | ホットキーを解除 |

`--hotkey` と `--clear-hotkey` を同時指定した場合: `clap` の排他グループで CLI バリデーション。

### 3.3 `list` 出力変更

`[ctrl+alt+1]` を `label` の後に括弧付きで追加表示。例:
```
1  メールアドレス  [ctrl+alt+1]  plaintext
```

> **Phase 切替**: Phase 1 / Phase 2 の切替戦略は **`feature-spec.md §7`** を唯一の参照先とする。

## 4. 外部連携（新規依存 crate）

| crate | バージョン方針 | 追加先 | 根拠 | セキュリティ審査 |
|-------|-------------|--------|------|----------------|
| `arboard` | `^3.6` (minor ピン) | `shikomi-daemon` | Wayland `wayland-data-control` feature が 3.6+ で安定。`1Password` メンテ、MIT ライセンス | **RustSec: advisory なし（2026-05 確認）**。1Password 社がメンテし、OSS セキュリティ審査が定期実施されている。`cargo-deny` で継続監視 |
| `tauri-plugin-global-shortcut` | `^2.2` (minor ピン) | `shikomi-daemon` | Tauri v2 公式プラグイン。macOS / Windows / Linux X11 対応 | **RustSec: advisory なし（2026-05 確認）**。tauri-apps org 公式、Apache-2.0/MIT。tauri-apps/plugins-workspace リポジトリで一元管理 |
| `ashpd` | `^0.13` (minor ピン) | `shikomi-daemon` (Linux only) | Wayland XDG Portal の Rust バインディング。`global_shortcuts` feature が 0.13 で安定 | **RustSec: advisory なし（2026-05 確認）**。LGPL-2.1+。zbus を基盤とし、GNOME / KDE 公式 portal 仕様に準拠。OSS で監査可能 |
| `notify-rust` | `^4.11` (minor ピン) | `shikomi-daemon` | OS ネイティブ通知（R1-HK-13 / R1-HK-14）。Linux: libnotify / macOS: NSUserNotification / Windows: Windows Toast | **RustSec: advisory なし（2026-05 確認）**。MIT ライセンス。純粋 OS API ラッパで攻撃面が小さい |

**`cargo-deny` 監視方針**: 上記 4 crate を `deny.toml` の `skip` 対象に追加せず、advisory 検出 → CI 失敗を二重防御として機能させる。

**Linux-only feature**: `ashpd` は `#[cfg(target_os = "linux")]` ガードで Linux ビルドにのみ依存させる。macOS / Windows ビルドへ混入しない。

## 5. セキュリティ設計

| 脅威 | 対策 |
|------|------|
| 暗号化 vault ロック中のクリップボード投入 | OS 通知で「ロック中」をユーザーに伝え、クリップボード書き込みをスキップ（R1-HK-13）|
| クリップボード内 secret の残留 | `ClearTimer` が 30 秒後に `clear()` を実行（R1-HK-05）|
| ホットキー発火の監査証跡 | `tracing::info!(target: "shikomi::audit", ...)` で RecordId・結果を記録。ペイロード値は記録しない（R1-HK-12）|
| IPC 接続の認証 | `crates/shikomi-daemon/src/permission/peer_credential/` 実装済み（Issue #26）。Unix: `SO_PEERCRED` UID 検証 / Windows: `GetNamedPipeClientProcessId` SID 検証（`process-model.md §4.2`）。本 feature は既存機構を継承 |
| HotkeyEvent の偽装 | UDS / Named Pipe のピア UID/SID 検証は上記実装済みの IPC 層で担保。ホットキーイベント自体は OS カーネル経由のため偽装不可 |
| ホットキー組み合わせ列挙攻撃 | 同一プロセス（daemon）内のみがイベントを受信。他プロセスへの露出なし |
| `arboard` による機密値のメモリ残留 | クリップボード API は OS が管理するヒープを使用。`zeroize` 適用範囲外。`ClearTimer` が 30 秒以内に上書きするまでは acceptable リスクとして `docs/architecture/context/threat-model.md` に明記 |

## 6. エラーハンドリング方針

| エラー発生箇所 | 方針 |
|-------------|------|
| `HotkeyBackend::register` 失敗 | `tracing::error!` + そのホットキーをスキップ（他のホットキーは登録継続） |
| `ClipboardWriter::new()` 失敗 | `tracing::warn!` + `ClipboardWriter` を無効状態に（ホットキー押下時はスキップ）。daemon 起動は継続 |
| `ClipboardWriter::write()` 失敗 | `tracing::warn!` でログ記録 + OS 通知「クリップボードへの書き込みに失敗しました」を送信（R1-HK-14）。エラーを握り潰さない |
| `ClearTimer` abort エラー | 無視（tokio task abort は `JoinError::is_cancelled()` で正常扱い） |
| Linux セッション検出失敗 | `tracing::warn!` + X11 バックエンドへフォールバック |
