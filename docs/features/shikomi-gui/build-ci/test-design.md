# テスト設計書 — build-ci（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: build-ci / Issue #98 -->
<!-- 配置先: docs/features/shikomi-gui/build-ci/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->
<!-- 参照: basic-design.md §モジュール契約 / detailed-design.md §1〜9 -->

## §0. テスト方針参照

本テスト設計書は `config/prompts/test_strategy.md` に定めるテスト戦略（Vモデル階層化・ダブル方針・CI ワークフロー対応）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは親 `system-test-design.md` に委ねる。

**build-ci sub-feature 固有の特性**:

`build-ci` の実装成果物は `.github/workflows/bundler.yml`（新規）・`test-gui.yml` の `e2e-smoke` ジョブ追記・`audit.yml` 拡張・`deny.toml` 更新であり、`crates/` 配下に Rust ソースコードを持たない。したがってテスト設計は以下の 2 種類に集約される:

1. **IT（結合テスト）**: `e2e-smoke` ジョブで実行する E2E smoke テスト（TC-GUI-E01）— 実バイナリを xvfb 環境で起動し、shikomi-daemon との IPC 結合を検証する
2. **UT（静的検証 / CI設定検証）**: ワークフロー YAML の `actionlint` 検証・`cargo deny check` による依存 RUSTSEC クリーン確認

---

## §1. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 | Fixture 状態 |
|-------|---------|---------|------|------------|
| IT（TC-GUI-E01 smoke） | OS プロセス（shikomi-daemon） | `shikomi start` バックグラウンド起動 | 実バイナリを直接起動（モック不要） | 不要（実バイナリ使用）|
| IT（TC-GUI-E01 smoke） | UDS ソケット（IPC） | `shikomi list` コマンドで接続確認 | 実 IPC を通す（モック不要） | 不要 |
| IT（TC-GUI-E01 smoke） | 仮想ディスプレイ（xvfb） | `Xvfb :99 -screen 0 1280x720x24` セッション | CI ubuntu-22.04 ランナーで直接起動（`DISPLAY=:99`）| 不要 |
| UT（actionlint） | なし（静的 YAML 解析） | — | 外部依存なし | 不要 |
| UT（cargo deny） | RUSTSEC advisory DB（オンライン） | `deny.toml` + advisory feed | `deny.toml` の `[advisories.ignore]` エントリで対処 | 不要 |

> **Characterization fixture 不要**: 本 sub-feature の IT テストはすべて実バイナリ間の統合検証であり、外部 API モックを行わない。assumed mock 禁止原則は「モックが存在しないため」適用対象外——実データそのものをテスト入力とする。

---

## §2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT（actionlint） | `.github/workflows/bundler.yml`・`.github/workflows/test-gui.yml`（設定ファイル） | `actionlint .github/workflows/bundler.yml`、`actionlint .github/workflows/test-gui.yml` |
| UT（cargo deny） | `deny.toml`（設定ファイル） | `cargo deny check` |
| IT（E2E smoke） | `.github/workflows/test-gui.yml` `e2e-smoke` ジョブ内シェルスクリプト | GitHub Actions `e2e-smoke` ジョブ / ローカル: `bash` で smoke スクリプトを手動実行（要 xvfb）|

> **`cargo test` 対象外**: build-ci sub-feature の成果物は YAML + シェルスクリプトのみ。`crates/` 配下に Rust テストファイルを配置しない。

---

## §3. テスト用ダブルの方針

E2E smoke（IT）はすべて実バイナリを使用する。モックは一切挿入しない。

| テスト対象 | ダブル要否 | 実装方法 |
|----------|---------|---------|
| shikomi-daemon | **不要** | 実バイナリ（`./target/release/shikomi`）をバックグラウンド起動 |
| shikomi-gui | **不要** | 実バイナリ（`./target/release/shikomi-gui`）を `DISPLAY=:99` で起動 |
| xvfb 仮想ディスプレイ | **不要（実環境）** | CI ランナーで `Xvfb :99` を直接起動 |
| APPLE_* Secrets（macOS 公証） | **スキップ（条件分岐）** | fork PR では `if: github.event.pull_request.head.repo.full_name == github.repository` でジョブ全体をスキップ |

---

## §4. テストマトリクス（トレーサビリティ）

### 4.1 ユニットテスト（CI 静的検証）

| テスト ID | REQ-CI | 設計根拠 | テスト内容 | 種別 |
|---------|--------|--------|----------|------|
| TC-GUI-CI-UT01 | REQ-CI-01, REQ-CI-08 | `detailed-design.md §1.2`（paths フィルタ）・`§1.3`（権限設計） | `actionlint` で `bundler.yml` 構文・アクションバージョン・secrets 参照・`if:` 式を検証 | 正常系 |
| TC-GUI-CI-UT02 | REQ-CI-07 | `detailed-design.md §6`（e2e-smoke ジョブ全体） | `actionlint` で `test-gui.yml`（e2e-smoke 追記後）構文検証 | 正常系 |
| TC-GUI-CI-UT03 | REQ-CI-06 | `detailed-design.md §7.3`（RUSTSEC 対応手順） | `cargo deny check` が shikomi-gui 依存に対して未登録 advisory を報告しない | 正常系 |

### 4.2 結合テスト（E2E smoke — TC-GUI-E01）

| テスト ID | REQ-CI | 設計根拠 | テスト内容 | 種別 |
|---------|--------|--------|----------|------|
| TC-GUI-CI-IT01 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.6`（起動確認） | `shikomi gui` を xvfb 環境で起動し 10 秒後に `kill -0 $GUI_PID` が exit 0 | 正常系 |
| TC-GUI-CI-IT02 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.6`（IPC 接続確認） | daemon 起動済み状態で `shikomi list` が exit 0（0 件以上返る） | 正常系 |
| TC-GUI-CI-IT03 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.6`（正常終了確認） | GUI プロセスへ `SIGTERM` 送信後 5 秒以内に exit 0 で終了する | 正常系 |
| TC-GUI-CI-IT04 | REQ-CI-07 | `detailed-design.md §9.1`（smoke 失敗シナリオ） | daemon 未起動時に `shikomi list` が非ゼロ exit → smoke スクリプトが exit 1 | 異常系 |

---

## §5. ユニットテスト詳細設計

### TC-GUI-CI-UT01: `bundler.yml` actionlint 検証

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT01 |
| 対応する要件ID | REQ-CI-01（R1-GUI-16）、REQ-CI-08 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §1`） |
| 種別 | 正常系 |
| 前提条件 | `actionlint` インストール済み（`go install github.com/rhysd/actionlint/cmd/actionlint@latest`）、`bundler.yml` 実装済み |
| 操作 | `actionlint .github/workflows/bundler.yml` |
| 期待結果 | exit 0、エラーなし。secrets 参照（`secrets.APPLE_CERTIFICATE` 等）・アクションバージョン（`@v4`）・`if: github.event.pull_request.head.repo.full_name == github.repository` 式・`on.paths` フィルタが有効と判定される |

**設計根拠**: `detailed-design.md §1.3` の権限設計（`permissions.contents: read`）と §1.2 の paths フィルタエントリ（`crates/shikomi-gui/**` 等）が YAML として正当な式であることを静的に確認する。実行前に構文エラーを検出し、CI 実行コストを節約する。

---

### TC-GUI-CI-UT02: `test-gui.yml`（e2e-smoke 追記後）actionlint 検証

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT02 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6`） |
| 種別 | 正常系 |
| 前提条件 | `test-gui.yml` に `e2e-smoke` ジョブが追記済み |
| 操作 | `actionlint .github/workflows/test-gui.yml` |
| 期待結果 | exit 0、エラーなし。`xvfb` インストールステップ・smoke スクリプト実行ステップ・`timeout-minutes: 15` 設定が有効と判定される |

**設計根拠**: `detailed-design.md §6.3` のステップ一覧（checkout → Rust → cache → system libs → Node.js → npm ci → build → smoke test）が YAML として実行可能であることを確認する。特に `apt-get install xvfb` の追加（§6.4）が既存ステップと衝突しないことを静的に検証する。

---

### TC-GUI-CI-UT03: `cargo deny check` — shikomi-gui 依存 RUSTSEC クリーン

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT03 |
| 対応する要件ID | REQ-CI-06 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §7.3`） |
| 種別 | 正常系 |
| 前提条件 | `deny.toml` に shikomi-gui 依存の ignore エントリが必要に応じて登録済み |
| 操作 | `cargo deny check` |
| 期待結果 | exit 0。`tauri-plugin-shell@2` 等 shikomi-gui 新規依存の advisory が `[advisories.ignore]` に登録済みか、そもそも advisory が存在しない。未登録の advisory が検出された場合は本 TC が FAIL |

**設計根拠**: `detailed-design.md §7.2` の影響分析に従い、Sub-E で追加される依存が `deny.toml` の管理下に入っていることを確認する。RUSTSEC advisory 発生時は §7.3 の手順（影響分析 → Fix or Ignore 登録 + 理由コメント + Issue 番号）に従って対処し、本 TC を再 PASS させる。

---

## §6. 結合テスト詳細設計（E2E smoke: TC-GUI-E01）

本セクションの TC-GUI-CI-IT01〜IT03 は `basic-design.md §4` の `TC-GUI-E01`（E2E スモークテスト）を IT レベル（モジュール間結合）として詳細化したものである。3 件の検証ポイントはシーケンシャルに実行される（`detailed-design.md §6.5` シーケンス図参照）。

### TC-GUI-CI-IT01: GUI プロセス起動確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT01 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.3` step 8 / `§6.6` 起動確認） |
| 種別 | 正常系 |
| 前提条件 | `Xvfb :99 -screen 0 1280x720x24 &` 起動済み（`DISPLAY=:99`）、`./target/release/shikomi start &` で daemon 起動済み（`sleep 2` 待機後）、`shikomi-gui` バイナリビルド済み |
| 操作 | `DISPLAY=:99 ./target/release/shikomi-gui &` でバックグラウンド起動し `GUI_PID=$!`、`sleep 10` 後に `kill -0 $GUI_PID` を実行 |
| 期待結果 | `kill -0 $GUI_PID` が exit 0。GUI プロセスが 10 秒後も生存している（クラッシュ・即時終了なし） |
| 失敗時の CI 挙動 | スクリプトが exit 1 → `e2e-smoke` ジョブ FAIL。`bundler.yml` の 3 OS ビルドには影響しない（独立ジョブ設計）|

---

### TC-GUI-CI-IT02: daemon IPC 接続確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT02 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.6` IPC 接続確認） |
| 種別 | 正常系 |
| 前提条件 | TC-GUI-CI-IT01 通過後（GUI プロセス生存）、shikomi-daemon 起動済み |
| 操作 | `./target/release/shikomi list` を実行 |
| 期待結果 | `shikomi list` が exit 0。標準出力は 0 件以上（空リストも合格）。daemon への IPC 接続が確立されていることを証明する |
| 失敗時の CI 挙動 | スクリプトが exit 1 → `e2e-smoke` ジョブ FAIL |

**設計根拠**: `AC-GUI-01`「`shikomi gui` で GUI が起動し、daemon と IPC 接続が確立される」の自動検証。`shikomi list` の exit 0 は daemon IPC ソケットへの到達を証明する最小限の接続確認である。

---

### TC-GUI-CI-IT03: GUI プロセス正常終了確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT03 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.6` 正常終了確認） |
| 種別 | 正常系 |
| 前提条件 | TC-GUI-CI-IT01・IT02 通過後 |
| 操作 | `kill -TERM $GUI_PID`、`timeout 5 wait $GUI_PID` を実行 |
| 期待結果 | `wait $GUI_PID` が 5 秒以内に exit 0 を返す（SIGTERM を受けて正常終了。ゾンビプロセスなし）|
| 失敗時の CI 挙動 | `timeout` が exit 124（タイムアウト）または `wait` が非ゼロ → スクリプトが exit 1 → ジョブ FAIL |

---

### TC-GUI-CI-IT04: daemon 未起動時 smoke FAIL 検証（逆正常性確認）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT04 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §9.1` 失敗シナリオ） |
| 種別 | 異常系（逆正常性確認） |
| 前提条件 | xvfb 起動済み、shikomi-daemon **未起動**（意図的な fault injection） |
| 操作 | GUI を起動した後、`./target/release/shikomi list` を実行（daemon ソケットなし） |
| 期待結果 | `shikomi list` が非ゼロ exit → smoke スクリプトが exit 1。CI ジョブが FAIL と判定される |
| 実行方法 | ローカルで手動実行。daemon を起動せずに smoke スクリプトを流す |
| 備考 | 「smoke スクリプトは接続失敗を正しく検出できる」ことを確認する逆正常性テスト。CI トリガーでは daemon を意図停止する仕組みがないためローカル手動確認 |

---

## §7. テスト対象外の明示

| 機能 | 対象外の理由 | 代替検証 |
|------|-----------|--------|
| macOS コード署名・公証（REQ-CI-02）の実効性 | `macos-latest` ランナーと APPLE_* Secrets が必要。fork PR では実行不可 | CI 上の `bundler.yml` 実行（内部 PR のみ）+ 手動受入（AC-GUI-09 Gatekeeper 検証）|
| Windows MSI / NSIS ビルド（REQ-CI-03） | `windows-latest` ランナーが必要 | CI 上の `bundler.yml` 実行 + 手動受入（AC-GUI-08 SmartScreen 確認）|
| Linux AppImage ダブルクリック起動（REQ-CI-04） | GUI 操作が必要。CI headless では不可 | 手動受入（AC-GUI-10）|
| artifact 保持期間（REQ-CI-05）の実効性 | 7 日 / 30 日の確認には時間経過が必要 | PR マージ後の GitHub Actions artifact UI で目視確認 |
| macOS Keychain `if: always()` クリーンアップ | `tauri build` 失敗後のクリーンアップ動作は macOS CI 実行中のみ確認可能 | macOS ランナーでジョブ失敗 → Keychain 残留なしを手動確認 |
| GUI 画面の描画・レイアウト | Xvfb はウィンドウ生成のみ保証。画面キャプチャ検証は scope 外（basic-design.md §4.3 参照） | 手動受入（AC-GUI-01〜07）|
| トレイアイコン操作 headless 確認 | `libappindicator3` の動作は GNOME 環境依存。Xvfb では動作保証なし | システムテスト（実機 GNOME 環境）|
| 30 秒カウントダウン表示（AC-GUI-07） | トレイ操作が必要 | システムテスト（実機）|
| `cargo deny check` advisory DB 鮮度 | CI 実行時の advisory DB は日々更新。ローカルとは乖離しうる | `audit.yml` を daily / PR トリガーで定期実行 |

---

## §8. モック方針まとめ

| テスト対象 | モック要否 | 実装方法 |
|----------|---------|---------|
| shikomi-daemon プロセス | **不要** | 実バイナリ（`./target/release/shikomi`）を起動 |
| shikomi-gui プロセス | **不要** | 実バイナリ（`./target/release/shikomi-gui`）を起動 |
| xvfb 仮想ディスプレイ | **不要（実環境）** | CI ランナーで `Xvfb :99` を直接起動 |
| UDS ソケット（IPC） | **不要** | 実 IPC を使用（`shikomi list` が実ソケットに接続）|
| APPLE_* Secrets（macOS 公証） | **スキップ（条件分岐）** | fork PR では `if:` 条件で macOS ジョブ全体をスキップ |
| RUSTSEC advisory DB | **不要** | `deny.toml` の `[advisories.ignore]` で静的対処。`cargo deny` が advisory DB へのアクセスを自動処理 |

**assumed mock 禁止**: E2E smoke テストはすべて実バイナリ間の統合検証。中間レイヤーに仮定ベースのモックを挿入しない。

---

## §9. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-GUI-CI-UT01（bundler.yml actionlint） | `lint.yml`（`actionlint` ステップ追加を推奨）| PR CI で毎回実行。`actionlint` を lint ジョブに組み込む |
| TC-GUI-CI-UT02（test-gui.yml actionlint） | `lint.yml`（同上）| 同上 |
| TC-GUI-CI-UT03（cargo deny check） | `audit.yml` | `cargo deny check` が既存 audit ジョブに含まれる。shikomi-gui 依存追加後も継続実行 |
| TC-GUI-CI-IT01〜IT03（E2E smoke 正常系） | `test-gui.yml` `e2e-smoke` ジョブ | ubuntu-22.04 + xvfb で PR / main / develop プッシュ時に自動実行 |
| TC-GUI-CI-IT04（daemon 未起動 FAIL 確認） | ローカル手動検証 | CI トリガーでは daemon を故意停止する仕組みがないため手動。初回実装時に 1 回確認すれば充分 |

---

## §10. カバレッジ基準

| 観点 | 基準 |
|------|------|
| REQ-CI 全件 | REQ-CI-01〜08 が IT / UT またはシステムテスト（bundler.yml 実行）でカバーされること |
| 正常系（E2E smoke） | TC-GUI-E01 の 3 段階（起動確認 / IPC 接続確認 / 正常終了確認）が `e2e-smoke` ジョブで自動検証されること |
| 異常系 | TC-GUI-CI-IT04（daemon 未起動時の smoke FAIL）が手動確認されること |
| CI 静的検証 | `actionlint`（TC-GUI-CI-UT01/UT02）+ `cargo deny check`（TC-GUI-CI-UT03）が PR CI で常に実行されること |
| 手動受入との役割分担 | AC-GUI-08（Windows SmartScreen）・AC-GUI-09（macOS Gatekeeper）・AC-GUI-10（Linux AppImage 起動）は bundler.yml 成果物を用いた手動受入で確認。本 test-design.md の自動テスト対象外 |
| テスト非重複 | E2E smoke（TC-GUI-E01）は `e2e-smoke` ジョブで 1 回のみ実行。`bundler.yml` とジョブが独立しているため二重実行なし |

---

*作成: 涅マユリ（テスト担当）/ 2026-05-11*
*設計根拠: `docs/features/shikomi-gui/build-ci/basic-design.md` §モジュール契約 / `detailed-design.md §1〜9` / Issue #98*
