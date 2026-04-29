#!/usr/bin/env bash
# shellcheck shell=bash
#
# `unsafe` ブロック検出 grep gate ライブラリ（TC-CI-019 / TC-CI-026 共通）
#
# 設計根拠:
# - docs/features/dev-workflow/detailed-design/scripts.md
#   §`audit-secret-paths.sh` の `unsafe` ブロック検出契約（TC-CI-019 / TC-CI-026 共通仕様）
#
# 公開関数: audit_unsafe_blocks <target_dir> [allowlist_substring...]
#
# 検出契約:
# - `unsafe[[:space:]]*\{` を `target_dir` 配下の `*.rs` 全行から検出
# - コメント行（`file:line:[空白]//[/!]?`、行頭から最初の非空白文字が `//` で
#   始まる行）を一律除外（`//` / `///` / `//!` を全て吸収）
# - 許可リスト（substring 一致）に該当する path を grep -vF で除外
# - 残行が 1 件以上なら FAIL（return 1、stderr に file:line:content 列挙）
#
# スコープ外（YAGNI、現状コードベースで用例ゼロ）:
# - `/* ... */` ブロックコメント内の `unsafe { ... }`
# - 文字列リテラル内の `"unsafe { ... }"`
# 将来必要になれば設計書 §コメント行の判定規則 を更新して契約拡張する。

audit_unsafe_blocks() {
    local target_dir="$1"
    shift
    local allowlist=("$@")

    # `set -o pipefail` 環境でもヒットゼロを正常終了として扱うため `|| true` で
    # grep 非 0 を吸収。検出パターンは設計書 §検出パターン 通り。
    local matches
    matches="$(grep -rnE 'unsafe[[:space:]]*\{' "$target_dir" \
        --include='*.rs' 2>/dev/null \
        | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//[/!]?' \
        || true)"

    # ヒットゼロなら早期 PASS（許可リスト走査も不要）
    if [[ -z "$matches" ]]; then
        return 0
    fi

    # 許可リスト substring を順次除外。空文字列要素は無視。
    local pat
    for pat in "${allowlist[@]}"; do
        [[ -z "$pat" ]] && continue
        matches="$(printf '%s\n' "$matches" | grep -vF "$pat" || true)"
    done

    # 末尾空行を除去
    matches="$(printf '%s\n' "$matches" | sed '/^$/d' || true)"

    if [[ -n "$matches" ]]; then
        printf '%s\n' "$matches" >&2
        return 1
    fi
    return 0
}
