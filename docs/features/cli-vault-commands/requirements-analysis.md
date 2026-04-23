# 要求分析書

<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/requirements-analysis.md -->

## 人間の要求

> Issue #1（Windows ACL 強制実装）が完了し、オープン Issue が 0 件になった。依頼:「開発を進めていって。現状分析、Issue を切って工程に従って設計実装を繰り返していって。複数 PR を同時並行で作業したりレビューしたりするのは禁止」。
>
> 2 案比較の結果（セル案: daemon 先行 / ペテルギウス案: CLI 先行）から、キャプテンが **CLI 先行** を決定:
> 「CLI コマンド層（`shikomi list/add/edit/remove`）から着手する。理由：既存 infra の上に薄く乗るだけでスコープ最小、Clean Arch の縦串が初めて通り、後続の骨格テンプレートになる。daemon は複雑性が高い——CLI が動いてからだ。」

依頼主（まこちゃん）は **README 公約の機能を実働状態にする順番**を開発チームに委ねており、チーム内で「縦串優先」「スコープ最小」「既存 Clean Arch 壁を壊さない」の 3 軸でこの順序に合意した。

## 背景・目的

- **現状: 製品として一行も動いていない**。`shikomi-cli` / `shikomi-daemon` / `shikomi-gui` はいずれも `fn main() {}` の 1 行スケルトン。実装済みは `shikomi-core`（vault ドメイン型）と `shikomi-infra`（SQLite 永続化 + Windows ACL 強制）のみ。README に謳った `shikomi list` / `add` / `edit` / `remove` / `daemon start` / `gui` のいずれも未実働。
- **Clean Architecture の縦串を先に通す**ことが、後続 feature（daemon、ホットキー、暗号化、GUI）の**骨格テンプレート**になる。CLI は最上位層で、既存 `shikomi-infra::VaultRepository` trait に**薄く乗るだけ**——新規外部依存は `clap` とテスト用 `assert_cmd` に限定され、OS 依存の爆発を伴わない。
- **ペルソナ B（山田 美咲 / フロントエンドエンジニア）**の「CLI で設定同期できる」期待を最小コストで満たす。`shikomi export` / `import` / `daemon start` / `vault encrypt` は後続 feature に委ね、本 feature は **vault CRUD 4 コマンドに限定**する。
- **プロセスモデルの原則との整合**: `docs/architecture/context/process-model.md` §4.1 ルール1 は「CLI/GUI は直接 vault を開かない。IPC 経由でのみ daemon に依頼する」と規定する。本 feature はこの原則と正面衝突するため、**MVP フェーズ区分を `process-model.md` に追記して整合を取る**（Phase 1: CLI 直結 / Phase 2: daemon IPC 経由へ移行）。`VaultRepository` trait が抽象化の壁として機能しているため、Phase 2 では `IpcVaultRepository` 実装を差し替えるだけで CLI コード本体は変更不要となる設計判断である（キャプテン決定）。
- **ビジネス価値**: 「動くもの」を初めて作る。Issue #1 時点で外部レビューが人間承認まで到達しているため、本 feature が完了すれば `shikomi list` / `add` / `edit` / `remove` がユーザー環境で実働する最初のバージョンとなる。

## 議論結果

Issue #1 完了後の工程 1 で以下を合意:

- 設計担当（セル）提案: daemon 先行（process-model 厳密準拠）
- 防衛線（ペテルギウス）提案: CLI 先行（縦串最優先、既存 infra に薄く乗るだけ）
- **キャプテン決定: CLI 先行（ペテルギウス案）**
  - 根拠: daemon 複雑性（シングルインスタンス / IPC / 自動起動）を待たずに縦串を通す価値が大きい
  - 原則整合: process-model.md §4.1 に MVP Phase 1 / Phase 2 区分を追記して整合を明示
- スコープ絞り込み: **`list` / `add` / `edit` / `remove` の 4 コマンドのみ**。`export` / `import` / `daemon` / `vault encrypt` / `vault decrypt` は別 feature（後続 Issue）
- 対応する vault モード: **平文モードのみ**（MVP Phase 1 相当）。暗号化モードの vault を検出した場合は専用エラーで Fail Fast し、「未対応」と案内して終了する
- Sub-issue 分割: 不要（4 コマンドは `VaultRepository` の上に薄く乗るだけで単一 PR で扱える規模）

## ペルソナ

本 feature のプライマリ体験者は**エンドユーザー**（`shikomi-cli` を実機で叩く人）と**開発貢献者**（CLI を後続 feature の骨格テンプレートとして参照する Rust 実装担当）の 2 種。エンドユーザーペルソナ（田中 / 山田 / 佐々木）は `docs/architecture/context/overview.md` §3.2 を参照。

| ペルソナ名 | 役割 | 技術レベル | 利用文脈 | 達成したいゴール |
|-----------|------|-----------|---------|----------------|
| 山田 美咲（28, FE エンジニア） — **プライマリ** | 日常ユーザー兼 OSS コントリビュータ | CLI / Homebrew / apt は日常使用、Wayland と X11 を区別できる | 開発中、`~/.local/share/shikomi/vault.db` に登録したエントリを `shikomi list` で確認し、`shikomi add --label "SSH: prod" --value "ssh -J bastion prod01"` で 1 行追加する | `shikomi-daemon` 未実装でも vault に対する CRUD が CLI 1 本で完結する |
| 田中 俊介（35, SaaS 営業職） — **セカンダリ** | 日常ユーザー（GUI 主、CLI 非常用） | ChatGPT / Slack は使えるが PowerShell は触れない | GUI 未実装の MVP 期間、コマンドプロンプトを起動して `shikomi list` で登録済みエントリを確認する | エラーメッセージが日本語で、次の行動（`shikomi add --help` 等）が明示される |
| 野木 拓海 | 後続 feature の Rust 実装担当 | Rust 中級（`clap` / `anyhow` / `tokio` 実務経験） | `shikomi-daemon` / `shikomi-gui` feature を着手する際、CLI の Clean Arch 配線を参考にする | Presenter / UseCase / Repository の 3 層分離が見えており、Phase 2 で IPC 実装に差し替える具体手順が設計書から辿れる |
| 志摩 凛 | OSS コントリビュータ | Rust 初級 | GitHub Issue を拾い、`shikomi list --json` 等の追加フラグを提案 PR する | 既存 CLI コードの責務分離が明瞭で、フラグ追加が安全に完了できる |

**ペルソナへの設計インパクト**:

- 山田: 秘密値（`--secret`）を投入する際、コマンドライン引数直書きは shell 履歴に残るため、**標準入力（stdin）からのパスワード入力を第一推奨**とし、引数直書きには警告を出す
- 田中: エラーメッセージは英語原文と日本語訳の **2 段表示**（`MSG-CLI-xxx` の i18n 写像）
- 野木: `shikomi-cli` のディレクトリ構造を `presenter/` / `usecase/` / `repository/` に分け、後続 feature の雛形として参照可能にする
- 志摩: `clap` v4 derive を使い、サブコマンド追加がコンパイル時型検査で安全に拡張できる

## 前提条件・制約

- **Issue #1 完了済み**: `shikomi-core` vault ドメイン型 / `shikomi-infra::VaultRepository` trait / `SqliteVaultRepository` 実装 / Unix パーミッション `0600` / Windows owner-only DACL 強制 すべて merge 済み。本 feature は**ゼロ infra 変更**が原則（既存 API を触らない）。
- **暗号化モード未対応**: `shikomi-infra` の永続化層は平文モードのみ対応（`TRACKING_ISSUE_ENCRYPTED_VAULT: Option<u32> = None`）。本 feature も平文モードに限定し、暗号化 vault を検出したら **Fail Fast で「Encryption not yet supported」エラー**を返す。`shikomi vault encrypt` / `decrypt` サブコマンドは本 feature のスコープ外。
- **MVP Phase 1（CLI 直結）**: `shikomi-cli` は `shikomi-infra::SqliteVaultRepository` を**直接構築**し、daemon を介さずに vault を読み書きする。process-model.md §4.1 にこの区分を追記して正当化する。Phase 2（daemon 経由）への移行パスは `VaultRepository` trait 差し替えで完了する想定。
- **pure CLI / no-hotkey**: 本 feature はホットキー購読・クリップボード投入を扱わない。`shikomi-core` / `shikomi-infra` が既に持つ読み書き API を CLI フェイスから使うだけ。
- **`[workspace.dependencies]` 経由の依存追加**: `clap` v4（derive feature）・`anyhow`（アプリ層のエラーラップ）・テスト用 `assert_cmd` / `predicates` を追加する。各 crate 側 `Cargo.toml` は `{ workspace = true }` のみ書く（`docs/architecture/tech-stack.md` §4.4）。
- **ターミナル TTY 検出**: secret 入力時の画面エコー抑止に `rpassword` を採用（Windows / Unix 両対応、Tauri 等の巨大依存なし）。
- **非対話モード許容**: CI / スクリプト利用を想定し、stdin から secret を読む際に TTY でない場合は警告せずにそのまま読む（標準 UNIX 的挙動）。

## 機能一覧

| 機能ID | 機能名 | 概要 | 優先度 |
|--------|-------|------|--------|
| REQ-CLI-001 | `shikomi list` | vault 内の全レコードを ID / ラベル / 種別で 1 件 1 行表示。種別が `Secret` の値はマスク表示 | 必須 |
| REQ-CLI-002 | `shikomi add` | 新規レコードを追加。`--kind text\|secret` / `--label` / `--value` or `--stdin` | 必須 |
| REQ-CLI-003 | `shikomi edit` | 既存レコードを部分更新。`--id` 指定必須、`--label` / `--value` / `--stdin` のいずれかを指定 | 必須 |
| REQ-CLI-004 | `shikomi remove` | レコードを削除。`--id` 指定、`--yes` 無しの場合は対話確認必須 | 必須 |
| REQ-CLI-005 | 共通: vault path 解決 | `SHIKOMI_VAULT_DIR` 環境変数 > `--vault-dir` フラグ > OS デフォルト の順で解決 | 必須 |
| REQ-CLI-006 | 共通: 終了コード契約 | 成功 0 / ユーザ入力エラー 1 / システムエラー 2 / 暗号化モード未対応 3 の 4 値で分離 | 必須 |
| REQ-CLI-007 | 共通: secret マスキング | Secret レコードの値は標準出力・エラー出力・panic メッセージ・`tracing` ログで一切露出しない | 必須 |
| REQ-CLI-008 | 共通: エラーメッセージ出力 | すべてのエラーは `stderr` に `error: 原因 / ヒント: 次の行動` の 2 行構成で出力 | 必須 |
| REQ-CLI-009 | 共通: 暗号化 vault 拒否 | 暗号化モードの vault を検出した場合、CRUD を実行せず Fail Fast で終了コード 3 | 必須 |
| REQ-CLI-010 | 共通: vault 未初期化時の自動作成 | `add` コマンド時、vault.db が存在しなければ平文モードの空 vault を自動作成。他コマンドは「vault 未作成」エラー | 必須 |
| REQ-CLI-011 | 共通: 確認プロンプトの TTY 要件 | `remove` の確認プロンプトは TTY 接続時のみ出す。非 TTY で `--yes` 未指定なら即エラー（スクリプトからの意図しない削除を防ぐ） | 必須 |
| REQ-CLI-012 | 共通: CLI Presenter / UseCase / Repository の 3 層分離 | 出力整形・ドメイン操作・I/O を `presenter/` / `usecase/` / リポジトリ参照 で分離。Phase 2（daemon 経由）への移行時に repository 層のみ差し替えるための設計 | 必須 |

## Sub-issue分割計画

該当なし — 理由: 4 コマンドは共通基盤（vault path 解決・終了コード・マスキング・Presenter 分離）を共有し、別 PR に分割すると中途半端な状態（例: `list` のみ動き `add` は `fn main() {}` のまま）の `shikomi-cli` が develop に乗る。単一 PR で 4 コマンドまとめて完成させる。

| Sub-issue名 | スコープ | 依存関係 |
|------------|---------|---------|
| — | — | — |

## 非機能要求

| 区分 | 指標 | 目標 |
|-----|------|------|
| 応答時間（平文モード） | `shikomi list`（100 レコード）の総実行時間 | p95 100 ms 以下（vault load + 出力整形、`nfr.md` §9 の平文モード p95 50 ms に対し CLI 起動オーバーヘッドを考慮して 2 倍のバジェット） |
| 応答時間（平文モード） | `shikomi add` / `edit` / `remove`（100 レコード vault）の総実行時間 | p95 150 ms 以下（load + domain 操作 + save のシリアル実行） |
| バイナリサイズ | `shikomi-cli` 単独リリースビルドサイズ（LTO + strip 後） | 8 MB 以下（`clap` / `rpassword` / `anyhow` + infra 依存） |
| ビルド時間 | `cargo build -p shikomi-cli` 初回 | 90 秒以下 |
| ユニット / 結合テスト実行時間 | `cargo test -p shikomi-cli` 全テスト | 20 秒以下（E2E は `assert_cmd` + `tempfile` で並列実行可） |
| 秘密値リーク耐性 | Secret レコード値の標準出力・標準エラー・panic メッセージ露出 | 0 件（`SecretString` 通しで扱い、Display 経由を禁止。doc テスト + 結合テストで検証） |
| Shell 履歴経由の secret 漏洩耐性 | `shikomi add --secret --value "pw"` 実行時の警告出力 | 必須（`stderr` に「値は shell 履歴に残る。`--stdin` 推奨」の警告を出し、終了コード 0 は維持） |
| i18n 対応 | エラーメッセージの日本語併記 | 全 `MSG-CLI-xxx` で英語原文 + 日本語訳を出力（環境変数 `LANG=C` 検知時は英語のみ） |
| 非対話実行（CI 利用） | `assert_cmd` での E2E テスト可能性 | 標準入出力・終了コードが stdin TTY を前提にせずに動作する |

## 受入基準

| # | 基準 | 検証方法 |
|---|------|---------|
| 1 | `shikomi list` が空 vault・1 件・複数件の各状態で正常に列挙する（Secret はマスク表示） | E2E テスト（`assert_cmd` + `tempfile` で `SHIKOMI_VAULT_DIR` 切替） |
| 2 | `shikomi add --kind text --label L --value V` が新規レコードを追加し、続けて `shikomi list` で反映される | E2E テスト |
| 3 | `shikomi add --kind secret --label L --stdin` が stdin から secret を読み、stdout / stderr に secret が一切出ない | E2E テスト + 標準出力 grep で「SECRET_TEST_VALUE」不在を確認 |
| 4 | `shikomi add --kind secret --label L --value V` が警告を stderr に出すが、終了コードは 0 | E2E テスト |
| 5 | `shikomi edit --id <uuid> --label NEW` が label のみ更新、`--value` と `--stdin` の併用を Fail Fast で拒否 | E2E テスト |
| 6 | `shikomi remove --id <uuid>` が TTY 確認プロンプトを表示、非 TTY で `--yes` 無しなら終了コード 1 で即エラー | E2E テスト（`assert_cmd` で stdin を pipe する非 TTY 状態を作る） |
| 7 | `shikomi remove --id <uuid> --yes` が確認なしで削除する | E2E テスト |
| 8 | 暗号化モードの vault を検出したら終了コード 3 で「Encryption not yet supported」エラー | E2E テスト（暗号化ヘッダを手動注入した vault.db に対して実行） |
| 9 | vault 未初期化時に `list` / `edit` / `remove` は終了コード 1 でエラー、`add` は平文モードで自動作成 | E2E テスト |
| 10 | `SHIKOMI_VAULT_DIR` / `--vault-dir` / OS デフォルト の優先順位が仕様通り | E2E テスト（環境変数とフラグの組合せ） |
| 11 | すべての `MSG-CLI-xxx` が英語 + 日本語の 2 段で出力され、`LANG=C` で英語のみ | E2E テスト |
| 12 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` すべて pass | CI |
| 13 | `cargo test -p shikomi-cli` 全 pass、行カバレッジ 80% 以上 | CI (`cargo llvm-cov`) |
| 14 | `process-model.md` §4.1 に MVP Phase 1 / Phase 2 の区分が追記されている | 設計レビュー |
| 15 | `shikomi-cli/src/` が `presenter/` / `usecase/` / 共通 main.rs の 3 層構造で分離されている | 設計レビュー + ディレクトリ構造確認 |
| 16 | 公開 API（将来 Phase 2 で差し替え予定の layer）が `VaultRepository` trait 境界のみに依存している | コード grep（`shikomi-cli` 内で `SqliteVaultRepository` 具体型を参照する箇所が main.rs の構築位置のみであること） |

## 扱うデータと機密レベル

本 feature は CLI 層だが、**パスワード等の最高機密データがコマンドラインを介して流れる**ため、各データの扱いを型と経路レベルで明示する。

| データ | 機密レベル | 本 feature での扱い |
|-------|----------|-----------------|
| レコードラベル（`--label` の値） | 低〜中 | `RecordLabel::try_new` 経由で検証。stdout 表示 OK |
| Text レコードの値（`--value` / `--stdin` の値） | 低〜高（ユーザ判断） | `SecretString` としては扱わない（Text kind はユーザが「秘密でない」と宣言した）。stdout 表示 OK |
| Secret レコードの値（`--value` / `--stdin` の値） | 最高 | `SecretString` として受け取り、`RecordPayload::Plaintext(SecretString)` に格納。stdout / stderr / panic / `tracing` のいずれにも露出禁止 |
| RecordId（`--id` の値） | 低 | `RecordId::try_from_str` で検証。stdout 表示 OK |
| vault path（`SHIKOMI_VAULT_DIR` / `--vault-dir` / OS デフォルト） | 低 | stdout / stderr / `tracing` に出して OK（ユーザ自身の環境情報） |
| マスターパスワード / BIP-39 リカバリコード | 最高 | **本 feature では扱わない**（暗号化モード未対応のため）。将来の `shikomi vault encrypt` feature で導入 |

設計担当・実装担当は、**Secret レコードの値は `SecretString` 型で CLI 最外殻から `VaultRepository::save` まで剥ぎ落とさずに運ぶ**こと。`String` / `&str` に剥ぎ落とす経路を作った瞬間、`Debug` 経由の漏洩リスクが生じる。
