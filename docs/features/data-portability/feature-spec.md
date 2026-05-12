# feature-spec — data-portability

<!-- feature: data-portability / Issue #135（Phase 2 export/import）/ Issue #140（Sub-A: domain）/ Issue #141（Sub-B: cli）-->
<!-- 配置先: docs/features/data-portability/feature-spec.md -->
<!-- 本ファイルは最初の sub-feature PR で凍結。以降の sub-feature PR は引用のみ -->

## 1. 業務概要

shikomi の vault データをファイル単位でエクスポート・インポートする機能を提供する。ユーザーが端末移行・バックアップ・別 vault への引越しを安全かつ確実に行えることが目的。

スコープ: **ローカル export/import のみ**。クラウド同期は将来拡張（`nfr.md §6 Out of Scope` 参照）。

本 feature は 2 Sub-issue に分割:

- **Sub-A（Issue #140）**: domain — `ExportRecord` / `ImportRecord` 型・JSON シリアライゼーション形式・バリデーション
- **Sub-B（Issue #141）**: cli — `export` / `import` サブコマンド・UseCase wiring・Presenter

## 2. ユースケース

### UC-DP-001: vault をファイルにエクスポートする

| 項目 | 内容 |
|------|------|
| アクター | エンドユーザー（CLI 使用者）|
| 事前条件 | daemon 起動済み（IPC 経路）または `--no-ipc` 指定済み。暗号化 vault の場合は vault がアンロック済み |
| 基本フロー | ① `shikomi export --output <FILE>` を実行 ② CLI が `VaultRepository` 経由で全レコードを取得 ③ `Secret` kind のペイロードは `{"kind":"redacted"}` tagged union（既定）で表現した `ExportPayload` を構築 ④ JSON にシリアライズして `<FILE>` に `0600` パーミッションで書き込む ⑤ stdout に成功メッセージ（件数・出力先）を表示 |
| 代替フロー A | `--export-secrets` フラグ指定時 → stderr に `MSG-CLI-145`（Secret 平文 export 警告）を出力してから Secret kind のペイロードを平文で含める |
| 代替フロー B | vault がロック済みの暗号化モード → `MSG-CLI-140` で exit 1 |
| 代替フロー C | `<FILE>` が既に存在 → `MSG-CLI-141`（上書き確認）で exit 1（`--force` で上書き可）|
| 事後条件 | `<FILE>` に有効な `ExportPayload` JSON が書き込まれている |

### UC-DP-002: ファイルから vault にインポートする

| 項目 | 内容 |
|------|------|
| アクター | エンドユーザー（CLI 使用者）|
| 事前条件 | daemon 起動済みまたは `--no-ipc` 指定済み。`<FILE>` が有効な `ExportPayload` JSON |
| 基本フロー | ① `shikomi import --input <FILE>` を実行 ② ファイルを読み込み・バリデーション ③ 既存レコードとの ID 衝突を確認 ④ `--on-conflict error`（既定）の場合、衝突あれば `MSG-CLI-142` で exit 1 ⑤ バリデーション通過後、`VaultRepository` 経由でレコードを追加 ⑥ stdout に成功メッセージ（追加件数・スキップ件数）を表示 |
| 代替フロー A | `--on-conflict skip` → 衝突 ID はスキップして残りを追加 |
| 代替フロー B | `--on-conflict overwrite` → 衝突 ID の既存レコードを置換 |
| 代替フロー C | フォーマットバージョン不一致 → `MSG-CLI-143` で exit 1 |
| 代替フロー D | `{"kind":"redacted"}` payload のレコードを import → `MSG-CLI-144`（リダクト済みレコードはインポート不可）で exit 1 |
| 事後条件 | 指定ファイルの有効レコードが vault に追加・更新されている |

## 3. 機能要件

| ID | 要件 |
|----|------|
| R1-DP-01 | `shikomi export --output <FILE>` で vault の全レコードを JSON 形式でファイルに書き出す |
| R1-DP-02 | `Secret` kind のペイロードは既定でリダクト（`{"kind":"redacted"}` tagged union）する。`--export-secrets` フラグで平文 export を明示的に許可する。`--export-secrets` 実行時は stderr に `MSG-CLI-145`（Secret 平文 export 警告）を必ず出力する（`--quiet` でも抑止不可）|
| R1-DP-03 | 暗号化 vault がロック済みの場合、export / import コマンドを拒否する（`MSG-CLI-140`）|
| R1-DP-04 | export ファイルに `format_version: 1` を含め、将来のフォーマット変更に備える |
| R1-DP-05 | `shikomi import --input <FILE>` で JSON ファイルを読み込み、バリデーション後に vault へ追加する |
| R1-DP-06 | import 時の衝突戦略を `--on-conflict skip|overwrite|error` で指定可能にする（既定: `error`）|
| R1-DP-07 | `{"kind":"redacted"}` payload を持つレコードのインポートを拒否する（`MSG-CLI-144`）|
| R1-DP-08 | export は `VaultRepository` trait 経由で実装し、IPC 経路（daemon）と SQLite 直結（`--no-ipc`）の両方で動作する |
| R1-DP-09 | export / import の処理は `tempfile` を用いた atomic な書き込みで実装する（import の部分書き込み防止）|
| R1-DP-10 | `hotkey` フィールドを export ファイルに含める（`null` または文字列）。import 時は hotkey フィールドも復元する |

## 4. 非機能要件（本 feature スコープ）

| 項目 | 要件 |
|------|------|
| セキュリティ | Secret kind の平文は `--export-secrets` 明示フラグなしでは export ファイルに含まれない |
| ファイルパーミッション | export ファイルは `0600`（owner read/write のみ）で作成する。Unix 系は `tempfile::Builder::new().permissions(0o600)` で保証する |
| --export-secrets 警告 | `--export-secrets` 実行時は `MSG-CLI-145` を stderr に出力する。誤操作による Secret 全漏洩を防ぐ最終ゲート |
| 互換性 | `format_version: 1` の JSON ファイルは将来バージョンでも読み込み可能にする（フォーマットは後方互換）|
| アトミック性 | import 中にクラッシュしても vault が壊れない（`tempfile` + rename による atomic commit）|
| エラーメッセージ | 不正な JSON / 不正なフィールド値は具体的なフィールド名付きエラーで報告する（`MSG-CLI-143`）|

## 5. 受入基準

### Sub-A（Issue #140）: domain

| ID | 基準 |
|----|------|
| AC-DP-01 | `ExportRecord` / `ExportPayload` が JSON にシリアライズ・デシリアライズできる |
| AC-DP-02 | `Secret` kind のレコードが `{"kind":"redacted"}` tagged union で export される（`--export-secrets` なし）|
| AC-DP-03 | `{"kind":"redacted"}` payload を持つレコードを `ImportValidator` に渡すと `ImportValidationError::RedactedPayload` が返る |
| AC-DP-04 | 同一 ID を持つ 2 レコードを `ImportValidator` に渡すと `ImportValidationError::DuplicateId` が返る |
| AC-DP-05 | `format_version: 999`（未知のバージョン）を `ImportValidator` に渡すと `ImportValidationError::UnknownFormatVersion` が返る |

### Sub-B（Issue #141）: cli

| ID | 基準 |
|----|------|
| AC-DP-06 | `shikomi export --output /tmp/test.json` が成功し、`format_version: 1` を含む有効な JSON が `/tmp/test.json` に書き込まれる |
| AC-DP-07 | `shikomi import --input /tmp/test.json` が成功し、export 元と同じレコードが vault に存在する（round-trip）|
| AC-DP-08 | `--export-secrets` なしで `shikomi export` した Secret kind のレコードが、`shikomi import` で拒否される（`MSG-CLI-144`）|
| AC-DP-09 | `shikomi import --on-conflict skip` が、衝突レコードをスキップして残りを追加する |
| AC-DP-10 | `shikomi import --input /tmp/test.json` を 2 回実行すると 2 回目は全件衝突で `MSG-CLI-142` が表示される（`--on-conflict error` 既定）|

## 6. スコープ外

| 項目 | 理由 |
|------|------|
| クラウド同期 | 単一障害点・セキュリティ境界複雑化（`nfr.md §6 Out of Scope`）|
| CSV / YAML 形式 | YAGNI（JSON で十分。`format_version` で将来追加可能）|
| 暗号化 vault のロック済み状態での export | 復号キーなしに export 不可。ロック解除を事前条件とする |
| import 時の新規 ID 採番オプション | YAGNI（`--regenerate-ids` は将来拡張。ID 衝突は `--on-conflict` で対処）|
| GUI からの export / import | `shikomi-gui` feature の後続スコープ |
