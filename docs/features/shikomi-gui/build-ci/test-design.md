# テスト設計書 — build-ci（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: build-ci / Issue #98 -->
<!-- 配置先: docs/features/shikomi-gui/build-ci/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->
<!-- 参照: basic-design.md §モジュール契約 / detailed-design.md §1〜11 -->

## §0. テスト方針参照

本テスト設計書は `config/prompts/test_strategy.md` に定めるテスト戦略（Vモデル階層化・ダブル方針・CI ワークフロー対応）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは親 `system-test-design.md` に委ねる。

**build-ci sub-feature 固有の特性**:

`build-ci` の実装成果物は `.github/workflows/bundler.yml`（新規）・`test-gui.yml` への `e2e-smoke` ジョブ追記・`audit.yml` 拡張・`deny.toml` 更新・`scripts/smoke.sh`（新規）であり、`crates/` 配下に Rust ソースコードを持たない。したがってテスト設計は以下の 2 種類に集約される:

1. **IT（結合テスト）**: `e2e-smoke` ジョブで実行する E2E smoke テスト（TC-GUI-E01）— 実バイナリを xvfb 環境で起動し、shikomi-daemon との IPC 結合を検証する
2. **UT（静的検証 / CI設定検証）**: ワークフロー YAML の `actionlint` 検証（正常系 + 異常系）・`cargo deny check` による依存 RUSTSEC クリーン確認

**smoke スクリプト SSoT**: E2E smoke の実装は `scripts/smoke.sh` 単一ファイルに集約する（`test-gui.yml` にインライン記述しない）。YAML インライン shell は `shellcheck` の対象外になるため、`scripts/smoke.sh` として独立させ CI YAML からは `bash scripts/smoke.sh` で呼び出す。

---

## §1. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 | Fixture 状態 |
|-------|---------|---------|------|------------|
| IT（TC-GUI-E01 smoke） | OS プロセス（shikomi-daemon） | `shikomi start` バックグラウンド起動 | 実バイナリを直接起動。ソケットファイル生成をポーリングで待機（固定 sleep 廃止）| 不要（実バイナリ使用）|
| IT（TC-GUI-E01 smoke） | UDS ソケット（IPC） | `shikomi list` + daemon 状態確認コマンドで接続確認 | 実 IPC を通す（モック不要）| 不要 |
| IT（TC-GUI-E01 smoke） | 仮想ディスプレイ（xvfb） | `Xvfb :99 -screen 0 1280x720x24` セッション | CI ubuntu-22.04 ランナーで直接起動（`DISPLAY=:99`）。`trap EXIT` で終了を保証 | 不要 |
| IT（TC-GUI-CI-IT04 逆正常性） | OS プロセス（shikomi-daemon） | **意図的に起動しない**（`--no-daemon` フラグ） | `scripts/smoke.sh --no-daemon` で daemon 起動ステップをスキップ | 不要 |
| UT（actionlint 正常系） | なし（静的 YAML 解析） | — | 外部依存なし | 不要 |
| UT（actionlint 負例） | なし（静的 YAML 解析） | 意図的に壊した YAML 断片 | `actionlint` が非ゼロ exit を返すことを確認 | 不要 |
| UT（cargo deny） | RUSTSEC advisory DB（オンライン） | `deny.toml` + advisory feed | `deny.toml` の `[advisories.ignore]` エントリで対処 | 不要 |

> **Characterization fixture 不要**: 本 sub-feature の IT テストはすべて実バイナリ間の統合検証であり、外部 API モックを行わない。assumed mock 禁止原則は「モックが存在しないため」適用対象外——実データそのものをテスト入力とする。

---

## §2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT（actionlint 正常系）| `.github/workflows/bundler.yml`・`.github/workflows/test-gui.yml`（設定ファイル）| `actionlint .github/workflows/bundler.yml`、`actionlint .github/workflows/test-gui.yml` |
| UT（actionlint 負例）| 一時 YAML ファイル（CI スクリプト内で `mktemp` 生成・使用後削除）| `actionlint <temp>.yml`（非ゼロ exit を期待）|
| UT（cargo deny）| `deny.toml`（設定ファイル）| `cargo deny check` |
| IT（E2E smoke 正常系）| `scripts/smoke.sh`（SSoT）← `test-gui.yml` `e2e-smoke` ジョブが呼び出す | `bash scripts/smoke.sh`（CI）/ `bash scripts/smoke.sh` ローカル（要 xvfb + ビルド済みバイナリ）|
| IT（E2E smoke 逆正常性）| `scripts/smoke.sh --no-daemon` ← `test-gui.yml` `e2e-smoke-no-daemon` ジョブが呼び出す | `bash scripts/smoke.sh --no-daemon`（exit 1 を期待）|

> **`cargo test` 対象外**: build-ci sub-feature の成果物は YAML + シェルスクリプトのみ。`crates/` 配下に Rust テストファイルを配置しない。
> **shellcheck 対象**: `scripts/smoke.sh` は `lint.yml` の shellcheck ステップで検証する。YAML インライン shell は shellcheck 対象外になるため SSoT 化が必須。

---

## §3. テスト用ダブルの方針

E2E smoke（IT）はすべて実バイナリを使用する。モックは一切挿入しない。

| テスト対象 | ダブル要否 | 実装方法 |
|----------|---------|---------|
| shikomi-daemon | **不要** | 実バイナリ（`./target/release/shikomi`）をバックグラウンド起動。`--no-daemon` フラグ指定時は起動しない |
| shikomi-gui | **不要** | 実バイナリ（`./target/release/shikomi-gui`）を `DISPLAY=:99` で起動 |
| xvfb 仮想ディスプレイ | **不要（実環境）** | CI ランナーで `Xvfb :99` を直接起動。`trap EXIT` でプロセス終了を保証 |
| APPLE_* Secrets（macOS 公証） | **スキップ（条件分岐）** | fork PR では `if: github.event.pull_request.head.repo.full_name == github.repository` でジョブ全体をスキップ |

---

## §4. テストマトリクス（トレーサビリティ）

### 4.1 ユニットテスト（CI 静的検証）

| テスト ID | REQ-CI | 設計根拠 | テスト内容 | 種別 |
|---------|--------|--------|----------|------|
| TC-GUI-CI-UT01 | REQ-CI-01, REQ-CI-08 | `detailed-design.md §1.2`（paths フィルタ）・`§1.3`（権限設計） | `actionlint` で `bundler.yml` 構文・アクションバージョン・secrets 参照・`if:` 式を検証 | 正常系 |
| TC-GUI-CI-UT02 | REQ-CI-01, REQ-CI-05 | `detailed-design.md §1.3`（権限）・`§5`（artifact 命名）| `bundler.yml` 内 `permissions: write` 混入・`@v1` 等の旧バージョン使用・不正 artifact 命名を **actionlint が検知する** ことを確認（意図的に壊した YAML で FAIL を期待） | 異常系（負例） |
| TC-GUI-CI-UT03 | REQ-CI-07 | `detailed-design.md §6`（e2e-smoke ジョブ） | `actionlint` で `test-gui.yml`（e2e-smoke 追記後）構文検証 | 正常系 |
| TC-GUI-CI-UT04 | REQ-CI-07 | `detailed-design.md §6`（e2e-smoke ジョブ） | `test-gui.yml` 内 `e2e-smoke` ジョブに意図的な構文エラー（例: 存在しないアクション参照）を注入し **actionlint が検知する** ことを確認（FAIL を期待） | 異常系（負例） |
| TC-GUI-CI-UT05 | REQ-CI-06 | `detailed-design.md §7.3`（RUSTSEC 対応手順） | `cargo deny check` が shikomi-gui 依存に対して未登録 advisory を報告しない | 正常系 |

### 4.2 結合テスト（E2E smoke — TC-GUI-E01）

| テスト ID | REQ-CI | 設計根拠 | テスト内容 | 種別 |
|---------|--------|--------|----------|------|
| TC-GUI-CI-IT01 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.5`（ポーリング待機）・`§6.6`（起動確認） | `shikomi gui` を xvfb 環境で起動し、ポーリングループで生存確認（`kill -0`）。プロセス生存を確認 | 正常系 |
| TC-GUI-CI-IT02 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.6`（IPC 接続確認） | daemon 起動済み状態で `shikomi list` が exit 0 + daemon 状態確認コマンドが正常応答（IPC 接続を二重確認） | 正常系 |
| TC-GUI-CI-IT03 | REQ-CI-07, AC-GUI-01 | `detailed-design.md §6.6`（正常終了確認）・`§6.5`（`trap EXIT` 設計） | GUI プロセスへ `SIGTERM` 送信後 5 秒以内に exit 0 で終了。`trap EXIT` によりリソースは必ず解放される | 正常系 |
| TC-GUI-CI-IT04 | REQ-CI-07 | `detailed-design.md §9.1`（smoke 失敗シナリオ）・`§6.5`（`--no-daemon` フラグ）| `bash scripts/smoke.sh --no-daemon` 実行（daemon 起動ステップをスキップ）→ `shikomi list` が非ゼロ exit → smoke スクリプトが exit 1。**CI ジョブ `e2e-smoke-no-daemon` で自動検証** | 異常系 |

---

## §5. ユニットテスト詳細設計

### TC-GUI-CI-UT01: `bundler.yml` actionlint 検証（正常系）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT01 |
| 対応する要件ID | REQ-CI-01（R1-GUI-16）、REQ-CI-08 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §1`） |
| 種別 | 正常系 |
| 前提条件 | `actionlint` インストール済み、`bundler.yml` 実装済み（composite action 参照を含む） |
| 操作 | `actionlint .github/workflows/bundler.yml` |
| 期待結果 | exit 0、エラーなし。secrets 参照（`secrets.APPLE_CERTIFICATE` 等）・アクションバージョン（`@v4`）・`if: github.event.pull_request.head.repo.full_name == github.repository` 式・`on.paths` フィルタが有効と判定される |

**設計根拠**: `detailed-design.md §1.3` の権限設計（`permissions.contents: read`）と §1.2 の paths フィルタエントリが YAML として正当な式であることを静的に確認する。

---

### TC-GUI-CI-UT02: `bundler.yml` actionlint 負例検証（異常系）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT02 |
| 対応する要件ID | REQ-CI-01（R1-GUI-16）、REQ-CI-05 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §1.3` 権限・`§5.2` artifact 命名） |
| 種別 | 異常系（actionlint が機能していることの確認）|
| 前提条件 | `actionlint` インストール済み |
| 操作 | `mktemp` で一時 YAML ファイルを生成し、以下の意図的エラーを含む断片を記述して `actionlint <temp>.yml` を実行: ①`permissions: write-all`（過剰権限）、②`uses: actions/checkout@v1`（旧バージョン）、③不正な `secrets.*` 参照 |
| 期待結果 | `actionlint` が非ゼロ exit を返し、エラー行を指摘する |
| 後処理 | `rm <temp>.yml` で一時ファイルを削除 |

**設計根拠**: 正常系 TC-GUI-CI-UT01 だけでは「actionlint が常に exit 0 を返す壊れた設定」を見逃す可能性がある。負例で「linter が実際に lint している」ことを保証する。

---

### TC-GUI-CI-UT03: `test-gui.yml`（e2e-smoke 追記後）actionlint 検証（正常系）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT03 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6`） |
| 種別 | 正常系 |
| 前提条件 | `test-gui.yml` に `e2e-smoke`・`e2e-smoke-no-daemon` ジョブが追記済み |
| 操作 | `actionlint .github/workflows/test-gui.yml` |
| 期待結果 | exit 0、エラーなし。`xvfb` インストールステップ・`bash scripts/smoke.sh` 呼び出し・`bash scripts/smoke.sh --no-daemon` 呼び出しが有効と判定される |

**設計根拠**: `detailed-design.md §6.3` のステップ一覧が YAML として実行可能であること、および `scripts/smoke.sh --no-daemon` フラグ引数渡しが正当な shell コマンドであることを確認する。

---

### TC-GUI-CI-UT04: `test-gui.yml` actionlint 負例検証（異常系）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT04 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6`） |
| 種別 | 異常系（linter 機能確認）|
| 前提条件 | `actionlint` インストール済み |
| 操作 | `mktemp` で一時 YAML ファイルを生成し、e2e-smoke ジョブに意図的エラー（存在しない action 参照 `uses: nonexistent/action@v99`）を含む断片を記述して `actionlint <temp>.yml` を実行 |
| 期待結果 | `actionlint` が非ゼロ exit を返す |
| 後処理 | `rm <temp>.yml` |

---

### TC-GUI-CI-UT05: `cargo deny check` — shikomi-gui 依存 RUSTSEC クリーン

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-UT05 |
| 対応する要件ID | REQ-CI-06 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §7.3`） |
| 種別 | 正常系 |
| 前提条件 | `deny.toml` に shikomi-gui 依存の ignore エントリが必要に応じて登録済み |
| 操作 | `cargo deny check` |
| 期待結果 | exit 0。`tauri-plugin-shell@2` 等 shikomi-gui 新規依存の advisory が `[advisories.ignore]` に登録済みか、advisory が存在しない。未登録 advisory が検出された場合は本 TC が FAIL |

**設計根拠**: `detailed-design.md §7.2` の影響分析に従い、Sub-E で追加される依存が `deny.toml` の管理下に入っていることを確認する。RUSTSEC advisory 発生時は §7.3 の手順（影響分析 → Fix or Ignore 登録 + 理由コメント + Issue 番号）に従って対処し、本 TC を再 PASS させる。

---

## §6. 結合テスト詳細設計（E2E smoke: TC-GUI-E01）

本セクションの TC-GUI-CI-IT01〜IT04 は `basic-design.md §4` の `TC-GUI-E01` を IT レベル（モジュール間結合）として詳細化したものである。IT01〜IT03 は `scripts/smoke.sh` 内でシーケンシャルに実行される（`detailed-design.md §6.5` シーケンス図参照）。IT04 は `--no-daemon` フラグを用いた別ジョブで自動実行する。

**`scripts/smoke.sh` の共通前提**: スクリプト冒頭で `trap 'kill $GUI_PID $DAEMON_PID $XVFB_PID 2>/dev/null; exit' EXIT` を設定し、exit ハンドラで Xvfb・daemon・GUI プロセスの終了を保証する。これにより CI ジョブ失敗時もランナーリソースが残留しない（`detailed-design.md §9.3` の Keychain `if: always()` と対称な設計）。

---

### TC-GUI-CI-IT01: GUI プロセス起動確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT01 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.5` ポーリング設計・`§6.6` 起動確認） |
| 種別 | 正常系 |
| 前提条件 | `Xvfb :99` 起動済み（`trap EXIT` 登録済み）。daemon が UDS ソケットファイル生成まで **ポーリング待機**（`while ! [ -S <socket-path> ]; do sleep 0.5; done`、最大 10 秒でタイムアウト）。shikomi-gui バイナリビルド済み |
| 操作 | `DISPLAY=:99 ./target/release/shikomi-gui &` でバックグラウンド起動し `GUI_PID=$!`。`kill -0 $GUI_PID` ポーリングループ（0.5 秒間隔、最大 15 秒）でプロセス生存を確認 |
| 期待結果 | ポーリング内で `kill -0 $GUI_PID` が exit 0 を返す。GUI プロセスが生存している（クラッシュ・即時終了なし）|
| 失敗条件 | 15 秒以内にプロセス生存を確認できない → スクリプトが exit 1 → `e2e-smoke` ジョブ FAIL |
| 注意 | 固定 `sleep 10` は使用しない。ポーリングにより「実際の起動完了を検知」する設計（flaky test 防止）|

---

### TC-GUI-CI-IT02: daemon IPC 接続確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT02 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.6` IPC 接続確認） |
| 種別 | 正常系 |
| 前提条件 | TC-GUI-CI-IT01 通過後（GUI プロセス生存）、shikomi-daemon 起動・UDS ソケット生成済み |
| 操作 | ①`./target/release/shikomi list` を実行し exit 0 を確認。②`./target/release/shikomi status`（または `shikomi daemon-version` 相当コマンド）を実行し daemon が正常応答することを確認 |
| 期待結果 | ①`shikomi list` が exit 0（0 件以上）。②daemon 状態確認コマンドが exit 0 かつ daemon 情報を返す。両方が成功することで IPC 接続を二重確認 |
| 失敗条件 | いずれかが非ゼロ exit → スクリプトが exit 1 |

**設計根拠**: `shikomi list` の exit 0 のみでは、「daemon 未接続でも空リスト exit 0 を返す実装が将来導入された場合」に接続失敗を見逃す。daemon 状態確認コマンドを二重チェックとして加えることで、接続確立を確実に証明する（`AC-GUI-01`「daemon と IPC 接続が確立される」の厳密な検証）。

> **実装注意**: `detailed-design.md §6` が設計する daemon 状態確認コマンドの名称・シグネチャに合わせて本 TC の操作②を実装すること（`shikomi status` / `shikomi daemon-version` 等、セルの設計に従う）。

---

### TC-GUI-CI-IT03: GUI プロセス正常終了確認

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT03 |
| 対応する要件ID | REQ-CI-07、AC-GUI-01 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §6.5` `trap EXIT` 設計・`§6.6` 正常終了確認） |
| 種別 | 正常系 |
| 前提条件 | TC-GUI-CI-IT01・IT02 通過後。`trap EXIT` が `$GUI_PID` / `$DAEMON_PID` / `$XVFB_PID` をカバーして登録済み |
| 操作 | `kill -TERM $GUI_PID`、`timeout 5 wait $GUI_PID` を実行 |
| 期待結果 | `wait $GUI_PID` が 5 秒以内に exit 0 を返す（SIGTERM を受けて正常終了。ゾンビプロセスなし）。`trap EXIT` により daemon / Xvfb も続いて終了する |
| 失敗条件 | `timeout` が exit 124（タイムアウト）または `wait` が非ゼロ → スクリプトが exit 1 → ジョブ FAIL |
| trap による保証 | スクリプトが IT01〜IT03 どの時点で失敗・終了しても、`trap EXIT` が `kill $GUI_PID $DAEMON_PID $XVFB_PID` を実行する。CI ランナーにプロセスが残留しない |

---

### TC-GUI-CI-IT04: daemon 未起動時 smoke FAIL 検証（逆正常性 — CI 自動実行）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-CI-IT04 |
| 対応する要件ID | REQ-CI-07 |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §9.1` 失敗シナリオ・`§6.5` `--no-daemon` フラグ）|
| 種別 | 異常系（逆正常性確認）|
| 前提条件 | xvfb 起動済み。`scripts/smoke.sh` が `--no-daemon` フラグを受け付け、daemon 起動ステップをスキップする実装済み |
| 操作 | `bash scripts/smoke.sh --no-daemon` を実行（`test-gui.yml` の `e2e-smoke-no-daemon` ジョブで自動実行）|
| 期待結果 | daemon ソケット未生成のため `shikomi list` が非ゼロ exit → smoke スクリプトが exit 1。CI ジョブが FAIL と判定される |
| CI での検証方法 | `e2e-smoke-no-daemon` ジョブの step に `continue-on-error: false` が設定され、`bash scripts/smoke.sh --no-daemon` の exit 1 がジョブ FAIL として記録される。ジョブ FAIL が「期待された失敗」であることを CI ログで確認する |

**設計根拠**: 「smoke スクリプトは接続失敗を正しく検出できる」ことを CI で自動検証する。逆正常性テストをローカル手動に委ねると、スクリプト変更時の回帰に気付けない。`--no-daemon` フラグを追加することで手動操作不要の CI 完結設計にする。

---

## §7. テスト対象外の明示

| 機能 | 対象外の理由 | 代替検証 |
|------|-----------|--------|
| macOS コード署名・公証（REQ-CI-02）の実効性 | `macos-latest` ランナーと APPLE_* Secrets が必要。fork PR では実行不可 | CI 上の `bundler.yml` 実行（内部 PR のみ）+ 手動受入（AC-GUI-09 Gatekeeper 検証）。REQ-CI-02 の自動カバレッジは `bundler.yml` のビルド成功を受入証拠とする（system-test-design.md §macOS 受入 に委ねる）|
| Windows MSI / NSIS ビルド（REQ-CI-03）| `windows-latest` ランナーが必要 | CI 上の `bundler.yml` 実行 + 手動受入（AC-GUI-08 SmartScreen 確認。Windows MVP コード署名なしは `feature-spec.md` AC-GUI-08 に手動受入として明記済み）|
| Linux AppImage ダブルクリック起動（REQ-CI-04）| GUI 操作が必要。CI headless では不可 | 手動受入（AC-GUI-10）|
| artifact 保持期間（REQ-CI-05）の実効性 | 7 日 / 30 日の確認には時間経過が必要 | PR マージ後の GitHub Actions artifact UI で目視確認 |
| macOS Keychain `if: always()` クリーンアップ | `tauri build` 失敗後のクリーンアップ動作は macOS CI 実行中のみ確認可能 | macOS ランナーでジョブ失敗 → Keychain 残留なしを手動確認 |
| GUI 画面の描画・レイアウト | Xvfb はウィンドウ生成のみ保証。画面キャプチャ検証はコスト過大（`basic-design.md §4.3` 参照）| 手動受入（AC-GUI-01〜07）|
| トレイアイコン操作 headless 確認 | `libappindicator3` の動作は GNOME 環境依存 | システムテスト（実機 GNOME 環境）|
| 30 秒カウントダウン表示（AC-GUI-07）| トレイ操作が必要 | システムテスト（実機）|
| `cargo deny check` advisory DB 鮮度 | CI 実行時の advisory DB は日々更新。ローカルとは乖離しうる | `audit.yml` を daily / PR トリガーで定期実行 |
| `tauri-driver` + WebDriverIO フル E2E | YAGNI（現スコープ超過・環境セットアップ負荷大）。headless smoke で必要十分 | アクセシビリティ要件が生まれた時点で別 Issue として起票 |
| 3 OS 共通 composite action 自体のテスト | composite action は YAML 構文検証（actionlint）で充分。action 内部ロジックは各 OS ジョブの成功が証拠 | bundler.yml 実行（CI）|

---

## §8. モック方針まとめ

| テスト対象 | モック要否 | 実装方法 |
|----------|---------|---------|
| shikomi-daemon プロセス | **不要** | 実バイナリを起動（IT01〜03）/ 意図的に起動しない（IT04 `--no-daemon`）|
| shikomi-gui プロセス | **不要** | 実バイナリを起動 |
| xvfb 仮想ディスプレイ | **不要（実環境）** | CI ランナーで `Xvfb :99` を直接起動。`trap EXIT` で終了保証 |
| UDS ソケット（IPC）| **不要** | 実 IPC を使用（`shikomi list` + daemon 状態確認が実ソケットに接続）|
| APPLE_* Secrets（macOS 公証）| **スキップ（条件分岐）** | fork PR では `if:` 条件で macOS ジョブ全体をスキップ |
| RUSTSEC advisory DB | **不要** | `deny.toml` の `[advisories.ignore]` で静的対処 |

**assumed mock 禁止**: E2E smoke テストはすべて実バイナリ間の統合検証。中間レイヤーに仮定ベースのモックを挿入しない。

---

## §9. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-GUI-CI-UT01（bundler.yml actionlint 正常系）| `lint.yml`（actionlint ステップ）| PR CI で毎回実行 |
| TC-GUI-CI-UT02（bundler.yml actionlint 負例）| `lint.yml`（actionlint 負例 step）| mktemp YAML を生成して actionlint 実行 → 非ゼロ exit を期待。`continue-on-error: false` |
| TC-GUI-CI-UT03（test-gui.yml actionlint 正常系）| `lint.yml`（actionlint ステップ）| PR CI で毎回実行 |
| TC-GUI-CI-UT04（test-gui.yml actionlint 負例）| `lint.yml`（actionlint 負例 step）| 同上、test-gui.yml 対象 |
| TC-GUI-CI-UT05（cargo deny check）| `audit.yml` | shikomi-gui 依存追加後も継続実行 |
| TC-GUI-CI-IT01〜IT03（E2E smoke 正常系）| `test-gui.yml` `e2e-smoke` ジョブ（`bash scripts/smoke.sh`）| ubuntu-22.04 + xvfb。PR / main / develop プッシュ時に自動実行 |
| TC-GUI-CI-IT04（逆正常性 CI 自動）| `test-gui.yml` `e2e-smoke-no-daemon` ジョブ（`bash scripts/smoke.sh --no-daemon`）| exit 1 を期待。`e2e-smoke` ジョブと並列または直後に実行 |

---

## §10. カバレッジ基準

| 観点 | 基準 |
|------|------|
| REQ-CI-01 / REQ-CI-08 | TC-GUI-CI-UT01（actionlint 正常系）+ `bundler.yml` CI 実行で自動カバー |
| REQ-CI-02（macOS 公証）| `bundler.yml` `build-macos` ジョブの成功（内部 PR）が自動証拠。Gatekeeper 手動受入（AC-GUI-09）が補完。system-test-design.md §macOS 手動受入に詳細 |
| REQ-CI-03（Windows ビルド）| `bundler.yml` `build-windows` ジョブの成功が自動証拠。AC-GUI-08（SmartScreen 警告）は `feature-spec.md` に手動受入として明記済み |
| REQ-CI-04 / REQ-CI-05 | `bundler.yml` `build-linux` / artifact upload ジョブの成功が自動証拠 |
| REQ-CI-06（cargo audit）| TC-GUI-CI-UT05（cargo deny）で自動カバー |
| REQ-CI-07（E2E smoke TC-GUI-E01）| TC-GUI-CI-IT01〜IT03（正常系 3 段階）+ TC-GUI-CI-IT04（逆正常性）で CI 完全自動カバー |
| CI 静的検証品質 | TC-GUI-CI-UT02 / UT04（actionlint 負例）により「linter が実際に機能している」ことを保証 |
| cleanup 対称性 | `trap EXIT`（smoke）+ `if: always()`（macOS Keychain）の両方が設計に明示されていること |
| 手動受入との役割分担 | AC-GUI-08（Windows SmartScreen）・AC-GUI-09（macOS Gatekeeper）・AC-GUI-10（Linux AppImage 起動）は `bundler.yml` 成果物を用いた手動受入で確認。本 test-design.md の自動テスト対象外 |
| テスト非重複 | E2E smoke（TC-GUI-E01）は `e2e-smoke` ジョブで 1 回のみ実行。逆正常性（IT04）は `e2e-smoke-no-daemon` ジョブで独立実行。`bundler.yml` とジョブが独立しているため三重実行なし |

---

*作成: 涅マユリ（テスト担当）/ 2026-05-11*
*改訂 v2（2026-05-11）: ペテルギウス・ロマネコンティ査読フィードバック対応 — TC-GUI-CI-IT04 CI 自動化・actionlint 負例追加・sleep 固定 → ポーリング・smoke SSoT（scripts/smoke.sh）・trap EXIT・IPC 二重確認・REQ-CI-02/03 カバレッジ articulate*
*設計根拠: `docs/features/shikomi-gui/build-ci/basic-design.md` §モジュール契約 / `detailed-design.md §1〜9` / Issue #98*
