# 受入テストシナリオ — SC-DAEMON-001: daemon 初回起動（vault.db 不在）

<!-- 配置先: docs/acceptance-tests/scenarios/SC-DAEMON-001-first-launch.md -->
<!-- 対応要件: REQ-DAEMON-028（basic-design/module-contracts.md §REQ-DAEMON-028）/ Bug-F-008 / Issue #80 -->
<!-- Vモデル対応: 受入テスト（最上位、業務シナリオ横断）-->

## シナリオ概要

| 項目 | 内容 |
|------|------|
| シナリオ ID | SC-DAEMON-001 |
| タイトル | daemon 初回起動時に vault.db が存在しない状態でも正常起動し、IPC 操作が可能になる |
| 対象ペルソナ | ペルソナ A（田中俊介 — Mac ユーザ、CLI 非使用）/ ペルソナ C（佐々木健二 — 開発者、初回インストール）|
| 優先度 | High（MVP 受入必須）|
| 前提条件 | shikomi をインストールしたばかりで vault.db が存在しない、または vault.db を削除した状態 |
| 関連 Issue | #80 |

## 受入基準

### AC-001: daemon が自動起動して vault.db を生成する

**Given**: `$SHIKOMI_VAULT_DIR`（または OS デフォルト `$XDG_DATA_HOME/shikomi/`）に vault.db が存在しない  
**When**: `shikomi-daemon` を起動する（または GUI アプリを初回起動して daemon が launchd/systemd 経由で自動起動する）  
**Then**:
- daemon が exit 0 以外で異常終了しないこと（起動成功）
- vault.db ファイルが `$SHIKOMI_VAULT_DIR` に生成されていること
- `shikomi --ipc list` で空リスト（エラーなし）が返ること

### AC-002: ペルソナ A/C の初回体験が GUI で完結する

**Given**: daemon が AC-001 の手順で初回起動した直後  
**When**: GUI アプリを開く  
**Then**:
- `VaultStatusBanner`（`REQ-UI-03`）に `[平文]`（橙色）バナーが表示されること
- バナーから暗号化設定（`shikomi vault encrypt` 相当の操作）に誘導できること
- CLI の `shikomi vault encrypt` コマンドを使わなくても暗号化設定が可能であること

### AC-003: ペルソナ B（技術者）向け補助ログが出力される

**Given**: vault.db が存在しない状態で daemon を起動する  
**When**: daemon の起動ログ（`stderr`、`SHIKOMI_DAEMON_LOG=info`）を確認する  
**Then**:
- `"vault not found; created new plaintext vault at "` を含む INFO ログが出力されること
- `"hint: to enable encryption"` を含む INFO ログが続けて出力されること
- ログに秘密情報（パスワード / vault 内容 / secret 値）が含まれないこと

### AC-004: 2 回目以降の起動では vault.db が再生成されない

**Given**: AC-001 で vault.db が生成された状態  
**When**: daemon を停止して再起動する  
**Then**:
- `"vault not found; created new plaintext vault"` ログが**出力されない**こと（既存 vault を再利用）
- 起動前後で vault.db の mtime が変化しないこと（上書きなし）

## テスト実行計画

| レベル | TC-ID | 担当 |
|-------|-------|------|
| ユニットテスト | TC-UT-140〜TC-UT-142（`unit.md §2.18`）| 実装担当 |
| 結合テスト | TC-IT-100〜TC-IT-102（`integration.md §11`）| 実装担当 |
| システムテスト | ST-DAEMON-010（`system-test-design.md §5`）| テスト担当 |
| 受入テスト | 本シナリオ（手動 or E2E）| QA / オーナー |

## 関連設計書

- `docs/features/daemon-ipc/basic-design/module-contracts.md §REQ-DAEMON-028`（機能要件詳細）
- `docs/features/daemon-ipc/detailed-design/composition-root.md §処理順序 ステップ 6`（実装詳細）
- `docs/features/daemon-ipc/basic-design/security.md §vault.db 不在時の init ログへの secret 混入`（脅威モデル）
- `docs/features/shikomi-gui/ui/basic-design.md §REQ-UI-03`（GUI VaultStatusBanner 仕様）
