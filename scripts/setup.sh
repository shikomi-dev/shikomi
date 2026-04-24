#!/usr/bin/env bash
# shikomi — POSIX dev environment bootstrap.
#
# 設計書: docs/features/dev-workflow/detailed-design.md §scripts/setup.sh のステップ契約
#
# 冪等性 (REQ-DW-009): 既導入ツールは cargo install の "already up to date" で 0 秒終了、
# GitHub Releases 経由のバイナリは command -v 検出で skip。
#
# 使い方:
#   bash scripts/setup.sh           # フル実行 (lefthook install を含む)
#   bash scripts/setup.sh --tools-only  # ツール配置のみ (CI が呼ぶ)

set -euo pipefail

# -------- ピン定数 (step 2) ------------------------------------------
# 値は upstream の公式 checksum から転記する。空値なら即 Fail Fast。
# 更新方法: `gh release view v<NEW_VER> -R evilmartians/lefthook --json assets`
#           `curl -L .../lefthook_<VER>_checksums.txt` から転記し、
#           `setup.ps1` の同名変数と完全同期させる (audit-pin-sync.sh が CI で検証)。
LEFTHOOK_VERSION="2.1.6"
# upstream: https://github.com/evilmartians/lefthook/releases/download/v2.1.6/lefthook_checksums.txt
LEFTHOOK_SHA256_LINUX_X86_64="fab3d2715a922d9625c9024e6ffb6e1271edd613aa9b213c2049482cde8ae183"
LEFTHOOK_SHA256_LINUX_ARM64="3fd749629968beb7f7f68cd0fc7b1b5ab801a1ec2045892586005cce75944118"
LEFTHOOK_SHA256_MACOS_X86_64="93c6d51823f94a7f26a2bbb84f59504378b178f55d6c90744169693ed3e89013"
LEFTHOOK_SHA256_MACOS_ARM64="f07c97c32376749edb5b34179c16c6d87dd3e7ca0040aee911f38c821de0daab"
LEFTHOOK_SHA256_WINDOWS_X86_64="6704b01a72414affcc921740a7d6c621fe60c3082b291c9730900a2c6a352516"

GITLEAKS_VERSION="8.30.1"
GITLEAKS_SHA256_LINUX_X86_64="551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb"
GITLEAKS_SHA256_LINUX_ARM64="e4a487ee7ccd7d3a7f7ec08657610aa3606637dab924210b3aee62570fb4b080"
GITLEAKS_SHA256_MACOS_X86_64="dfe101a4db2255fc85120ac7f3d25e4342c3c20cf749f2c20a18081af1952709"
GITLEAKS_SHA256_MACOS_ARM64="b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5"
GITLEAKS_SHA256_WINDOWS_X86_64="d29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e"

TOOLS_ONLY=0
case "${1:-}" in
    --tools-only) TOOLS_ONLY=1 ;;
    "") ;;
    *) echo "[FAIL] unknown argument: $1" >&2; exit 2 ;;
esac

# -------- メッセージヘルパ -------------------------------------------
# MSG 文言は docs/features/dev-workflow/detailed-design.md §MSG 確定文言表に一致
msg_fail_toolchain() {
    echo "[FAIL] Rust toolchain が未検出です。" >&2
    echo "次のコマンド: https://rustup.rs/ の手順に従って rustup を導入してください。" >&2
}
msg_fail_no_git() {
    echo "[FAIL] .git/ ディレクトリが見つかりません。リポジトリルートで実行してください。" >&2
    echo "現在のディレクトリ: $(pwd)" >&2
}
msg_fail_sha() {
    local tool="$1"
    echo "[FAIL] ${tool} バイナリの SHA256 検証に失敗しました。サプライチェーン改ざんの可能性があります。" >&2
    echo "次のコマンド: 一時ファイルを削除後にネットワーク状況を確認し再実行。繰り返し失敗する場合は Issue で報告してください。" >&2
}
msg_skip() { echo "[SKIP] $1 は既にインストール済みです。"; echo "バージョン: $2"; }
msg_ok_setup() { echo "[OK] Setup complete. Git フックが有効化されました。"; }

# -------- step 2: ピン定数の空値チェック ------------------------------
for var in LEFTHOOK_VERSION LEFTHOOK_SHA256_LINUX_X86_64 LEFTHOOK_SHA256_LINUX_ARM64 \
           LEFTHOOK_SHA256_MACOS_X86_64 LEFTHOOK_SHA256_MACOS_ARM64 LEFTHOOK_SHA256_WINDOWS_X86_64 \
           GITLEAKS_VERSION GITLEAKS_SHA256_LINUX_X86_64 GITLEAKS_SHA256_LINUX_ARM64 \
           GITLEAKS_SHA256_MACOS_X86_64 GITLEAKS_SHA256_MACOS_ARM64 GITLEAKS_SHA256_WINDOWS_X86_64; do
    if [[ -z "${!var}" ]]; then
        echo "[FAIL] pin 定数 ${var} が空です。setup.sh の冒頭で値を確定してください。" >&2
        exit 1
    fi
done

# -------- step 3: cwd 検査 -------------------------------------------
if [[ ! -d ".git" ]]; then
    msg_fail_no_git
    exit 1
fi

# -------- step 4: Rust toolchain 検査 --------------------------------
if ! command -v rustc > /dev/null 2>&1 || ! command -v cargo > /dev/null 2>&1; then
    msg_fail_toolchain
    exit 1
fi

# -------- プラットフォーム判定 ---------------------------------------
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux)  case "$arch" in
                    x86_64) echo "linux_x86_64" ;;
                    aarch64|arm64) echo "linux_arm64" ;;
                    *) echo "unsupported"; return 1 ;;
                esac ;;
        Darwin) case "$arch" in
                    x86_64) echo "macos_x86_64" ;;
                    arm64) echo "macos_arm64" ;;
                    *) echo "unsupported"; return 1 ;;
                esac ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows_x86_64" ;;
        *) echo "unsupported"; return 1 ;;
    esac
}

PLATFORM="$(detect_platform)"
if [[ "$PLATFORM" == "unsupported" ]]; then
    echo "[FAIL] 未サポートのプラットフォーム: $(uname -s) $(uname -m)" >&2
    exit 1
fi

# -------- cargo install helper (step 5 / 6) --------------------------
# 既導入なら [SKIP]、未導入なら cargo install --locked
install_cargo_tool() {
    local tool="$1"
    if command -v "$tool" > /dev/null 2>&1; then
        local ver
        ver="$("$tool" --version 2>/dev/null | head -1 || echo "unknown")"
        msg_skip "$tool" "$ver"
        return 0
    fi
    cargo install --locked "$tool"
}

install_cargo_tool just
install_cargo_tool convco

# -------- GitHub Releases 経由ツール (step 7 / 8) ---------------------
# ~/.cargo/bin は PATH に含まれている前提 (Rust toolchain 済み)
BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
mkdir -p "$BIN_DIR"

sha256_of() {
    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# lefthook は .gz 単体圧縮 (tarball ではない)。gunzip でバイナリを直接取り出す。
install_lefthook() {
    if command -v lefthook > /dev/null 2>&1; then
        msg_skip "lefthook" "$(lefthook version 2>/dev/null || echo unknown)"
        return 0
    fi
    local asset expected url tmp
    case "$PLATFORM" in
        linux_x86_64)   asset="lefthook_${LEFTHOOK_VERSION}_Linux_x86_64.gz"; expected="$LEFTHOOK_SHA256_LINUX_X86_64" ;;
        linux_arm64)    asset="lefthook_${LEFTHOOK_VERSION}_Linux_arm64.gz";  expected="$LEFTHOOK_SHA256_LINUX_ARM64" ;;
        macos_x86_64)   asset="lefthook_${LEFTHOOK_VERSION}_MacOS_x86_64.gz"; expected="$LEFTHOOK_SHA256_MACOS_X86_64" ;;
        macos_arm64)    asset="lefthook_${LEFTHOOK_VERSION}_MacOS_arm64.gz";  expected="$LEFTHOOK_SHA256_MACOS_ARM64" ;;
        windows_x86_64) asset="lefthook_${LEFTHOOK_VERSION}_Windows_x86_64.gz"; expected="$LEFTHOOK_SHA256_WINDOWS_X86_64" ;;
    esac
    url="https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/${asset}"
    tmp="$(mktemp)"
    trap 'rm -f "$tmp" "${tmp}.bin"' RETURN
    curl -sSfL "$url" -o "$tmp"
    local actual
    actual="$(sha256_of "$tmp")"
    if [[ "$actual" != "$expected" ]]; then
        rm -f "$tmp"
        msg_fail_sha "lefthook"
        exit 1
    fi
    gunzip -c "$tmp" > "${tmp}.bin"
    local dst="$BIN_DIR/lefthook"
    [[ "$PLATFORM" == "windows_x86_64" ]] && dst="$BIN_DIR/lefthook.exe"
    mv "${tmp}.bin" "$dst"
    [[ "$PLATFORM" != "windows_x86_64" ]] && chmod +x "$dst"
}

# gitleaks は tar.gz (Windows は zip)。Windows からは Git Bash 経由で tar が使える前提。
install_gitleaks() {
    if command -v gitleaks > /dev/null 2>&1; then
        msg_skip "gitleaks" "$(gitleaks version 2>/dev/null || echo unknown)"
        return 0
    fi
    local asset expected url tmp ext
    case "$PLATFORM" in
        linux_x86_64)   asset="gitleaks_${GITLEAKS_VERSION}_linux_x64.tar.gz";    expected="$GITLEAKS_SHA256_LINUX_X86_64";   ext="tar.gz" ;;
        linux_arm64)    asset="gitleaks_${GITLEAKS_VERSION}_linux_arm64.tar.gz";  expected="$GITLEAKS_SHA256_LINUX_ARM64";    ext="tar.gz" ;;
        macos_x86_64)   asset="gitleaks_${GITLEAKS_VERSION}_darwin_x64.tar.gz";   expected="$GITLEAKS_SHA256_MACOS_X86_64";   ext="tar.gz" ;;
        macos_arm64)    asset="gitleaks_${GITLEAKS_VERSION}_darwin_arm64.tar.gz"; expected="$GITLEAKS_SHA256_MACOS_ARM64";    ext="tar.gz" ;;
        windows_x86_64) asset="gitleaks_${GITLEAKS_VERSION}_windows_x64.zip";     expected="$GITLEAKS_SHA256_WINDOWS_X86_64"; ext="zip" ;;
    esac
    url="https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/${asset}"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    curl -sSfL "$url" -o "$tmp/$asset"
    local actual
    actual="$(sha256_of "$tmp/$asset")"
    if [[ "$actual" != "$expected" ]]; then
        rm -rf "$tmp"
        msg_fail_sha "gitleaks"
        exit 1
    fi
    if [[ "$ext" == "zip" ]]; then
        unzip -qq "$tmp/$asset" -d "$tmp"
    else
        tar -xzf "$tmp/$asset" -C "$tmp"
    fi
    local bin="$tmp/gitleaks"
    [[ "$PLATFORM" == "windows_x86_64" ]] && bin="$tmp/gitleaks.exe"
    local dst="$BIN_DIR/gitleaks"
    [[ "$PLATFORM" == "windows_x86_64" ]] && dst="$BIN_DIR/gitleaks.exe"
    mv "$bin" "$dst"
    [[ "$PLATFORM" != "windows_x86_64" ]] && chmod +x "$dst"
}

install_lefthook
install_gitleaks

# -------- step 9: lefthook install (--tools-only では skip) -----------
if [[ "$TOOLS_ONLY" -eq 0 ]]; then
    lefthook install
fi

# -------- step 10: 完了ログ ------------------------------------------
msg_ok_setup
