# feature-spec — daemon-hotkey-clipboard

<!-- feature: daemon-hotkey-clipboard / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/feature-spec.md -->
<!-- 本ファイルは最初の sub-feature PR で凍結。以降の sub-feature PR は引用のみ -->

## 1. 業務概要

shikomi のコア価値機能。ユーザが事前登録した文字列に **OS グローバルホットキー** を紐付け、キー押下一回でフォアグラウンドアプリのテキスト入力欄にクリップボード経由で即時投入する。パスワード等の機密エントリは投入後 30 秒でクリップボードを自動クリアする。

本 feature が完成して初めて shikomi は「動くプロダクト」になる。

## 2. ユースケース

### UC-HK-001: ホットキーをエントリに登録する

| 項目 | 内容 |
|------|------|
| アクター | エンドユーザー（CLI または GUI 経由） |
| 事前条件 | daemon が起動済み・vault にエントリが存在する |
| 基本フロー | ① `shikomi add --hotkey "ctrl+alt+1" --label "メールアドレス" --value "..."` を実行 ② daemon が IPC 経由で Vault にホットキーを登録し保存 ③ daemon がホットキーを OS に登録（グローバルショートカット） |
| 代替フロー | ホットキーが既に別エントリに登録済み → エラー `HotkeyConflict` を返す |
| 事後条件 | エントリに `hotkey: Hotkey` フィールドが保存され、OS ホットキー登録が有効になる |

### UC-HK-002: ホットキーでクリップボードに投入する

| 項目 | 内容 |
|------|------|
| アクター | エンドユーザー（ホットキー押下） |
| 事前条件 | daemon が起動済み・対象エントリにホットキーが登録済み |
| 基本フロー | ① ユーザがホットキー（例: `Ctrl+Alt+1`）を押下 ② daemon が対象エントリの値を取得 ③ daemon が値を OS クリップボードに書き込む ④ ユーザが任意アプリで `Ctrl/Cmd+V` で貼り付ける |
| 代替フロー A | vault が暗号化ロック中 → クリップボード書き込みをスキップ・通知なし（security: サイレント失敗） |
| 代替フロー B | エントリが secret フラグ付き → 書き込み後 30 秒で自動クリア（UC-HK-003 へ） |
| 事後条件 | OS クリップボードにエントリの値が書き込まれている |

### UC-HK-003: 機密エントリのクリップボードを自動クリアする

| 項目 | 内容 |
|------|------|
| アクター | daemon（自動処理） |
| 事前条件 | secret フラグ付きエントリが UC-HK-002 でクリップボードに書き込まれた |
| 基本フロー | ① 書き込みから 30 秒後に daemon がクリップボードを空文字で上書き ② （GUI 実装後）システムトレイのカウントダウン表示を更新 |
| 代替フロー | クリアタイマー動作中に別エントリが投入された → 前のタイマーをキャンセルし新しいタイマーを設定 |
| 事後条件 | OS クリップボードがクリアされている（空文字または無効化） |

### UC-HK-004: ホットキーを変更・解除する

| 項目 | 内容 |
|------|------|
| アクター | エンドユーザー（CLI または GUI 経由） |
| 事前条件 | エントリにホットキーが登録済み |
| 基本フロー A（変更）| `shikomi edit <id> --hotkey "ctrl+alt+2"` → 旧ホットキーを OS から解除 → 新ホットキーを OS に登録 → Vault 更新 |
| 基本フロー B（解除）| `shikomi edit <id> --clear-hotkey` → OS ホットキー解除 → Vault の hotkey フィールドを None に更新 |
| 事後条件 | 旧ホットキーが無効化され、新ホットキーまたはホットキーなし状態になる |

## 3. 機能要件

| ID | 要件 |
|----|------|
| R1-HK-01 | daemon はプロセス起動時に vault 内の全ホットキー登録済みエントリを OS に一括登録する |
| R1-HK-02 | ホットキー形式は `"modifier+modifier+key"` 文字列（例: `"ctrl+alt+1"`）。修飾キーは `ctrl` / `alt` / `shift` / `meta`、主キーは alphanumeric + ファンクションキー（F1〜F12）|
| R1-HK-03 | 同一ホットキーを複数エントリに登録することを禁止し、`HotkeyConflict` エラーで Fail Fast する |
| R1-HK-04 | クリップボード投入は `arboard` crate を使用し、Windows / macOS / Linux (X11・Wayland) の 3 OS で動作する |
| R1-HK-05 | secret フラグ付きエントリのクリップボード自動クリアは投入後 30 秒（設定変更なし、MVP固定値） |
| R1-HK-06 | Linux では起動時に `XDG_SESSION_TYPE` 環境変数と `ashpd` portal probe でセッション種別を判定し、ホットキー実装を実行時に選択する（feature flag を使わない） |
| R1-HK-07 | vault が暗号化ロック中のホットキー押下は投入をスキップし、外部に状態を漏洩しない（サイレント失敗） |
| R1-HK-08 | `shikomi add` / `shikomi edit` コマンドに `--hotkey <COMBO>` オプションを追加する |
| R1-HK-09 | `shikomi edit` コマンドに `--clear-hotkey` フラグを追加し、ホットキー解除を可能にする |
| R1-HK-10 | `shikomi list` 出力にホットキー割り当て状況を表示する（例: `[ctrl+alt+1]`） |
| R1-HK-11 | SQLite スキーマに `hotkey_combo TEXT` カラムを追加するマイグレーションを実施する（既存レコードは `NULL`） |

## 4. 非機能要件（本 feature スコープ）

| 項目 | 要件 |
|------|------|
| ホットキー応答遅延 | ホットキー受信からクリップボード書き込みまで 200ms 以内（HID イベント処理を除く） |
| macOS Secure Event Input | 対象アプリが SecureEventInput 状態の場合はサイレントスキップ（入力注入は MVP 非スコープ） |
| クリップボード競合 | 自動クリア直前に外部プロセスが書き込んだ場合もクリアを実行する（誤消去リスクは acceptable、MVP 仕様） |

## 5. 受入基準

| ID | 基準 |
|----|------|
| AC-HK-01 | `shikomi add --hotkey "ctrl+alt+1" --label "test" --value "hello"` で登録後、`Ctrl+Alt+1` 押下でクリップボードに "hello" が入る |
| AC-HK-02 | secret エントリのホットキー押下後 30 秒で OS クリップボードが空になる |
| AC-HK-03 | 既存の暗号化 vault テスト（e2e_encrypted.rs 系）が全通過する |
| AC-HK-04 | `shikomi list` に `[ctrl+alt+1]` 形式でホットキーが表示される |
| AC-HK-05 | Linux Wayland セッションで動作確認（CI: `XDG_SESSION_TYPE=wayland` 擬似環境） |

## 6. スコープ外（MVP 後回し）

- システムトレイのカウントダウン UI（GUI Issue #90 で対応）
- OS キーチェーンへのマスターキー保管連携
- ホットキー競合時のユーザーへのフィードバック通知
- `--paste-mode=inject`（キー注入フォールバック）
- Flatpak / Snap でのホットキー対応
- ホットキーのカスタム修飾キー組み合わせ検証（F1〜F12 以外の特殊キー）
