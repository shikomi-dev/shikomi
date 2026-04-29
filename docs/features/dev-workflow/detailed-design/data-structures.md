# 詳細設計書 — dev-workflow / データ構造

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- 配置先: docs/features/dev-workflow/detailed-design/data-structures.md -->
<!-- 兄弟: ./index.md, ./classes.md, ./messages.md, ./setup.md, ./scripts.md -->

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
| `commit-msg-check` | `file` 引数 1 個 | **shebang bash レシピ**で `convco check --from-stdin --strip < {{FILE}}` を実行（確定 E: convco の commit-msg 正規方式。`check-message` サブコマンドは存在しない）。shebang を使う理由は `windows-shell := pwsh` 環境で `<` が PowerShell の ParserError になるため、Git for Windows の `bash.exe` に閉じる必要があるから（確定 D と整合） | — |
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
| `windows.yml` | `cargo test -p shikomi-infra` | `shell: pwsh` で `cargo install --locked just` → `just test-infra`、加えて **Windows 側 commit-msg 経路の回帰ジョブ `commit-msg-windows` を追加**（`commit-msg-check` good/bad 各 1 ケース + `commit-msg-no-ai-footer` clean/AI trailer 各 1 ケースを pwsh で実コール） | `justfile` の `set windows-shell := ["pwsh", ...]` と整合。PowerShell 7+ は GitHub Actions windows-latest に既定搭載（2023 以降）。shebang bash レシピが Git for Windows 同梱 `bash.exe` 経由で動くことの回帰防壁（PR #24 レビューで検出した致命の再発防止） |
