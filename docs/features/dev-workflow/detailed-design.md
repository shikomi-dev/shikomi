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

## ユーザー向けメッセージの確定文言

`requirements.md` §ユーザー向けメッセージ一覧 で ID のみ定義した MSG-DW-001〜012 の **正確な文言**を本節で凍結する。実装者・Sub-issue が勝手に改変できない契約として扱う。変更は本設計書の更新 PR のみで許可される。

### プレフィックス統一

全メッセージは 5 種類のプレフィックスのいずれかで始まる。色非対応端末でもプレフィックスのテキストだけで重要度が識別可能（A09 対策）。

| プレフィックス | 意味 | 色（対応端末時） |
|--------------|-----|--------------|
| `[FAIL]` | 処理中止を伴う失敗 | 赤 |
| `[OK]` | 成功完了 | 緑 |
| `[SKIP]` | 冪等実行による省略 | 灰 |
| `[WARN]` | 警告（処理は継続） | 黄 |
| `[INFO]` | 情報提供（処理は継続） | 既定色 |

色付けは `just` / `lefthook` / `cargo` のデフォルトに従い、TTY 非検出時は自動で無効化される（既存ツールの振る舞い）。**本 feature で独自に ANSI エスケープを出力しない**（KISS）。

### 2 行構造ルール

失敗メッセージ（`[FAIL]` プレフィックス）は常に **2 行構造**とする。検証は受入基準 11 で assertion される。

```
[FAIL] <何が失敗したかを日本語 1 文で要約>
次のコマンド: <実行すべき復旧コマンド 1 つ>
```

改行は LF 固定（`.gitattributes` で強制済み）。`fail_text` 内で動的変数（ファイル名・ユーザ名等）は使わない（T7 対策）。

### MSG 確定文言表

| ID | 出力先 | 文言（改行区切りで上下 2 行） |
|----|------|------------------------------|
| MSG-DW-001 | lefthook `fail_text` | `[FAIL] cargo fmt 違反を検出しました。` / `次のコマンド: just fmt` |
| MSG-DW-002 | lefthook `fail_text` | `[FAIL] cargo clippy 違反を検出しました。` / `次のコマンド: just clippy` |
| MSG-DW-003 | lefthook `fail_text` | `[FAIL] cargo test に失敗しました。` / `次のコマンド: just test` |
| MSG-DW-004 | lefthook `fail_text` | `[FAIL] コミットメッセージが Conventional Commits 1.0 に準拠していません。` / `規約: CONTRIBUTING.md §コミット規約 または https://www.conventionalcommits.org/ja/v1.0.0/` |
| MSG-DW-005 | setup stdout | `[OK] Setup complete. Git フックが有効化されました。`（1 行のみ、成功は 2 行構造ルールの例外） |
| MSG-DW-006 | setup stdout | `[SKIP] {tool} は既にインストール済みです。` / `バージョン: {version}`（`{tool}` / `{version}` は setup スクリプトが動的挿入、ただし **stdout のみ・CI ログ共有なし**のため T7 対象外） |
| MSG-DW-007 | CONTRIBUTING.md 静的記載 | `[WARN] --no-verify の使用は規約で原則禁止です。` / `PR 本文に理由を明記してください。CI が同一チェックを再実行します。` |
| MSG-DW-008 | setup stderr | `[FAIL] Rust toolchain が未検出です。` / `次のコマンド: https://rustup.rs/ の手順に従って rustup を導入してください。` |
| MSG-DW-009 | setup stderr | `[FAIL] .git/ ディレクトリが見つかりません。リポジトリルートで実行してください。` / `現在のディレクトリ: {cwd}`（`{cwd}` は setup stdout のみ、CI ログには流れない） |
| MSG-DW-010 | lefthook `fail_text` | `[FAIL] secret の混入が検出されました。該当行を除去後、git add を再実行してください。` / `既に push 済みの場合: CONTRIBUTING.md §Secret 混入時の緊急対応` |
| MSG-DW-011 | setup stderr | `[FAIL] PowerShell 7 以上が必要です（検出: {version}）。` / `次のコマンド: winget install Microsoft.PowerShell` |
| MSG-DW-012 | setup stderr | `[FAIL] {tool} バイナリの SHA256 検証に失敗しました。サプライチェーン改ざんの可能性があります。` / `次のコマンド: 一時ファイルを削除後にネットワーク状況を確認し再実行。繰り返し失敗する場合は Issue で報告してください。` |
| MSG-DW-013 | lefthook `fail_text` | `[FAIL] コミットメッセージに AI 生成フッターが含まれています（🤖 Generated with Claude Code / Co-Authored-By: Claude 等）。` / `次のコマンド: 該当行を削除して再コミット。許可されない trailer のポリシーは CONTRIBUTING.md §AI 生成フッターの禁止 を参照。` |

**gitleaks / audit-secret-paths.sh の `file:line` 出力**: これらのツール自体の stdout に検出箇所が出力される。`fail_text` には載せず、ツール出力との「縦の情報階層」（ツール出力 → 空行 → `[FAIL]` 2 行）を形成する。

## lefthook / gitleaks の配布経路と SHA256 検証（REQ-DW-015 詳細）

### バイナリ取得 URL のテンプレート

setup スクリプト冒頭に以下の定数を置き、アップデート時は PR で明示差分を提示する:

| 定数名 | 例 | 用途 |
|-------|-----|------|
| `LEFTHOOK_VERSION` | `1.7.18` | ピンバージョン。**設計時の契約**は「定数として設置し空なら Fail Fast」。具体値は Sub-issue C の初期実装で upstream の `checksums.txt` から転記（設計判断の先送りではなく、upstream の将来リリース版を引くための運用上の転記） |
| `LEFTHOOK_SHA256_LINUX_X86_64` | `<64 hex chars>` | `lefthook_${VERSION}_Linux_x86_64.tar.gz` の SHA256 |
| `LEFTHOOK_SHA256_LINUX_ARM64` | `<64 hex chars>` | 同 aarch64 |
| `LEFTHOOK_SHA256_MACOS_X86_64` | `<64 hex chars>` | Intel Mac |
| `LEFTHOOK_SHA256_MACOS_ARM64` | `<64 hex chars>` | Apple Silicon |
| `LEFTHOOK_SHA256_WINDOWS_X86_64` | `<64 hex chars>` | `lefthook_${VERSION}_Windows_x86_64.zip` の SHA256 |
| `GITLEAKS_VERSION` / `GITLEAKS_SHA256_*` | — | 同様のピン |

**初期値は Sub-issue C の実装 PR で確定**させる。本設計書では「定数として設置する」契約だけを凍結し、具体値は実装時の `curl -sL https://github.com/.../releases/download/v${V}/checksums.txt` を取得して転記する運用とする（upstream の公式 SHA256 を再計算せず、公式リリース成果物のチェックサムを信頼する）。

### ダウンロード → 検証 → 配置の手順

1. URL 合成: `https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/lefthook_${LEFTHOOK_VERSION}_${PLATFORM}.${EXT}`
   - `PLATFORM`: OS と arch から決定（例: `Linux_x86_64`, `Darwin_arm64`, `Windows_x86_64`）
   - `EXT`: Unix は `tar.gz`、Windows は `zip`
2. `curl -sSfL <URL> -o <tmpfile>` でダウンロード（`-f` で HTTP エラー時に Fail Fast）
3. 実測 SHA256 を取得:
   - Unix: `sha256sum <tmpfile>` の先頭 64 hex を抽出
   - Windows: `(Get-FileHash <tmpfile> -Algorithm SHA256).Hash.ToLower()`
4. ピン定数と**完全一致**（大小文字・空白含め）を検証。不一致なら:
   - 一時ファイルを削除
   - MSG-DW-012 を stderr に出して exit 非 0
5. 一致なら展開し、バイナリを `~/.cargo/bin/`（Windows は `$env:USERPROFILE\.cargo\bin\`）に移動
6. Unix のみ `chmod +x <binary>` を適用

`gitleaks` も同一手順（バージョンと SHA256 定数を別に持つ）。

### なぜ `~/.cargo/bin/` に置くか

- 既に `just` / `convco` が `cargo install` で同ディレクトリに入る。PATH 設定の追加案内が不要（DRY）
- shikomi の開発者は全員 Rust toolchain 済み → `~/.cargo/bin/` は PATH に含まれている前提
- `/usr/local/bin` に入れる案は管理者権限要求で Fail Fast 契約を損なう

### CODEOWNERS で保護する 5 パス（REQ-DW-016 詳細）

`.github/CODEOWNERS` に Sub-issue B で以下を追記:

| パス | 保護対象の理由（T5・T8 脅威対応） |
|-----|----------------------------------|
| `/lefthook.yml` | フック定義の改変で検知スキップ・任意コマンド実行を仕込める |
| `/justfile` | レシピ内のコマンド改変で CI とローカルの乖離を作れる |
| `/scripts/setup.sh` | ダウンロード URL / SHA256 ピン改変でサプライチェーン攻撃経路を作れる |
| `/scripts/setup.ps1` | 同上 |
| `/scripts/ci/` | secret 検知契約（TC-CI-012〜015）の改変で水際検知を無効化できる |

追記順は既存 CODEOWNERS の「ルート直下のガバナンスファイル」節の直後に配置（可読性優先）。

## データ構造

本 feature は永続化データを持たないため、主要な **設定ファイルのキー構造**を表形式で定義する。実ファイルは Sub-issue の実装 PR で作成される。

### `lefthook.yml` のキー構造

| キー | 型 | 用途 | デフォルト値 / 採用値 |
|-----|---|------|-----------------|
| `pre-commit.parallel` | bool | fmt-check / clippy / audit-secrets の並列実行可否 | `true` |
| `pre-commit.commands.fmt-check.run` | string | 実行コマンド | `just fmt-check` |
| `pre-commit.commands.fmt-check.fail_text` | string | 失敗時メッセージ（静的 2 行） | MSG-DW-001 |
| `pre-commit.commands.clippy.run` | string | 実行コマンド | `just clippy` |
| `pre-commit.commands.clippy.fail_text` | string | 失敗時メッセージ（静的 2 行） | MSG-DW-002 |
| `pre-commit.commands.audit-secrets.run` | string | 実行コマンド（REQ-DW-013） | `just audit-secrets` |
| `pre-commit.commands.audit-secrets.fail_text` | string | 失敗時メッセージ（静的 2 行） | MSG-DW-010 |
| `pre-push.commands.test.run` | string | 実行コマンド | `just test` |
| `pre-push.commands.test.fail_text` | string | 失敗時メッセージ（静的 2 行） | MSG-DW-003 |
| `commit-msg.parallel` | bool | convco と no-ai-footer の並列実行可否 | `true`（両者とも grep 相当の軽量検査、数百ミリ秒で完了） |
| `commit-msg.commands.convco.run` | string | 実行コマンド（`{1}` はメッセージファイルパス） | `just commit-msg-check {1}` |
| `commit-msg.commands.convco.fail_text` | string | 失敗時メッセージ（静的 2 行） | MSG-DW-004 |
| `commit-msg.commands.no-ai-footer.run` | string | AI 生成フッター検出コマンド（REQ-DW-018） | `just commit-msg-no-ai-footer {1}` |
| `commit-msg.commands.no-ai-footer.fail_text` | string | 失敗時メッセージ（静的 2 行） | MSG-DW-013 |
| `skip_output` | 配列 | 出力抑制対象 | 未設定（lefthook のデフォルト挙動に従う。MSG の 2 行構造が埋もれない限り抑制しない） |
| `colors` | string | 色出力制御 | 未設定（lefthook デフォルトで TTY 自動判定。CI 非 TTY で自動無効化）|

出典: lefthook Configuration Reference https://lefthook.dev/configuration/ の `Commands` / `fail_text` / `parallel` / `skip_output` 各節

### `justfile` のレシピ契約（初期定義）

| レシピ名 | 引数 | 実行コマンド（論理） | 対応する CI ワークフロー |
|---------|-----|------------------|--------------------|
| `default` | なし | `just --list` を呼ぶ | — |
| `fmt-check` | なし | `cargo fmt --all -- --check` | `lint.yml` step |
| `fmt` | なし | `cargo fmt --all`（自動修正） | — |
| `clippy` | なし | `cargo clippy --all-targets --all-features`（workspace.lints.clippy の設定を尊重、`-D warnings` は付けない。既存 `lint.yml` と同方針） | `lint.yml` step |
| `test` | なし | `cargo test --workspace`（確定 C: `--all-features` は付けない） | `unit-core.yml` + `test-infra.yml` + `windows.yml` |
| `test-core` | なし | `cargo test -p shikomi-core` | `unit-core.yml` |
| `test-infra` | なし | `cargo test -p shikomi-infra` | `test-infra.yml` + `windows.yml` |
| `test-cli` | なし | `cargo test -p shikomi-cli` | **新設。CI 側はこの時点で対応ワークフローを持たない**（`test-infra.yml` が shikomi-infra 専用のため）。`just check-all` と Sub-issue D の CONTRIBUTING 更新でローカル開発者向けに可視化する。CI 統合は本 feature のスコープ外（`shikomi-cli` の CI ジョブ新設は独立 Issue で扱う、YAGNI） |
| `audit` | なし | `cargo deny check advisories licenses bans sources` → `bash scripts/ci/audit-secret-paths.sh`（確定 B: `cargo-deny-action` 廃止。確定 D: `.ps1` 版は作成せず、Windows でも `bash.exe` 経由で POSIX 実装を呼ぶ） | `audit.yml` |
| `audit-secrets` | なし | `gitleaks protect --staged --no-banner` → `bash scripts/ci/audit-secret-paths.sh` | pre-commit 経由（REQ-DW-013） |
| `check-all` | なし | `fmt-check` → `clippy` → `test` → `audit` → `audit-secrets` を順次呼ぶ（失敗時に途中終了） | 全 CI 相当 |
| `commit-msg-check` | `file` 引数 1 個 | `convco check --from-stdin --strip < {{file}}`（確定 E: convco の commit-msg 正規方式。`check-message` サブコマンドは存在しない） | — |
| `commit-msg-no-ai-footer` | `file` 引数 1 個 | `grep -iqE '<PATTERN>' {{file}} && exit 1 \|\| exit 0` を実行（REQ-DW-018、下記 §AI 生成フッター検出パターンを参照）。POSIX 互換なので Windows でも Git for Windows の `bash.exe` 経由で動作 | — |

### `scripts/setup.sh` のステップ契約

| ステップ | 処理 | Fail Fast 条件 |
|---------|-----|-------------|
| 1. shebang / strict mode | `#!/usr/bin/env bash` + `set -euo pipefail` | — |
| 2. ピン定数宣言 | `LEFTHOOK_VERSION` / `LEFTHOOK_SHA256_*` / `GITLEAKS_VERSION` / `GITLEAKS_SHA256_*` を冒頭で定数定義 | 値が空なら即 exit（未確定状態でのマージ防止） |
| 3. cwd 検査 | リポジトリルート（`.git/` が存在）で実行されているか | 非リポジトリで実行 → exit 非 0、MSG-DW-009 |
| 4. Rust toolchain 検査 | `rustc --version` / `cargo --version` の成功可否 | 失敗 → MSG-DW-008 |
| 5. `just` 導入（Rust 製） | `command -v just` 成否で `cargo install --locked just` の要否判定。既存時は MSG-DW-006 | `cargo install` 失敗 → exit 非 0 |
| 6. `convco` 導入（Rust 製） | 同上 | 同上 |
| 7. `lefthook` 導入（Go 製）| `command -v lefthook` 成否を確認 → 未検出なら GitHub Releases から tar.gz 取得 → SHA256 検証 → `~/.cargo/bin/lefthook` に配置 → `chmod +x` | SHA256 不一致 → MSG-DW-012 |
| 8. `gitleaks` 導入（Go 製） | 同上 | 同上 |
| 9. `lefthook install` | `.git/hooks/` へラッパ配置 | 失敗 → MSG-DW-009 |
| 10. 完了ログ | MSG-DW-005 を表示 | — |

### `scripts/setup.ps1` のステップ契約

setup.sh と **同一のステップ番号・同一の責務**。差分のみ表記。

| ステップ | sh 版 | ps1 版（差分） |
|---------|-----|------------|
| 0（ps1 専用） | — | **冒頭で `$PSVersionTable.PSVersion.Major -lt 7` を検査**、未満なら MSG-DW-011 で exit（確定 A）|
| 1 | `#!/usr/bin/env bash` + `set -euo pipefail` | 冒頭 `$ErrorActionPreference = 'Stop'`。`Set-StrictMode -Version Latest` を併用 |
| 2 | ピン定数を bash 変数で宣言 | PowerShell 変数（`$LEFTHOOK_VERSION` 等）で同値を宣言。**ピン値は sh / ps1 で完全同期させる**（二重管理だが、共通化のための third file を作るのは YAGNI） |
| 3 | `.git/` 検査（`-d .git`） | `Test-Path .git` |
| 4 | `rustc --version` / `cargo --version` | 同左、`$LASTEXITCODE` 非 0 を検査 |
| 5-6 | `command -v <tool>` | `Get-Command <tool> -ErrorAction SilentlyContinue` |
| 7-8 | `curl -sSfL` → `sha256sum` → 文字列比較 | `Invoke-WebRequest -Uri <URL> -OutFile <tmp>` → `(Get-FileHash <tmp> -Algorithm SHA256).Hash.ToLower()` → `-eq` 比較 |
| 9-10 | 同一 | 同一 |

**ピン同期の担保（設計時確定）**: `setup.sh` と `setup.ps1` の `LEFTHOOK_VERSION` / `GITLEAKS_VERSION` / 各 SHA256 値が乖離すると、Windows 開発者だけ別バイナリを引く事故（T4 脅威のバリエーション）が起きる。Sub-issue C で **`just audit-pin-sync` レシピ（および `audit.yml` ステップ）を必須実装**する。挙動: 両ファイルから同一変数名の値を正規表現で抽出し、各組で文字列完全一致を検証、乖離があれば exit 非 0。これにより**人間の注意力に依存せず機械的にピン同期を強制**する（Fail Fast 原則）。判断は本設計書で凍結し Sub-issue 側での再判定は行わない。

### AI 生成フッター検出パターン（REQ-DW-018 詳細、確定パターン）

`just commit-msg-no-ai-footer FILE` レシピは、以下 3 パターンを **case-insensitive** な **拡張正規表現（ERE）** で照合する。いずれか 1 件でもヒットすれば exit 1 でコミットを中止する。lefthook `parallel: true` により既存の convco 検査と独立に走る。

| # | パターン（拡張正規表現・ERE） | 検出対象の例 | 意図 |
|---|---------------------------|-----------|------|
| P1 | `🤖.*Generated with.*Claude` | `🤖 Generated with [Claude Code](https://claude.com/claude-code)` | Claude Code が自動挿入する emoji 付きフッター行 |
| P2 | `Co-Authored-By:.*@anthropic\.com` | `Co-Authored-By: Claude <noreply@anthropic.com>` | anthropic.com ドメインを含む Co-Authored-By trailer（メールアドレスドメインで識別） |
| P3 | `Co-Authored-By:.*\bClaude\b` | `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` | Claude を name に含む Co-Authored-By trailer（モデル名の揺れに強い） |

**実装契約**:
- 3 パターンを `\|`（ERE のアルタネーション）で結合し、単一の `grep -iqE '(P1)|(P2)|(P3)' FILE` 呼び出しで検査する（サブプロセス起動を最小化）
- `-i` で大小文字を無視（`co-authored-by` / `CO-AUTHORED-BY` 等の表記揺れを吸収）
- `-E` で拡張正規表現（`\|` でアルタネーション、`\b` で単語境界）
- `-q` で match 時に出力抑止（match/no-match は exit code のみ）
- match（exit 0）なら shell 側で `exit 1` へ反転してコミット中止、no-match（exit 1）なら `exit 0` へ反転してコミット続行

**誤検知の境界**:
- `Claude` という単語を**本文・ファイル名・PR 番号**等で引用する正規のコミットメッセージは、P3 の単語境界（`\bClaude\b`）と Co-Authored-By 行のみの照合制約により誤検知しない（P3 は `Co-Authored-By:.*\bClaude\b` なので `Co-Authored-By:` 接頭辞を必須とする）
- 将来「claude」を別の文脈（例: プロジェクトオーナー名が偶然 Claude）で合法的に使うケースが出た場合は、本設計書を更新して例外経路を追加する（現時点では shikomi プロジェクトのオーナー `@kkm-horikawa` に該当者なし）

**対象外（YAGNI）**:
- 他の AI（ChatGPT / Gemini / Copilot 等）のフッター検出は本 Issue のスコープ外。オーナーが同様の懸念を表明した時点で追加パターンを登録する（パターン追加は本設計書の P4 以降として正規表現 1 行を追記すれば完了）
- コミット **body / description** に散文として Claude を言及するケースは P3 の `Co-Authored-By:` 前置制約により自動的に対象外

**`--no-verify` バイパス時の対応**:
- T9 脅威で述べた通り、ローカル commit-msg フックは `--no-verify` で無効化可能。`pre-receive` hook は GitHub 無料プランで提供されず、機械的な完全遮断は不可能
- 代替: (a) CONTRIBUTING.md §AI 生成フッターの禁止 で明文化し、Agent-C ペルソナ（Claude Code 等）への明示的教示 (b) PR レビュー時の人間レビュワー / `@kkm-horikawa` による目視検知 (c) 将来 squash merge 時に GitHub UI が自動挿入する Co-Authored-By への対応は後続 Issue（YAGNI）

### `.github/workflows/*.yml` の編集契約

| ファイル | 旧 `run:` | 新 `run:` | 備考 |
|---------|---------|---------|------|
| `lint.yml` | `cargo fmt --all -- --check` / `cargo clippy --all-targets --all-features` | `just fmt-check` / `just clippy`（ステップ前に `cargo install --locked just` 追加） | 既存の `workspace.lints.clippy` 設定尊重方針は justfile 側で継承 |
| `unit-core.yml` | `cargo test -p shikomi-core` | `just test-core` | 同上 |
| `test-infra.yml` | `cargo test -p shikomi-infra` | `just test-infra` | 同上 |
| `audit.yml` | `EmbarkStudios/cargo-deny-action@v2` + `bash scripts/ci/audit-secret-paths.sh` | `cargo install --locked just cargo-deny` → `just audit`（確定 B: action 廃止） | DRY、`just audit` の定義が CI / local / 手動の 3 経路で単一真実源 |
| `windows.yml` | `cargo test -p shikomi-infra` | `shell: pwsh` で `cargo install --locked just` → `just test-infra` | `justfile` の `set windows-shell := ["pwsh", "-Cu", "-c"]` と整合。PowerShell 7+ は GitHub Actions windows-latest に既定搭載（2023 以降） |

### CI ワークフローで追加する secret scan ステップ

`audit.yml` に**二重防護**のため `just audit-secrets` ステップを追加する（T8 脅威対応）。ローカル pre-commit が `lefthook.yml` 改変で無効化された場合でも CI 側で独立に検知。

| ステップ順 | `run:` | 目的 |
|---------|-------|------|
| 1 | `actions/checkout@v4`（`fetch-depth: 0`）| gitleaks に履歴全体を渡すため全履歴取得 |
| 2 | `dtolnay/rust-toolchain@stable` | `cargo install` 前提 |
| 3 | `Swatinem/rust-cache@v2` | キャッシュ |
| 4 | `cargo install --locked just cargo-deny` | `just` / `cargo deny` 導入 |
| 5 | `just audit` | `cargo deny` + `audit-secret-paths.sh` |
| 6 | **gitleaks setup: `bash scripts/setup.sh --tools-only` を CI から呼び出す**（設計時確定） | setup ロジックを CI inline に複製すると DRY 違反。**`setup.sh` に `--tools-only` オプション**（`lefthook install` ステップを skip、ツール配置のみ実施）を追加し、CI も開発者ローカルも同一コード経路で Go 製バイナリを SHA256 検証つきで導入する。Sub-issue C の setup.sh 実装時にこのオプションを含める。判断は本設計書で凍結 |
| 7 | `just audit-secrets`（または `gitleaks detect --no-banner` を直接） | 履歴全体に対する secret 検知 |

## ビジュアルデザイン

該当なし — 理由: 本 feature は CLI のみで GUI 要素を持たない。フック失敗時のテキスト出力は `lefthook` / `just` / `cargo` のデフォルト配色・書式に従う。

---

## 出典・参考

- lefthook 公式ドキュメント: https://lefthook.dev/ / https://github.com/evilmartians/lefthook
- lefthook Configuration Reference (`Commands` / `fail_text` / `parallel`): https://lefthook.dev/configuration/
- lefthook Releases（SHA256 ピン元）: https://github.com/evilmartians/lefthook/releases
- just 公式ドキュメント: https://just.systems/
- just `windows-shell` 設定: https://just.systems/man/en/chapter_33.html
- convco: https://github.com/convco/convco README §Git hooks
- gitleaks: https://github.com/gitleaks/gitleaks
- gitleaks `protect` サブコマンド（staged 対象）: https://github.com/gitleaks/gitleaks#scan-commands
- gitleaks Releases（SHA256 ピン元）: https://github.com/gitleaks/gitleaks/releases
- Git `core.hooksPath` ドキュメント: https://git-scm.com/docs/githooks#_core_hookspath
- `cargo install --locked` 挙動: https://doc.rust-lang.org/cargo/commands/cargo-install.html
- Conventional Commits 1.0 仕様: https://www.conventionalcommits.org/ja/v1.0.0/
- `cargo-deny` 公式: https://embarkstudios.github.io/cargo-deny/
- `git filter-repo` 公式（履歴書換え推奨手段）: https://github.com/newren/git-filter-repo
- GitHub Secret scanning: https://docs.github.com/en/code-security/secret-scanning/about-secret-scanning
- GitHub Actions `Swatinem/rust-cache@v2`: https://github.com/Swatinem/rust-cache
- Microsoft Learn "Installing PowerShell on Windows": https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows
- OWASP Top 10 2021: https://owasp.org/Top10/
