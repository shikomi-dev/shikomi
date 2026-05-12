# 受入テストシナリオ — SC-DDM-002: daemon OS 自動起動（Sub-B）

<!-- 配置先: docs/acceptance-tests/scenarios/SC-DDM-002-daemon-autostart.md -->
<!-- 対応要件: REQ-DDM-010〜017（daemon-default-mode/autostart/basic-design.md §モジュール契約）/ Issue #127 -->
<!-- Vモデル対応: 受入テスト（最上位、業務シナリオ横断）-->

## シナリオ概要

| 項目 | 内容 |
|------|------|
| シナリオ ID | SC-DDM-002 |
| タイトル | `shikomi daemon install` / `uninstall` / `status` が OS 自動起動の登録・解除・状態確認を正しく実行し、冪等性を保証する |
| 対象ペルソナ | ペルソナ B（山田 美咲 — エンジニア / CLI 主体）|
| 優先度 | High（Sub-B マージの受入必須）|
| 前提条件 | shikomi Phase 2 バイナリがインストール済み（`shikomi` / `shikomi-daemon` が同ディレクトリに配置されている）。OS 自動起動設定ファイルの書き込み権限あり |
| 関連 Issue | #125（Phase 2 全体）/ #127（Sub-B: OS 自動起動）|

---

## 受入基準

### AC-DDM-07: `shikomi daemon install` が OS 固有ファイルを配置して exit 0 で成功すること

**Given**: `shikomi` / `shikomi-daemon` バイナリが同ディレクトリにインストール済みである。OS 自動起動設定ディレクトリへの書き込み権限がある。

**When**: `shikomi daemon install` を実行する。

**Then**:
- コマンドが **exit 0** で成功すること
- stdout に `"shikomi-daemon autostart enabled"` が出力されること
- stdout に OS 固有の hint が出力されること:
  - macOS: `"hint: to start immediately: launchctl kickstart gui/{uid}/dev.shikomi.daemon"` が含まれること
  - Linux (systemd): `"hint: to check status: systemctl --user status shikomi-daemon"` が含まれること
  - Linux (XDG Autostart): `"hint: this uses XDG Autostart; shikomi-daemon will start on next login"` が含まれること
  - Windows: `"hint: to start immediately: schtasks /Run /TN "shikomi\shikomi-daemon""` が含まれること
- OS 固有の自動起動ファイルが正しいパスに配置されていること:
  - macOS: `~/Library/LaunchAgents/dev.shikomi.daemon.plist` が存在すること
  - Linux (systemd): `~/.config/systemd/user/shikomi-daemon.service` が存在すること
  - Linux (XDG): `~/.config/autostart/shikomi-daemon.desktop` が存在すること
  - Windows: `schtasks /Query /TN "shikomi\shikomi-daemon"` が exit 0 で成功すること
- stderr に `"error:"` が含まれないこと

---

### AC-DDM-08: `shikomi daemon uninstall` が OS 固有ファイルを削除して exit 0 で成功すること

**Given**: `shikomi daemon install` が完了している状態である（AC-DDM-07 の Then が成立している）。

**When**: `shikomi daemon uninstall` を実行する。

**Then**:
- コマンドが **exit 0** で成功すること
- stdout に `"shikomi-daemon autostart disabled"` が出力されること
- OS 固有の自動起動ファイルが削除されていること:
  - macOS: `~/Library/LaunchAgents/dev.shikomi.daemon.plist` が存在しないこと
  - Linux (systemd): `~/.config/systemd/user/shikomi-daemon.service` が存在しないこと
  - Linux (XDG): `~/.config/autostart/shikomi-daemon.desktop` が存在しないこと
  - Windows: `schtasks /Query /TN "shikomi\shikomi-daemon"` が非 0 exit で失敗すること
- stderr に `"error:"` が含まれないこと

---

### AC-DDM-09: `shikomi daemon status` が常に exit 0 で稼働状態と自動起動状態を 2 行で出力すること

**シナリオ A: daemon 起動中 + autostart 登録済み**

**Given**: `shikomi-daemon` プロセスが起動済みで IPC ソケットが確立されている。`shikomi daemon install` が完了している状態である。

**When**: `shikomi daemon status` を実行する。

**Then**:
- コマンドが **exit 0** で成功すること
- stdout の 1 行目に `"daemon: running"` が出力されること
- stdout の 2 行目に `"autostart: enabled"` が出力されること

**シナリオ B: daemon 未起動 + autostart 未登録**

**Given**: `shikomi-daemon` プロセスが起動していない。`shikomi daemon install` を実行していない状態である。

**When**: `shikomi daemon status` を実行する。

**Then**:
- コマンドが **exit 0** で成功すること（status は情報提供のみ、副作用なし — REQ-DDM-012）
- stdout の 1 行目に `"daemon: not running"` が出力されること
- stdout の 2 行目に `"autostart: disabled"` が出力されること

**シナリオ C: `--no-ipc` フラグ指定時**

**Given**: daemon の起動状態は問わない。

**When**: `shikomi --no-ipc daemon status` を実行する。

**Then**:
- コマンドが **exit 0** で成功すること
- stdout の 1 行目に `"daemon: unknown (--no-ipc)"` が出力されること（IPC probe を省略）
- stdout の 2 行目に `"autostart: enabled"` または `"autostart: disabled"` のいずれかが出力されること
- IPC 接続試行が行われないこと（タイムアウト待ちが発生しない）

---

### AC-DDM-10: `shikomi daemon install` の 2 回連続実行が冪等であること

**Given**: `shikomi daemon install` を 1 回実行して AC-DDM-07 の Then が成立している状態である。

**When**: `shikomi daemon install` を**再度**実行する（2 回目）。

**Then**:
- 2 回目のコマンドが **exit 0** で成功すること（重複登録エラーにならない）
- stdout に `"shikomi-daemon autostart enabled"` が出力されること
- 1 回目と同じ OS 固有の hint が出力されること
- OS 固有の自動起動ファイルが正しいパスに引き続き存在すること

---

## テスト実行計画

| レベル | TC-ID / ST-ID | 配置 / 担当 |
|-------|--------------|------------|
| ユニットテスト | TC-UT-160〜TC-UT-176（`autostart/test-design/unit.md`）| 実装担当 |
| 結合テスト | TC-IT-120〜TC-IT-132（`autostart/test-design/integration.md`）| 実装担当 |
| システムテスト | ST-DDM-020〜ST-DDM-025（`daemon-default-mode/system-test-design.md §Sub-B`）| テスト担当 |
| 受入テスト（自動化可能部分）| ST-DDM-020〜025（`crates/shikomi-cli/tests/st_autostart*.rs`）| テスト担当 |
| 受入テスト（手動確認必須）| 下記「手動確認事項」参照 | QA / オーナー |

### 自動化可能な AC

| AC ID | 自動化 | 担当 ST-ID | 備考 |
|-------|--------|-----------|------|
| AC-DDM-07 | 自動化可 | ST-DDM-020 | `tempfile::TempDir` を `HOME` 代替に使用。OS 固有ファイルの `Path::exists()` 確認 |
| AC-DDM-08 | 自動化可 | ST-DDM-021 | install 後 uninstall → ファイル削除確認 |
| AC-DDM-09 (シナリオ A) | 自動化可 | ST-DDM-022 | daemon subprocess を `assert_cmd` で起動後に status 確認 |
| AC-DDM-09 (シナリオ B) | 自動化可 | ST-DDM-023 | daemon 未起動環境での status 確認 |
| AC-DDM-09 (シナリオ C) | 自動化可 | ST-DDM-025 | `--no-ipc` フラグ付き status |
| AC-DDM-10 | 自動化可 | ST-DDM-024 | install を 2 回実行して両方 exit 0 を確認 |

---

## 手動確認事項（CI では実施不可）

以下の項目は CI 環境での OS 再起動テストが不可能であるため、手動受入テストとして実施する。

| 確認項目 | 手順 | 期待結果 |
|---------|------|---------|
| OS 再起動後の daemon 自動起動 | 1. `shikomi daemon install` を実行する。2. OS を再起動する。3. ログイン後に `shikomi daemon status` を実行する | 1 行目に `"daemon: running"` が出力されること。daemon が自動起動して IPC ソケットが確立済みであること |
| daemon 自動起動後の `shikomi list` IPC 成功 | OS 再起動後、`shikomi list` を実行する | コマンドが exit 0 で成功し、vault のレコード一覧が表示されること（IPC 経由でのアクセスが成功する） |
| OS 再起動後の `shikomi daemon uninstall` 確認 | 1. `shikomi daemon uninstall` を実行する。2. OS を再起動する。3. ログイン後に `shikomi daemon status` を実行する | 2 行目に `"autostart: disabled"` が出力されること。daemon が自動起動しないこと |

---

## スコープ外

| 項目 | 理由 |
|------|------|
| OS 再起動後の daemon 自動起動実証 | CI 環境での OS 再起動テストは実施不可。上記「手動確認事項」として記録。`feature-spec.md §6 スコープ外` にも記載済み |
| MSG-CLI-110 hint への `shikomi daemon install` 誘導 | Sub-B 完了後の別 PR で実施（`autostart/basic-design.md §Sub-B 完了後に更新するメッセージ`）|
| macOS `launchctl bootstrap` の完全 CI テスト | CI ログインセッションが不安定。ファイル書き込みのみ IT で検証し、`launchctl` 呼び出しは手動確認 |
| `--no-ipc` と暗号化 vault の組み合わせ | `daemon-vault-encryption` feature スコープ（未起票）|

---

## トレーサビリティ

```
AC-DDM-07〜10 (feature-spec.md §5 §Sub-B 受入基準)
   └── SC-DDM-002 (本ファイル — 受入シナリオ)
         ├── ST-DDM-020〜025 (system-test-design.md §Sub-B — システムテスト)
         │     ├── TC-IT-120〜132 (autostart/test-design/integration.md — 結合テスト)
         │     │     └── TC-UT-160〜176 (autostart/test-design/unit.md — ユニットテスト)
         │     └── [自動化] ST-DDM-020〜025 (crates/shikomi-cli/tests/st_autostart*.rs)
         └── [手動確認] OS 再起動後の daemon 自動起動・shikomi list IPC 成功
```

---

## 関連設計書

- `docs/features/daemon-default-mode/feature-spec.md §5 §Sub-B`（受入基準 AC-DDM-07〜10）
- `docs/features/daemon-default-mode/autostart/basic-design.md §モジュール契約`（REQ-DDM-010〜017）
- `docs/features/daemon-default-mode/autostart/detailed-design/`（変更対象ファイル一覧・実装詳細 — index.md / backend-trait.md / launchd.md / systemd.md / xdg.md / windows.md / presenter.md）
- `docs/features/daemon-default-mode/system-test-design.md §Sub-B`（ST-DDM-020〜025 システムテスト戦略）
- `docs/features/daemon-default-mode/autostart/test-design/unit.md`（TC-UT-160〜176 ユニットテスト）
- `docs/features/daemon-default-mode/autostart/test-design/integration.md`（TC-IT-120〜132 結合テスト）
- `docs/analysis/personas.md §ペルソナ B`（山田 美咲 — 本シナリオの主ペルソナ）
