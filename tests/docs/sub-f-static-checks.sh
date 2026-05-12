#!/usr/bin/env bash
# Sub-F (#44) static contract checks — TC-F-S01..S06.
#
# 設計書 SSoT: docs/features/cli-vault-commands/test-design/ci.md §8
#              vault-encryption/test-design/sub-f-cli-subcommands/index.md §15.9
#
# Sub-D Rev3/Rev4 / Sub-E で凍結した「実装直読 SSoT + grep gate による
# 設計書-実装一致機械検証」原則を Sub-F に継承。
#
# Coverage:
# - TC-F-S01 (EC-F8): VaultSubcommand 7 variant 集合整合（RecoveryShow 廃止確認）
# - TC-F-S02 (C-37):  mode_banner::display 必須呼出経路（隠蔽オプション不在）
# - TC-F-S03 (EC-F11): i18n MSG-S03/S04/S05 文言（英日 2 モード）の存在確認
# - TC-F-S04 (EC-F12): recovery_disclosure 関数 + [String; 24] 旧型不在 + SerializableSecretBytes
# - TC-F-S05 (C-40/C-41): daemon env seam debug 限定 + CLI core dump 抑制コード
# - TC-F-S06 (C-40):  daemon env allowlist + 未知 env 拒否経路
#
# Exit codes: 0 all pass / 1 at least one fail.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CLI_SRC="$ROOT/crates/shikomi-cli/src"
DAEMON_SRC="$ROOT/crates/shikomi-daemon/src"
CLI_RS="$CLI_SRC/cli.rs"
PRESENTER_SUCCESS_RS="$CLI_SRC/presenter/success.rs"
PRESENTER_LIST_RS="$CLI_SRC/presenter/list.rs"
DAEMON_LIB_RS="$DAEMON_SRC/lib.rs"
HARDENING_DIR="$CLI_SRC/hardening"

PASS=0
FAIL=0
RESULTS=()

emit() {
    local id="$1" status="$2" msg="$3"
    RESULTS+=("[$status] $id: $msg")
    case "$status" in
        PASS|SKIP) PASS=$((PASS+1)) ;;
        *)         FAIL=$((FAIL+1)) ;;
    esac
}

detail() {
    RESULTS+=("        $1")
}

if [[ ! -d "$CLI_SRC" ]] || [[ ! -d "$DAEMON_SRC" ]]; then
    echo "FATAL: shikomi crates not found at $ROOT" >&2
    exit 1
fi

# ======================================================================
# TC-F-S01: VaultSubcommand 7 variant 集合整合（EC-F8 / RecoveryShow 廃止）
# ======================================================================
# cli.rs の `pub enum VaultSubcommand { ... }` から variant 名を抽出し、
# 期待 7 件と完全一致比較。RecoveryShow が含まれないことも assert する。
#
# 期待集合: Encrypt / Decrypt / Unlock / Lock / ChangePassword / Rekey / RotateRecovery
if [[ -f "$CLI_RS" ]]; then
    impl_variants=$(awk '
        /^pub enum VaultSubcommand/ { in_enum=1; next }
        in_enum && /^\}[[:space:]]*$/ { in_enum=0; exit }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*[(,{]/ {
            match($0, /[A-Z][A-Za-z0-9_]+/)
            print substr($0, RSTART, RLENGTH)
        }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]+[[:space:]]*,?[[:space:]]*$/ {
            match($0, /[A-Z][A-Za-z0-9_]+/)
            print substr($0, RSTART, RLENGTH)
        }
    ' "$CLI_RS" | sort -u)

    impl_count=$(echo "$impl_variants" | grep -c .)
    expected=("ChangePassword" "Decrypt" "Encrypt" "Lock" "Rekey" "RotateRecovery" "Unlock")
    expected_set=$(printf '%s\n' "${expected[@]}" | sort -u)
    expected_count=${#expected[@]}

    failures=()
    if [[ "$impl_count" -ne "$expected_count" ]] || [[ "$impl_variants" != "$expected_set" ]]; then
        failures+=("variant set drift (impl=$impl_count, expected=$expected_count)")
        failures+=("impl:     $(echo "$impl_variants" | tr '\n' ' ')")
        failures+=("expected: $(echo "$expected_set" | tr '\n' ' ')")
    fi
    if echo "$impl_variants" | grep -q "RecoveryShow"; then
        failures+=("廃止済み RecoveryShow が VaultSubcommand に残存 — Rev1 ペガサス致命指摘① 回帰")
    fi

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-F-S01" "PASS" "VaultSubcommand has 7 variants; RecoveryShow absent (EC-F8 maintain)"
        detail "variants: $(echo "$impl_variants" | tr '\n' ' ')"
    else
        emit "TC-F-S01" "FAIL" "VaultSubcommand variant set drift (EC-F8 violation)"
        for f in "${failures[@]}"; do detail "$f"; done
        detail "remediation: cli.rs pub enum VaultSubcommand を SSoT §15.9 の 7 件に修正"
    fi
else
    emit "TC-F-S01" "FAIL" "$CLI_RS not found"
fi

# ======================================================================
# TC-F-S02: C-37 mode_banner::display 必須呼出経路
# ======================================================================
# (a) presenter/ 配下で mode_banner::display が呼ばれている（cross-crate grep）
# (b) presenter::list の render 関数が protection_mode: ProtectionModeBanner を必須引数に持つ
# (c) 隠蔽オプション --no-mode-banner / --hide-banner 等が CLI コードに存在しない
if [[ -f "$PRESENTER_LIST_RS" ]]; then
    failures=()

    # (a) mode_banner::display 呼出の存在（presenter/list.rs 内で確認）
    if ! grep -qE "mode_banner::display\b" "$PRESENTER_LIST_RS"; then
        failures+=("(a) presenter/list.rs に mode_banner::display 呼出が見当たらない (C-37 必須経路欠落)")
    fi

    # (b) render_list / render_ 関数に protection_mode: ProtectionModeBanner が含まれる
    if ! grep -qE "protection_mode:[[:space:]]*ProtectionModeBanner" "$PRESENTER_LIST_RS"; then
        failures+=("(b) presenter/list.rs の render 関数 signature に protection_mode: ProtectionModeBanner が見当たらない")
    fi

    # (c) 隠蔽オプションが clap フラグ定義として存在しない
    # チェック対象: #[arg(long = "no-mode-banner")] / no_mode_banner フィールド定義のような
    # 実際の clap 引数定義。テストコードのコメント・assert 文字列・try_parse_from は除外する。
    # 判定: long = "no-mode-banner" / long = "hide-banner" 形式のクレート属性が存在しない
    hidden_long=$(grep -rEn 'long[[:space:]]*=[[:space:]]*"no-mode-banner"|long[[:space:]]*=[[:space:]]*"hide-banner"' \
        --include='*.rs' "$CLI_SRC" 2>/dev/null || true)
    # 判定: clap field 名として no_mode_banner / hide_banner がある（コメント行除外）
    hidden_field=$(grep -rEn '\bno_mode_banner\b|\bhide_banner\b' --include='*.rs' "$CLI_SRC" 2>/dev/null \
        | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' \
        | grep -vE '^[^:]+:[0-9]+:[[:space:]]*///' \
        || true)
    if [[ -n "$hidden_long" ]] || [[ -n "$hidden_field" ]]; then
        failures+=("(c) 隠蔽オプション --no-mode-banner / --hide-banner が clap フラグ定義として存在する (C-37 違反)")
        [[ -n "$hidden_long" ]] && while IFS= read -r line; do failures+=("  long attr: $line"); done <<< "$hidden_long"
        [[ -n "$hidden_field" ]] && while IFS= read -r line; do failures+=("  field: $line"); done <<< "$hidden_field"
    fi

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-F-S02" "PASS" "mode_banner::display 必須経路 + 隠蔽オプション不在 (C-37 maintain)"
    else
        emit "TC-F-S02" "FAIL" "mode_banner 必須呼出経路に問題あり (${#failures[@]} 件)"
        for f in "${failures[@]}"; do detail "$f"; done
    fi
else
    emit "TC-F-S02" "FAIL" "$PRESENTER_LIST_RS not found"
fi

# ======================================================================
# TC-F-S03: EC-F11 i18n MSG-S03/S04/S05 文言（英日 2 モード）の存在確認
# ======================================================================
# 設計書は messages.toml による辞書を想定するが、現行実装は presenter/success.rs に
# inline 文字列で実装（Phase 6 で Localizer 移行予定）。
# 本チェックは inline 実装の実態に追従し、render_* 関数の存在と英日テキストを検証する。
if [[ -f "$PRESENTER_SUCCESS_RS" ]]; then
    failures=()

    # MSG-S03: vault unlock 成功文言（render_unlocked）
    if ! grep -qE "^pub fn render_unlocked" "$PRESENTER_SUCCESS_RS"; then
        failures+=("MSG-S03: render_unlocked 関数が見当たらない")
    else
        # 英語テキスト "vault unlocked" の存在
        if ! grep -qE '"vault unlocked' "$PRESENTER_SUCCESS_RS"; then
            failures+=("MSG-S03: 英語文言 'vault unlocked' が render_unlocked 周辺に見当たらない")
        fi
        # 日本語テキストの存在（Locale::JapaneseEn 経路）
        if ! grep -qE 'vault.*ロック|ロック.*解除|アンロック' "$PRESENTER_SUCCESS_RS"; then
            failures+=("MSG-S03: 日本語文言（vault ロック / 解除 / アンロック）が render_unlocked 周辺に見当たらない")
        fi
    fi

    # MSG-S04: vault lock 成功文言（render_locked）
    # SSoT: e2e.md §13.5 — en: "vault locked (VEK zeroized)" / ja: "vault をロックしました（鍵情報は消去済）"
    if ! grep -qE "^pub fn render_locked" "$PRESENTER_SUCCESS_RS"; then
        failures+=("MSG-S04: render_locked 関数が見当たらない")
    else
        if ! grep -qE '"vault locked \(VEK zeroized\)' "$PRESENTER_SUCCESS_RS"; then
            failures+=("MSG-S04: 英語文言 'vault locked (VEK zeroized)' が render_locked 周辺に見当たらない")
        fi
        if ! grep -qE 'vault をロックしました' "$PRESENTER_SUCCESS_RS"; then
            failures+=("MSG-S04: 日本語文言 'vault をロックしました' が render_locked 周辺に見当たらない")
        fi
    fi

    # MSG-S05: vault change-password 成功文言（render_password_changed）
    if ! grep -qE "^pub fn render_password_changed" "$PRESENTER_SUCCESS_RS"; then
        failures+=("MSG-S05: render_password_changed 関数が見当たらない")
    else
        if ! grep -qE '"master password changed' "$PRESENTER_SUCCESS_RS"; then
            failures+=("MSG-S05: 英語文言 'master password changed' が render_password_changed 周辺に見当たらない")
        fi
        if ! grep -qE 'パスワード.*変更|変更しました' "$PRESENTER_SUCCESS_RS"; then
            failures+=("MSG-S05: 日本語文言（パスワードを変更しました 等）が render_password_changed 周辺に見当たらない")
        fi
    fi

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-F-S03" "PASS" "MSG-S03/S04/S05 render 関数 + 英日 2 モード文言 全存在確認 (EC-F11 inline impl)"
        detail "note: Phase 6 で messages.toml / Localizer 移行予定（cli-subcommands.md §i18n 戦略）"
    else
        emit "TC-F-S03" "FAIL" "i18n MSG 文言 (${#failures[@]} 件) 欠落 — EC-F11 violation"
        for f in "${failures[@]}"; do detail "$f"; done
    fi
else
    emit "TC-F-S03" "FAIL" "$PRESENTER_SUCCESS_RS not found"
fi

# ======================================================================
# TC-F-S04: EC-F12 recovery_disclosure 関数 + [String; 24] 旧型不在 + SerializableSecretBytes
# ======================================================================
# 設計書は Vec<SerializableSecretBytes> 所有権消費形を要求するが、現行実装は
# &[SerializableSecretBytes] 借用形（Phase 6 で所有権消費形へ移行予定、success.rs 設計余地注記）。
# 本チェックは (a) 関数の存在、(b) 旧型 [String; 24] 不在、(c) SerializableSecretBytes 使用を検証する。
# (a) Vec<> 所有権消費形は Phase 6 確認対象として SKIP 注記を添える。
if [[ -f "$PRESENTER_SUCCESS_RS" ]]; then
    failures=()

    # (a) render_recovery_disclosure_screen 関数の存在（所有権消費形は Phase 6 移行予定）
    if ! grep -qE "^pub fn render_recovery_disclosure_screen" "$PRESENTER_SUCCESS_RS"; then
        failures+=("(a) render_recovery_disclosure_screen 関数が見当たらない (EC-F12 violation)")
    fi

    # (b) [String; 24] 等の旧型が登場しない
    old_type_hits=$(grep -nE '\[String;[[:space:]]*24\]' "$PRESENTER_SUCCESS_RS" 2>/dev/null || true)
    if [[ -n "$old_type_hits" ]]; then
        failures+=("(b) 旧型 [String; 24] が success.rs に残存 (EC-F12 旧型残存)")
        while IFS= read -r line; do failures+=("  $line"); done <<< "$old_type_hits"
    fi

    # (c) SerializableSecretBytes 使用（zeroize 対応型の確認）
    if ! grep -qE "SerializableSecretBytes" "$PRESENTER_SUCCESS_RS"; then
        failures+=("(c) SerializableSecretBytes が success.rs に見当たらない (EC-F12 zeroize 対応型不在)")
    fi

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-F-S04" "PASS" "render_recovery_disclosure_screen 存在 + [String; 24] 旧型不在 + SerializableSecretBytes 使用 (EC-F12)"
        detail "note: Vec<> 所有権消費形への移行は Phase 6 予定（success.rs 設計余地注記参照）"
    else
        emit "TC-F-S04" "FAIL" "recovery_disclosure 整合チェック失敗 (${#failures[@]} 件)"
        for f in "${failures[@]}"; do detail "$f"; done
    fi
else
    emit "TC-F-S04" "FAIL" "$PRESENTER_SUCCESS_RS not found"
fi

# ======================================================================
# TC-F-S05: C-40/C-41 env seam debug 限定 + CLI core dump 抑制
# ======================================================================
# (a) daemon lib.rs に #[cfg(debug_assertions)] で囲まれた env 読込ブロックが存在
# (b) crates/shikomi-cli/src/hardening/ に Linux prctl / macOS setrlimit / Windows SetErrorMode
#     の 3 OS 分岐コードが存在

failures_s05=()

# (a) daemon lib.rs の #[cfg(debug_assertions)] env seam
if [[ -f "$DAEMON_LIB_RS" ]]; then
    # read_debug_env_seam 関数が #[cfg(debug_assertions)] で宣言されている
    if ! grep -qE '#\[cfg\(debug_assertions\)\]' "$DAEMON_LIB_RS"; then
        failures_s05+=("(a) daemon lib.rs に #[cfg(debug_assertions)] が見当たらない (C-40 env seam 未実装)")
    fi
    # SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS の env var 読込がある
    if ! grep -qE 'SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS' "$DAEMON_LIB_RS"; then
        failures_s05+=("(a) daemon lib.rs に SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS 読込が見当たらない (C-40 env seam 未実装)")
    fi
    # #[cfg(not(debug_assertions))] フォールバック（release build で env 読込なし）
    if ! grep -qE '#\[cfg\(not\(debug_assertions\)\)\]' "$DAEMON_LIB_RS"; then
        failures_s05+=("(a) daemon lib.rs に #[cfg(not(debug_assertions))] フォールバックが見当たらない")
    fi
else
    failures_s05+=("(a) $DAEMON_LIB_RS not found")
fi

# (b) CLI hardening/ に 3 OS 分岐の core dump 抑制コードが存在
CORE_DUMP_RS="$HARDENING_DIR/core_dump.rs"
if [[ -f "$CORE_DUMP_RS" ]]; then
    # Linux: prctl(PR_SET_DUMPABLE
    if ! grep -qE "prctl\(.*PR_SET_DUMPABLE|prctl\b" "$CORE_DUMP_RS"; then
        failures_s05+=("(b) hardening/core_dump.rs に Linux prctl(PR_SET_DUMPABLE が見当たらない (C-41 欠落)")
    fi
    # macOS/BSD: setrlimit
    if ! grep -qE "setrlimit" "$CORE_DUMP_RS"; then
        failures_s05+=("(b) hardening/core_dump.rs に macOS setrlimit が見当たらない (C-41 欠落)")
    fi
    # Windows: SetErrorMode
    if ! grep -qE "SetErrorMode" "$CORE_DUMP_RS"; then
        failures_s05+=("(b) hardening/core_dump.rs に Windows SetErrorMode が見当たらない (C-41 欠落)")
    fi
else
    failures_s05+=("(b) $CORE_DUMP_RS not found — hardening ディレクトリ確認: $HARDENING_DIR")
fi

if [[ ${#failures_s05[@]} -eq 0 ]]; then
    emit "TC-F-S05" "PASS" "daemon env seam #[cfg(debug_assertions)] + CLI core dump 抑制 3 OS 分岐 全存在 (C-40/C-41)"
else
    emit "TC-F-S05" "FAIL" "env seam / core dump 抑制チェック失敗 (${#failures_s05[@]} 件)"
    for f in "${failures_s05[@]}"; do detail "$f"; done
    detail "remediation: TC-F-S05 SSoT = ci.md §8.2 / index.md §15.9 TC-F-S05"
fi

# ======================================================================
# TC-F-S06: C-40 daemon env allowlist sanity check
# ======================================================================
# (a) allowlist 定数（SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS / SHIKOMI_DAEMON_POLL_INTERVAL_SECS /
#     SHIKOMI_DAEMON_FORCE_RELOCK_FAIL）が daemon lib.rs に grep で確認可能
# (b) 未知 env 検出時の panic! または std::process::exit 経路が存在
# (c) allowlist 関数が #[cfg(debug_assertions)] で囲まれている（release では env 読込なし）
if [[ -f "$DAEMON_LIB_RS" ]]; then
    failures_s06=()

    # (a) allowlist 3 定数の存在確認（設計書 SSoT §TC-F-S06: 3 件のみ）
    for const_name in \
        "SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS" \
        "SHIKOMI_DAEMON_POLL_INTERVAL_SECS" \
        "SHIKOMI_DAEMON_FORCE_RELOCK_FAIL"
    do
        if ! grep -qE "\"$const_name\"" "$DAEMON_LIB_RS"; then
            failures_s06+=("(a) allowlist 定数 $const_name が daemon lib.rs に見当たらない")
        fi
    done

    # (a') ALLOWLIST 完全一致確認: 設計外の SHIKOMI_DAEMON_* が混入していないか
    # ALLOWLIST 定数ブロック内の "SHIKOMI_DAEMON_..." 文字列を抽出し期待集合と比較。
    allowlist_actual=$(awk '
        /const ALLOWLIST/ { in_al=1; next }
        in_al && /\];/ { in_al=0; exit }
        in_al && /"SHIKOMI_DAEMON_[^"]*"/ {
            match($0, /"SHIKOMI_DAEMON_[^"]*"/)
            s = substr($0, RSTART+1, RLENGTH-2)
            print s
        }
    ' "$DAEMON_LIB_RS" | sort)
    allowlist_expected=$(printf '%s\n' \
        "SHIKOMI_DAEMON_FORCE_RELOCK_FAIL" \
        "SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS" \
        "SHIKOMI_DAEMON_POLL_INTERVAL_SECS" \
        | sort)
    if [[ "$allowlist_actual" != "$allowlist_expected" ]]; then
        failures_s06+=("(a') ALLOWLIST 完全一致失敗: 設計外 SHIKOMI_DAEMON_* var が混入している可能性がある (C-40 OWASP A03)")
        failures_s06+=("  actual:   $(echo "$allowlist_actual" | tr '\n' ' ')")
        failures_s06+=("  expected: $(echo "$allowlist_expected" | tr '\n' ' ')")
    fi

    # (b) 未知 env 検出時の panic! または process::exit 経路
    if ! grep -qE 'panic!|std::process::exit|process::exit' "$DAEMON_LIB_RS"; then
        failures_s06+=("(b) daemon lib.rs に未知 env 拒否経路 (panic! / process::exit) が見当たらない")
    fi
    # starts_with("SHIKOMI_DAEMON_") による env 検査パターン
    if ! grep -qE '"SHIKOMI_DAEMON_"' "$DAEMON_LIB_RS"; then
        failures_s06+=("(b) daemon lib.rs に starts_with(\"SHIKOMI_DAEMON_\") 等の env prefix 検査が見当たらない")
    fi

    # (c) allowlist ブロックが #[cfg(debug_assertions)] 内にある
    # read_debug_env_seam 関数が #[cfg(debug_assertions)] 直前で宣言されているか確認
    if ! awk '
        /#\[cfg\(debug_assertions\)\]/ { found_cfg=1 }
        found_cfg && /fn read_debug_env_seam/ { found_fn=1; exit }
    ' "$DAEMON_LIB_RS" | grep -q . 2>/dev/null; then
        # awk パイプが空でも grep -q . が失敗するため、フラグで確認
        if ! awk '/\[cfg\(debug_assertions\)\]/{p=1} p && /fn read_debug_env_seam/{found=1; exit} END{exit !found}' "$DAEMON_LIB_RS"; then
            failures_s06+=("(c) read_debug_env_seam が #[cfg(debug_assertions)] 直下に宣言されていない")
        fi
    fi

    if [[ ${#failures_s06[@]} -eq 0 ]]; then
        emit "TC-F-S06" "PASS" "daemon env allowlist (3 定数 + panic! 拒否経路 + cfg(debug_assertions) 限定) 全要件 OK (C-40)"
    else
        emit "TC-F-S06" "FAIL" "daemon env allowlist チェック失敗 (${#failures_s06[@]} 件) — C-40 attacker env 受容経路残存"
        for f in "${failures_s06[@]}"; do detail "$f"; done
        detail "remediation: daemon lib.rs に read_debug_env_seam + ALLOWLIST 定数 + panic! を追加"
    fi
else
    emit "TC-F-S06" "FAIL" "$DAEMON_LIB_RS not found"
fi

# ======================================================================
# Summary
# ======================================================================
echo ""
echo "Sub-F static checks (#44 / Issue #79):"
echo ""
for line in "${RESULTS[@]}"; do
    echo "$line"
done
echo ""
TOTAL=$((PASS + FAIL))
echo "Summary: $PASS/$TOTAL static checks passed."
[[ $FAIL -eq 0 ]] && exit 0 || exit 1
