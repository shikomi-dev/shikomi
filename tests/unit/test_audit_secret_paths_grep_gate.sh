#!/usr/bin/env bash
# shellcheck shell=bash
#
# tests/unit/test_audit_secret_paths_grep_gate.sh
#
# TC-CI-026 サブケース a〜f の連続実行ランナー。`scripts/ci/lib/audit_unsafe_blocks.sh`
# の `audit_unsafe_blocks` 関数を直接テストし、Issue #85 (grep gate コメント行
# 誤検出) と Bug-CI-031 (許可リスト substring 過剰許可) の回帰防止 SSoT を機械検証する。
#
# 設計根拠:
# - docs/features/dev-workflow/detailed-design/scripts.md §`audit-secret-paths.sh` の
#   `unsafe` ブロック検出契約（TC-CI-019 / TC-CI-026 共通仕様）
# - docs/features/dev-workflow/test-design.md §TC-CI-026 サブケース
#
# 終了コード: 全 PASS で 0、1 件でも FAIL があれば 1

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=../../scripts/ci/lib/audit_unsafe_blocks.sh
source "$REPO_ROOT/scripts/ci/lib/audit_unsafe_blocks.sh"

# 全ての target / 許可リスト引数は REPO_ROOT 相対 path で渡す。grep -rn の出力
# $1 と awk $1 完全一致除外がドリフトしないため。
FIXTURE_ROOT_REL="tests/fixtures/grep_gate"
TMP_ERR="$(mktemp -t audit_grep_gate.XXXXXX)"
trap 'rm -f "$TMP_ERR"' EXIT

PASS_COUNT=0
FAIL_COUNT=0
TOTAL_COUNT=0

# 関数: 期待 PASS（exit 0）のサブケースを実行
# 引数: <name> <target_dir> [allowlist_path...]
expect_pass() {
    local name="$1"
    local target="$2"
    shift 2
    local allowlist=("$@")
    TOTAL_COUNT=$((TOTAL_COUNT + 1))

    if audit_unsafe_blocks "$target" "${allowlist[@]}" 2>"$TMP_ERR"; then
        echo "[$name] PASS — exit 0 (no unsafe detected outside allowlist)"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo "[$name] FAIL — expected exit 0 but audit_unsafe_blocks returned non-0"
        echo "  stderr was:"
        sed 's/^/    /' "$TMP_ERR"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}

# 関数: 期待 FAIL（exit 非 0）のサブケースを実行
# 引数: <name> <expected_marker> <target_dir> [allowlist_path...]
expect_fail() {
    local name="$1"
    local expected_marker="$2"
    local target="$3"
    shift 3
    local allowlist=("$@")
    TOTAL_COUNT=$((TOTAL_COUNT + 1))

    if audit_unsafe_blocks "$target" "${allowlist[@]}" 2>"$TMP_ERR"; then
        echo "[$name] FAIL — expected non-0 exit but audit_unsafe_blocks returned 0"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        return
    fi

    if grep -qF "$expected_marker" "$TMP_ERR"; then
        echo "[$name] PASS — non-0 exit + stderr contains '$expected_marker'"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo "[$name] FAIL — non-0 exit ok but stderr missing marker '$expected_marker'"
        echo "  stderr was:"
        sed 's/^/    /' "$TMP_ERR"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}

echo "=== TC-CI-026 grep gate サブケース 連続実行 ==="
echo "fixture root: $REPO_ROOT/$FIXTURE_ROOT_REL"
echo

# TC-CI-026-a: 許可リスト外で実 unsafe ブロック → FAIL 期待
# 検証目的: コメント行除外パイプを挟んでも実コードの unsafe ブロックは正しくヒット
expect_fail "TC-CI-026-a" "case_a/really_unsafe.rs" \
    "$FIXTURE_ROOT_REL/case_a"

# TC-CI-026-b: doc コメントのみ（PR #82 実例の最小再現）→ PASS 期待
# 検証目的: doc コメント `/// unsafe { ... }` 文字列を grep が誤検出しない
expect_pass "TC-CI-026-b" \
    "$FIXTURE_ROOT_REL/case_b"

# TC-CI-026-c: コメント形式 5 パターン網羅 → PASS 期待
# 検証目的: `//` / `///` / `//!` / 先頭空白あり / 空白なし / タブ 全てを除外
expect_pass "TC-CI-026-c" \
    "$FIXTURE_ROOT_REL/case_c"

# TC-CI-026-d: 行内コメント形式の実コード → FAIL 期待
# 検証目的: 行内コメントを「コメント行」として除外する誤陽性化を sentinel 検出
expect_fail "TC-CI-026-d" "case_d/inline_comment.rs" \
    "$FIXTURE_ROOT_REL/case_d"

# TC-CI-026-e: 許可リスト登録 2 ファイルに実 unsafe → PASS 期待
# 検証目的: 許可リスト path 完全一致による awk $1 != p 除外が機能
expect_pass "TC-CI-026-e" \
    "$FIXTURE_ROOT_REL/case_e" \
    "$FIXTURE_ROOT_REL/case_e/io/windows_sid.rs" \
    "$FIXTURE_ROOT_REL/case_e/hardening/core_dump.rs"

# TC-CI-026-f: Bug-CI-031 path 偽装 sentinel → FAIL 期待
# 検証目的: 許可リスト entry 文字列 (`windows_sid.rs`) を path に substring として
#   含む `windows_sid.rs.bypass/evil.rs` の実 unsafe ブロックを silent 許可しない
#   こと。awk $1 完全一致除外が path 偽装攻撃を構造的に塞ぐ sentinel。
# 設計根拠: scripts.md §検出契約 §許可リスト「ファイル path 完全一致」、
#   test-design.md §TC-CI-026-f
expect_fail "TC-CI-026-f" "case_f/io/windows_sid.rs.bypass/evil.rs" \
    "$FIXTURE_ROOT_REL/case_f" \
    "$FIXTURE_ROOT_REL/case_f/io/windows_sid.rs"

echo
echo "=== Summary ==="
echo "PASS: $PASS_COUNT / $TOTAL_COUNT"
echo "FAIL: $FAIL_COUNT / $TOTAL_COUNT"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
    echo "TC-CI-026 grep gate 回帰防止: ✗ FAIL"
    exit 1
fi

echo "TC-CI-026 grep gate 回帰防止: ✓ ALL PASS"
