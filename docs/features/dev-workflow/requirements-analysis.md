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

### 重要な選定確定（レビュー指摘 R1 応答）

#### 確定 R1-A: `lefthook` は GitHub Releases バイナリ + SHA256 検証で導入

`lefthook` は **Go 製で crates.io に配布されていない**。`cargo install --locked lefthook` は実行不能。よって setup スクリプト内で **GitHub Releases から OS/arch に合致するバイナリを取得し、SHA256 を setup スクリプトにピンされた値と照合**する方式を採る。バージョンピンと SHA256 は setup スクリプト冒頭に定数として定義し、アップデート時は PR で明示差分を提示する（§detailed-design §lefthook 配布経路）。`brew` / `scoop` / `winget` / `npm` を併用しない理由は「ツール固有パッケージマネージャの併用で OS×管理ツールの導線が肥大化し、README が破綻する」ため。

#### 確定 R1-B: Windows は **PowerShell 7+ 必須**（案 B 採用）

案 A（setup.ps1 で pwsh を強制導入）は権限昇格要求や winget 非搭載環境での失敗経路が多すぎ、setup の責務を超える。案 C（`just` 側で `powershell.exe` フォールバック）は Windows のみ `just` の振る舞いが変わる罠を生む。**案 B**: PowerShell 7+ を対応前提とし、`setup.ps1` 冒頭で `$PSVersionTable.PSVersion.Major -lt 7` なら Fail Fast + `winget install Microsoft.PowerShell` の導入コマンドを提示する。Windows 10 21H2 初期環境でも `winget` は OS 標準で利用可能、1 コマンドで完了するため新規参画者の導線は確保される。README 対応 OS 表に「Windows: PowerShell 7+ 必須、導入コマンドは `winget install Microsoft.PowerShell`」を明記する。

#### 確定 R1-C: Secret 検出フックを **pre-commit に追加**（gitleaks + 既存 `audit-secret-paths.sh` の 2 本立て）

PR #21 で守った secret echo 禁止契約を継承し、**git object に混入する前に水際で検知**する。fmt-check / clippy と並列で走らせることで pre-commit 総所要時間は維持しつつ、以下 2 レイヤを適用する:

| レイヤ | ツール | 対象 | 権限 |
|-------|-------|------|------|
| L1: 汎用 secret スキャン | **`gitleaks`**（Go 製 OSS、業界標準） | API キー / AWS キー / 秘密鍵 / `.env` 流出パターンの正規表現マッチ | staged diff のみ対象（高速） |
| L2: shikomi 独自契約 | **`scripts/ci/audit-secret-paths.sh`** | TC-CI-012/013/014/015（`expose_secret` / panic hook / `SqliteVaultRepository` 参照の漏洩監査） | ワークスペース全体 |

`gitleaks` は lefthook と同じ Go バイナリであり、**同一の GitHub Releases バイナリ取得 + SHA256 検証方式**で setup する。`audit-secret-paths.sh` は既存スクリプトをそのまま `just audit-secrets` から呼ぶ（スクリプト本体を本 feature で改変しない）。

#### 確定 R1-D: `audit-secret-paths.sh` の契約は設計書で凍結

検知対象と改変ポリシーは `docs/features/cli-vault-commands/test-design/ci.md` に既存。本 feature では **`scripts/ci/audit-secret-paths.sh` の中身を改変せず `just audit-secrets` レシピから引き回す**ことに限定する。検知パターン・除外ルールの改変は本 feature のスコープ外とし、別 Issue で扱う（既存契約 TC-CI-012〜015 を壊さない）。詳細設計に「スクリプトのブラックボックス扱いは禁止、引き回し専用」を明記する。

#### 確定 R1-E: `--no-verify` と git history 残留への対応

`--no-verify` は Git の設計上**技術的に止められない**（pre-commit/pre-push フックは opt-out 可能）。よって以下 2 段構えで対処する:

1. **CI 側再実行による事後検知**: push 済みコミットに対し同一 `just <recipe>` を CI で再実行。通らないコミットは PR マージ不可
2. **secret 混入時の履歴リライト手順を CONTRIBUTING に明記**: `git reset --soft HEAD~1` / `git filter-repo`（推奨、`git filter-branch` は非推奨）/ GitHub 側 secret scanning + revoke の順で対応する運用を文書化

本 Issue の Sub-issue D（ドキュメント整備）で CONTRIBUTING に履歴書換手順を追記する。

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
| REQ-DW-006 | CI との単一実行経路化 | GitHub Actions ワークフロー（`lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml` / **`windows.yml`**）を `just <recipe>` 呼び出しに統一し、ローカルと CI で同一コマンドを走らせる | 必須 |
| REQ-DW-007 | setup スクリプト（Unix） | `scripts/setup.sh`: `just` / `lefthook` / `convco` を `cargo install --locked` で導入し、`lefthook install` を実行 | 必須 |
| REQ-DW-008 | setup スクリプト（Windows） | `scripts/setup.ps1`: **PowerShell 7+ 必須**（REQ-DW-014 で明示検査）。同等の導入処理を PowerShell で実装 | 必須 |
| REQ-DW-009 | 冪等性と再実行耐性 | setup スクリプトは既にインストール済みのツールをスキップし、複数回実行しても差分を出さない | 必須 |
| REQ-DW-010 | README / CONTRIBUTING 更新 | `git clone` 後のワンステップ setup 手順、`--no-verify` 禁止ポリシー、利用可能な `just` レシピ一覧を明文化 | 必須 |
| REQ-DW-011 | `--no-verify` バイパス検知 | サーバ側 CI で全チェックを再実行し、バイパスされたコミットを必ず落とす（CI を最後の砦として維持） | 必須 |
| REQ-DW-012 | フック失敗時のメッセージ品質 | 失敗した際にユーザーが次に取るべきコマンド（例: `just fmt` で自動修正）を提示 | 必須 |
| REQ-DW-013 | Secret 混入検知 pre-commit フック | `gitleaks` による汎用 secret スキャンと `audit-secret-paths.sh` による shikomi 独自契約検証を pre-commit で並列実行する | 必須 |
| REQ-DW-014 | PowerShell 7+ 必須化 | Windows 開発者に PowerShell 7+ を必須前提とし、`setup.ps1` 冒頭で未満バージョンを検出して Fail Fast する | 必須 |
| REQ-DW-015 | lefthook バイナリの完全性検証 | `setup.{sh,ps1}` は lefthook / gitleaks バイナリを GitHub Releases からダウンロードし、setup スクリプトにピンされた SHA256 と照合。不一致なら Fail Fast | 必須 |
| REQ-DW-016 | 開発ワークフロー設定ファイルの CODEOWNERS 保護 | `lefthook.yml` / `justfile` / `scripts/setup.{sh,ps1}` / `scripts/ci/**` を CODEOWNERS で保護し、PR レビューなしの改変を不能にする | 必須 |
| REQ-DW-017 | Git 履歴からの secret リムーブ運用 | push 後に secret 混入が判明した場合の履歴書換え手順（`git filter-repo` 推奨）と GitHub 側 secret scanning + revoke 運用を CONTRIBUTING に明記 | 必須 |
| REQ-DW-018 | AI 生成フッターのコミットメッセージ混入禁止 | commit-msg フックで `🤖 Generated with Claude Code` / `Co-Authored-By: Claude <noreply@anthropic.com>` 等の AI 生成識別フッターを含むコミットメッセージを検出して reject する。CONTRIBUTING にも禁止ポリシーを明記。背景: コミット履歴はプロジェクトの真実源であり、著者情報は `author` / `committer` フィールドで表現されるため、trailer による生成元識別は重複・冗長。オーナー（`@kkm-horikawa`）の方針に準拠 | 必須 |

## Sub-issue分割計画

本 Issue #22 は調査 Issue。**設計確定（本書 + 基本設計書 + 詳細設計書）の後、以下 4 本の Sub-issue を `gh issue create` で一括発行する**。REQ-DW-001〜018 の全 18 要件をいずれかの Sub-issue に紐付け、孤児要件を作らない。

| Sub-issue 名 | 紐付く REQ | スコープ | 依存関係 |
|------------|-----------|---------|---------|
| **A**: `feat(dev-workflow): introduce just as task runner` | REQ-DW-005, 006, 011, **018（レシピ側）** | `justfile` 作成（default / fmt-check / fmt / clippy / test / test-core / test-infra / test-cli / audit / audit-secrets / check-all / commit-msg-check / **commit-msg-no-ai-footer** の全レシピを §確定 C に従って定義）。CI 5 ワークフロー（`lint` / `unit-core` / `test-infra` / `audit` / `windows`）を `just <recipe>` 呼び出しへ統一。`audit.yml` は §確定 B に従い `cargo-deny-action` を廃止し `cargo install --locked just cargo-deny` → `just audit` へ。`cargo install --locked just` を CI 前提化（Rust 製のみ、Go 製は本 Sub-issue では触らない） | なし（先行着手可） |
| **B**: `feat(dev-workflow): add lefthook for local git hooks with secret scan` | REQ-DW-001, 002, 003, 004, 012, 013, 016, **018（フック側）** | `lefthook.yml` 作成（pre-commit は `fmt-check` / `clippy` / `audit-secrets` の 3 並列、pre-push は `test`、**commit-msg は `convco` と `no-ai-footer` の 2 コマンド並列**）。`fail_text` は詳細設計書の MSG-DW-001〜004, 010, **013** 確定文言をそのまま静的文字列として埋め込み（2 行構造）。`.github/CODEOWNERS` に 5 パス（`/lefthook.yml` / `/justfile` / `/scripts/setup.sh` / `/scripts/setup.ps1` / `/scripts/ci/`）を追記（REQ-DW-016） | A に依存（フックから `just` レシピを呼ぶため） |
| **C**: `feat(dev-workflow): add cross-platform setup scripts with SHA256 verification` | REQ-DW-007, 008, 009, 014, 015 | `scripts/setup.sh` / `scripts/setup.ps1` 作成。Rust 製（`just` / `convco`）は `cargo install --locked` で導入、**Go 製（`lefthook` / `gitleaks`）は GitHub Releases からバイナリ取得 + SHA256 ピン定数で改ざん検証**（REQ-DW-015）。`setup.ps1` 冒頭で PowerShell 7+ を検査し未満なら Fail Fast + `winget install Microsoft.PowerShell` 案内（REQ-DW-014、MSG-DW-011）。`.git/` 検査・`rustc`/`cargo` 検査・冪等実行のすべてを実装。ピン定数の初期値（`LEFTHOOK_VERSION` / `LEFTHOOK_SHA256_*` / `GITLEAKS_VERSION` / `GITLEAKS_SHA256_*`）は本 Sub-issue 実装時に upstream の公式 `checksums.txt` から転記 | B に依存 |
| **D**: `docs(dev-workflow): update README and CONTRIBUTING for local-first quality workflow` | REQ-DW-010, 017, **018（ポリシー側）** | README 更新（setup 1 ステップ、対応 OS 表に「Windows: PowerShell 7+ 必須」追記、`winget` コマンド案内）。CONTRIBUTING 更新（`--no-verify` 禁止ポリシー／MSG-DW-007／`just` レシピ一覧／**§Secret 混入時の緊急対応**: 即 revoke → `git filter-repo` → GitHub secret scanning resolve の 3 段手順を REQ-DW-017 に従い明文化。`main`/`develop` への force-push は引き続き禁止、feature ブランチ限定で実施する旨も明記／**§AI 生成フッターの禁止**: `Co-Authored-By: Claude` / `🤖 Generated with Claude Code` 等の trailer をコミットメッセージに含めないポリシーを REQ-DW-018 に従い明文化） | C に依存（実際の手順が確定してから文書化） |

**`cargo install --locked lefthook` という表現は本計画から撤去済み**。確定 R1-A（§議論結果）および REQ-DW-015（§機能一覧）により、lefthook / gitleaks は**GitHub Releases + SHA256 検証経路でのみ導入**する。

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
| 5 | CI ワークフロー（`lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml` / **`windows.yml`**）が `just <recipe>` 呼び出しに統一されている | 該当 YAML の `run:` 行を grep し、直接 `cargo ...` 呼び出しが消えていることを確認 |
| 6 | setup スクリプトを 2 回連続で実行しても差分が発生せず、成功終了する（冪等） | 連続実行して exit code 0 を確認 |
| 7 | Windows / macOS / Linux の 3 OS で setup → コミット → push が同一手順で動作する | 3 OS で手動検証（将来 CI で matrix 化） |
| 8 | `--no-verify` で意図的にバイパスしたコミットを push しても CI が全ジョブで同一のチェックを再実行して落とす | GitHub Actions 実行結果で確認 |
| 9 | README / CONTRIBUTING に setup 1 ステップと `--no-verify` 禁止ポリシーが明記されている | 対応 PR の diff で確認 |
| 10 | `just --list` ですべてのレシピが 1 行説明つきで一覧表示される | `just --list` のコンソール出力で確認 |
| 11 | pre-commit / pre-push / commit-msg の各失敗時に stderr の**最終行が `[FAIL] <原因要約>` → 次行に `次のコマンド: just <recipe>` の 2 行構造**で表示される（REQ-DW-012 の検証基準） | 意図的に違反コミットを試み、stderr 末尾 2 行を assertion で確認。MSG-DW-001〜004 の文言が一致 |
| 12 | `gitleaks` または `audit-secret-paths.sh` のいずれかで secret 混入を含むコミットが阻止される（REQ-DW-013 の検証基準） | テスト用に `AWS_SECRET_ACCESS_KEY="AKIA..."` 相当の擬似値を含むファイルを staged し、コミットが exit 非 0 で中止されることを確認 |
| 13 | `setup.ps1` を PowerShell 5.1 で起動した場合、exit 非 0 + MSG-DW-011 の Fail Fast が発火する（REQ-DW-014 の検証基準） | Windows 10 21H2 の既定 PowerShell 5.1 で起動 → 失敗メッセージの文言確認 |
| 14 | `setup.{sh,ps1}` が lefthook / gitleaks ダウンロード時に SHA256 検証を行い、改ざんされたバイナリを拒否する（REQ-DW-015 の検証基準） | ダウンロード後のファイルをテスト用に書換え → 再 setup で MSG-DW-012 が発火することを確認 |
| 15 | `.github/CODEOWNERS` に `/lefthook.yml` / `/justfile` / `/scripts/setup.sh` / `/scripts/setup.ps1` / `/scripts/ci/` が登録され、該当 PR でオーナーレビューが要求される（REQ-DW-016 の検証基準） | CODEOWNERS を grep で確認、該当ファイル変更 PR の GitHub UI でレビュー要求表示を確認 |
| 16 | CONTRIBUTING.md に **§Secret 混入時の緊急対応** 節が存在し、以下 3 項目が明記されている（REQ-DW-017 の検証基準）: (a) 該当キーを発行元で即 revoke (b) `git filter-repo --path <file> --invert-paths` による履歴書換えと feature ブランチ限定 force-push（`main` / `develop` への force-push は禁止を明記）(c) GitHub Support への cache purge 依頼と secret scanning alert の resolve | 対応 PR の CONTRIBUTING.md 該当節を diff 確認。見出しと 3 項目の存在を grep で検証 |
| 17 | commit-msg フックが以下 3 パターンのいずれかを含むコミットメッセージを検出して reject する（REQ-DW-018 の検証基準）: (a) `🤖 Generated with Claude Code` または `🤖` + `Generated with` + `Claude` を含む行 (b) `Co-Authored-By:` で始まり `@anthropic.com` ドメインを含む trailer (c) `Co-Authored-By:` で始まり `Claude` を name に含む trailer | 意図的に各パターンを含むコミットを試み、exit 非 0 で中止され MSG-DW-013 が stderr に出力されることを確認。大文字小文字は無視（case-insensitive 照合） |

## 扱うデータと機密レベル

本 feature はソースコードの品質検査と開発者ワークフロー整備のみが対象であり、**ユーザ秘密情報（vault / マスターパスワード / リカバリコード）には触れない**。ただし以下 2 点のセキュリティ境界に留意する。

| 区分 | 内容 | 機密レベル |
|-----|------|----------|
| 開発者ローカル環境の改変 | `git config core.hooksPath` 設定、`.git/hooks/` へのフック書込み | 低（開発者自身の作業ツリーに閉じる） |
| `cargo install` によるサプライチェーン | `just` / `lefthook` / `convco` crate の脆弱性・供給元信頼性 | 中（`cargo-deny` の `advisories` / `sources` チェックと `--locked` で緩和。ローカル開発環境限定、配布バイナリには含まれない） |
