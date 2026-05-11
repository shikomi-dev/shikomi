# システムテスト設計書 — shikomi-gui

<!-- feature: shikomi-gui / Issue #90 -->
<!-- 配置先: docs/features/shikomi-gui/system-test-design.md -->
<!-- システムテスト（E2E）はここにのみ記述。sub-feature の test-design.md には IT / UT のみ -->

## 1. テスト戦略概要

本 feature のシステムテストは「GUI 起動 → daemon 接続 → エントリ操作 → 結果検証」の**エンドツーエンド経路**を検証する。GUI の Webview（WebView2 / WKWebView）操作は CI ヘッドレス環境で再現不可な部分があるため、**Tauri Commands レイヤーを直接呼び出す統合 CLI テスト**と**手動受入テスト**の 2 本立てで補完する。

| テストレベル | 担当ファイル | 主な対象 |
|------------|------------|---------|
| E2E（システムテスト）| 本ファイル | GUI 起動〜daemon 接続〜エントリ操作の全体フロー |
| 結合テスト（IT）| `ipc-client/test-design.md` | Tauri Commands ↔ daemon IPC ラウンドトリップ |
| 結合テスト（IT）| `ui/test-design.md` | SolidJS コンポーネントとストアの結合 |
| 結合テスト（IT）| `system-tray/test-design.md` | トレイイベント処理の統合動作 |
| ユニットテスト（UT）| 各 sub-feature の test-design.md | 単体関数・型の契約検証 |

## 2. E2E テストケース

### TC-GUI-E01: GUI 起動 → daemon IPC ハンドシェイク確立

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E01 |
| 対応要件 | R1-GUI-01, R1-GUI-02 |
| 前提 | daemon が起動済み（test fixture の vault.db を使用） |
| 手順 | ① `shikomi gui --headless` でバックエンドのみ起動（WebView なし） ② Tauri Command `list_entries` を CLI 経由で呼び出す ③ 応答を確認 |
| 期待結果 | `IpcResponse::Records` が返り、entries 列が取得できる |
| CI 実行 | `test-gui.yml`（ヘッドレスモード、daemon は fixture 起動） |

### TC-GUI-E02: daemon 未起動時の Fail Fast 表示

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E02 |
| 対応要件 | R1-GUI-03 |
| 前提 | daemon が起動していない |
| 手順 | ① daemon を起動せずに `shikomi gui --headless` を起動 ② `list_entries` Tauri Command を呼び出す |
| 期待結果 | `GUIError::DaemonNotRunning` が返る。ペイロードに「daemon が起動していません」メッセージが含まれる |
| CI 実行 | `test-gui.yml`（daemon 起動なし） |

### TC-GUI-E03: エントリ追加 → 一覧反映

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E03 |
| 対応要件 | R1-GUI-05 |
| 前提 | TC-GUI-E01 の環境（daemon 接続済み） |
| 手順 | ① `add_entry(label="e2e-test", value="secret123", kind="text", hotkey=null)` Tauri Command を呼び出す ② `list_entries` で一覧取得 |
| 期待結果 | 追加された `id` が返り、一覧に `e2e-test` が含まれる |
| CI 実行 | `test-gui.yml` |

### TC-GUI-E04: ホットキー割当 → 競合検出

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E04 |
| 対応要件 | R1-GUI-08, R1-GUI-09 |
| 前提 | `ctrl+alt+1` を割り当て済みのエントリが存在する |
| 手順 | ① 別エントリに `assign_hotkey(id, combo="ctrl+alt+1")` Tauri Command を呼び出す |
| 期待結果 | `GUIError::HotkeyConflict { existing_label }` が返る。競合エントリのラベルが含まれる |
| CI 実行 | `test-gui.yml` |

### TC-GUI-E05: vault 暗号化オプトイン → recovery 24 語受信

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E05 |
| 対応要件 | R1-GUI-10, R1-GUI-11 |
| 前提 | vault が平文モード |
| 手順 | ① `encrypt_vault(master_password="Str0ng!Pass#2026")` Tauri Command ② 応答確認 |
| 期待結果 | `EncryptResult { words: Vec<String>(24件) }` が返る。words が BIP-39 フォーマット |
| CI 実行 | `test-gui.yml` |

### TC-GUI-E06: vault ロック中の書き込み操作 → アンロック要求

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E06 |
| 対応要件 | R1-GUI-13 |
| 前提 | vault が暗号化ロック状態 |
| 手順 | ① `add_entry(...)` Tauri Command を呼び出す |
| 期待結果 | `GUIError::VaultLocked` が返る |
| CI 実行 | `test-gui.yml` |

### TC-GUI-E07: `tauri-bundler` インストーラ生成（CI ビルド確認）

| 項目 | 内容 |
|------|------|
| テスト ID | TC-GUI-E07 |
| 対応要件 | R1-GUI-16 |
| 手順 | `cargo tauri build` を各 OS ランナーで実行 |
| 期待結果 | Windows: `.msi` または `.exe` 生成 / macOS: `.dmg` 生成 / Linux: `.deb` + `.AppImage` 生成 |
| CI 実行 | `test-gui-bundle.yml`（Sub-E #98 で実装） |

## 3. 手動受入テスト（CI 自動化不可領域）

GUI の Webview 操作・視覚的確認・システムトレイ動作は手動で実施する。

| AC | 対象 OS | 確認内容 |
|----|--------|---------|
| AC-GUI-01 | Windows / macOS / Linux | `shikomi gui` でウィンドウが開き、エントリ一覧が表示される |
| AC-GUI-02 | 全 OS | エントリ追加・削除・ホットキー設定が GUI から操作できる |
| AC-GUI-03 | 全 OS | vault 暗号化で recovery 24 語が画面表示され、「転記完了」まで次操作がブロックされる |
| AC-GUI-04 | 全 OS | システムトレイアイコン右クリックでメニューが表示される |
| AC-GUI-05 | 全 OS | ホットキー押下後 30 秒以内にトレイアイコンにカウントダウン表示がされる |
| AC-GUI-06 | Windows | NSIS / MSI インストーラでインストール → `shikomi gui` が起動する |
| AC-GUI-07 | macOS | DMG マウント → Applications フォルダへコピー → Notarization 通過（Gatekeeper 警告なし） |
| AC-GUI-08 | Linux | AppImage をダブルクリックで起動できる |

## 4. CI ワークフロー対応方針

- `test-gui.yml` を新規追加。`TC-GUI-E01〜E06` を `--headless` モードで実行
  - Ubuntu / macOS / Windows の 3 ランナーで実行
  - daemon は fixture vault.db を使用して別プロセスで起動
  - WebView は起動せず Tauri Commands のみを検証する
- `TC-GUI-E07`（bundler CI）は Sub-E #98 の `test-gui-bundle.yml` で実装
- ヘッドレス環境での Tauri 起動は `TAURI_DEV_HOST=headless` または `--no-webview` フラグで制御

## 5. テスト環境・前提条件

| 項目 | 内容 |
|------|------|
| daemon fixture | `tests/fixtures/gui_e2e_vault.db`（エントリ 3 件 + ホットキー `ctrl+alt+1` 割当済み 1 件） |
| IPC プロトコル | V2（`shikomi-core::ipc::IpcProtocolVersion::V2`） |
| vault 暗号化テスト | 別 fixture `gui_e2e_encrypted_vault.db` を使用 |
| 並列実行 | テストケースは独立した daemon プロセスを使用し並列実行可能 |
