# 要件定義書

<!-- feature単位で1ファイル。新規featureならテンプレートコピー、既存featureなら既存ファイルをREAD→EDIT -->
<!-- 配置先: docs/features/dev-workflow/requirements.md -->

## 機能要件

### REQ-DW-001: フックツール導入（lefthook）

| 項目 | 内容 |
|------|------|
| 入力 | `lefthook install` コマンド実行、または setup スクリプト経由の呼び出し |
| 処理 | `lefthook.yml` を読み、`.git/hooks/{pre-commit,pre-push,commit-msg}` を生成・上書き |
| 出力 | `.git/hooks/` 以下に lefthook が管理するラッパスクリプトが配置され、以後の `git` 操作で自動発火 |
| エラー時 | lefthook バイナリ未検出 → 「`cargo install --locked lefthook` を実行してください」のヒントつき exit 非 0 |

### REQ-DW-002: pre-commit フック

| 項目 | 内容 |
|------|------|
| 入力 | `git commit` 実行（`--no-verify` 非指定） |
| 処理 | `just fmt-check` と `just clippy` を **並列**実行。staged ファイルのみを対象とするのではなく、**ワークツリー全体**に対し実行（ワークスペース全体 lints が workspace.lints で定義されており、staged のみだと漏れるため） |
| 出力 | 全チェック成功: コミット続行 / いずれか失敗: コミット中止、失敗箇所と復旧コマンド（例: `just fmt`）を stderr 表示 |
| エラー時 | exit 非 0 でコミット中止。MSG-DW-001 / MSG-DW-002 を表示 |

### REQ-DW-003: pre-push フック

| 項目 | 内容 |
|------|------|
| 入力 | `git push` 実行（`--no-verify` 非指定） |
| 処理 | `just test` を実行（`cargo test --workspace` 相当） |
| 出力 | 成功: push 続行 / 失敗: push 中止、失敗テスト名と復旧手順（`just test` で再現）を stderr 表示 |
| エラー時 | exit 非 0 で push 中止。MSG-DW-003 を表示 |

### REQ-DW-004: commit-msg フック

| 項目 | 内容 |
|------|------|
| 入力 | コミットメッセージファイル（`.git/COMMIT_EDITMSG`） |
| 処理 | `convco check` でメッセージを Conventional Commits 規約に照合。merge / revert / fixup コミットは convco 側のデフォルト挙動で素通り |
| 出力 | 適合: コミット続行 / 不適合: コミット中止、許可される type 一覧（CONTRIBUTING.md §コミット規約参照）を stderr 表示 |
| エラー時 | exit 非 0。MSG-DW-004 を表示 |

### REQ-DW-005: タスクランナー導入（just）

| 項目 | 内容 |
|------|------|
| 入力 | `just <recipe>` または `just --list` |
| 処理 | `justfile` を読み、該当レシピのコマンドを実行。レシピ内からは `cargo ...` / `bash scripts/ci/...` を直接呼ぶ |
| 出力 | レシピ実行結果。`just --list` は全レシピを 1 行コメントつきで列挙 |
| エラー時 | レシピ未定義: just が usage を表示し exit 非 0 |

### REQ-DW-006: CI との単一実行経路化

| 項目 | 内容 |
|------|------|
| 入力 | GitHub Actions workflow job のステップ `run:` |
| 処理 | `lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml` / `windows.yml` の **すべての直接的な `cargo ...` 呼び出しを `just <recipe>` に置換**。各 workflow は最初のステップに「`cargo install --locked just`」を追加（Swatinem/rust-cache が効くため 2 回目以降は高速） |
| 出力 | ローカルフック / 手動 `just <recipe>` / CI が同一レシピ呼び出し経路で実行される状態 |
| エラー時 | `just` バイナリ未インストール: workflow ステップが fail（CI セーフティネット） |

### REQ-DW-007: setup スクリプト（Unix, `scripts/setup.sh`）

| 項目 | 内容 |
|------|------|
| 入力 | `bash scripts/setup.sh` 実行（引数なし） |
| 処理 | 1. `rustc` / `cargo` バージョン確認 → 満たさない場合エラー 2. `just` / `lefthook` / `convco` の順で `cargo install --locked` 3. 既存バイナリは **`--force` を付けず再導入スキップ**（冪等）4. `lefthook install` 実行 5. 成功ログ表示 |
| 出力 | exit 0 で成功、標準出力に各ステップの完了メッセージ |
| エラー時 | 途中失敗で即 exit（`set -euo pipefail`）、失敗箇所を stderr に表示 |

### REQ-DW-008: setup スクリプト（Windows, `scripts/setup.ps1`）

| 項目 | 内容 |
|------|------|
| 入力 | `pwsh scripts/setup.ps1` 実行（引数なし）。PowerShell 5.1+ / PowerShell 7+ 両対応 |
| 処理 | REQ-DW-007 と同一ロジックを PowerShell で表現。`$ErrorActionPreference = 'Stop'` で Fail Fast |
| 出力 | REQ-DW-007 と同様 |
| エラー時 | REQ-DW-007 と同様 |

### REQ-DW-009: 冪等性

| 項目 | 内容 |
|------|------|
| 入力 | setup スクリプトの連続再実行 |
| 処理 | 各ツールについて `command -v <tool>` / `Get-Command <tool>` で存在確認し、存在時は `cargo install` をスキップ。`lefthook install` は冪等（既存 `.git/hooks/` を上書き） |
| 出力 | 2 回目以降は「既にインストール済み」ログを出して 0 秒で完了 |
| エラー時 | 初回エラーは通常通り Fail Fast |

### REQ-DW-010: README / CONTRIBUTING 更新

| 項目 | 内容 |
|------|------|
| 入力 | 設計確定と Sub-issue C 完了後の PR |
| 処理 | README.md §ビルド方法、CONTRIBUTING.md §開発環境セットアップを更新。1 コマンドで setup 完了することを冒頭で明示 |
| 出力 | 両ファイルに setup 手順、利用可能な `just` レシピ、`--no-verify` 禁止ポリシーが追記された状態 |
| エラー時 | 該当なし（ドキュメント変更のみ） |

### REQ-DW-011: `--no-verify` バイパス検知

| 項目 | 内容 |
|------|------|
| 入力 | GitHub への push（`--no-verify` の有無に関わらず） |
| 処理 | CI の `lint` / `unit-core` / `test-infra` / `audit` が同一レシピを再実行 |
| 出力 | push 後の CI ランで結果が可視化される。規約違反は PR レビューで却下 |
| エラー時 | CI ジョブが非 0 終了。PR マージ不可 |

### REQ-DW-012: フック失敗時のメッセージ品質

| 項目 | 内容 |
|------|------|
| 入力 | pre-commit / pre-push / commit-msg のいずれかの失敗 |
| 処理 | lefthook の `fail_text` フィールドで、次に取るべきコマンド（`just fmt` 等）を明示 |
| 出力 | stderr に復旧コマンドが 1 行で示される |
| エラー時 | 該当なし（メッセージ表示自体は exit コードに影響しない） |

## 画面・CLI仕様

### `just` レシピ一覧（初期定義）

| レシピ名 | 概要 | 対応する CI 相当 |
|---------|------|----------------|
| `just` | 引数なしで `just --list` を実行する default レシピ | — |
| `just fmt-check` | `cargo fmt --all -- --check` | `lint.yml` step |
| `just fmt` | `cargo fmt --all`（自動修正） | — |
| `just clippy` | `cargo clippy --all-targets --all-features` | `lint.yml` step |
| `just test` | `cargo test --workspace` | `unit-core.yml` + `test-infra.yml` 両方相当 |
| `just test-core` | `cargo test -p shikomi-core` | `unit-core.yml` |
| `just test-infra` | `cargo test -p shikomi-infra` | `test-infra.yml` |
| `just test-cli` | `cargo test -p shikomi-cli` | `test-infra.yml` に相当する CLI 群 |
| `just audit` | `cargo deny check advisories licenses bans sources` + `bash scripts/ci/audit-secret-paths.sh` | `audit.yml` |
| `just check-all` | `fmt-check` → `clippy` → `test` → `audit` を順次実行（最終確認用） | 全ワークフロー相当 |
| `just commit-msg-check FILE` | `convco check --from-stdin < FILE` 相当。commit-msg フックから呼ばれる | — |

### setup スクリプトの CLI

| スクリプト | 呼び出し | 引数 | 出力 |
|----------|---------|-----|------|
| `scripts/setup.sh` | `bash scripts/setup.sh` | なし | インストール進捗ログ、最終行に「Setup complete.」 |
| `scripts/setup.ps1` | `pwsh scripts/setup.ps1` | なし | 同上 |

### `lefthook.yml` の CLI 契約

本書では設定構造の外形のみ記載（詳細設計書で詳述）。ユーザ可視の CLI は無く、`git commit` / `git push` から暗黙に発火する。

## API仕様

該当なし — 理由: 本 feature は開発者ツールチェーンの整備であり、ランタイム API を提供しない。

## データモデル

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| `lefthook.yml` | `pre-commit.commands` | map<string, {run: string, fail_text: string, parallel: bool}> | 各コマンドの `run` は `just <recipe>` を呼ぶ | `justfile` のレシピ名と一致 |
| `lefthook.yml` | `pre-push.commands` | 同上 | `fail_text` 必須 | 同上 |
| `lefthook.yml` | `commit-msg.commands` | 同上 | 引数に `{1}` でメッセージファイルパスを受ける | `justfile` の `commit-msg-check` |
| `justfile` | 先頭の `set` 宣言 | 平文 | `set dotenv-load := false`, `set windows-shell := ["pwsh", "-Cu", "-c"]`（Windows でも `sh` 非依存） | — |
| `justfile` | 各 recipe | 名前・引数・本文 | 本文 1 行あたり 1 コマンド。複数行は `\` 継続ではなく recipe 分割で表現 | `lefthook.yml` / `.github/workflows/*.yml` が参照 |
| `scripts/setup.sh` | シェバン | 文字列 | `#!/usr/bin/env bash` | — |
| `scripts/setup.sh` | 冒頭 | `set -euo pipefail` | Fail Fast 必須 | — |
| `scripts/setup.ps1` | 冒頭 | `$ErrorActionPreference = 'Stop'` | Fail Fast 必須 | — |

## ユーザー向けメッセージ一覧

| ID | 種別 | メッセージ（要旨。正確な文言は詳細設計で確定） | 表示条件 |
|----|------|----------------------------------------|---------|
| MSG-DW-001 | エラー | `cargo fmt` 違反検出。`just fmt` で自動修正してください。 | pre-commit で fmt-check 失敗 |
| MSG-DW-002 | エラー | `cargo clippy` 違反検出。`just clippy` の出力に従って修正してください。 | pre-commit で clippy 失敗 |
| MSG-DW-003 | エラー | `cargo test` 失敗。`just test` でローカル再現後に push してください。 | pre-push で test 失敗 |
| MSG-DW-004 | エラー | コミットメッセージが Conventional Commits に準拠していません。CONTRIBUTING.md §コミット規約を参照してください。 | commit-msg で convco 失敗 |
| MSG-DW-005 | 成功 | Setup complete. フックは有効化されました。 | setup スクリプト正常終了 |
| MSG-DW-006 | 情報 | `<tool>` は既にインストール済みです。スキップします。 | setup 冪等実行時 |
| MSG-DW-007 | 警告 | `--no-verify` はコミット/push 規約で禁止されています。CI が同一チェックを再実行します。 | CONTRIBUTING.md に静的記載（動的表示ではない） |
| MSG-DW-008 | エラー | Rust toolchain が未検出です。https://rustup.rs/ からインストールしてください。 | setup で `rustc --version` 失敗 |
| MSG-DW-009 | エラー | `lefthook install` 失敗。`.git/` ディレクトリが存在するリポジトリルートで実行してください。 | setup で lefthook install 失敗 |

## 依存関係

| 区分 | 依存 | バージョン方針 | 導入経路 | 備考 |
|-----|------|-------------|---------|------|
| 開発ツール | `just` | `cargo install --locked just` で最新安定 | ローカル開発者環境 / CI ランナー | バイナリサイズ小、Rust 製、Windows ネイティブ対応 |
| 開発ツール | `lefthook` | `cargo install --locked lefthook`（または GitHub Releases からバイナリ取得でも可） | 同上 | Go 製、並列実行、YAML 設定 |
| 開発ツール | `convco` | `cargo install --locked convco` | 同上 | Rust 製、Conventional Commits 専用 |
| Rust toolchain | `rustc` / `cargo` | stable（`rust-toolchain.toml` に準拠）、MSRV 1.80.0 | rustup | 既存 |
| Git | Git 2.9+ | setup ガイドに明記 | 各 OS パッケージマネージャ | `core.hooksPath` は Git 2.9 で導入済み。本 feature は lefthook 経由なので `.git/hooks/` 既定パスを使う |
| GitHub Actions | `Swatinem/rust-cache@v2` | 既存利用 | 既存 | `cargo install` のキャッシュを兼ねる |
| CONTRIBUTING / README | 既存 Markdown | — | — | §REQ-DW-010 で更新 |

**配布バイナリ（shikomi 本体）への影響**: 本 feature のツールは **すべて開発者ツールチェーンに閉じる**。`shikomi-core` / `shikomi-infra` / `shikomi-cli` / `shikomi-daemon` / `shikomi-gui` の build artifact および `Cargo.lock` には一切混入しない（`[workspace.dependencies]` に追加しない、`dev-dependencies` にも追加しない）。よって `docs/architecture/tech-stack.md` §4.3.2 の「暗号クリティカル crate ignore 禁止リスト」とは独立な領域として扱う。
