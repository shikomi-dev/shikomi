#!/usr/bin/env bash
# shellcheck shell=bash
#
# CI audit script — cli-vault-commands feature の secret 経路契約を静的に検証する。
#
# 対応 TC:
# - TC-CI-013: `crates/shikomi-cli/src/` 配下で `expose_secret` 呼び出しが 0 件
# - TC-CI-014: panic hook 内で `tracing::*` マクロを呼ばない
# - TC-CI-015: panic hook 内で `info.payload()` / `info.message()` 等を参照しない
#
# 設計根拠: docs/features/cli-vault-commands/test-design/ci.md §1.1

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

fail() {
    echo "::error::$1"
    exit 1
}

# --- TC-CI-013 ------------------------------------------------------
echo "[TC-CI-013] expose_secret 呼び出しが shikomi-cli/src/ に 0 件であることを確認"
if matches="$(grep -rn 'expose_secret' crates/shikomi-cli/src/)"; then
    echo "$matches"
    fail "TC-CI-013 FAIL: crates/shikomi-cli/src/ 配下に expose_secret 呼び出しが存在します"
fi
echo "[TC-CI-013] PASS"

# --- TC-CI-014 / TC-CI-015 -----------------------------------------
# panic hook ブロック（std::panic::set_hook から fn panic_hook 本体まで）を抽出し、
# tracing::* マクロ呼び出し / PanicHookInfo payload 参照が存在しないことを確認する。
echo "[TC-CI-014/015] panic hook 内で tracing 呼び出し / payload 参照を禁止する契約を検証"

# lib.rs / main.rs 限定で fn panic_hook(...) の本体を抽出（awk で `fn panic_hook` 開始 〜 末尾閉じ括弧）。
panic_hook_body="$(awk '/fn panic_hook\(/,/^}$/' \
    crates/shikomi-cli/src/lib.rs \
    crates/shikomi-cli/src/main.rs 2>/dev/null || true)"

if [[ -n "$panic_hook_body" ]]; then
    if echo "$panic_hook_body" | grep -qE 'tracing::'; then
        echo "$panic_hook_body"
        fail "TC-CI-014 FAIL: panic hook 内で tracing マクロが呼ばれています"
    fi
    if echo "$panic_hook_body" | grep -qE '\.payload\(\)|info\.payload|PanicHookInfo::payload|info\.message|info\.location'; then
        echo "$panic_hook_body"
        fail "TC-CI-015 FAIL: panic hook 内で PanicHookInfo::payload/message/location を参照しています"
    fi
fi
echo "[TC-CI-014/015] PASS"

# --- TC-CI-012（補強）----------------------------------------------
# 具体型 `SqliteVaultRepository` の参照が usecase/presenter/error/view/input/io に漏れていないか
echo "[TC-CI-012] SqliteVaultRepository 具体型参照の漏洩監査"
leak_dirs=(
    "crates/shikomi-cli/src/usecase/"
    "crates/shikomi-cli/src/presenter/"
    "crates/shikomi-cli/src/io/"
)
leak_files=(
    "crates/shikomi-cli/src/error.rs"
    "crates/shikomi-cli/src/view.rs"
    "crates/shikomi-cli/src/input.rs"
    "crates/shikomi-cli/src/cli.rs"
)
for target in "${leak_dirs[@]}" "${leak_files[@]}"; do
    if [[ -e "$target" ]] && grep -rn 'SqliteVaultRepository' "$target" > /dev/null; then
        grep -rn 'SqliteVaultRepository' "$target"
        fail "TC-CI-012 FAIL: $target に SqliteVaultRepository 参照が漏れています（lib.rs の run() 周辺に限定する契約）"
    fi
done
echo "[TC-CI-012] PASS"

echo "ALL secret-path audits PASS"
