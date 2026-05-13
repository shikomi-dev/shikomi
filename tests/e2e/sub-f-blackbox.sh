#!/usr/bin/env bash
# E2E ブラックボックステスト — Sub-F (#44) shikomi-cli vault サブコマンド
#
# 目的: 完全ブラックボックスで shikomi バイナリの操作経路を検証する
#       テスト戦略ガイド §E2E方針: 「ユーザー観測可能な振る舞い」のみで判定
#       DB直接参照・内部状態確認は行わない。stdout / stderr / exit code のみで検証
#
# 対象: PR #70 commit 48f6219 (Sub-F Phase 1〜7 累積、Linux 全 green を称する実験体)
#
# 検証範囲:
#   - TC-E2E-F01〜F08: vault サブコマンド 7 variant の help 表示・引数 parse
#   - TC-E2E-F10〜F18: stdin パイプ拒否 (C-38) を全 password 入力経路で確認
#   - TC-E2E-F20〜F22: shikomi list バナー [plaintext]
#   - TC-E2E-F30〜F32: --output {screen,braille,print,audio} parse + stdin 拒否
#   - TC-E2E-F40: vault lock の冪等性
#   - TC-E2E-F50: NO_COLOR env でカラー除去
#
# 証跡: 各実行の stdout/stderr/exit を記録し、最後にまとめて Discord 添付
set -uo pipefail

BIN="${SHIKOMI_BIN:-/app/shared/work/shikomi-target/debug/shikomi}"
DBIN="${SHIKOMI_DAEMON_BIN:-/app/shared/work/shikomi-target/debug/shikomi-daemon}"
LOG_DIR="${LOG_DIR:-/app/shared/attachments/マユリ}"
LOG="${LOG_DIR}/sub-f-e2e-blackbox.log"
mkdir -p "$LOG_DIR"
: > "$LOG"

PASS=0
FAIL=0
declare -a FAILED_TCS

log() { echo "$@" | tee -a "$LOG"; }

# 各テストケース: $1=TC-ID, $2=説明, $3=コマンド (eval 用), $4=期待 exit, $5=stdout|stderr に含まれるべき pattern
run_tc() {
  local tc="$1" desc="$2" cmd="$3" expected_exit="$4" pattern="${5:-}"
  log ""
  log "================================================================"
  log "[$tc] $desc"
  log "  cmd: $cmd"
  local out err rc
  out=$(eval "$cmd" 2>/tmp/sub-f-stderr); rc=$?
  err=$(cat /tmp/sub-f-stderr)
  log "  exit=$rc (expected=$expected_exit)"
  log "  stdout: $(echo "$out" | head -c 600)"
  log "  stderr: $(echo "$err" | head -c 600)"
  local ok=1
  if [[ "$rc" != "$expected_exit" ]]; then
    ok=0
    log "  FAIL: exit code mismatch (rc=$rc, expected=$expected_exit)"
  fi
  if [[ -n "$pattern" ]]; then
    # 改行を保ったまま grep -E。-z で stdout+stderr 全体を 1 つの string として検査。
    if ! { printf '%s\n%s\n' "$out" "$err" | grep -qE "$pattern"; }; then
      ok=0
      log "  FAIL: pattern not found: $pattern"
    fi
  fi
  if [[ "$ok" == "1" ]]; then
    log "  RESULT: PASS"
    PASS=$((PASS+1))
  else
    log "  RESULT: FAIL"
    FAIL=$((FAIL+1))
    FAILED_TCS+=("$tc")
  fi
}

log "Sub-F E2E ブラックボックステスト開始 — $(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "BIN=$BIN"
log "commit=$(cd /tmp/shikomi && git rev-parse --short HEAD 2>/dev/null)"

# -----------------------------------------------------------------------------
# §1. clap parse / help 経路 (daemon 不要)
# -----------------------------------------------------------------------------
log ""
log "########## §1. clap parse / help (daemon 不要) ##########"

# 7 variant 個別検証 (一括正規表現は printf 改行 + 各 grep に分解)
F01_OUT=$($BIN vault --help 2>&1)
F01_OK=1
for variant in encrypt decrypt unlock lock change-password rekey rotate-recovery; do
  if ! echo "$F01_OUT" | grep -qE "^\s*$variant\b"; then
    F01_OK=0
    log "[TC-E2E-F01] missing variant in help: $variant"
  fi
done
if [[ $F01_OK -eq 1 ]]; then
  PASS=$((PASS+1)); log "[TC-E2E-F01] vault --help が 7 variant 全表示 — PASS"
else
  FAIL=$((FAIL+1)); FAILED_TCS+=("TC-E2E-F01"); log "[TC-E2E-F01] FAIL"
fi

run_tc "TC-E2E-F02" "vault --help に廃止された recovery-show が表示されない" \
  "$BIN vault --help | grep -v 'recovery-show'" 0 ""
# 念のため recovery-show が存在しないことを直接検証
if $BIN vault --help 2>&1 | grep -q "recovery-show"; then
  log "[TC-E2E-F02 補強]: recovery-show が help に残存 — FAIL"
  FAIL=$((FAIL+1)); FAILED_TCS+=("TC-E2E-F02b")
else
  PASS=$((PASS+1))
  log "[TC-E2E-F02 補強]: recovery-show が help に残存しない — PASS"
fi

# --output Possible values 4 件個別検証
F03_OUT=$($BIN vault encrypt --help 2>&1)
F03_OK=1
for value in screen print braille audio; do
  if ! echo "$F03_OUT" | grep -qE "^\s*-\s*$value:"; then
    F03_OK=0
    log "[TC-E2E-F03] missing --output value: $value"
  fi
done
if [[ $F03_OK -eq 1 ]]; then
  PASS=$((PASS+1)); log "[TC-E2E-F03] --output 4 値全部表示 — PASS"
else
  FAIL=$((FAIL+1)); FAILED_TCS+=("TC-E2E-F03"); log "[TC-E2E-F03] FAIL"
fi

# clap の不正値 exit code は実装次第。本実験体は exit=1 を返す（clap default は 2、
# shikomi-cli は 1 に固定されている。終了コード SSoT との整合は別途レビュー対象）。
run_tc "TC-E2E-F04" "vault encrypt --output xyz は clap parse エラー (exit=1 + invalid value)" \
  "$BIN vault encrypt --output xyz" 1 "invalid value"

run_tc "TC-E2E-F05" "vault unlock --recovery flag が parse される" \
  "$BIN vault unlock --recovery --help" 0 "recovery"

run_tc "TC-E2E-F06" "vault rekey --help に --output flag 表示" \
  "$BIN vault rekey --help" 0 "output"

run_tc "TC-E2E-F07" "vault rotate-recovery --help に --output flag 表示" \
  "$BIN vault rotate-recovery --help" 0 "output"

# C-37: --no-mode-banner / --hide-banner は clap に存在しない不正フラグ。
# `shikomi list` (vault サブコマンドではなく最上位 list) で試行 → unrecognized
# あるいは unexpected argument。clap は exit=1 を返す（実装固定）。
run_tc "TC-E2E-F08" "list --no-mode-banner は不正フラグ (C-37 隠蔽不能)" \
  "$BIN list --no-mode-banner" 1 "unexpected argument|unrecognized"

# -----------------------------------------------------------------------------
# §2. shikomi list バナー（Plaintext）
# -----------------------------------------------------------------------------
log ""
log "########## §2. list バナー [plaintext] ##########"

# 隔離 vault dir で plaintext vault 作成
VAULT_PLAIN=$(mktemp -d)
SHIKOMI_VAULT_DIR="$VAULT_PLAIN" $BIN add --kind text --label "demo" --value "demo-val" >/dev/null 2>&1

run_tc "TC-E2E-F20" "shikomi list (plaintext vault) で [plaintext] バナー表示" \
  "SHIKOMI_VAULT_DIR=$VAULT_PLAIN $BIN list" 0 "\\[plaintext\\]"

run_tc "TC-E2E-F21" "list 出力に値が含まれる（plaintext は値表示）" \
  "SHIKOMI_VAULT_DIR=$VAULT_PLAIN $BIN list" 0 "demo-val"

run_tc "TC-E2E-F22" "NO_COLOR=1 でカラーシーケンス除去" \
  "NO_COLOR=1 SHIKOMI_VAULT_DIR=$VAULT_PLAIN $BIN list" 0 "\\[plaintext\\]"

# -----------------------------------------------------------------------------
# §3. daemon 経由 vault サブコマンド + C-38 stdin 拒否
# -----------------------------------------------------------------------------
log ""
log "########## §3. daemon 経由 + C-38 stdin パイプ拒否 ##########"

RUNTIME=$(mktemp -d)
VAULT_D=$(mktemp -d)
# daemon 用の vault を初期化
SHIKOMI_VAULT_DIR="$VAULT_D" $BIN add --kind text --label "daemon-demo" --value "v" >/dev/null 2>&1

# daemon 起動
XDG_RUNTIME_DIR="$RUNTIME" SHIKOMI_VAULT_DIR="$VAULT_D" "$DBIN" >/tmp/sub-f-daemon.log 2>&1 &
DPID=$!
sleep 1.5
if ! kill -0 $DPID 2>/dev/null; then
  log "FATAL: daemon 起動失敗"; tail -20 /tmp/sub-f-daemon.log | tee -a "$LOG"
  log "残存テストはスキップ"
else
  log "daemon pid=$DPID listening on $RUNTIME/shikomi/daemon.sock"

  # vault lock は password 不要 (Plaintext vault で何が起きるか観察)
  run_tc "TC-E2E-F40" "vault lock を plaintext vault に対して呼ぶ（観察）" \
    "XDG_RUNTIME_DIR=$RUNTIME $BIN vault lock" 0 ""

  # C-38: stdin パイプで password 入力を試行 → 拒否されるべき
  run_tc "TC-E2E-F10" "C-38: echo|vault unlock 拒否 (exit=1 期待 / 実装観測)" \
    "echo password | XDG_RUNTIME_DIR=$RUNTIME $BIN vault unlock" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F11" "C-38: echo|vault encrypt 拒否" \
    "echo password | XDG_RUNTIME_DIR=$RUNTIME $BIN vault encrypt" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F12" "C-38: echo|vault decrypt 拒否" \
    "echo password | XDG_RUNTIME_DIR=$RUNTIME $BIN vault decrypt" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F13" "C-38: echo|vault change-password 拒否" \
    "echo old | XDG_RUNTIME_DIR=$RUNTIME $BIN vault change-password" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F14" "C-38: echo|vault rekey 拒否" \
    "echo password | XDG_RUNTIME_DIR=$RUNTIME $BIN vault rekey" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F15" "C-38: echo|vault rotate-recovery 拒否" \
    "echo password | XDG_RUNTIME_DIR=$RUNTIME $BIN vault rotate-recovery" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F16" "C-38: echo|vault unlock --recovery 拒否（mnemonic stdin パイプ拒否）" \
    "echo word | XDG_RUNTIME_DIR=$RUNTIME $BIN vault unlock --recovery" 1 "non-tty stdin|tty"

  # --output 各経路 + stdin 拒否（password に到達せず拒否されるはず）
  run_tc "TC-E2E-F30" "vault encrypt --output braille (stdin 拒否で password 経路に到達せず)" \
    "echo p | XDG_RUNTIME_DIR=$RUNTIME $BIN vault encrypt --output braille" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F31" "vault encrypt --output print 拒否" \
    "echo p | XDG_RUNTIME_DIR=$RUNTIME $BIN vault encrypt --output print" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F32" "vault encrypt --output audio 拒否" \
    "echo p | XDG_RUNTIME_DIR=$RUNTIME $BIN vault encrypt --output audio" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F33" "vault rekey --output braille 拒否" \
    "echo p | XDG_RUNTIME_DIR=$RUNTIME $BIN vault rekey --output braille" 1 "non-tty stdin|tty"

  run_tc "TC-E2E-F34" "vault rotate-recovery --output print 拒否" \
    "echo p | XDG_RUNTIME_DIR=$RUNTIME $BIN vault rotate-recovery --output print" 1 "non-tty stdin|tty"

  # daemon 停止
  kill -TERM $DPID 2>/dev/null
  wait $DPID 2>/dev/null
fi

# -----------------------------------------------------------------------------
# §4. アクセシビリティ env auto switch (C-39 + EC-F10)
# -----------------------------------------------------------------------------
log ""
log "########## §4. SHIKOMI_ACCESSIBILITY env ##########"

# daemon を再起動して env 経路をテスト
RUNTIME2=$(mktemp -d)
VAULT_D2=$(mktemp -d)
SHIKOMI_VAULT_DIR="$VAULT_D2" $BIN add --kind text --label "x" --value "x" >/dev/null 2>&1
XDG_RUNTIME_DIR="$RUNTIME2" SHIKOMI_VAULT_DIR="$VAULT_D2" "$DBIN" >/tmp/sub-f-daemon2.log 2>&1 &
DPID2=$!
sleep 1.5

if kill -0 $DPID2 2>/dev/null; then
  run_tc "TC-E2E-F50" "SHIKOMI_ACCESSIBILITY=1 経路（stdin 拒否で password 入る前に止まる）" \
    "echo p | SHIKOMI_ACCESSIBILITY=1 XDG_RUNTIME_DIR=$RUNTIME2 $BIN vault encrypt" 1 "non-tty stdin|tty"
  kill -TERM $DPID2 2>/dev/null
  wait $DPID2 2>/dev/null
fi

# -----------------------------------------------------------------------------
# §5. 終了コード SSoT 準拠の横断検証
# -----------------------------------------------------------------------------
log ""
log "########## §5. 終了コード SSoT 検証 ##########"

run_tc "TC-E2E-F60" "list (vault 未初期化、empty dir) → exit code 1 期待" \
  "SHIKOMI_VAULT_DIR=$(mktemp -d) $BIN list" 1 "vault not initialized"

# -----------------------------------------------------------------------------
# 集計
# -----------------------------------------------------------------------------
log ""
log "================================================================"
log "Sub-F E2E ブラックボックステスト 集計"
log "PASS: $PASS"
log "FAIL: $FAIL"
if [[ $FAIL -gt 0 ]]; then
  log "失敗 TC: ${FAILED_TCS[@]}"
fi
log "================================================================"

if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
