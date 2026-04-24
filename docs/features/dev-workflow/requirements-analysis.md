# 要求分析書

<!-- feature単位で1ファイル。新規featureならテンプレートコピー、既存featureなら既存ファイルをREAD→EDIT -->
<!-- 配置先: docs/features/dev-workflow/requirements-analysis.md -->

## 人間の要求

Issue #22 から原文引用:

> # [調査] clone直後から自動で有効になるGitフック運用の整備
>
> ## 背景
> 現状、pre-commit / pre-push等のGitフックを有効化するには、clone後に手動で `pre-commit install` を実行したり、`cargo test` 経由で `cargo-husky` の自動インストールを走らせる必要がある。このワンステップが挟まることで以下の問題がある：
> - cloneした直後の状態ではフックが効かず、セットアップ忘れで素通りコミットが発生する
> - 新規参画者やエージェントを含む全ての開発主体が、同じセットアップ手順を踏む必要がある
> - `cargo-husky` のように「特定コマンド実行時にインストール」される方式は、その経路を通らない限り有効化されない
>
> 理想は **clone直後、追加のインストール手順なしに `git commit` / `git push` で自動的にフックが走る状態**。
>
> また、CIに品質保証を寄せる方針は現実的でない。Actionsのコストが嵩みすぎるため、**ローカルで品質を担保し、CIは最終確認の位置付け**にしたい。ローカルフックが動かないこと自体が直接的な品質リスクになる。
>
> ## 課題
> - clone直後からフックが有効である必要がある（手動の有効化ステップを挟まない）
> - 品質担保の本体はローカル、CIは最終確認
> - `--no-verify` で安易にバイパスされない運用

（全文は Issue #22 に残置、本書ではスコープ確定のため主要段落のみ引用）

## 背景・目的

### 現状の痛点

1. **PR #21 の実例**: `cargo fmt --check --all` で整形差分が残ったまま push され、`lint` / `test-infra` / `test-infra-windows` の 3 ジョブが同時に失敗。外部レビュー提出後に差戻し、再修正・再レビューで 1 往復を消費した。**ローカル pre-commit で `cargo fmt` を強制していれば発生しなかった事象**である（commit `1b57b38` の経緯）。
2. **CI を最後の砦にする構造的限界**: shikomi は 8 本のワークフロー（`audit` / `lint` / `test-infra` / `test-infra-windows` / `unit-core` / `branch-policy` / `pr-title-check` / `back-merge-check`）を走らせており、**同一 commit で全ジョブが走るたびに GitHub Actions のクレジットを消費する**。`cargo fmt --check` のように秒で終わるチェックで CI が落ちるのは、時間・コスト双方の無駄。
3. **`cargo-husky` は機能要件を満たさない**: `cargo test` 実行時に `.git/hooks/` へインストールされる設計であり、**`cargo test` を一度も走らせずに `git commit` すると素通りする**。本 Issue の「clone 直後から有効」という要件に対して構造的に不適合。

### 解決されれば変わること

- ローカルで fmt / clippy 失敗が早期に検出され、push 前に修正が完結する
- CI は「ローカルをバイパスした場合のセーフティネット」として機能し、通常フローでは緑通過が既定になる
- 新規参画者・エージェント（Claude Code 等）・既存メンバーが同一の setup ステップで環境を揃えられる
- `--no-verify` による意図的バイパスはユーザー責任領域として明示され、規約違反として扱える

### ビジネス価値

- CI コスト削減（fmt 落ち程度のやり直し PR を減らす）
- 外部レビューへの差戻し頻度低減 → リードタイム短縮
- オンボーディング時間の短縮（新規コントリビュータ・エージェントの環境構築ばらつき排除）

## 議論結果

### 設計担当による採用前提

- **フックツール**: `lefthook`（Go 製バイナリ、YAML 設定、並列実行、Windows ネイティブ対応）
- **タスクランナー**: `just`（`justfile`、Rust コミュニティで広く採用、Windows ネイティブ対応、`cargo install just` で統一導入可）
- **commit-msg 検査**: `convco`（Rust 製、Conventional Commits 専用、`cargo install convco` で統一導入可）
- **セットアップ**: `scripts/setup.sh`（Unix）+ `scripts/setup.ps1`（Windows）の 2 本。`cargo xtask setup` と `build.rs` 方式は副作用過多で不採用

### 却下候補と根拠

| 候補 | 却下理由 |
|-----|---------|
| `cargo-husky` | `cargo test` 初回実行まで `.git/hooks/` が空のまま。clone 直後要件を満たさない |
| `pre-commit` framework (Python) | Python ランタイム依存。Windows 開発者の環境ノイズ増 |
| `rusty-hook` | `cargo-husky` と同じく `cargo test` 初回実行依存の構造 |
| `build.rs` によるフック設定 | `cargo check`/IDE/CI が暗黙に叩くたびに git config を書換える副作用。Fail Fast 原則違反 |
| `cargo xtask setup` | 別 crate 追加のメンテコスト、初回コンパイル時間、setup は 1 度しか走らないため KISS に反する |
| `core.hooksPath` + 生シェル | Windows の CRLF / 実行ビット / POSIX 互換シェル依存で罠が多い。並列化を自前実装する追加コスト |
| `Makefile` | Windows ネイティブで動かない（GNU Make 別途必要） |
| `commitlint` (Node.js) | Node.js 依存を開発必須化すると shikomi のコア技術スタック（Rust）から外れる |
| `npm scripts` | 同上 |

### 「clone 直後から完全自動」の扱い

Git の仕様上、`.git/hooks/` は clone で配布できない（`.git/` はリポジトリ本体の外）。よって **どの方式でも最低 1 コマンドの有効化操作は必要**。本 feature では「`git clone` 後に `scripts/setup.{sh,ps1}` を 1 回実行すれば完了」を「**実現可能な最短**」として受容する。README / CONTRIBUTING にこの 1 ステップを明文化する。

## ペルソナ

| ペルソナ名 | 役割 | 技術レベル | 利用文脈 | 達成したいゴール |
|-----------|------|-----------|---------|----------------|
| 新田 圭介（27） | OSS 新規コントリビュータ（Rust 中級） | Rust 1 年、Linux 主、Git GUI 併用 | Issue を拾って feature ブランチで初 PR を上げる | clone → setup 1 回 → 通常の `git commit` / `git push` でローカル検証が自動で走り、push 前に落ちる失敗を push 後に知らずに済む |
| Agent-C（Claude Code） | 自動化エージェント（LLM） | Rust トリビア平均、シェル実行可 | Issue からドラフト PR を生成、CI 結果を見てループ修正 | 手動で `lefthook install` や `pre-commit install` を忘れない。setup スクリプト 1 本で決定論的に環境を用意できる |
| 倉田 美保（34） | レビュワー兼メンテナ（Rust 上級） | Rust 5 年、3 OS（Mac/Win/Linux）で検証 | 全 PR の最終承認、release/* ブランチ運用 | ローカルで `just check-all` 相当を一発で回せる。CI を通過するコミットがローカルで必ず通ることの担保 |

## 前提条件・制約

| 区分 | 内容 |
|-----|------|
| 既存技術スタック | Rust (stable, MSRV 1.80.0)、Cargo workspace 5 crate、Tauri v2（GUI は Sub 後続 Issue）、Windows/macOS/Linux サポート必須 |
| 既存 CI | GitHub Actions 8 ワークフロー（§背景・目的 2）。**削除禁止**（本 feature は CI を補強するものであり置換ではない） |
| 既存ブランチ戦略 | GitFlow（`develop` → `feature/*`、`release/*` → `main`）。CONTRIBUTING.md §ブランチ戦略参照 |
| コミット規約 | Conventional Commits。`pr-title-check` ワークフローで PR タイトルを検証済み |
| line endings | `.gitattributes` により全テキスト LF 強制（Windows の CRLF 混入防止済み）。setup スクリプトの Windows 版も LF でコミット |
| 実行権限 | 管理者権限不要。`cargo install` 可能な環境を前提（Rust toolchain が導入済みならいずれのツールもインストール可能） |
| ネットワーク | `cargo install` は crates.io への接続を要する。オフライン環境は本 feature のスコープ外（YAGNI） |
| 対象 OS | Windows 10 21H2 以上 / macOS 12 以上 / Linux（Ubuntu 22.04 以上、Arch、Fedora 相当）。README §動作環境と同一 |

## 機能一覧

| 機能ID | 機能名 | 概要 | 優先度 |
|--------|-------|------|--------|
| REQ-DW-001 | フックツール導入 | `lefthook.yml` をリポジトリにコミットし、`lefthook install` で `.git/hooks/` に配置する | 必須 |
| REQ-DW-002 | pre-commit フック | コミット時に `cargo fmt --check` と `cargo clippy` を並列実行 | 必須 |
| REQ-DW-003 | pre-push フック | push 時に `cargo test` を走らせる（`unit-core` / `test-infra` 相当） | 必須 |
| REQ-DW-004 | commit-msg フック | Conventional Commits 規約（`convco check`）でメッセージ検証 | 必須 |
| REQ-DW-005 | タスクランナー導入 | `justfile` を配置し、`fmt-check` / `lint` / `test` / `audit` / `check-all` 等のレシピを定義 | 必須 |
| REQ-DW-006 | CI との単一実行経路化 | GitHub Actions ワークフロー（`lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml`）を `just <recipe>` 呼び出しに統一し、ローカルと CI で同一コマンドを走らせる | 必須 |
| REQ-DW-007 | setup スクリプト（Unix） | `scripts/setup.sh`: `just` / `lefthook` / `convco` を `cargo install --locked` で導入し、`lefthook install` を実行 | 必須 |
| REQ-DW-008 | setup スクリプト（Windows） | `scripts/setup.ps1`: PowerShell 5.1+ 前提、同等の導入処理 | 必須 |
| REQ-DW-009 | 冪等性と再実行耐性 | setup スクリプトは既にインストール済みのツールをスキップし、複数回実行しても差分を出さない | 必須 |
| REQ-DW-010 | README / CONTRIBUTING 更新 | `git clone` 後のワンステップ setup 手順、`--no-verify` 禁止ポリシー、利用可能な `just` レシピ一覧を明文化 | 必須 |
| REQ-DW-011 | `--no-verify` バイパス検知 | サーバ側 CI で全チェックを再実行し、バイパスされたコミットを必ず落とす（CI を最後の砦として維持） | 必須 |
| REQ-DW-012 | フック失敗時のメッセージ品質 | 失敗した際にユーザーが次に取るべきコマンド（例: `just fmt` で自動修正）を提示 | 必須 |

## Sub-issue分割計画

本 Issue #22 は調査 Issue。**設計確定（本書 + 基本設計書 + 詳細設計書）の後、以下 4 本の Sub-issue を `gh issue create` で一括発行する**。

| Sub-issue 名 | スコープ | 依存関係 |
|------------|---------|---------|
| `feat(dev-workflow): introduce just as task runner` | `justfile` 作成、CI ワークフロー（`lint.yml` 等）を `just <recipe>` 呼び出しへ統一。`cargo install --locked just` を前提化 | なし（先行着手可） |
| `feat(dev-workflow): add lefthook for local git hooks` | `lefthook.yml` 作成、`pre-commit` / `pre-push` / `commit-msg` 定義、失敗メッセージで `just <recipe>` を案内 | 上記 `just` 導入に依存（フックから `just` レシピを呼ぶため） |
| `feat(dev-workflow): add cross-platform setup scripts` | `scripts/setup.sh` / `scripts/setup.ps1` 作成。`cargo install --locked just lefthook convco` と `lefthook install` を冪等に実行 | 上記 `lefthook` 導入に依存 |
| `docs(dev-workflow): update README and CONTRIBUTING for local-first quality workflow` | README / CONTRIBUTING 更新、`--no-verify` 禁止明文化、`just` レシピ一覧を掲載 | 上記 setup スクリプトの完成後（実際の手順が確定してから文書化） |

## 非機能要求

| 区分 | 要求 |
|-----|------|
| パフォーマンス | pre-commit は **5 秒以内**（fmt + clippy の差分のみ）、pre-push は **3 分以内**（`cargo test` の冷たい初回を除く）。キャッシュ効くケースを基準に測定 |
| 可用性 | ネットワーク断でも setup 済みの環境ではフックが動作すること（`cargo install` は初回のみ、以降はローカルバイナリ実行） |
| 保守性 | フック定義・レシピ定義・CI ワークフローの 3 層で**同一コマンド**を参照すること（DRY）。変更は `justfile` 一箇所で反映 |
| 可搬性 | Windows/macOS/Linux の 3 OS すべてで同一の `just <recipe>` が動作 |
| セキュリティ | `cargo install --locked` で `Cargo.lock` を使い再現性を担保。任意バージョン導入でサプライチェーンリスクを最小化 |
| ドキュメント性 | `just` 実行時のヘルプ（`just --list`）で全レシピと 1 行説明を自動表示。コメントをレシピ直上に記述し `--list` に反映 |

## 受入基準

| # | 基準 | 検証方法 |
|---|------|---------|
| 1 | `git clone` 直後に `scripts/setup.sh`（または `.ps1`）を 1 回実行するだけで、フックが有効化される | 新規作業ディレクトリで clone → setup → 意図的に fmt 違反を含むコミットを試み、pre-commit が阻止することを確認 |
| 2 | pre-commit が fmt / clippy の違反を検知してコミットを中断する | 同上、コミット結果の exit code が非 0 |
| 3 | pre-push が `cargo test` 失敗を検知して push を中断する | 意図的に落ちるテストを追加し `git push`、push 拒否されることを確認 |
| 4 | commit-msg が Conventional Commits 違反を検知する | `feat` や `fix` 以外の type を含まないメッセージを試し、コミットが拒否される |
| 5 | CI ワークフロー（`lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml`）が `just <recipe>` 呼び出しに統一されている | 該当 YAML の `run:` 行を grep し、直接 `cargo ...` 呼び出しが消えていることを確認 |
| 6 | setup スクリプトを 2 回連続で実行しても差分が発生せず、成功終了する（冪等） | 連続実行して exit code 0 を確認 |
| 7 | Windows / macOS / Linux の 3 OS で setup → コミット → push が同一手順で動作する | 3 OS で手動検証（将来 CI で matrix 化） |
| 8 | `--no-verify` で意図的にバイパスしたコミットを push しても CI が全ジョブで同一のチェックを再実行して落とす | GitHub Actions 実行結果で確認 |
| 9 | README / CONTRIBUTING に setup 1 ステップと `--no-verify` 禁止ポリシーが明記されている | 対応 PR の diff で確認 |
| 10 | `just --list` ですべてのレシピが 1 行説明つきで一覧表示される | `just --list` のコンソール出力で確認 |

## 扱うデータと機密レベル

本 feature はソースコードの品質検査と開発者ワークフロー整備のみが対象であり、**ユーザ秘密情報（vault / マスターパスワード / リカバリコード）には触れない**。ただし以下 2 点のセキュリティ境界に留意する。

| 区分 | 内容 | 機密レベル |
|-----|------|----------|
| 開発者ローカル環境の改変 | `git config core.hooksPath` 設定、`.git/hooks/` へのフック書込み | 低（開発者自身の作業ツリーに閉じる） |
| `cargo install` によるサプライチェーン | `just` / `lefthook` / `convco` crate の脆弱性・供給元信頼性 | 中（`cargo-deny` の `advisories` / `sources` チェックと `--locked` で緩和。ローカル開発環境限定、配布バイナリには含まれない） |
