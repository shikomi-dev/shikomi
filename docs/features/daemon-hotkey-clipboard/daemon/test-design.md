# テスト設計書 — daemon（daemon-hotkey-clipboard）

<!-- feature: daemon-hotkey-clipboard / sub-feature: daemon / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/daemon/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->

## 0. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 |
|-------|---------|---------|------|
| `HotkeyManager` UT | OS ホットキー登録 API | `HotkeyBackend` trait | `MockBackend` で差し替え |
| `HotkeyEventLoop` IT | OS ホットキー API + OS クリップボード | `HotkeyBackend` / `ClipboardWriter` trait | `MockBackend` + `MockClipboardWriter` で差し替え |
| `ClearTimer` UT | `tokio::time::sleep` | `tokio::time::pause()` / `advance()` で制御 | `#[tokio::test]` + `tokio::time::pause` |
| IPC ハンドラ IT | UDS / Named Pipe | `tempfile` + テスト用ソケットパス | 既存 daemon IT パターン準拠 |
| CLI 出力 IT (list OS status) | daemon IPC | `MockDaemon` | `assert_cmd` + mock |

**`MockBackend` と `MockClipboardWriter` の配置**: `crates/shikomi-daemon/tests/common/mock_backend.rs` / `mock_clipboard.rs`。テスト専用コードを本番コードに混入させない（`#[cfg(test)]` ガード不要、`tests/` 配下に物理分離）。

## 1. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT | `crates/shikomi-daemon/src/hotkey/mod.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-daemon` |
| UT | `crates/shikomi-daemon/src/hotkey/clear_timer.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-daemon` |
| IT | `crates/shikomi-daemon/tests/it_hotkey_manager.rs` | `cargo test -p shikomi-daemon` |
| IT | `crates/shikomi-daemon/tests/it_hotkey_event_loop.rs` | `cargo test -p shikomi-daemon` |
| IT | `crates/shikomi-daemon/tests/it_ipc_hotkey.rs` | `cargo test -p shikomi-daemon` |

## 2. テスト用ダブルの方針

`HotkeyBackend` trait を実装した `MockBackend` を `crates/shikomi-daemon/tests/common/mock_backend.rs` に配置。

`MockBackend` の仕様:
- `register` / `unregister` は登録済みコンボを `HashSet<String>` で保持（検証用 getter を持つ）
- `event_stream` は `tokio::sync::mpsc::Sender<HotkeyEvent>` を返す。テストから `Sender::send` でイベントを注入できる
- クリップボード操作は `MockClipboard`（`Vec<String>` で書き込み履歴を保持）で差し替える

## 3. ユニットテスト一覧

### TC-HD-DU01: `HotkeyManager::register_all` が vault エントリを全件登録する

| ID | 前提 | 手順 | 期待結果 |
|----|------|------|---------|
| TC-HD-DU01-a | 2 件のホットキー付きエントリ | `register_all()` 呼び出し | `mock_backend.registered()` に 2 コンボが含まれる |
| TC-HD-DU01-b | 1 件が `backend.register` で失敗 | 同上 | 成功した 1 件のみ登録済み。失敗ログが出力される |

### TC-HD-DU02: `HotkeyManager::register_one` / `unregister_one`

| ID | 手順 | 期待結果 |
|----|------|---------|
| TC-HD-DU02-a | `register_one("ctrl+alt+1")` | `registered` に追加 |
| TC-HD-DU02-b | `unregister_one("ctrl+alt+1")` | `registered` から除去 |
| TC-HD-DU02-c | 未登録コンボの `unregister_one` | `backend.unregister` は呼ばれない（noop） |

### TC-HD-DU03: `HotkeyManager` Drop が全コンボを解除する

Drop 後に `mock_backend.registered()` が空になることを確認。

### TC-HD-DU04: `ClearTimer::schedule` が 30 秒後に `clear()` を呼ぶ

`tokio::time::pause()` + `tokio::time::advance(Duration::from_secs(31))` で時間を早送りし、`MockClipboard.cleared` が `true` になることを確認。

### TC-HD-DU05: `ClearTimer::schedule` の再呼び出しが前のタイマーをキャンセルする

1. `schedule(Duration::from_secs(30), writer_a)` を呼ぶ
2. 15 秒経過後に `schedule(Duration::from_secs(30), writer_b)` を呼ぶ
3. 合計 45 秒後: `writer_a.cleared` は `false`、`writer_b.cleared` は `true`

### TC-HD-DU06: `ClipboardWriter::write` + `clear` の動作確認

`MockClipboard` を使用し、write 後に value が保持され、clear 後に空になることを確認。

## 4. 結合テスト一覧

### TC-HD-DI01: `HotkeyEventLoop` — ホットキーイベント受信からクリップボード書き込みまで

| 手順 | 期待結果 |
|------|---------|
| ① vault にエントリ `ctrl+alt+1 → "hello"` を登録 | |
| ② `HotkeyEventLoop` を起動（MockBackend, MockClipboard 使用） | |
| ③ `MockBackend.sender.send(HotkeyEvent { combo: "ctrl+alt+1" })` | `MockClipboard` に `"hello"` が書き込まれる |

### TC-HD-DI02: ロック中 vault でのホットキーイベントはスキップ

| 手順 | 期待結果 |
|------|---------|
| ① `VekCache` をロック状態に設定 | |
| ② ホットキーイベントを送信 | `MockClipboard` に変化なし |

### TC-HD-DI03: secret エントリでクリアタイマーが起動する

| 手順 | 期待結果 |
|------|---------|
| ① `RecordKind::Secret` のエントリに `ctrl+alt+2` を登録 | |
| ② ホットキーイベントを送信 | クリップボードに値が書き込まれる |
| ③ 30 秒（pause/advance）後 | クリップボードがクリアされる |

### TC-HD-DI04: IPC `AddRecord` でホットキーが vault と OS に登録される

| 手順 | 期待結果 |
|------|---------|
| ① daemon を起動（mock 差し替え） | |
| ② IPC `AddRecord { hotkey: "ctrl+alt+1", ... }` を送信 | vault の hotkey フィールドが更新 + MockBackend に `ctrl+alt+1` が登録 |

### TC-HD-DI05: IPC `AddRecord` でホットキー競合時は `HotkeyConflict` を返す

| 手順 | 期待結果 |
|------|---------|
| ① 先に `ctrl+alt+1` を別エントリに登録 | |
| ② `AddRecord { hotkey: "ctrl+alt+1", ... }` を送信 | `IpcResponse::Error(IpcErrorCode::HotkeyConflict)` が返る |

### TC-HD-DI06: IPC `EditRecord` で `clear_hotkey` + `hotkey` 同時指定は拒否

| 手順 | 期待結果 |
|------|---------|
| ① `EditRecord { hotkey: "ctrl+alt+2", clear_hotkey: true, ... }` を送信 | `IpcResponse::Error(IpcErrorCode::HotkeyParseError)` が返る |

### TC-HD-DI07: `list` 出力にホットキーが表示される

`shikomi list` の出力（CLI IT として `assert_cmd` で検証）に `[ctrl+alt+1]` が含まれることを確認。

## 5. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-HD-DU01〜06 | `unit-core.yml` + `test-daemon.yml` | mock 使用のためヘッドレス OK |
| TC-HD-DI01〜06 | `test-daemon.yml` | mock 使用のためヘッドレス OK |
| TC-HD-DI07 | `test-cli.yml` | `assert_cmd` で CLI 出力確認 |
| Windows パス | `windows.yml` | Named Pipe 経路で同テストを実行 |

**`arboard` 実クリップボードテストは E2E に委ねる**: IT では `MockClipboard` で代替し、実 OS クリップボードのテストはシステムテスト設計書 TC-HK-E02〜E03 で行う。
