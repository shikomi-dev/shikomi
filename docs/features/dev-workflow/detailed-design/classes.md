# 詳細設計書 — dev-workflow / クラス設計

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- 配置先: docs/features/dev-workflow/detailed-design/classes.md -->
<!-- 兄弟: ./index.md, ./messages.md, ./setup.md, ./data-structures.md, ./scripts.md -->

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

### レビュー指摘で確定した 5 項目（先送り撤廃）

#### 確定 A: Windows の shell 選定 — **PowerShell 7+ 必須化**

`justfile` は `set windows-shell := ["pwsh", "-Cu", "-c"]` を宣言する。`powershell.exe`（5.1）フォールバックは**採用しない**。根拠:

- 案 A（setup.ps1 で pwsh を強制導入）は権限昇格要求・winget 非搭載環境での失敗経路で導線破綻
- 案 C（`just` 側で `powershell.exe` フォールバック）は Windows のみ振る舞い差分を抱え、`pwsh` と 5.1 の構文互換性差（`$PSVersionTable` 直参照、strict mode の挙動等）で潜在的バグ源
- 案 B 採用: `setup.ps1` 冒頭の PowerShell バージョン検査で Fail Fast + winget 導入案内（MSG-DW-011）

検出: `$PSVersionTable.PSVersion.Major -lt 7` → exit 非 0。導入コマンド `winget install Microsoft.PowerShell` を stderr に 1 行提示。

出典: Microsoft Learn "Installing PowerShell on Windows" https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows

#### 確定 B: `audit.yml` から `cargo-deny-action` を廃止 — **案 2 採用**

`EmbarkStudios/cargo-deny-action@v2` は便利だが、本 feature の DRY 原則から逸脱する（ローカル開発者は `just audit` を叩くのに CI は action を叩くと「同一経路」にならない）。`audit.yml` のステップを以下に統一:

1. `actions/checkout@v4`
2. `dtolnay/rust-toolchain@stable`
3. `Swatinem/rust-cache@v2`
4. `cargo install --locked just cargo-deny`
5. `just audit`（`cargo deny check advisories licenses bans sources` → `bash scripts/ci/audit-secret-paths.sh`）

`cargo-deny` 自体は Rust 製で crates.io 配布物なので `cargo install --locked` で統一。

#### 確定 C: `cargo test` の feature 指定 — **`--workspace` のみ、`--all-features` は付けない**

shikomi は将来 `target_os` 別の feature が crate 単位に追加される見込み（§tech-stack §3.1 のバックエンド選択ロジック）。`--all-features` は独立であるべき feature を同時有効化し、結合ビルドが壊れる危険がある。CI の 3 OS matrix とも整合しないため、以下で確定:

- `just test`: `cargo test --workspace`
- `just test-core`: `cargo test -p shikomi-core`
- `just test-infra`: `cargo test -p shikomi-infra`
- `just test-cli`: `cargo test -p shikomi-cli`

個別 feature のテストが必要になった時点で `just test-feat name=foo` のような引数つきレシピを追加（YAGNI、Sub-issue で対応）。

#### 確定 D: `audit-secret-paths.ps1` は **作成しない**

POSIX 実装 1 本（`bash scripts/ci/audit-secret-paths.sh`）に収束させる。Windows からは **Git for Windows 同梱の `bash.exe`** を経由して実行する。根拠:

- 検知契約を 2 実装で二重管理すると、片方の修正忘れで Windows 環境だけ検知漏れが発生する（T8 脅威のバリエーション）
- Git for Windows は shikomi の Windows 開発者にとって必須依存（既に README に記載）。追加導線不要
- `just audit-secrets` レシピは `bash scripts/ci/audit-secret-paths.sh` を直接呼び、`pwsh` 経由でも `bash.exe` が PATH で解決されることに依存

失敗時の挙動（Git for Windows 未導入）: `bash: command not found` → `fail_text` が発火 → MSG-DW-010 の補足に「Windows: Git for Windows の `bash.exe` が必要」を記載。

#### 確定 E: `convco` の commit-msg 呼び出し CLI — **`convco check --from-stdin --strip`**

convco には `check-message` というサブコマンドは存在せず、公式が提供するのは `convco check` の `--from-stdin` / `--strip` フラグである（`convco --help` / `convco check --help` で実地確認）。commit-msg フックは `.git/COMMIT_EDITMSG` ファイルパスを引数で受け取るため、本プロジェクトはファイルを stdin へリダイレクトする方式を採用する:

- `--from-stdin`: 単一のコミットメッセージを stdin から読み、Conventional Commits 1.0 準拠を検証
- `--strip`: 先頭が `#` のコメント行と空白を除去（`git commit --cleanup=strip` 相当、COMMIT_EDITMSG がコメント付きでも正しく検証できる）
- 検証失敗時は非 0 で exit、fail_text で MSG-DW-004 を出力

レシピ定義: `just commit-msg-check {{FILE}}` → 本体は `convco check --from-stdin --strip < {{FILE}}`。

`convco check [REVRANGE]` は履歴範囲検証専用（`--from-stdin` なし）で commit-msg フックには不適。

出典: `convco check --help`（v0.6.x）の Options セクション https://github.com/convco/convco

### 設計判断の補足

#### なぜ `justfile` をルート直下に置くか

- `just` は実行ディレクトリから親方向へ `justfile` を探索する。リポジトリルートに置くことで、サブディレクトリ（`crates/shikomi-cli/` 等）からも `just <recipe>` を呼べる
- `.cargo/config.toml` にエイリアスを書く選択肢もあるが、cargo 以外のツール（`bash scripts/ci/audit-secret-paths.sh`）を呼ぶ統一インタフェースになり得ない（cargo alias は `cargo <alias>` 経由のみ）

#### なぜ `set windows-shell := ["pwsh", "-Cu", "-c"]` を宣言するか

- `just` のデフォルトは Windows で `sh` を探しに行く。Git Bash のみ導入環境での振る舞いに差が出ることを避けるため `pwsh`（PowerShell 7+）を明示する
- 確定 A の通り `powershell.exe`（5.1）フォールバックは採用しない。PowerShell 7+ を前提とし、未満は `setup.ps1` 冒頭で Fail Fast

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

#### CI ワークフローの書き換え方針（§確定 B と整合）

- 既存 8 本中、本 feature が編集対象とするのは **チェック系 5 本**（`lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml` / `windows.yml`）
- **編集対象外**: `branch-policy.yml` / `pr-title-check.yml` / `back-merge-check.yml` は Git 運用系であり `cargo` / `just` を呼ばない
- 編集方針: 直接的な `cargo fmt --all -- --check` / `cargo clippy ...` / `cargo test -p ...` の `run:` を **削除し**、`run: just <recipe>` に置換する。**`audit.yml` は §確定 B により `EmbarkStudios/cargo-deny-action@v2` を廃止**し、`cargo install --locked just cargo-deny` → `just audit` ステップに統一する。判断先送りは行わない
- `Swatinem/rust-cache@v2` は保持。`cargo install --locked just cargo-deny` のキャッシュを兼ねる
