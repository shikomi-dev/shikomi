#!/usr/bin/env bash
# smoke-e2e.sh — E2E スモークテスト（TC-GUI-E01）
#
# 設計根拠: docs/features/shikomi-gui/build-ci/detailed-design/e2e.md §6.6
#
# 前提条件:
#   - cargo build --release -p shikomi-daemon -p shikomi-cli -p shikomi-gui 実行済み
#   - Linux 環境かつ xvfb インストール済み
#
# 終了コード:
#   0: 全検証 PASS（daemon 起動 / GUI 起動 / IPC 接続 / 正常終了）
#   1: いずれかの検証 FAIL
#
# shellcheck disable=SC2317  # cleanup 関数は trap 経由で呼ばれるため直接参照なし

set -euo pipefail

# ---------------------------------------------------------------------------
# 設定
# ---------------------------------------------------------------------------

# CI で XDG_RUNTIME_DIR が未設定の場合は /tmp を使用しソケットパスを確定させる
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
DAEMON_SOCKET_PATH="${XDG_RUNTIME_DIR}/shikomi/daemon.sock"

DAEMON_BIN="./target/release/shikomi-daemon"
GUI_BIN="./target/release/shikomi-gui"
CLI_BIN="./target/release/shikomi"

DAEMON_PID=""
GUI_PID=""
XVFB_PID=""

DAEMON_WAIT_SECS=10
GUI_WAIT_SECS=15
GUI_TERM_SECS=5

# ---------------------------------------------------------------------------
# cleanup — trap EXIT で必ず実行（対称性保証）
# 設計根拠: e2e.md §6.6 cleanup 関数
# ---------------------------------------------------------------------------
cleanup() {
    if [ -n "$GUI_PID" ]; then
        kill -TERM "$GUI_PID" 2>/dev/null || true
        wait "$GUI_PID" 2>/dev/null || true
    fi
    if [ -n "$DAEMON_PID" ]; then
        kill -TERM "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    if [ -n "$XVFB_PID" ]; then
        kill -TERM "$XVFB_PID" 2>/dev/null || true
        wait "$XVFB_PID" 2>/dev/null || true
    fi
}

# trap はスクリプト最初で宣言する（設計根拠: e2e.md §6.6 先頭宣言）
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Xvfb 起動（仮想ディスプレイ）
# ---------------------------------------------------------------------------
echo "[smoke] Starting Xvfb :99 ..."
Xvfb :99 -screen 0 1280x720x24 &
XVFB_PID=$!
echo "[smoke] Xvfb PID=$XVFB_PID"

# ---------------------------------------------------------------------------
# daemon 起動
# ---------------------------------------------------------------------------
echo "[smoke] Starting daemon ..."
"$DAEMON_BIN" &
DAEMON_PID=$!
echo "[smoke] Daemon PID=$DAEMON_PID"

# daemon ソケット待機（最大 DAEMON_WAIT_SECS 秒、0.5s ごとポーリング）
# 設計根拠: e2e.md §6.6 / §6.7
echo "[smoke] Waiting for daemon socket: $DAEMON_SOCKET_PATH"
WAITED=0
while ! [ -S "$DAEMON_SOCKET_PATH" ]; do
    if [ "$WAITED" -ge "$((DAEMON_WAIT_SECS * 2))" ]; then
        echo "[smoke] FAIL: daemon socket not created within ${DAEMON_WAIT_SECS}s"
        exit 1
    fi
    sleep 0.5
    WAITED=$((WAITED + 1))
done
echo "[smoke] Daemon socket ready (waited ~$((WAITED / 2))s)"

# ---------------------------------------------------------------------------
# GUI 起動
# ---------------------------------------------------------------------------
echo "[smoke] Starting GUI (DISPLAY=:99) ..."
DISPLAY=:99 "$GUI_BIN" &
GUI_PID=$!
echo "[smoke] GUI PID=$GUI_PID"

# GUI プロセス生存確認（最大 GUI_WAIT_SECS 秒、0.5s ごとポーリング）
# 設計根拠: e2e.md §6.6 / §6.7
echo "[smoke] Polling GUI process for ${GUI_WAIT_SECS}s ..."
WAITED=0
while [ "$WAITED" -lt "$((GUI_WAIT_SECS * 2))" ]; do
    if ! kill -0 "$GUI_PID" 2>/dev/null; then
        echo "[smoke] FAIL: GUI process exited unexpectedly"
        exit 1
    fi
    sleep 0.5
    WAITED=$((WAITED + 1))
done
echo "[smoke] GUI alive for ${GUI_WAIT_SECS}s — startup stable"

# ---------------------------------------------------------------------------
# IPC 接続確認
# 設計根拠: e2e.md §6.7 — shikomi list が exit 0 = IPC 接続証明
# ---------------------------------------------------------------------------
echo "[smoke] Checking IPC connection via 'shikomi list --ipc' ..."
if ! "$CLI_BIN" list --ipc; then
    echo "[smoke] FAIL: IPC connection check failed (shikomi list --ipc returned non-zero)"
    exit 1
fi
echo "[smoke] IPC connection OK"

# ---------------------------------------------------------------------------
# GUI 正常終了確認
# 設計根拠: e2e.md §6.6 / §6.7
# ---------------------------------------------------------------------------
echo "[smoke] Sending SIGTERM to GUI (PID=$GUI_PID) ..."
kill -TERM "$GUI_PID"
# wait はシェル組み込みのため timeout と直接組み合わせられない。
# ポーリングでプロセス終了を確認してから wait で回収する。
# 設計根拠: e2e.md §6.6 / §6.7
WAITED=0
while kill -0 "$GUI_PID" 2>/dev/null; do
    if [ "$WAITED" -ge "$((GUI_TERM_SECS * 2))" ]; then
        echo "[smoke] FAIL: GUI did not terminate within ${GUI_TERM_SECS}s after SIGTERM"
        exit 1
    fi
    sleep 0.5
    WAITED=$((WAITED + 1))
done
wait "$GUI_PID" 2>/dev/null || true
GUI_PID=""  # cleanup で二重 kill しないよう解除
echo "[smoke] GUI terminated cleanly"

# ---------------------------------------------------------------------------
# PASS
# ---------------------------------------------------------------------------
echo "[smoke] All checks PASSED"
exit 0
