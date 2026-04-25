# shikomi — unified command surface for local dev / lefthook / CI.
#
# 設計書: docs/features/dev-workflow/detailed-design.md §justfile のレシピ契約
#
# 原則:
# - lefthook / CI / 手動実行の 3 経路すべてがこのファイルを唯一の真実源とする (DRY)
# - 失敗メッセージは lefthook 側の fail_text で提示する。justfile はコマンド実行に徹する (SRP)
# - Windows では `pwsh` (PowerShell 7+) 固定。5.1 フォールバックは採用しない (確定 A)

set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]
set shell := ["bash", "-euo", "pipefail", "-c"]

# `just` のみで一覧表示
default:
    @just --list

# ------------------------------------------------------------------ format

# cargo fmt --check — pre-commit / lint.yml 共通
fmt-check:
    cargo fmt --all -- --check

# cargo fmt — ローカル自動修正用 (CI では使わない)
fmt:
    cargo fmt --all

# ------------------------------------------------------------------ lint

# workspace.lints.clippy の設定を尊重。-D warnings は付けない (既存 lint.yml 方針継承)
clippy:
    cargo clippy --all-targets --all-features

# ------------------------------------------------------------------ test

# 全 workspace テスト (確定 C: --all-features は付けない)
test:
    cargo test --workspace

test-core:
    cargo test -p shikomi-core

# Sub-A (#39) Issue #39 工程4 で導入。`cargo test -p shikomi-core` に既に
# doctest は含まれるが、compile_fail doctest の件数のみを独立に確認したい
# ケース (CI ログ抽出ミスによる Bug-A-001 の再発防止) のための補助レシピ。
test-doc-core:
    cargo test --doc -p shikomi-core

test-infra:
    cargo test -p shikomi-infra

test-cli:
    cargo test -p shikomi-cli

# ------------------------------------------------------------------ audit

# cargo deny + secret 経路監査 (確定 B: cargo-deny-action 廃止、統一経路)
audit:
    cargo deny check advisories licenses bans sources
    bash scripts/ci/audit-secret-paths.sh

# pre-commit 用の軽量 secret スキャン (staged 差分のみ)
# gitleaks が未導入なら即失敗 (Fail Fast)。scripts/setup.sh が導入を保証する
audit-secrets:
    gitleaks protect --staged --no-banner
    bash scripts/ci/audit-secret-paths.sh

# setup.sh / setup.ps1 の SHA256 / バージョンピンが同期しているかを静的検証
audit-pin-sync:
    bash scripts/ci/audit-pin-sync.sh

# 順次実行。途中失敗で即中止 (bash -e による)
check-all: fmt-check clippy test audit audit-secrets audit-pin-sync

# ------------------------------------------------------------------ commit-msg hook

# commit-msg 用: Conventional Commits 1.0 準拠を convco で検証 (確定 E)
# convco の commit-msg 統合は `check --from-stdin --strip`。
# --strip で `#` コメント行を除去してから検証（git commit 時の COMMIT_EDITMSG 互換）。
#
# shebang レシピで bash 強制 (commit-msg-no-ai-footer と同じ方式)。
# `windows-shell := pwsh` 環境では `<` が PowerShell の ParserError になるため、
# bash に閉じる必要がある (確定 D: Git for Windows の bash.exe が前提)。
commit-msg-check FILE:
    #!/usr/bin/env bash
    set -euo pipefail
    convco check --from-stdin --strip < {{FILE}}

# commit-msg 用: AI 生成フッター検出 (REQ-DW-018)
# P1 絵文字 + Generated with + Claude
# P2 Co-Authored-By: ... @anthropic.com
# P3 Co-Authored-By: ... \bClaude\b (単語境界)
# いずれか 1 件でもヒットしたら exit 1 でコミット中止
commit-msg-no-ai-footer FILE:
    #!/usr/bin/env bash
    set -euo pipefail
    if grep -iqE '🤖.*Generated with.*Claude|Co-Authored-By:.*@anthropic\.com|Co-Authored-By:.*\bClaude\b' {{FILE}}; then
        exit 1
    fi
    exit 0
