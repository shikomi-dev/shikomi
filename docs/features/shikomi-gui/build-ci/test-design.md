# テスト設計書 — build-ci（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: build-ci / Issue #98 -->
<!-- 配置先: docs/features/shikomi-gui/build-ci/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->
<!-- 参照: basic-design.md §モジュール契約 / detailed-design.md §1〜11 -->

## §0. テスト方針参照

本テスト設計書は `config/prompts/test_strategy.md` に定めるテスト戦略（Vモデル階層化・ダブル方針・CI ワークフロー対応）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは親 `system-test-design.md` に委ねる。

**build-ci sub-feature 固有の特性**:

`build-ci` の実装成果物は `.github/workflows/bundler.yml`（新規）・`test-gui.yml` への `e2e-smoke` / `e2e-smoke-fault` ジョブ追記・`scripts/smoke-e2e.sh`（新規）・`audit.yml` 拡張・`deny.toml` 更新・`.github/actions/tauri-build-setup/action.yml`（composite action）であり、`crates/` 配下に Rust ソースコードを持たない。したがってテスト設計は以下の 2 種類に集約される:

1. **IT（結合テスト）**: `e2e-smoke` ジョブで実行する E2E smoke テスト（TC-GUI-E01 正常系）と `e2e-smoke-fault` ジョブで実行する逆正常性確認（TC-GUI-E01 異常系）
2. **UT（静的検証 / CI設定検証）**: ワークフロー YAML の `actionlint` 検証（正常系 + 負例検証）・`cargo deny check` による依存 RUSTSEC クリーン確認

---

## §1. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 | Fixture 状態 |
|-------|---------|---------|------|------------|
| IT（TC-GUI-CI-IT01〜IT03 smoke 正常系） | OS プロセス（shikomi-daemon） | `shikomi start` バックグラウンド起動 | 実バイナリを直接起動（モック不要） | 不要（実バイナリ使用）|
| IT（TC-GUI-CI-IT01〜IT03 smoke 正常系） | UDS ソケット（IPC） | `shikomi list` コマンドで接続確認 | 実 IPC を通す（モック不要） | 不要 |
| IT（TC-GUI-CI-IT01〜IT03 smoke 正常系） | 仮想ディスプレイ（xvfb） | `Xvfb :99 -screen 0 1280x720x24` セッション | CI ubuntu-22.04 ランナーで直接起動（`DISPLAY=:99`）| 不要 |
| IT（TC-GUI-CI-IT04 fault check） | なし（fault injection: daemon 未起動） | `shikomi list` の接続失敗 exit code 確認 | daemon を意図的に起動しない | 不要 |
| UT（actionlint 正常系） | なし（静的 YAML 解析） | — | 外部依存なし | 不要 |
| UT（actionlint 負例） | なし（静的 YAML 解析） | 意図的に壊した YAML fixture | `test/fixtures/bad-workflow.yml` を使用 | 負例 YAML fixture 1 件 |
| UT（cargo deny） | RUSTSEC advisory DB（オンライン） | `deny.toml` + advisory feed | `deny.toml` の `[advisories.ignore]` エントリで対処 | 不要 |

> **Characterization fixture 不要（IT 正常系）**: 本 sub-feature の IT 正常系テストはすべて実バイナリ間の統合検証であり、外部 API モックを行わない。assumed mock 禁止原則は「モックが存在しないため」適用対象外——実データそのものをテスト入力とする。

---

## §2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT（actionlint 正常系） | `.github/workflows/bundler.yml`・`test-gui.yml`（設定ファイル） | `actionlint .github/workflows/bundler.yml`、`actionlint .github/workflows/test-gui.yml` |
| UT（actionlint 負例） | `test/fixtures/bad-workflow.yml`（fixture） | `! actionlint test/fixtures/bad-workflow.yml`（反転チェック） |
| UT（cargo deny） | `deny.toml`（設定ファイル） | `cargo deny check` |
| IT（E2E smoke 正常系） | `scripts/smoke-e2e.sh`（SSoT）+ `test-gui.yml` `e2e-smoke` ジョブ | CI: `e2e-smoke` ジョブ / ローカル: `bash scripts/smoke-e2e.sh`（要 xvfb） |
| IT（E2E smoke 異常系） | `test-gui.yml` `e2e-smoke-fault` ジョブ | CI: `e2e-smoke-fault` ジョブ / ローカル: `! ./target/release/shikomi list`（daemon 未起動） |

> **`cargo test` 対象外**: build-ci sub-feature の成果物は YAML + シェルスクリプトのみ。`crates/` 配下に Rust テストファイルを配置しない。

---

## §3. テスト用ダブルの方針

E2E smoke（IT 正常系）はすべて実バイナリを使用する。モックは一切挿入しない。

| テスト対象 | ダブル要否 | 実装方法 |
|----------|---------|---------|
| shikomi-daemon | **不要** | 実バイナリをバックグラウンド起動 |
| shikomi-gui | **不要** | 実バイナリを `DISPLAY=:99` で起動 |
| xvfb 仮想ディスプレイ | **不要（実環境）** | CI ランナーで `Xvfb :99` を直接起動 |
| APPLE_* Secrets（macOS 公証） | **スキップ（条件分岐）** | fork PR では `if: github.event.pull_request.head.repo.full_name == github.repository` でジョブ全体をスキップ |
| IPC 接続（IT04 fault check） | **不要（fault injection）** | daemon を起動しない = 接続先を存在させない |

---

## §4. テストマトリクス（トレーサビリティ）

### 4.1 ユニットテスト（CI 静的検証）

| テスト ID | REQ-CI | 設計根拠 | テスト内容 | 種別 |
|---------|--------|--------|----------|------|
| TC-GUI-CI-UT01 | REQ-CI-01, REQ-CI-08 | `detailed-design.md §1.2`（paths フィルタ）・`§1.3`（権限設計） | `actionlint` で `bundler.yml` 構文・アクションバージョン・secrets 参照・`if:` 式を検証 | 正常系 |
| TC-GUI-CI-UT01N | REQ-CI-01 | `detailed-design.md §1.3`（権限設計） | 意図的に `permissions: write-all` を含む bad-workflow.yml を `actionlint` にかけ、エラーが**出る**ことを確認 | 負例（反転） |
| TC-GUI-CI-UT02 | REQ-CI-07 | `detailed-design.md §6`（e2e-smoke ジョブ全体） | `actionlint` で `test-gui.yml`（e2e-smoke / e2e-smoke-fault 追記後）構文検証 | 正常系 |
| TC-GUI-CI-UT02N | REQ-CI-07 | `detailed-design.md §6`（e2e-smoke ジョブ設計） | 意図的に `uses: actions/checkout@v1`（古いピン）を含む bad-workflow.yml を `actionlint` にかけ、エラーが**出る**ことを確認 | 負例（反転） |
| TC-GUI-CI-UT03 | REQ-CI-06 | `detailed-design.md §7.3`（RUSTSEC 対応手順） | `cargo deny check` が shikomi-gui 依存に対して未登録 advisory を報告しない | 正常系 |

### 4.2 結合テスト（E2E smoke — TC-GUI-E01）

| テスト ID | REQ-CI | 設計根拠 | テスト内容 | 種別 |
|---------|--------|--------|----------|------|
| TC-GUI-CI-IT01 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.7`（起動確認） | `shikomi-gui` を xvfb 環境で起動し 15 秒ポーリング後も生存（`kill -0` が 0 を返す） | 正常系 |
| TC-GUI-CI-IT02 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.7`（IPC 接続確認） | daemon 起動済み状態で `shikomi list` が exit 0（IPC ソケット到達を証明） | 正常系 |
| TC-GUI-CI-IT03 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.7`（正常終了確認） | GUI プロセスへ `SIGTERM` 送信後 5 秒以内に exit 0 で終了する | 正常系 |
| TC-GUI-CI-IT04 | REQ-CI-07 | `detailed-design.md §6.8`（e2e-smoke-fault ジョブ） | daemon **未起動**時に `shikomi list` が非ゼロ exit → `e2e-smoke-fault` ジョブが PASS（反転検証） | 異常系（CI 自動） |

---

## §5. ユニットテスト詳細設計

### TC-GUI-CI-UT01: `bundler.yml` actionlint 正常系検証

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT01 |
| 対応する要件ID | REQ-CI-01（R1-GUI-16）、REQ-CI-08 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §1`） |
| 種別 | 正常系 |
| 前提条件 | `actionlint` インストール済み、`bundler.yml` 実装済み |
| 操作 | `actionlint .github/workflows/bundler.yml` |
| 期待結果 | exit 0、エラーなし。secrets 参照・`@v4` ピン・paths フィルタ・`permissions.contents: read` が有効と判定される |

---

### TC-GUI-CI-UT01N: `bundler.yml` actionlint 負例検証

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT01N |
| 対応する要件ID | REQ-CI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §1.3`） |
| 種別 | 負例（actionlint が**エラーを出すこと**を検証） |
| 前提条件 | `actionlint` インストール済み。`test/fixtures/bad-workflow.yml` に以下を含む: `permissions: write-all`（過剰権限）または `uses: actions/checkout@v1`（非推奨ピン） |
| 操作 | `! actionlint test/fixtures/bad-workflow.yml` |
| 期待結果 | `actionlint` が exit 非ゼロ（エラー検出）→ `!` 反転で CI ステップが exit 0（PASS） |

**設計根拠**: actionlint が「実際に壊れた YAML を検知できる」ことを保証しなければ、TC-GUI-CI-UT01 の正常系検証が意味をなさない。負例を追加することで actionlint の検知能力そのものを回帰テストで固定する。

---

### TC-GUI-CI-UT02: `test-gui.yml` actionlint 正常系検証

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT02 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6`） |
| 種別 | 正常系 |
| 前提条件 | `test-gui.yml` に `e2e-smoke` / `e2e-smoke-fault` ジョブが追記済み |
| 操作 | `actionlint .github/workflows/test-gui.yml` |
| 期待結果 | exit 0、エラーなし。`xvfb` インストールステップ・`bash scripts/smoke-e2e.sh` 呼び出し・`! ./target/release/shikomi list` 反転チェック・`timeout-minutes` 設定が有効と判定される |

---

### TC-GUI-CI-UT02N: `test-gui.yml` actionlint 負例検証

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT02N |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.3`） |
| 種別 | 負例 |
| 前提条件 | `actionlint` インストール済み。TC-GUI-CI-UT01N と同一 `test/fixtures/bad-workflow.yml` を使用 |
| 操作 | `! actionlint test/fixtures/bad-workflow.yml` |
| 期待結果 | TC-GUI-CI-UT01N と同じ期待結果（fixture を共用） |

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
| 期待結果 | exit 0。`tauri-plugin-shell@2` 等 shikomi-gui 新規依存の advisory が `[advisories.ignore]` に登録済みか advisory が存在しない。未登録の advisory が検出された場合は本 TC が FAIL |

---

## §6. 結合テスト詳細設計（E2E smoke: TC-GUI-E01）

本セクションの TC-GUI-CI-IT01〜IT04 は `basic-design.md §4` の `TC-GUI-E01`（E2E スモークテスト）を IT レベルで詳細化したものである。IT01〜IT03 はシーケンシャルに実行される（`detailed-design.md §6.6` シーケンス図参照）。IT04 は独立した `e2e-smoke-fault` ジョブで実行する（`detailed-design.md §6.8` 参照）。

### TC-GUI-CI-IT01: GUI プロセス起動確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT01 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.3` step 8 / `§6.7` 起動確認） |
| 種別 | 正常系 |
| 前提条件 | `Xvfb :99` 起動済み、daemon ソケットが存在する（ポーリング確認済み）、`shikomi-gui` バイナリビルド済み |
| 操作 | `DISPLAY=:99 ./target/release/shikomi-gui &` でバックグラウンド起動後、0.5s 間隔 × 最大 15s ポーリングで `kill -0 $GUI_PID` を確認 |
| 期待結果 | ポーリング期間中 `kill -0 $GUI_PID` が継続して exit 0（プロセスが生存・クラッシュなし） |
| 失敗時の CI 挙動 | `scripts/smoke-e2e.sh` が exit 1 → `e2e-smoke` ジョブ FAIL。`bundler.yml` の 3 OS ビルドには影響しない |

---

### TC-GUI-CI-IT02: daemon IPC 接続確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT02 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.7` IPC 接続確認） |
| 種別 | 正常系 |
| 前提条件 | TC-GUI-CI-IT01 通過後（GUI プロセス生存確認済み）、shikomi-daemon 起動済み |
| 操作 | `./target/release/shikomi list` を実行 |
| 期待結果 | `shikomi list` が exit 0。daemon IPC ソケットへの到達を証明する（空リストも合格） |
| 失敗時の CI 挙動 | `scripts/smoke-e2e.sh` が exit 1 → `e2e-smoke` ジョブ FAIL |

**設計根拠**: `AC-GUI-01`「`shikomi gui` で GUI が起動し、daemon と IPC 接続が確立される」の自動検証。`shikomi list` の exit 0 は daemon IPC ソケットへの到達を証明する。TC-GUI-CI-IT04（daemon 未起動時に exit 非ゼロ）との組み合わせで「exit 0 = 実際に接続している」という性質を固定する。

---

### TC-GUI-CI-IT03: GUI プロセス正常終了確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT03 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.7` 正常終了確認） |
| 種別 | 正常系 |
| 前提条件 | TC-GUI-CI-IT01・IT02 通過後 |
| 操作 | `kill -TERM $GUI_PID`、`timeout 5 wait $GUI_PID` を実行 |
| 期待結果 | `wait $GUI_PID` が 5 秒以内に exit 0 を返す（SIGTERM を受けて正常終了）|
| 失敗時の CI 挙動 | `timeout` が exit 124（タイムアウト）または `wait` が非ゼロ → スクリプトが exit 1 → ジョブ FAIL |

---

### TC-GUI-CI-IT04: daemon 未起動時 IPC 失敗検証（CI 自動 — e2e-smoke-fault ジョブ）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT04 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.8`） |
| 種別 | 異常系（逆正常性確認 / fault injection） |
| 前提条件 | `shikomi-cli` バイナリビルド済み。shikomi-daemon **未起動**（故意に起動しない） |
| 操作 | `! ./target/release/shikomi list` を実行（シェル否定演算子で exit code を反転） |
| 期待結果 | `shikomi list` が非ゼロ exit（daemon IPC ソケット不在 → 接続失敗）→ `!` 反転で CI ステップが exit 0（PASS）|
| 実行ジョブ | `test-gui.yml` の `e2e-smoke-fault` ジョブ（`detailed-design.md §6.8` 参照）で **CI 自動実行** |
| 備考 | このテストにより「TC-GUI-CI-IT02 の `shikomi list` exit 0 が接続成功の真の証拠」であることを構造的に担保する |

---

## §7. テスト対象外の明示

| 機能 | 対象外の理由 | 代替検証 |
|------|-----------|--------|
| macOS コード署名・公証（REQ-CI-02）の実効性 | `macos-latest` ランナーと APPLE_* Secrets が必要。fork PR では実行不可 | CI 上の `bundler.yml` 実行（内部 PR のみ）+ **手動受入（AC-GUI-09 Gatekeeper 検証）**。カバレッジ root = build-macos ジョブ成功 + 手動 |
| Windows MSI / NSIS ビルド（REQ-CI-03）の実効性 | `windows-latest` ランナーが必要 | CI 上の `bundler.yml` 実行（内部 PR のみ）+ **手動受入（AC-GUI-08 SmartScreen 確認）**。カバレッジ root = build-windows ジョブ成功 + 手動 |
| Linux AppImage ダブルクリック起動（REQ-CI-04） | GUI 操作が必要。CI headless では不可 | 手動受入（AC-GUI-10） |
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
| shikomi-daemon プロセス | **不要** | 実バイナリを起動（IT 正常系）/ 意図的に起動しない（IT04 fault） |
| shikomi-gui プロセス | **不要** | 実バイナリを `DISPLAY=:99` で起動 |
| xvfb 仮想ディスプレイ | **不要（実環境）** | CI ランナーで `Xvfb :99` を直接起動 |
| UDS ソケット（IPC） | **不要** | 実 IPC を使用（`shikomi list` が実ソケットに接続）|
| APPLE_* Secrets（macOS 公証） | **スキップ（条件分岐）** | fork PR では `if:` 条件で macOS ジョブ全体をスキップ |
| RUSTSEC advisory DB | **不要** | `deny.toml` の `[advisories.ignore]` で静的対処。`cargo deny` が advisory DB へのアクセスを自動処理 |

**assumed mock 禁止**: E2E smoke テストはすべて実バイナリ間の統合検証。中間レイヤーに仮定ベースのモックを挿入しない。

---

## §9. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-GUI-CI-UT01（bundler.yml actionlint 正常系） | `lint.yml`（`actionlint` ステップ追加を推奨）| PR CI で毎回実行 |
| TC-GUI-CI-UT01N（bundler.yml actionlint 負例） | `lint.yml`（同上）| 同上。`test/fixtures/bad-workflow.yml` を使用 |
| TC-GUI-CI-UT02（test-gui.yml actionlint 正常系） | `lint.yml`（同上）| 同上 |
| TC-GUI-CI-UT02N（test-gui.yml actionlint 負例） | `lint.yml`（同上）| fixture を TC-GUI-CI-UT01N と共用 |
| TC-GUI-CI-UT03（cargo deny check） | `audit.yml` | `cargo deny check` が既存 audit ジョブに含まれる |
| TC-GUI-CI-IT01〜IT03（E2E smoke 正常系） | `test-gui.yml` `e2e-smoke` ジョブ | ubuntu-22.04 + xvfb + `bash scripts/smoke-e2e.sh` で PR / main / develop プッシュ時に自動実行 |
| TC-GUI-CI-IT04（daemon 未起動 FAIL 確認） | `test-gui.yml` `e2e-smoke-fault` ジョブ | **CI 自動実行**。`! ./target/release/shikomi list` の反転チェックで daemon 不在 → IPC 失敗を検証 |

---

## §10. カバレッジ基準

| 観点 | 基準 |
|------|------|
| REQ-CI 全件 | REQ-CI-01〜08 が IT / UT またはシステムテスト（bundler.yml 実行 + 手動受入）でカバーされること |
| 正常系（E2E smoke） | TC-GUI-E01 の 3 段階（起動確認 / IPC 接続確認 / 正常終了確認）が `e2e-smoke` ジョブで CI 自動検証されること |
| 異常系 | TC-GUI-CI-IT04（daemon 未起動時の `shikomi list` 非ゼロ exit）が `e2e-smoke-fault` ジョブで **CI 自動検証**されること |
| CI 静的検証 | `actionlint` 正常系（TC-GUI-CI-UT01/UT02）+ 負例（TC-GUI-CI-UT01N/UT02N）+ `cargo deny check`（TC-GUI-CI-UT03）が PR CI で常に実行されること |
| REQ-CI-02/03 カバレッジ | macOS / Windows ビルドジョブの成功 = 成果物生成の自動検証。Gatekeeper / SmartScreen の手動受入（AC-GUI-09/08）で最終確認。役割分担を §7 に明示 |
| 手動受入との役割分担 | AC-GUI-08（Windows SmartScreen）・AC-GUI-09（macOS Gatekeeper）・AC-GUI-10（Linux AppImage 起動）は bundler.yml 成果物を用いた手動受入で確認。本 test-design.md の自動テスト対象外 |
| テスト非重複 | E2E smoke（TC-GUI-E01）は `e2e-smoke` ジョブで 1 回のみ実行。`bundler.yml` とジョブが独立しているため二重実行なし |

---

*作成: 涅マユリ（テスト担当）/ 更新: セル（設計責任者）/ 2026-05-11*
*設計根拠: `docs/features/shikomi-gui/build-ci/basic-design.md` §モジュール契約 / `detailed-design.md §1〜11` / Issue #98*
