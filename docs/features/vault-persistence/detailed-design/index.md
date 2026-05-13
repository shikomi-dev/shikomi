# 詳細設計書（目次）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/detailed-design/ ディレクトリ以下に分割 -->

## 記述ルール（必ず守ること）

詳細設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

本書では Rust の関数シグネチャは**プレーンテキスト（インライン `code`）**で示し、実装本体は書かない。Mermaid クラス図と表と箇条書きで設計判断を記述する。

## 分割構成

前版（単一 `detailed-design.md`）が 546 行に達しファイルサイズ上限（500 行）を超えたため、以下の 4 ファイルに**サフィックス分割せずディレクトリ内でセクション別分割**した。読む順序はこの通り:

| # | ファイル | 内容 |
|---|--------|------|
| 1 | [`index.md`](./index.md) | 目次、記述ルール、公開 API（再エクスポート一覧）、ビジュアルデザイン |
| 2 | [`classes.md`](./classes.md) | クラス設計（詳細クラス図、12 項目の設計判断補足） |
| 3 | [`data.md`](./data.md) | データ構造（定数・境界値、モジュール別公開メソッドシグネチャ、SQLite スキーマ DDL） |
| 4 | [`flows.md`](./flows.md) | 制御フロー（エラー型詳細、load/save アルゴリズム、OS 別パーミッション、SQL 要点、テスト設計担当向けメモ） |

各ファイルは独立に読めるが、`classes.md` → `data.md` → `flows.md` の順で読むとトップダウンに辿れる（クラス → 型 → 挙動）。

## 公開 API（`shikomi_infra::persistence` からの再エクスポート一覧）

`shikomi_infra::persistence::` 直下からアクセス可能にする型（**外部公開**）:

- `VaultRepository`（trait）
- `SqliteVaultRepository`（具象実装）
- `VaultPaths`
- `PersistenceError`, `CorruptedReason`, `AtomicWriteStage`, `VaultDirReason`

**公開しないもの**（`pub(crate)` のみ、モジュール内部実装）:

- `VaultLock`（RAII ハンドル、REQ-P13）
- `Audit`（`audit.rs` モジュールの 5 関数、REQ-P14）
- `AtomicWriter`, `PermissionGuard`, `Mapping`, `SchemaSql`

**`pub(super)` のみ**:

- OS 別実装（`permission::unix` / `permission::windows`）

### バリアント数の整合

- `PersistenceError` は**全 11 バリアント**（設計判断 §12 参照: 旧 `DomainError` 廃止で 10→9 に統合、その後 `Locked`/`InvalidVaultDir` 追加で最終 11）
- `VaultDirReason` は全 6 バリアント（`NotAbsolute`/`PathTraversal`/`SymlinkNotAllowed`/`Canonicalize`/`ProtectedSystemArea`/`NotADirectory`）
- `CorruptedReason` は全 7 バリアント、`AtomicWriteStage` は全 6 バリアント
- **カバレッジ基準**: テスト設計（`test-design/`）で全 11 + 6 + 7 + 6 = 30 バリアントを TC で網羅すること（実数、過剰カウントと不足を避けるため定期同期が必要）

## ビジュアルデザイン

該当なし — 理由: UI なし。本 crate は `shikomi-daemon` / `shikomi-cli` / `shikomi-gui` から呼ばれる永続化ライブラリ。エンドユーザーに見える画面はない。

## 関連設計書

- 基本設計書: [`../basic-design/`](../basic-design/)（`index.md` / `security.md` / `error.md` の 3 分冊）
- 要件定義書: [`../requirements.md`](../requirements.md)
- 要求分析書: [`../requirements-analysis.md`](../requirements-analysis.md)
- テスト設計書: [`../test-design/`](../test-design/)
