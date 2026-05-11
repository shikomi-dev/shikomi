# システムテスト設計書 — daemon-hotkey-clipboard

<!-- feature: daemon-hotkey-clipboard / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/system-test-design.md -->
<!-- システムテスト（E2E）はここにのみ記述。sub-feature の test-design.md には IT / UT のみ -->

## 1. テスト戦略概要

本 feature のシステムテストは「ホットキー押下 → クリップボード書き込み」の **エンドツーエンド経路**を検証する。ホットキーイベントは CI 環境でグローバルホットキー受信不可のため、**`MockBackend::send_event()` によるイベント直接注入**で発火を再現する（`IpcRequest::TriggerHotkey` 等の本番コードへの裏口 variant は設けない）。実 OS ホットキーの確認は手動受入テスト（AC-HK-01〜05）で補う。

| テストレベル | 担当ファイル | 主な対象 |
|------------|------------|---------|
| E2E（システムテスト）| 本ファイル | daemon 起動〜ホットキー登録〜クリップボード書き込みの全体フロー |
| 結合テスト（IT）| `domain/test-design.md` | Hotkey VO の解析・バリデーション、IPC スキーマ往復 |
| 結合テスト（IT）| `daemon/test-design.md` | HotkeyManager / ClipboardWriter / ClearTimer の統合動作 |
| ユニットテスト（UT）| 各 sub-feature の test-design.md | 単体関数・型の契約検証 |

## 2. E2E テストケース

### TC-HK-E01: daemon 起動時の vault ホットキー一括登録

| 項目 | 内容 |
|------|------|
| テスト ID | TC-HK-E01 |
| 対応要件 | R1-HK-01 |
| 前提 | ホットキー付きエントリを 2 件含む vault.db が存在する |
| 手順 | ① daemon を起動 ② `SHIKOMI_DAEMON_LOG=debug` のログを確認 |
| 期待結果 | `hotkey registered: ctrl+alt+1` / `hotkey registered: ctrl+alt+2` がログに出力される |
| CI 実行 | `test-daemon.yml` で `SHIKOMI_VAULT_DIR` を fixture に向けて実行 |

### TC-HK-E02: IPC 経由ホットキー付きエントリ追加 + クリップボード書き込み

| 項目 | 内容 |
|------|------|
| テスト ID | TC-HK-E02 |
| 対応要件 | R1-HK-04, R1-HK-08 |
| 前提 | daemon が起動済み |
| 手順 | ① `shikomi add --hotkey "ctrl+alt+1" --label "e2e" --value "hello-e2e"` ② `MockBackend::send_event(HotkeyEvent { combo: "ctrl+alt+1" })` でイベントを直接注入（daemon は `MockBackend` 差し替え済みで起動）③ OS クリップボードの内容を `arboard` で読み取る |
| 期待結果 | クリップボードに `"hello-e2e"` が書き込まれている |
| CI 実行 | `test-daemon.yml` に追加（`Xvfb` または `SHIKOMI_DISABLE_CLIPBOARD=1` 不使用: arboard 実クリップボードを検証） |

### TC-HK-E03: secret エントリの 30 秒自動クリア

| 項目 | 内容 |
|------|------|
| テスト ID | TC-HK-E03 |
| 対応要件 | R1-HK-05 |
| 前提 | TC-HK-E02 の環境 |
| 手順 | ① `shikomi add --hotkey "ctrl+alt+2" --label "pw" --secret` → stdin でパスワード入力 ② `MockBackend::send_event(HotkeyEvent { combo: "ctrl+alt+2" })` でイベントを注入 ③ `tokio::time::pause()` + `advance(Duration::from_secs(31))` で時間を早送り ④ クリップボードを読み取る |
| 期待結果 | クリップボードが空文字またはデータなし状態 |
| CI 実行 | `tokio::time` の仮想時間制御を使用するため実時間 60s 待機不要。`test-daemon.yml` で即時実行可 |

### TC-HK-E04: ホットキー重複登録エラー

| 項目 | 内容 |
|------|------|
| テスト ID | TC-HK-E04 |
| 対応要件 | R1-HK-03 |
| 手順 | ① `shikomi add --hotkey "ctrl+alt+1" --label "a" --value "x"` ② 同一ホットキーで `shikomi add --hotkey "ctrl+alt+1" --label "b" --value "y"` |
| 期待結果 | 2 回目で `Error: hotkey "ctrl+alt+1" is already assigned to entry "a"` が stderr に出力。exit code 非 0 |
| CI 実行 | `test-cli.yml` に追加 |

### TC-HK-E05: ロック中 vault のホットキー押下は OS 通知 + スキップ

| 項目 | 内容 |
|------|------|
| テスト ID | TC-HK-E05 |
| 対応要件 | R1-HK-07, R1-HK-13 |
| 手順 | ① 暗号化 vault で daemon 起動（ロック状態、`MockBackend` 差し替え済み）② `MockBackend::send_event(HotkeyEvent { combo: "ctrl+alt+1" })` でイベントを注入 ③ クリップボードを確認 ④ OS 通知の発火を検証 |
| 期待結果 | クリップボードに変化なし。OS 通知「vault がロック中です。`shikomi vault unlock` を実行してください」が発火する（R1-HK-13）。監査ログに `result: "skipped:vault_locked"` が記録される（R1-HK-12） |
| CI 実行 | `test-daemon.yml`（Sub-E fixture 流用。OS 通知は `MockNotifier` で検証） |

## 3. 手動受入テスト（CI 自動化不可領域）

CI 環境ではグローバルホットキーの実 OS 登録が不可のため、以下は手動で実施する。

| AC | 対象 OS | 手順 |
|----|--------|------|
| AC-HK-01 | Windows / macOS / Linux X11 | 実機で `ctrl+alt+1` 押下後にメモ帳等に貼り付け確認 |
| AC-HK-02 | 全 OS | secret エントリで 30 秒後クリアをストップウォッチで確認 |
| AC-HK-05 | Linux Wayland | GNOME 44+ / KDE Plasma 6+ セッションで動作確認 |

## 4. CI ワークフロー対応方針

- `test-daemon.yml` に `TC-HK-E01〜E05` を追加
- `test-cli.yml` に `TC-HK-E04` を追加
- ヘッドレス環境での `arboard` 動作は `Xvfb` または `weston --headless`（Wayland）を CI ジョブで起動して対応
- Windows CI は `windows.yml` に追加（Named Pipe 経路）
