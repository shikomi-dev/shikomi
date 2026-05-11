# テスト設計書 — system-tray（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: system-tray / Issue #97 -->
<!-- 配置先: docs/features/shikomi-gui/system-tray/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->
<!-- 参照: basic-design.md §モジュール契約 / detailed-design.md §1〜9 -->

## 0. テスト方針参照

本テスト設計書は `config/prompts/test_strategy.md` に定めるテスト戦略（Vモデル階層化・ダブル方針・CI ワークフロー対応）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは親 `system-test-design.md` に委ねる。

**Tauri ランタイム依存の扱い**:

`TrayIcon`・`AppHandle`・`WebviewWindow` などの Tauri ランタイム API は、UT/IT レベルでの完全モックが困難なため、次の方針を取る:

1. **純粋ロジックを関数として分離**し UT で検証する（ツールチップ文字列生成・残秒計算）
2. **Tauri Command (`get_clipboard_countdown`)** は `AppState` + MockDaemon で IT 検証する
3. **Tauri ランタイム操作（`window.show()`・`app.exit()`・`tray.set_tooltip()`）** は Tauri フレームワーク動作として信頼し、IT/UT 対象外とする。tracing ログを証跡として残す

---

## 1. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 | Fixture 状態 |
|-------|---------|---------|------|------------|
| IT（`get_clipboard_countdown`） | UDS / Named Pipe（daemon 接続） | `GuiIpcClient`（`IpcRequest::GetClipboardStatus`） | `MockDaemon`（Sub-B 実装済み `tests/common/mock_daemon.rs`）で差し替え | 流用可（Sub-B の MockDaemon は `shikomi-core::ipc` 実フォーマット準拠済み） |
| UT（残秒計算） | `Instant::now()` | 時刻 | テスト内で `Instant` を固定値として引数渡し（依存注入。`countdown_started_at` + `now` を引数で受け取る純粋関数として設計） | 不要 |
| UT（ツールチップ文字列） | なし | 純粋計算（文字列フォーマット） | モック不要 | 不要 |
| IT（`get_clipboard_countdown` — IPC エラー） | UDS | MockDaemon が接続切断 | `MockDaemon` が接続拒否するよう設定 | 流用可 |

> **assumed mock 禁止**: `MockDaemon` が返す `IpcResponse::ClipboardStatus` は `shikomi-core::ipc` の実型を使用する。インラインバイト列・手作り JSON は却下対象。
> **daemon 側テスト分離**: `GetClipboardStatus` ハンドラ（`countdown_started_at` 残秒計算）は `shikomi-daemon` crate の UT スコープ。本設計書は GUI 側（Tauri Command + countdown タスク）のみを対象とする。

---

## 2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT（ツールチップ文字列生成） | `crates/shikomi-gui/src/system_tray/countdown.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-gui` |
| UT（残秒計算ロジック） | `crates/shikomi-gui/src/system_tray/countdown.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-gui` |
| IT（`get_clipboard_countdown` Command） | `crates/shikomi-gui/tests/it_system_tray.rs` | `cargo test -p shikomi-gui` |

---

## 3. テスト用ダブルの方針

### 3.1 `MockDaemon`（Sub-B 流用）

Sub-B で実装済みの `tests/common/mock_daemon.rs` を流用する。`IpcResponse::ClipboardStatus { remaining_secs }` を返すパターンを追加する。

| 項目 | 仕様 |
|------|------|
| 実装 | `MockDaemon::with_response(IpcResponse::ClipboardStatus { remaining_secs: Some(20) })` |
| フレームコーデック | `basic-design.md §1.3` と同一（little-endian 4バイト長 / MessagePack）|
| 新規追加レスポンスパターン | `IpcResponse::ClipboardStatus { remaining_secs: None }` / `Some(n)` の 2 種 |

### 3.2 `AppState` の直接構築

Sub-B と同様。Tauri Command ハンドラは `tauri::State<AppState>` を受け取る。

| パターン | 構築方法 |
|---------|---------|
| daemon 接続済み | `Arc::new(tokio::sync::Mutex::new(Some(client)))` |
| daemon 未接続 | `Arc::new(tokio::sync::Mutex::new(None))` |

### 3.3 時刻固定（残秒計算 UT）

`remaining_secs` 計算ロジックは `fn calc_remaining(started_at: Option<Instant>, now: Instant) -> Option<u64>` として純粋関数に分離する（詳細設計 §6.2 の計算をテスト容易な形に実装する）。テストでは `Instant` を固定値として渡すことで `Instant::now()` への依存をなくす。

---

## 4. テストマトリクス（トレーサビリティ）

### 4.1 ユニットテスト

| テスト ID | REQ-TRAY | 設計根拠 | テスト内容 | 種別 |
|---------|---------|--------|----------|------|
| TC-GUI-TRAY-UT01 | REQ-TRAY-05 | `detailed-design.md §9` 文言一覧 | `remaining_secs=Some(15)` → `"shikomi — クリップボードを自動消去まで 15 秒"` | 正常系 |
| TC-GUI-TRAY-UT02 | REQ-TRAY-05 | `detailed-design.md §9` 文言一覧 | `remaining_secs=Some(1)` → `"shikomi — クリップボードを自動消去まで 1 秒"`（最小正値） | 正常系（境界値） |
| TC-GUI-TRAY-UT03 | REQ-TRAY-05 | `detailed-design.md §4.1`（`n > 0` 条件） | `remaining_secs=Some(0)` → `"shikomi"`（0秒は非アクティブ扱い） | 正常系（境界値） |
| TC-GUI-TRAY-UT04 | REQ-TRAY-05 | `detailed-design.md §9` 文言一覧 | `remaining_secs=None` → `"shikomi"` | 正常系 |
| TC-GUI-TRAY-UT05 | REQ-TRAY-04 | `detailed-design.md §6.2` | `countdown_started_at=None` → `calc_remaining(None, now)` → `None` | 正常系 |
| TC-GUI-TRAY-UT06 | REQ-TRAY-04 | `detailed-design.md §6.2` | `elapsed=10s`（`CLEAR_TIMEOUT_SECS=30`）→ `calc_remaining(...)` → `Some(20)` | 正常系 |
| TC-GUI-TRAY-UT07 | REQ-TRAY-04 | `detailed-design.md §6.2` | `elapsed=30s`（境界: 丁度タイムアウト）→ `calc_remaining(...)` → `None` | 正常系（境界値） |
| TC-GUI-TRAY-UT08 | REQ-TRAY-04 | `detailed-design.md §6.2` | `elapsed=31s`（超過）→ `calc_remaining(...)` → `None` | 正常系（境界値） |
| TC-GUI-TRAY-UT09 | REQ-TRAY-04 | `detailed-design.md §6.2` | `elapsed=29s` → `calc_remaining(...)` → `Some(1)`（最小正値境界） | 正常系（境界値） |

### 4.2 結合テスト

| テスト ID | REQ-TRAY | 設計根拠 | テスト内容 | 種別 |
|---------|---------|--------|----------|------|
| TC-GUI-TRAY-IT01 | REQ-TRAY-04 | `detailed-design.md §5.2` | `AppState=None` → `get_clipboard_countdown` → `Ok(ClipboardCountdownResult { remaining_secs: None })`（エラーなし） | 正常系（daemon 未接続） |
| TC-GUI-TRAY-IT02 | REQ-TRAY-04 | `detailed-design.md §5.1` | `AppState=Some`, MockDaemon が `ClipboardStatus { remaining_secs: Some(20) }` 返却 → `ClipboardCountdownResult { remaining_secs: Some(20) }` | 正常系 |
| TC-GUI-TRAY-IT03 | REQ-TRAY-04 | `detailed-design.md §5.1` | `AppState=Some`, MockDaemon が `ClipboardStatus { remaining_secs: None }` 返却 → `ClipboardCountdownResult { remaining_secs: None }` | 正常系 |
| TC-GUI-TRAY-IT04 | REQ-TRAY-04 | `detailed-design.md §4.2`（IPC エラー時の扱い） | IPC 通信エラー（MockDaemon が接続切断）→ `Ok(ClipboardCountdownResult { remaining_secs: None })`（エラー非伝搬。`tracing::debug!` のみ） | 異常系 |
| TC-GUI-TRAY-IT05 | REQ-TRAY-04 | `detailed-design.md §5.1` シリアライズ | `remaining_secs: Some(15)` → JSON シリアライズで `{ "remaining_secs": 15 }`（数値） | 正常系 |
| TC-GUI-TRAY-IT06 | REQ-TRAY-04 | `detailed-design.md §5.1` シリアライズ | `remaining_secs: None` → JSON シリアライズで `{ "remaining_secs": null }` | 正常系 |

---

## 5. ユニットテスト詳細設計

### TC-GUI-TRAY-UT01〜UT04: ツールチップ文字列生成（REQ-TRAY-05）

| 項目 | 内容 |
|------|------|
| 対応する要件ID | REQ-TRAY-05（R1-GUI-15） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §9` 文言一覧） |
| 種別 | 正常系・境界値 |
| テスト対象関数 | `tooltip_text(remaining_secs: Option<u64>) -> &'static str / String` |
| 前提条件 | 純粋関数呼び出し。外部依存なし |
| 操作・期待結果 | 下表参照 |

| テスト ID | 入力 `remaining_secs` | 期待ツールチップ文字列 | 種別 |
|---------|-------|--------|------|
| TC-GUI-TRAY-UT01 | `Some(15)` | `"shikomi — クリップボードを自動消去まで 15 秒"` | 正常系 |
| TC-GUI-TRAY-UT02 | `Some(1)` | `"shikomi — クリップボードを自動消去まで 1 秒"` | 境界値（最小正値）|
| TC-GUI-TRAY-UT03 | `Some(0)` | `"shikomi"` | 境界値（0秒 = 非アクティブ扱い）|
| TC-GUI-TRAY-UT04 | `None` | `"shikomi"` | 正常系（非アクティブ）|

**設計根拠**: `detailed-design.md §4.1` の分岐条件「`remaining_secs == Some(n), n > 0`」と `§9` 凍結文言テーブルの完全一致を検証する。ツールチップ文字列はこの関数が単一責務を持ち、`countdown::run()` と `tray.set_tooltip()` 呼び出し側で生成しない（DRY）。

---

### TC-GUI-TRAY-UT05〜UT09: 残秒計算ロジック（REQ-TRAY-04）

| 項目 | 内容 |
|------|------|
| 対応する要件ID | REQ-TRAY-04（R1-GUI-15） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.2`） |
| テスト対象関数 | `calc_remaining(started_at: Option<Instant>, now: Instant) -> Option<u64>` |
| 前提条件 | `CLEAR_TIMEOUT_SECS = 30`（定数）。`now` と `started_at` を引数で渡す（時刻依存注入） |

| テスト ID | `started_at` | `now - started_at` | 期待 `remaining_secs` | 種別 |
|---------|------------|---------|------|------|
| TC-GUI-TRAY-UT05 | `None` | — | `None`（非アクティブ） | 正常系 |
| TC-GUI-TRAY-UT06 | `Some(t)` | 10秒 | `Some(20)` | 正常系 |
| TC-GUI-TRAY-UT07 | `Some(t)` | 30秒（境界: 丁度タイムアウト） | `None` | 境界値 |
| TC-GUI-TRAY-UT08 | `Some(t)` | 31秒（超過） | `None` | 境界値 |
| TC-GUI-TRAY-UT09 | `Some(t)` | 29秒 | `Some(1)`（最小正値） | 境界値 |

**設計根拠**: `detailed-design.md §6.2` の 3 条件分岐（`None` / `elapsed >= CLEAR_TIMEOUT_SECS` / `elapsed < CLEAR_TIMEOUT_SECS`）を全て網羅する。`Instant::now()` を引数として外から渡すことでテスト時の時刻固定を実現し、assumed mock 禁止を守る。

---

## 6. 結合テスト詳細設計

### TC-GUI-TRAY-IT01: `get_clipboard_countdown` — daemon 未接続（AppState=None）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-TRAY-IT01 |
| 対応する要件ID | REQ-TRAY-04（R1-GUI-15）、`detailed-design.md §5.2` |
| 対応する工程 | 階層 3 基本設計（`basic-design.md §3.3` Tauri Command 契約） |
| 種別 | 正常系（未接続サイレントフォールバック） |
| 前提条件 | `AppState = Arc::new(Mutex::new(None))`（daemon 未接続） |
| 操作 | `get_clipboard_countdown(state).await` |
| 期待結果 | `Ok(ClipboardCountdownResult { remaining_secs: None })` が返る。IPC 送信は発生しない。countdown ポーリングがエラーパネルを誘発しないこと |

**設計根拠**: `detailed-design.md §5.2`「`AppState == None` の場合、IPC 呼び出しをスキップして即 `{ remaining_secs: null }` を返す」の契約検証。

---

### TC-GUI-TRAY-IT02: `get_clipboard_countdown` — `remaining_secs: Some(20)` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-TRAY-IT02 |
| 対応する要件ID | REQ-TRAY-04 |
| 種別 | 正常系 |
| 前提条件 | `AppState = Some(client)`、MockDaemon が `IpcResponse::ClipboardStatus { remaining_secs: Some(20) }` を返す |
| 操作 | `get_clipboard_countdown(state).await` |
| 期待結果 | `Ok(ClipboardCountdownResult { remaining_secs: Some(20) })` が返る |

---

### TC-GUI-TRAY-IT03: `get_clipboard_countdown` — `remaining_secs: None` 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-TRAY-IT03 |
| 対応する要件ID | REQ-TRAY-04 |
| 種別 | 正常系（カウントダウン非アクティブ） |
| 前提条件 | `AppState = Some(client)`、MockDaemon が `IpcResponse::ClipboardStatus { remaining_secs: None }` を返す |
| 操作 | `get_clipboard_countdown(state).await` |
| 期待結果 | `Ok(ClipboardCountdownResult { remaining_secs: None })` が返る |

---

### TC-GUI-TRAY-IT04: `get_clipboard_countdown` — IPC 通信エラー → エラー非伝搬

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-TRAY-IT04 |
| 対応する要件ID | REQ-TRAY-04、`detailed-design.md §4.2`（IPC エラー時の扱い） |
| 種別 | 異常系 |
| 前提条件 | `AppState = Some(client)`、MockDaemon が接続を即切断（`ConnectionFailed` 相当） |
| 操作 | `get_clipboard_countdown(state).await` |
| 期待結果 | `Ok(ClipboardCountdownResult { remaining_secs: None })` が返る（エラーは `tracing::debug!` のみ。`Err` を返さない）|

**設計根拠**: `detailed-design.md §4.2`「IPC 通信エラー → `tracing::debug!` でログのみ」「countdown タスクがエラーパネルを誘発しない」の契約検証。

---

### TC-GUI-TRAY-IT05〜IT06: `ClipboardCountdownResult` JSON シリアライズ契約

| テスト ID | `remaining_secs` 入力 | 期待 JSON | 種別 |
|---------|-----|------|------|
| TC-GUI-TRAY-IT05 | `Some(15)` | `{ "remaining_secs": 15 }` | 正常系 |
| TC-GUI-TRAY-IT06 | `None` | `{ "remaining_secs": null }` | 正常系 |

| 項目 | 内容 |
|------|------|
| 対応する要件ID | REQ-TRAY-04（R1-GUI-15）、`detailed-design.md §5.1` シリアライズ |
| 対応する工程 | 階層 3 詳細設計（SolidJS 側ペイロード型凍結） |
| 種別 | 正常系 |
| 操作 | `serde_json::to_value(&result).unwrap()` で JSON 変換し `["remaining_secs"]` フィールドを assert |

**設計根拠**: `detailed-design.md §9`「SolidJS イベントペイロード型（凍結）: `remaining_secs: number | null`」との一致を UT で確認する。SolidJS 側が `null` / 数値を正しく受け取れることを型契約として保証する。

---

## 7. テスト対象外の明示

| 機能 | 対象外の理由 | 代替検証 |
|------|-----------|--------|
| `TrayIconBuilder::build()` 成功 | Tauri ランタイムが必要。UI ライブラリの API 動作を信頼 | 実機動作確認（システムテスト / 受入テスト）|
| `CloseRequested` → `window.hide()` | Tauri `WebviewWindow` イベントのモックが困難 | システムテストで OS 操作から検証 |
| トレイメニュー右クリック → popup 表示 | Tauri `TrayIconEvent` のモックが困難 | システムテストで実機確認 |
| `"open_window"` メニュー → `window.show()` | Tauri API 動作 | システムテスト |
| `"restart_daemon"` メニュー → `tauri-plugin-shell` 実行 | OS プロセス起動。副作用あり | システムテスト |
| `"quit"` メニュー → `app.exit(0)` | Tauri プロセス終了。テスト環境で実行不可 | システムテスト |
| countdown ポーリングループ全体 | `AppHandle` の spawn + TrayIcon + emit が連動。Tauri ランタイム必要 | 上記 UT（純粋ロジック）+ IT（Command）で個別に担保 |
| macOS `Reopen` イベント | プラットフォーム固有。macOS 実機必要 | CI 外の実機検証 |
| Linux Wayland `set_tooltip` 失敗 | OS 依存。エラーを飲み込む設計（best-effort） | Wayland 環境での実機検証 |

---

## 8. daemon 側テスト設計（参照）

`GetClipboardStatus` ハンドラ・`countdown_started_at` 状態機械は `shikomi-daemon` crate のスコープ。以下は本設計書の参照情報として記載するが、実装・テストは `crates/shikomi-daemon/` 内で行う。

| daemon 側テスト | 対応する要件 | 配置先 |
|--------------|-----------|--------|
| `countdown_started_at=None` → `GetClipboardStatus` → `remaining_secs: None` | `basic-design.md §3.2` | daemon UT |
| Secret 投入 → `countdown_started_at = Some(Instant::now())` | `detailed-design.md §6.1` | daemon IT |
| ClearTimer 完了 → `countdown_started_at = None` | `detailed-design.md §6.1` | daemon IT |
| `elapsed < CLEAR_TIMEOUT_SECS` → `remaining_secs: Some(n)` | `detailed-design.md §6.2` | daemon UT |
| `elapsed >= CLEAR_TIMEOUT_SECS` → `remaining_secs: None` | `detailed-design.md §6.2` | daemon UT |

---

## 9. モック方針まとめ

| テスト対象 | モック要否 | 実装方法 |
|----------|---------|---------|
| UDS / Named Pipe（daemon 接続） | **IT で差し替え** | Sub-B 実装済み `tests/common/mock_daemon.rs` に `ClipboardStatus` レスポンス追加 |
| `Instant::now()` | **UT で依存注入** | `calc_remaining(started_at, now: Instant)` に `now` を引数で渡す |
| `TrayIcon`・`AppHandle`・`WebviewWindow` | **UT/IT 対象外** | 純粋ロジック分離で回避。残りはシステムテスト |
| `AppState` | **IT で直接構築** | `Arc::new(Mutex::new(Some(client)))` / `None` |
| `ClipboardCountdownResult` シリアライズ | **モック不要** | 純粋計算。実 `serde_json` を通す |

**assumed mock 禁止**: MockDaemon が返す `IpcResponse::ClipboardStatus` は `shikomi-core::ipc` の実型を使用すること。

---

## 10. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-GUI-TRAY-UT01〜UT09（9件） | `lint.yml` + 既存 `test-gui.yml` | UDS 不使用のためヘッドレス OK |
| TC-GUI-TRAY-IT01〜IT06（6件） | 既存 `test-gui.yml` | tempfile + UDS 使用。Linux/macOS で実行 |
| Windows IT | `windows.yml` | Named Pipe 経路で TC-TRAY-IT01〜IT04 相当を実行（Sub-B `it_ipc_client.rs` と同パターン）|

---

## 11. カバレッジ基準

| 観点 | 基準 |
|------|------|
| REQ-TRAY 全件網羅 | REQ-TRAY-04, 05 が IT または UT でカバーされること（REQ-TRAY-01〜03, 06 は Tauri ランタイム依存のためシステムテスト担当）|
| 正常系 | `get_clipboard_countdown` の全パス（未接続 / カウントダウン中 / 非アクティブ）必須 |
| 異常系 | IPC 通信エラー時の非伝搬を必ず検証 |
| 境界値 | 残秒 0・1・29・30・31 秒を必ず含む（`CLEAR_TIMEOUT_SECS=30` 前後の挙動） |
| シリアライズ | `remaining_secs: Some(n)` → `number`、`None` → `null` の JSON 型契約を IT で検証 |

---

*作成: 涅マユリ（テスト担当）/ 2026-05-11*
*設計根拠: `docs/features/shikomi-gui/system-tray/basic-design.md` §モジュール契約 / `detailed-design.md` §1〜9 / Issue #97*
