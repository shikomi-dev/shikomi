#!/usr/bin/env bash
# shellcheck shell=bash
#
# setup.sh と setup.ps1 の pin 定数が完全同期しているかを静的検証する。
#
# 設計書: docs/features/dev-workflow/detailed-design.md §ピン同期の担保
#
# 担保する不変条件:
#   - LEFTHOOK_VERSION / GITLEAKS_VERSION の値が両ファイルで一致
#   - 各 SHA256_* 定数の値が両ファイルで一致
#   - 片方にあって片方に無い定数が無い
#
# 乖離が検出されたら exit 1。audit.yml / `just audit-pin-sync` / `just check-all` から呼ばれる。

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SH="$REPO_ROOT/scripts/setup.sh"
PS1="$REPO_ROOT/scripts/setup.ps1"

for f in "$SH" "$PS1"; do
    if [[ ! -f "$f" ]]; then
        echo "[FAIL] ピン同期検証の対象ファイルが存在しません: $f" >&2
        exit 1
    fi
done

# 対象定数（sh 側は VAR=... / ps1 側は $VAR = ... で同名）
PIN_VARS=(
    LEFTHOOK_VERSION
    LEFTHOOK_SHA256_LINUX_X86_64
    LEFTHOOK_SHA256_LINUX_ARM64
    LEFTHOOK_SHA256_MACOS_X86_64
    LEFTHOOK_SHA256_MACOS_ARM64
    LEFTHOOK_SHA256_WINDOWS_X86_64
    GITLEAKS_VERSION
    GITLEAKS_SHA256_LINUX_X86_64
    GITLEAKS_SHA256_LINUX_ARM64
    GITLEAKS_SHA256_MACOS_X86_64
    GITLEAKS_SHA256_MACOS_ARM64
    GITLEAKS_SHA256_WINDOWS_X86_64
)

# sh 側: VAR="value" (先頭アンカーで代入行のみ抽出、冒頭 export も許容)
extract_sh_value() {
    local var="$1" file="$2"
    grep -E "^[[:space:]]*(export[[:space:]]+)?${var}=" "$file" \
        | head -1 \
        | sed -E "s/^[[:space:]]*(export[[:space:]]+)?${var}=[\"']?([^\"']*)[\"']?.*/\2/"
}

# ps1 側: $VAR = "value" (空白は任意)
extract_ps1_value() {
    local var="$1" file="$2"
    grep -E "^[[:space:]]*\\\$${var}[[:space:]]*=" "$file" \
        | head -1 \
        | sed -E "s/^[[:space:]]*\\\$${var}[[:space:]]*=[[:space:]]*[\"']([^\"']*)[\"'].*/\1/"
}

mismatch=0
for var in "${PIN_VARS[@]}"; do
    sh_val="$(extract_sh_value "$var" "$SH")"
    ps1_val="$(extract_ps1_value "$var" "$PS1")"
    if [[ -z "$sh_val" ]]; then
        echo "[FAIL] $var が setup.sh で未定義または空です" >&2
        mismatch=1
        continue
    fi
    if [[ -z "$ps1_val" ]]; then
        echo "[FAIL] $var が setup.ps1 で未定義または空です" >&2
        mismatch=1
        continue
    fi
    if [[ "$sh_val" != "$ps1_val" ]]; then
        echo "[FAIL] $var が setup.sh / setup.ps1 で乖離しています" >&2
        echo "  sh : $sh_val" >&2
        echo "  ps1: $ps1_val" >&2
        mismatch=1
    fi
done

if [[ "$mismatch" -ne 0 ]]; then
    echo "[FAIL] pin 定数が setup.sh と setup.ps1 で同期していません。" >&2
    echo "次のコマンド: 両ファイルの該当定数を同値にそろえて再コミットしてください。" >&2
    exit 1
fi

echo "[OK] pin 定数の sh/ps1 同期を確認しました（${#PIN_VARS[@]} 件）"
