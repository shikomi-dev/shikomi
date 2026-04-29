#!/usr/bin/env bash
# shellcheck shell=bash
#
# `unsafe` ブロック検出 grep gate ライブラリ（TC-CI-019 / TC-CI-026 共通）
#
# 設計根拠:
# - docs/features/dev-workflow/detailed-design/scripts.md
#   §`audit-secret-paths.sh` の `unsafe` ブロック検出契約（TC-CI-019 / TC-CI-026 共通仕様）
#
# 公開関数: audit_unsafe_blocks <target_dir> [allowlist_path...]
#
# 検出契約:
# - `unsafe[[:space:]]*\{` を `target_dir` 配下の `*.rs` 全行から検出
# - コメント行（`file:line:[空白]//[/!]?`、行頭から最初の非空白文字が `//` で
#   始まる行）を一律除外（`//` / `///` / `//!` を全て吸収）
# - 許可リストは **ファイルパス完全一致**（awk -F: で $1 完全一致）で除外する。
#   substring 一致だと `windows_sid.rs.bypass/evil.rs` のような path 偽装で
#   silent bypass を許してしまう（Bug-CI-031 / 服部・ペテルギウス工程5 指摘）
# - 残行が 1 件以上なら FAIL（return 1、stderr に file:line:content 列挙）
#
# 許可リスト引数の形式:
# - `target_dir` を起点に grep -rn が出力する $1（ファイルパス）と完全一致する
#   文字列を渡す。例: target_dir="crates/shikomi-cli/src/" の場合は
#   "crates/shikomi-cli/src/io/windows_sid.rs"
#
# 攻撃面（service-side fixture で sentinel 化済、TC-CI-026-f）:
# - path 偽装 `windows_sid.rs.bypass/evil.rs`: $1 != "windows_sid.rs" で検出
# - 拡張子偽装 `windows_sid.rsx`: `--include='*.rs'` で grep 自体がヒットしない
# - 許可ファイル名混入のサブディレクトリ: 同上、$1 完全一致で構造的に検出
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

    # 許可リスト: awk -F: で $1（ファイルパス）が完全一致する行のみ除外。
    # substring 一致を使わないことで Bug-CI-031 (path 偽装 silent bypass) を
    # 構造的に塞ぐ。空文字列要素は無視。
    local pat
    for pat in "${allowlist[@]}"; do
        [[ -z "$pat" ]] && continue
        matches="$(printf '%s\n' "$matches" \
            | awk -F: -v p="$pat" '$1 != p' \
            || true)"
    done

    # 末尾空行を除去
    matches="$(printf '%s\n' "$matches" | sed '/^$/d' || true)"

    if [[ -n "$matches" ]]; then
        printf '%s\n' "$matches" >&2
        return 1
    fi
    return 0
}
