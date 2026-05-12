# 受入テストシナリオ — SC-DDM-001: IPC 既定化（Phase 2 CLI 移行）

<!-- 配置先: docs/acceptance-tests/scenarios/SC-DDM-001-ipc-default-mode.md -->
<!-- 対応要件: REQ-DDM-001〜006（daemon-default-mode/cli/basic-design.md §モジュール契約）/ Issue #126 -->
<!-- Vモデル対応: 受入テスト（最上位、業務シナリオ横断）-->

## シナリオ概要

| 項目 | 内容 |
|------|------|
| シナリオ ID | SC-DDM-001 |
| タイトル | Phase 2 移行後、CLI が `--ipc` フラグなしで daemon 経由（IPC 既定）で動作し、`--no-ipc` エスケープハッチが正しく機能する |
| 対象ペルソナ | ペルソナ B（山田 美咲 — エンジニア / CLI 主体）|
| 優先度 | High（Sub-A マージの受入必須）|
| 前提条件 | shikomi Phase 2 バイナリがインストール済み（`shikomi` / `shikomi-daemon`）。vault.db が存在する状態（daemon 初回起動済み）|
| 関連 Issue | #125（Phase 2 全体）/ #126（Sub-A: CLI 既定化）|

---

## 受入基準

### AC-DDM-01: IPC が新しい既定経路である

**Given**: `shikomi-daemon` が起動済み（UDS ソケットが存在する）  
**When**: `shikomi list` を **`--ipc` フラグなし**で実行する  
**Then**:
- コマンドが exit 0 で成功すること
- stdout にレコード一覧（0 件含む）が表示されること
- stderr に `MSG-CLI-051`（`"IPC mode"` / `"opt-in"` 等の Phase 1 警告文言）が**出力されない**こと

### AC-DDM-02: `--no-ipc` エスケープハッチが機能する

**Given**: `shikomi-daemon` が**起動していない**（緊急復旧・CI 環境等）/ vault.db が存在する  
**When**: `shikomi --no-ipc list` を実行する  
**Then**:
- コマンドが exit 0 で成功すること
- stdout にレコード一覧が表示されること（SQLite 直結）
- daemon が起動していなくても動作すること（IPC 不要）

### AC-DDM-03: daemon 未起動時に `MSG-CLI-110` で即失敗する

**Given**: `shikomi-daemon` が**起動していない**  
**When**: `shikomi list` を `--no-ipc` フラグなしで実行する  
**Then**:
- コマンドが exit 1 で失敗すること
- stderr に `MSG-CLI-110`（`"not running"` または `"shikomi-daemon"` を含む文言）が出力されること
- stderr の hint 行に daemon 起動コマンドが案内されること（`"shikomi-daemon"` 等）
- **stderr の hint 行に `"--ipc"` が含まれないこと**（Phase 2 では `--ipc` フラグは廃止済み）

### AC-DDM-04: 廃止された `--ipc` フラグが即座に拒否される

**Given**: なし（daemon の起動状態を問わない）  
**When**: `shikomi --ipc list` を実行する  
**Then**:
- コマンドが exit 2 で失敗すること（clap の使用法エラー）
- stderr に `"--ipc"` を未知引数として示すエラーメッセージが出力されること
- 正常処理（IPC 接続 / SQLite アクセス）が一切行われないこと

### AC-DDM-05: IPC 既定化後も `MSG-CLI-051` が出力されない

**Given**: `shikomi-daemon` が起動済み  
**When**: `shikomi list` を `--ipc` なしで実行する  
**Then**:
- コマンドが exit 0 で成功すること
- stderr に以下の文言が**含まれない**こと:
  - `"IPC mode"` / `"opt-in"` / `"MSG-CLI-051"` / `"--ipc"`（Phase 1 の opt-in 警告は廃止）

### AC-DDM-06: vault サブコマンドは `--no-ipc` 指定時も IPC 強制される

**Given**: `shikomi-daemon` が**起動していない**  
**When**: `shikomi --no-ipc vault encrypt` を実行する  
**Then**:
- コマンドが exit 1 で失敗すること（daemon 未起動）
- stderr に `MSG-CLI-110`（daemon 未起動エラー）が出力されること
- **`--no-ipc` が指定されていても vault サブコマンドは SQLite 直結にフォールバックしない**こと
- vault.db が変更されていないこと（直接アクセスが行われない）

---

## テスト実行計画

| レベル | TC-ID | 配置 / 担当 |
|-------|-------|------------|
| ユニットテスト | TC-UT-150〜TC-UT-159（`cli/test-design/unit.md`）| 実装担当 |
| 結合テスト | TC-IT-110〜TC-IT-114（`cli/test-design/integration.md`）| 実装担当 |
| システムテスト | ST-DDM-001〜ST-DDM-006（`daemon-default-mode/system-test-design.md`）| テスト担当 |
| 受入テスト（E2E 自動化）| TC-E2E-120〜TC-E2E-125（`crates/shikomi-cli/tests/e2e_sc_ddm_001.rs`）| テスト担当 |
| 受入テスト（手動確認）| AC-DDM-02・AC-DDM-06（`--no-ipc` エスケープハッチ操作）| QA / オーナー |

### 自動化可能な AC

| AC ID | 自動化 | 備考 |
|-------|--------|------|
| AC-DDM-01 | ✅ TC-E2E-120 | バイナリ spawn + stdout/stderr/exit code 検証 |
| AC-DDM-02 | ✅ TC-E2E-121 | daemon 未起動 + `--no-ipc` + `tempfile` vault |
| AC-DDM-03 | ✅ TC-E2E-122 | daemon 未起動 + stderr 文言検証 |
| AC-DDM-04 | ✅ TC-E2E-123 | clap exit 2 検証 |
| AC-DDM-05 | ✅ TC-E2E-124 | stderr 非出力検証 |
| AC-DDM-06 | ✅ TC-E2E-125 | `--no-ipc vault encrypt` + daemon 未起動 → MSG-CLI-110 |

---

## トレーサビリティ

```
AC-DDM-01〜06 (feature-spec.md §5 受入基準)
   └── SC-DDM-001 (本ファイル — 受入シナリオ)
         ├── ST-DDM-001〜006 (system-test-design.md — システムテスト)
         │     ├── TC-IT-110〜114 (cli/test-design/integration.md — 結合テスト)
         │     │     └── TC-UT-150〜159 (cli/test-design/unit.md — ユニットテスト)
         │     └── [E2E 自動化] TC-E2E-120〜125 (e2e_sc_ddm_001.rs)
         └── [手動確認] AC-DDM-02, AC-DDM-06 の操作シナリオ
```

---

## 関連設計書

- `docs/features/daemon-default-mode/feature-spec.md §5`（受入基準 AC-DDM-01〜06）
- `docs/features/daemon-default-mode/cli/basic-design.md §モジュール契約`（REQ-DDM-001〜005）
- `docs/features/daemon-default-mode/cli/detailed-design.md`（変更対象ファイル一覧・実装詳細）
- `docs/features/daemon-default-mode/cli/security.md`（`--no-ipc` 脅威モデル・OWASP 対応）
- `docs/features/daemon-default-mode/system-test-design.md`（ST-DDM-001〜006 システムテスト戦略）
- `docs/analysis/personas.md §ペルソナ B`（山田 美咲 — 本シナリオの主ペルソナ）
