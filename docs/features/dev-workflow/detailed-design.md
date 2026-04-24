# 詳細設計書

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature単位で1ファイル。新規featureならテンプレートコピー、既存featureなら既存ファイルをREAD→EDIT -->
<!-- 配置先: docs/features/dev-workflow/detailed-design.md -->

## 記述ルール（必ず守ること）

詳細設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

## クラス設計（詳細）

本 feature は Rust クラスを持たないため、各設定ファイルの**構造契約**を詳細レベルで示す。各ファイルの実装（最終的な YAML / justfile / sh / ps1）は Sub-issue の実装 PR で書き、本設計書とは**構造契約と意図**のみで整合を取る。

### 設定ファイルの構造契約

```mermaid
classDiagram
    class Justfile {
        +set windows-shell
        +default: list
        +fmt-check() exit_code
        +fmt() exit_code
        +clippy() exit_code
        +test() exit_code
        +test-core() exit_code
        +test-infra() exit_code
        +test-cli() exit_code
        +audit() exit_code
        +check-all() exit_code
        +commit-msg-check(file) exit_code
    }
    class LefthookYml {
        +preCommitParallel
        +preCommit_fmtCheck
        +preCommit_clippy
        +prePush_test
        +commitMsg_convco
    }
    class SetupSh {
        +shebang: #!/usr/bin/env bash
        +strict: set -euo pipefail
        +step_check_toolchain() exit_code
        +step_install_just() exit_code
        +step_install_lefthook() exit_code
        +step_install_convco() exit_code
        +step_lefthook_install() exit_code
    }
    class SetupPs1 {
        +strict: ErrorActionPreference=Stop
        +step_check_toolchain()
        +step_install_just()
        +step_install_lefthook()
        +step_install_convco()
        +step_lefthook_install()
    }
    class WorkflowLintYml {
        +step_checkout
        +step_toolchain
        +step_rust_cache
        +step_install_just
        +step_just_fmt_check
        +step_just_clippy
    }
    class WorkflowUnitCoreYml {
        +step_checkout
        +step_toolchain
        +step_rust_cache
        +step_install_just
        +step_just_test_core
    }
    class WorkflowTestInfraYml {
        +step_checkout
        +step_toolchain
        +step_rust_cache
        +step_install_just
        +step_just_test_infra
    }
    class WorkflowAuditYml {
        +step_checkout
        +step_install_just
        +step_just_audit
    }
    class WorkflowWindowsYml {
        +shell: pwsh
        +step_checkout
        +step_toolchain
        +step_rust_cache
        +step_install_just
        +step_just_test_infra_windows
    }

    LefthookYml ..> Justfile : run:
    WorkflowLintYml ..> Justfile : run:
    WorkflowUnitCoreYml ..> Justfile : run:
    WorkflowTestInfraYml ..> Justfile : run:
    WorkflowAuditYml ..> Justfile : run:
    WorkflowWindowsYml ..> Justfile : run:
    SetupSh ..> LefthookYml : lefthook install
    SetupPs1 ..> LefthookYml : lefthook install
```

### 設計判断の補足

#### なぜ `justfile` をルート直下に置くか

- `just` は実行ディレクトリから親方向へ `justfile` を探索する。リポジトリルートに置くことで、サブディレクトリ（`crates/shikomi-cli/` 等）からも `just <recipe>` を呼べる
- `.cargo/config.toml` にエイリアスを書く選択肢もあるが、cargo 以外のツール（`bash scripts/ci/audit-secret-paths.sh`）を呼ぶ統一インタフェースになり得ない（cargo alias は `cargo <alias>` 経由のみ）

#### なぜ `set windows-shell := ["pwsh", "-Cu", "-c"]` を宣言するか

- `just` のデフォルトは Windows で `sh` を探しに行く。Git Bash がない環境で壊れるため `pwsh`（PowerShell 7+）を明示する
- `pwsh` 不在の Windows 10 21H2 初期環境では `powershell.exe`（Windows PowerShell 5.1）で代替するよう README で案内。詳細は REQ-DW-008 の実装 Sub-issue で調整

#### なぜ `lefthook.yml` で `parallel: true` を `pre-commit` に設定するか

- `fmt-check`（秒）と `clippy`（分）は独立かつ並列化可能
- lefthook のデフォルトは並列非実行。明示宣言で体感時間を短縮
- `pre-push` の `cargo test` は単一レシピなので `parallel` 指定不要

#### なぜ `fail_text` を lefthook 側で持ち、justfile 側で持たないか

- `justfile` は CI からも呼ばれる。CI で fmt 違反を「`just fmt` で修正しろ」と表示するのは文脈ミスマッチ（CI は自動判定が仕事、人間は PR 結果画面で見る）
- lefthook の `fail_text` は **ローカル開発者向けメッセージ**として適切な配置層

#### なぜ `cargo install --locked` を `cargo install` より優先するか

- `--locked` 指定で crate 側の `Cargo.lock` を使う。指定なしだと最新依存を解決し直し、毎回異なるバイナリになりうる（再現性崩壊）
- `just` / `lefthook` / `convco` いずれも `Cargo.lock` を crates.io 配布物に同梱する設計

#### なぜ setup スクリプトで `--force` を付けないか

- 冪等性の担保（REQ-DW-009）。2 回目以降の実行で `cargo install` が「already up to date」で 0 秒終了する
- バージョン更新は明示的な `cargo install --force <tool>` を開発者に任せる（setup スクリプトの責務外）

#### なぜ `lefthook install` を setup に含めるか

- lefthook は **`lefthook install` を実行して初めて** `.git/hooks/` にラッパを配置する
- setup スクリプト内で 1 コマンドだけなので、開発者に別ステップを要求しない KISS 設計

#### なぜ `core.hooksPath` を変更しないか

- lefthook は `.git/hooks/` の既定パスに書き込む設計。`core.hooksPath` を別ディレクトリに向ける必要はない
- `core.hooksPath` を変更すると、他のツール（Git GUI クライアント等）との相互作用で予期せぬ挙動が出うる。デフォルト経路を保つことで想定外副作用を避ける（KISS）

#### CI ワークフローの書き換え方針

- 既存 8 本中、本 feature が編集対象とするのは **チェック系 5 本**（`lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml` / `windows.yml`）
- **編集対象外**: `branch-policy.yml` / `pr-title-check.yml` / `back-merge-check.yml` は Git 運用系であり `cargo` / `just` を呼ばない
- 編集方針: 直接的な `cargo fmt --all -- --check` / `cargo clippy ...` / `cargo test -p ...` の `run:` を **削除し**、`run: just <recipe>` に置換。cargo-deny については `EmbarkStudios/cargo-deny-action@v2` を使っているため、`just audit` から `cargo deny ...` を呼ぶ形に統一するか、action を残すかを Sub-issue A の実装で判定する（action 継続利用の場合は `audit.yml` のみ例外扱い）
- `Swatinem/rust-cache@v2` は保持。`cargo install --locked just` のキャッシュを兼ねる

#### `just audit` と `cargo-deny-action` の関係（運用判断ポイント）

Sub-issue A 着手時に以下 2 案を比較して Plan 採用根拠を記録する:

| 案 | `just audit` 定義 | CI の `audit.yml` |
|----|-----------------|----------------|
| **案 1: action 継続** | `just audit` は `cargo deny check advisories licenses bans sources` と `bash scripts/ci/audit-secret-paths.sh` を直接呼ぶ | `cargo-deny-action@v2` を使い続け、`audit-secret-paths.sh` のみ別ステップで実行 |
| **案 2: action 廃止** | 案 1 と同じ | `just audit` を呼ぶだけ |

**推奨は案 2**: DRY（ローカル / CI が完全一致）、cargo-deny のバージョンを `Cargo.lock` 経由で一元管理できる。`cargo-deny-action` の軽微な入力変換ロジックが欲しいケースでのみ案 1 を選ぶ。

## データ構造

本 feature は永続化データを持たないため、主要な **設定ファイルのキー構造**を表形式で定義する。実ファイルは Sub-issue の実装 PR で作成される。

### `lefthook.yml` のキー構造

| キー | 型 | 用途 | デフォルト値 / 採用値 |
|-----|---|------|-----------------|
| `pre-commit.parallel` | bool | fmt-check と clippy の並列実行可否 | `true` |
| `pre-commit.commands.fmt-check.run` | string | 実行コマンド | `just fmt-check` |
| `pre-commit.commands.fmt-check.fail_text` | string | 失敗時メッセージ（静的） | MSG-DW-001 |
| `pre-commit.commands.clippy.run` | string | 実行コマンド | `just clippy` |
| `pre-commit.commands.clippy.fail_text` | string | 失敗時メッセージ（静的） | MSG-DW-002 |
| `pre-push.commands.test.run` | string | 実行コマンド | `just test` |
| `pre-push.commands.test.fail_text` | string | 失敗時メッセージ（静的） | MSG-DW-003 |
| `commit-msg.commands.convco.run` | string | 実行コマンド（`{1}` はメッセージファイルパス） | `just commit-msg-check {1}` |
| `commit-msg.commands.convco.fail_text` | string | 失敗時メッセージ（静的） | MSG-DW-004 |
| `skip_output` | 配列 | 出力抑制対象。`meta` / `summary` 等で lefthook の雑多な出力を抑える | 未設定（Sub-issue B で最小化の要否を判定） |

### `justfile` のレシピ契約（初期定義）

| レシピ名 | 引数 | 実行コマンド（論理） | 対応する CI ワークフロー |
|---------|-----|------------------|--------------------|
| `default` | なし | `just --list` を呼ぶ | — |
| `fmt-check` | なし | `cargo fmt --all -- --check` | `lint.yml` step |
| `fmt` | なし | `cargo fmt --all`（自動修正） | — |
| `clippy` | なし | `cargo clippy --all-targets --all-features`（workspace.lints.clippy の設定を尊重、`-D warnings` は付けない。既存 `lint.yml` と同方針） | `lint.yml` step |
| `test` | なし | `cargo test --workspace`（`--all-features` は将来の feature flag 追加に備えて Sub-issue A で判断） | `unit-core.yml` + `test-infra.yml` |
| `test-core` | なし | `cargo test -p shikomi-core` | `unit-core.yml` |
| `test-infra` | なし | `cargo test -p shikomi-infra` | `test-infra.yml` |
| `test-cli` | なし | `cargo test -p shikomi-cli` | `test-infra.yml` 相当の CLI テスト |
| `audit` | なし | `cargo deny check advisories licenses bans sources` → `bash scripts/ci/audit-secret-paths.sh`（POSIX 互換。Windows では `pwsh` から `bash` 呼出しまたは `scripts/ci/audit-secret-paths.ps1` を Sub-issue A で追加検討） | `audit.yml` |
| `check-all` | なし | `fmt-check` → `clippy` → `test` → `audit` を順次呼ぶ（失敗時に途中終了） | 全 CI 相当 |
| `commit-msg-check` | `file` 引数 1 個 | `convco check --commit-msg-file {{file}}`（convco の実 CLI 仕様は Sub-issue B で最終確認） | — |

### `scripts/setup.sh` のステップ契約

| ステップ | 処理 | Fail Fast 条件 |
|---------|-----|-------------|
| 1. shebang / strict mode | `#!/usr/bin/env bash` + `set -euo pipefail` | — |
| 2. cwd 検査 | リポジトリルート（`.git/` が存在）で実行されているか | 非リポジトリで実行 → exit 非 0、MSG-DW-009 |
| 3. Rust toolchain 検査 | `rustc --version` / `cargo --version` の成功可否 | 失敗 → MSG-DW-008 |
| 4. `just` 導入 | `command -v just` 成否で `cargo install --locked just` の要否判定 | `cargo install` 失敗 → exit 非 0 |
| 5. `lefthook` 導入 | 同上 | 同上 |
| 6. `convco` 導入 | 同上 | 同上 |
| 7. `lefthook install` | `.git/hooks/` へラッパ配置 | 失敗 → MSG-DW-009 |
| 8. 完了ログ | MSG-DW-005 を表示 | — |

### `scripts/setup.ps1` のステップ契約

setup.sh と **同一のステップ番号・同一の責務**。差分のみ表記。

| ステップ | sh 版 | ps1 版（差分） |
|---------|-----|------------|
| 1 | `#!/usr/bin/env bash` + `set -euo pipefail` | 冒頭 `$ErrorActionPreference = 'Stop'`。`Set-StrictMode -Version Latest` を併用 |
| 2 | `.git/` 検査 | `Test-Path .git` |
| 3 | `rustc --version` | `rustc --version`（PowerShell で呼び出し、`$LASTEXITCODE` 非 0 を検査） |
| 4-6 | `command -v <tool>` | `Get-Command <tool> -ErrorAction SilentlyContinue` |
| 7-8 | 同一 | 同一 |

### `.github/workflows/*.yml` の編集契約

| ファイル | 旧 `run:` | 新 `run:` | 備考 |
|---------|---------|---------|------|
| `lint.yml` | `cargo fmt --all -- --check` / `cargo clippy --all-targets --all-features` | `just fmt-check` / `just clippy`（ステップ前に `cargo install --locked just` 追加） | 既存の `workspace.lints.clippy` 設定尊重方針は justfile 側で継承 |
| `unit-core.yml` | `cargo test -p shikomi-core` | `just test-core` | 同上 |
| `test-infra.yml` | `cargo test -p shikomi-infra` | `just test-infra` | 同上 |
| `audit.yml` | `EmbarkStudios/cargo-deny-action@v2` + `bash scripts/ci/audit-secret-paths.sh` | `just audit`（推奨 = 案 2） | `cargo-deny-action` 廃止可否は Sub-issue A で確定 |
| `windows.yml` | （既存の Windows 向け cargo test） | `just test-infra`（Windows ランナー上、`windows-shell` が pwsh で機能） | `pwsh` に合わせた呼出しは `just` が吸収 |

## ビジュアルデザイン

該当なし — 理由: 本 feature は CLI のみで GUI 要素を持たない。フック失敗時のテキスト出力は `lefthook` / `just` / `cargo` のデフォルト配色・書式に従う。

---

## 出典・参考

- lefthook 公式ドキュメント: https://lefthook.dev/ / https://github.com/evilmartians/lefthook
- just 公式ドキュメント: https://just.systems/ / https://github.com/casey/just
- convco: https://github.com/convco/convco
- Git `core.hooksPath` ドキュメント: https://git-scm.com/docs/githooks#_core_hookspath
- `cargo install --locked` 挙動: https://doc.rust-lang.org/cargo/commands/cargo-install.html
- Conventional Commits 仕様: https://www.conventionalcommits.org/ja/v1.0.0/
- `cargo-deny` 公式: https://embarkstudios.github.io/cargo-deny/
- GitHub Actions `Swatinem/rust-cache@v2`: https://github.com/Swatinem/rust-cache
