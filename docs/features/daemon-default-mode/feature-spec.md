# feature-spec — daemon-default-mode

<!-- feature: daemon-default-mode / Issue #125（Phase 2 全体）/ Issue #126（Sub-A: CLI --ipc 既定化）/ Issue #127（Sub-B: daemon OS 自動起動）-->
<!-- 配置先: docs/features/daemon-default-mode/feature-spec.md -->
<!-- 本ファイルは最初の sub-feature PR で凍結。以降の sub-feature PR は引用のみ -->

## 1. 業務概要

shikomi の Phase 2 移行。**daemon が vault の唯一の真実源**となり、CLI・GUI はすべて IPC 経由で daemon にアクセスするアーキテクチャへ完全移行する。

Phase 1（`daemon-ipc` feature）では `--ipc` フラグが opt-in だった。Phase 2 では IPC が既定となり、SQLite 直結（Phase 1 既定）は `--no-ipc` エスケープハッチを使わない限り不可能になる。

| 項目 | Phase 1（`daemon-ipc`） | Phase 2（本 feature） |
|------|----------------------|----------------------|
| CLI 既定経路 | SQLite 直結 | IPC（daemon 経由）|
| SQLite 直結 | 既定 | `--no-ipc` 明示時のみ |
| daemon 起動 | 手動 | OS 自動起動（Sub-B） |
| `MSG-CLI-051` | 表示（IPC opt-in 警告） | 廃止（IPC が既定のため不要） |

本 feature は 2 Sub-issue に分割:

- **Sub-A（Issue #126）**: CLI `--ipc` 既定化・`--no-ipc` エスケープハッチ（本 feature の主テーマ）
- **Sub-B（Issue #127）**: daemon OS 自動起動（launchd / systemd / Windows Task Scheduler）

## 2. ユースケース

### UC-DDM-001: IPC 経由で CLI 操作を行う（新既定）

| 項目 | 内容 |
|------|------|
| アクター | エンドユーザー（CLI 使用者） |
| 事前条件 | `shikomi-daemon` が起動済み（Sub-B で OS 自動起動が保証される）|
| 基本フロー | ① `shikomi list` / `shikomi add ...` / `shikomi edit ...` / `shikomi remove ...` を実行 ② CLI がデフォルトで IPC 経路（daemon 経由）を使用 ③ daemon が vault を操作して結果を返す |
| 代替フロー A | daemon 未起動 → `MSG-CLI-110`（daemon が起動していない旨）で exit 1 |
| 代替フロー B | プロトコルバージョン不一致 → `MSG-CLI-111` で exit 1 |
| 事後条件 | IPC 経由でのレコード操作が成功し、結果が stdout に表示される |

### UC-DDM-002: SQLite 直結エスケープハッチ（--no-ipc）

| 項目 | 内容 |
|------|------|
| アクター | 上級ユーザー / システム管理者 |
| 事前条件 | daemon が起動できない環境（CI / 緊急復旧 / daemon クラッシュ）|
| 基本フロー | ① `shikomi --no-ipc list` を実行 ② CLI が SQLite 直結経路（Phase 1 既定相当）を使用 ③ vault.db に直接アクセスして結果を返す |
| 代替フロー | `--no-ipc` と `--vault-dir` を組み合わせた緊急復旧操作 |
| 事後条件 | daemon を介さず SQLite から直接レコードを取得して表示 |

## 3. 機能要件

| ID | 要件 |
|----|------|
| R1-DDM-01 | CLI サブコマンド（list / add / edit / remove）の既定接続経路を IPC（daemon 経由）にする |
| R1-DDM-02 | `--no-ipc` グローバルフラグを追加し、SQLite 直結経路を明示的に選択できるようにする |
| R1-DDM-03 | `--ipc` フラグを廃止する（Phase 1 との後方互換は意図的に切る）|
| R1-DDM-04 | `MSG-CLI-051`（IPC opt-in 警告）を廃止する。IPC が既定となったため表示不要 |
| R1-DDM-05 | `MSG-CLI-110` の hint 文面を更新する。`--ipc` フラグの案内を削除し、daemon 起動コマンドのみ案内する |
| R1-DDM-06 | vault サブコマンド（`shikomi vault *`）は従来通り IPC 強制（`--no-ipc` 指定時も IPC 経路を維持する）|
| R1-DDM-07 | GUI サブコマンド（`shikomi gui`）は IPC / SQLite 分岐に影響されない（従来通り）|
| R1-DDM-08 | daemon OS 自動起動（launchd / systemd / Windows Task Scheduler）を実装する（Sub-B スコープ）|

## 4. 非機能要件（本 feature スコープ）

| 項目 | 要件 |
|------|------|
| 後方互換 | `--ipc` フラグの廃止はユーザー設定スクリプトへの破壊的変更になる。`CHANGELOG.md` に migration guide を記載する |
| 応答遅延 | `--no-ipc` 経路（SQLite 直結）のパフォーマンスは Phase 1 と同等（変更なし）|
| IPC 既定化の副作用 | daemon 未起動時の `shikomi list` 等が `MSG-CLI-110` で即時失敗する。Sub-B の OS 自動起動が揃うまでの移行期間は、ユーザーに手動起動を求める |

## 5. 受入基準

### Sub-A（Issue #126）: CLI IPC 既定化

| ID | 基準 |
|----|------|
| AC-DDM-01 | daemon 起動状態で `shikomi list`（`--ipc` なし）が IPC 経由でレコード一覧を返す |
| AC-DDM-02 | `shikomi --no-ipc list` が SQLite 直結でレコード一覧を返す（daemon 未起動でも動作する）|
| AC-DDM-03 | daemon 未起動状態で `shikomi list` が `MSG-CLI-110` を stderr に出力して exit 1 で終了する |
| AC-DDM-04 | `shikomi --ipc list` が `unknown option: --ipc` エラーで exit 1 になる（廃止確認）|
| AC-DDM-05 | daemon 起動状態で `shikomi list` の stderr に `MSG-CLI-051` が出力**されない**こと（廃止確認）|
| AC-DDM-06 | `shikomi --no-ipc vault encrypt` が失敗し、vault サブコマンドが IPC 強制されていることを確認 |

### Sub-B（Issue #127）: daemon OS 自動起動

| ID | 基準 |
|----|------|
| AC-DDM-07 | `shikomi daemon install` が成功し、stdout に `"shikomi-daemon autostart enabled"` + OS 固有 hint を出力して exit 0 で終了する。OS 固有の自動起動ファイル（macOS: plist / Linux: systemd unit または .desktop / Windows: schtasks タスク）が配置されていること |
| AC-DDM-08 | `shikomi daemon uninstall` が成功し、stdout に `"shikomi-daemon autostart disabled"` を出力して exit 0 で終了する。自動起動ファイルが削除されていること |
| AC-DDM-09 | `shikomi daemon status` が常に exit 0 で終了し、`"daemon: running"` / `"daemon: not running"` + `"autostart: enabled"` / `"autostart: disabled"` の 2 行を正しく出力する |
| AC-DDM-10 | `shikomi daemon install` を 2 回連続実行しても 2 回目も exit 0 で終了する（冪等性確認）|

## 6. スコープ外

| 項目 | 理由 |
|------|------|
| OS 再起動後の daemon 自動起動実証 | CI 環境での OS 再起動テストは実施不可。手動受入テストとして実施する（`SC-DDM-002` §手動確認事項）|
| MSG-CLI-110 hint への `shikomi daemon install` 誘導追加 | Sub-B 完了後の別 PR で実施（`autostart/basic-design.md §Sub-B 完了後に更新するメッセージ`）|
| `--ipc` 廃止の移行支援 CLI（`shikomi migrate` 等）| YAGNI（移行コストは軽微、`CHANGELOG.md` で十分）|
| IPC プロトコルの変更 | `daemon-ipc` feature の protocol は `V1` で凍結済み（`ipc-protocol.md §バージョニングルール`）|
| ホットキー / クリップボード / 暗号化 vault の Phase 2 対応 | 各後続 feature のスコープ |

## 7. Phase 移行戦略

Sub-A（Issue #126）完了後、Sub-B（Issue #127）着手前の **移行期間** は:

- CLI は IPC 既定（daemon 必須）
- daemon は手動起動のみ
- `--no-ipc` が緊急脱出手段として機能する

Sub-B 完了後:

- daemon が OS 起動時に自動起動する
- ユーザーは daemon を意識せずに `shikomi` を使える（Phase 2 完全体）

| 参考資料 | |
|---------|---|
| `process-model.md §4.1.1` | 「Phase 2 移行はコンポジションルートの 1 行差し替えで完了」|
| `daemon-ipc/detailed-design/future-extensions.md §Phase 進捗` | Phase 2 が「後続 Issue（未起票）→ Issue #125/126/127」として確定 |
| `daemon-ipc/detailed-design/future-extensions.md §将来拡張` | `--ipc` 既定化の設計フック（本 feature で実体化）|
