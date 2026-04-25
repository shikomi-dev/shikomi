#!/usr/bin/env bash
# shellcheck shell=bash
#
# CI audit script — secret 経路契約を静的に検証する。
#
# 対応 TC:
# - TC-CI-013: `crates/shikomi-cli/src/` 配下で `expose_secret` 呼び出しが 0 件
# - TC-CI-014: panic hook 内で `tracing::*` マクロを呼ばない
# - TC-CI-015: `crates/shikomi-core/src/ipc/` 配下で `expose_secret` が 0 件（daemon-ipc）
# - TC-CI-016: `crates/shikomi-cli/src/io/` 配下で `expose_secret` が 0 件（daemon-ipc 拡張）
# - TC-CI-017: `crates/shikomi-daemon/src/` 配下で `expose_secret` が 0 件（daemon-ipc）
# - TC-CI-018: `crates/shikomi-core/src/ipc/` 配下で `rmp_serde::Raw` / `RawRef` が 0 件
# - TC-CI-019: `crates/shikomi-daemon/src/` 配下の `unsafe {` が `permission/{unix,windows,windows_acl}.rs` 限定
# - TC-CI-023/024: daemon panic hook 内で tracing / payload 参照禁止
# - TC-CI-026: `crates/shikomi-cli/src/` 配下の `unsafe {` が `io/windows_sid.rs` 限定
# - TC-CI-027: `SHIKOMI_DAEMON_SKIP_*` env 読取コードが本番 src/ に 0 件
#
# 設計根拠:
# - docs/features/cli-vault-commands/test-design/ci.md §1.1
# - docs/features/daemon-ipc/test-design/ci.md §1〜2

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

# --- TC-CI-015 ------------------------------------------------------
echo "[TC-CI-015] expose_secret in shikomi-core/src/ipc/"
if matches="$(grep -rn 'expose_secret' crates/shikomi-core/src/ipc/ 2>/dev/null)"; then
    echo "$matches"
    fail "TC-CI-015 FAIL: crates/shikomi-core/src/ipc/ 配下に expose_secret 呼び出しが存在します"
fi
echo "[TC-CI-015] PASS"

# --- TC-CI-016 ------------------------------------------------------
echo "[TC-CI-016] expose_secret in shikomi-cli/src/io/"
if matches="$(grep -rn 'expose_secret' crates/shikomi-cli/src/io/ 2>/dev/null)"; then
    echo "$matches"
    fail "TC-CI-016 FAIL: crates/shikomi-cli/src/io/ 配下に expose_secret 呼び出しが存在します"
fi
echo "[TC-CI-016] PASS"

# --- TC-CI-017 ------------------------------------------------------
echo "[TC-CI-017] expose_secret in shikomi-daemon/src/"
if matches="$(grep -rn 'expose_secret' crates/shikomi-daemon/src/ 2>/dev/null)"; then
    echo "$matches"
    fail "TC-CI-017 FAIL: crates/shikomi-daemon/src/ 配下に expose_secret 呼び出しが存在します"
fi
echo "[TC-CI-017] PASS"

# --- TC-CI-018 ------------------------------------------------------
echo "[TC-CI-018] rmp_serde::Raw / RawRef in shikomi-core/src/ipc/"
if matches="$(grep -rnE 'rmp_serde::(Raw|RawRef)|::Raw\b|::RawRef\b' crates/shikomi-core/src/ipc/ 2>/dev/null)"; then
    echo "$matches"
    fail "TC-CI-018 FAIL: crates/shikomi-core/src/ipc/ 配下で rmp_serde::Raw/RawRef が使われています"
fi
echo "[TC-CI-018] PASS"

# --- TC-CI-019 ------------------------------------------------------
echo "[TC-CI-019] unsafe blocks outside permission/ (shikomi-daemon)"
if matches="$(grep -rnE 'unsafe[[:space:]]*\{' crates/shikomi-daemon/src/ \
    --include='*.rs' \
    | grep -v 'crates/shikomi-daemon/src/permission/unix.rs' \
    | grep -v 'crates/shikomi-daemon/src/permission/windows.rs' \
    | grep -v 'crates/shikomi-daemon/src/permission/windows_acl.rs' \
    || true)"; then
    if [[ -n "$matches" ]]; then
        echo "$matches"
        fail "TC-CI-019 FAIL: crates/shikomi-daemon/src/permission/{unix,windows,windows_acl}.rs 以外で unsafe ブロックが存在します"
    fi
fi
echo "[TC-CI-019] PASS"

# --- TC-CI-023 / 024 ------------------------------------------------
echo "[TC-CI-023/024] daemon panic hook audit"
panic_hook_body="$(awk '/fn panic_hook\(/,/^}$/' \
    crates/shikomi-daemon/src/lib.rs \
    crates/shikomi-daemon/src/main.rs \
    crates/shikomi-daemon/src/panic_hook.rs 2>/dev/null || true)"
if [[ -n "$panic_hook_body" ]]; then
    if echo "$panic_hook_body" | grep -qE 'tracing::'; then
        echo "$panic_hook_body"
        fail "TC-CI-023 FAIL: daemon panic hook 内で tracing マクロが呼ばれています"
    fi
    if echo "$panic_hook_body" | grep -qE '\.payload\(\)|info\.payload|PanicHookInfo::payload|info\.message|info\.location'; then
        echo "$panic_hook_body"
        fail "TC-CI-024 FAIL: daemon panic hook 内で PanicHookInfo::payload/message/location を参照しています"
    fi
fi
echo "[TC-CI-023/024] PASS"

# --- TC-CI-026 ------------------------------------------------------
echo "[TC-CI-026] unsafe blocks outside io/windows_sid.rs (shikomi-cli)"
if matches="$(grep -rnE 'unsafe[[:space:]]*\{' crates/shikomi-cli/src/ \
    --include='*.rs' \
    | grep -v 'crates/shikomi-cli/src/io/windows_sid.rs' \
    || true)"; then
    if [[ -n "$matches" ]]; then
        echo "$matches"
        fail "TC-CI-026 FAIL: crates/shikomi-cli/src/io/windows_sid.rs 以外で unsafe ブロックが存在します"
    fi
fi
echo "[TC-CI-026] PASS"

# --- TC-CI-027 ------------------------------------------------------
echo "[TC-CI-027] SHIKOMI_DAEMON_SKIP_* env read in production src/"
if matches="$(grep -rnE 'env::var.*SHIKOMI_DAEMON_SKIP|std::env::var.*SHIKOMI_DAEMON_SKIP' \
    crates/shikomi-daemon/src/ \
    crates/shikomi-cli/src/ \
    --include='*.rs' 2>/dev/null)"; then
    echo "$matches"
    fail "TC-CI-027 FAIL: SHIKOMI_DAEMON_SKIP_* env 読取コードが本番 src/ に存在します"
fi
echo "[TC-CI-027] PASS"

echo "ALL secret-path audits PASS"
