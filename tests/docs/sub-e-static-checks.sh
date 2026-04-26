#!/usr/bin/env bash
# Sub-E (#43) static contract checks — TC-E-S01..S07.
#
# 設計書 SSoT: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md
# §14.9 Sub-E 静的検査 (grep gate)。Sub-D Rev3/Rev4 で凍結した「実装直読 SSoT
# + grep gate による設計書-実装一致機械検証」原則を Sub-E に継承。
#
# 完璧を嫌悪する者の儀式 (涅マユリ): 設計書数値 / variant 集合 / 関数シンボル /
# 拒否経路の存在を grep + awk で機械抽出し、実装ドリフトを構造封鎖する。
# テスト 1 件失敗 = remediation 行番号付きで提示、運用者は「設計書 SSoT を信じ、
# 実装を直読」する手順に倣って修正する。
#
# Coverage:
# - TC-E-S01 (C-22 ワイルドカード排除): v2_handler 配下の `match cache.state()`
#            / `match VaultUnlockState` / `match state` ブロックに bare wildcard
#            arm `^[[:space:]]+_[[:space:]]*=>` が混入していないこと。
# - TC-E-S02 (EC-7 VaultUnlockState variant 集合整合): cache/vek.rs から
#            `VaultUnlockState` の variant 名 (`Locked` / `Unlocked`) を抽出して
#            完全一致比較。
# - TC-E-S03 (EC-7 IpcRequest variant 集合整合): shikomi-core::ipc::request の
#            `IpcRequest` から V1 5 + V2 5 = 10 variants を抽出。
# - TC-E-S04 (EC-7 IpcResponse variant 集合整合): 同 V1 7 + V2 5 = 12 variants。
# - TC-E-S05 (EC-7 IpcErrorCode V2 4 新 variant 含有): `VaultLocked` /
#            `BackoffActive` / `RecoveryRequired` / `ProtocolDowngrade`。
# - TC-E-S06 (EC-8 Clean Arch): shikomi-core / shikomi-infra に OS API
#            (`DistributedNotificationCenter` / `WTSRegisterSessionNotification`
#            / `zbus` / `dbus` / `objc` / `windows::Win32::System::RemoteDesktop`)
#            の直接 import 不混入。OS purchase は shikomi-daemon 限定。
# - TC-E-S07 (C-28/C-29 handshake 許可リスト境界): `check_request_allowed`
#            関数の存在 / V1 許可セット (Handshake / ListRecords / AddRecord /
#            EditRecord / RemoveRecord) の明示列挙 / V2 専用セット
#            (Unlock / Lock / ChangePassword / RotateRecovery / Rekey) の
#            明示列挙 / `ProtocolDowngrade` 拒否経路 / `PreHandshake` 全拒否
#            の 5 要件を機械検証。
# - TC-E-S08 (Bug-E-001 方針B解決経路): `unlock_with_password` 経路の
#            `verify_header_aead` 失敗を `WrongPassword` に意味論再分類する
#            ヘルパ関数 `map_aead_failure_in_unlock_to_wrong_password` が
#            shikomi-infra に存在し、`tracing::warn!` で改竄通知を残す経路を
#            機械検証。Bug-E-001 (REQ-S11 / C-26 brute force backoff の現実
#            経路発動) を回帰させない構造防衛。
# - TC-E-S09 (Pegasus 工程5 致命指摘 ④ / C-32): `rekey.rs` / `rotate_recovery.rs`
#            に `cache_relocked: false` を明示的に返す経路 (`cache_relocked = false`
#            または `false,$` 末尾の if/match arm) が存在し、Lie-Then-Surprise
#            経路を構造的に拒絶していることを機械検証。fault-injection seam
#            (`FORCE_RELOCK_FAILURE`) は `cfg(test)` 限定で本番経路には混入しない
#            ことも合わせて確認。
#
# Exit codes: 0 all pass / 1 at least one fail.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CORE="$ROOT/crates/shikomi-core/src"
INFRA="$ROOT/crates/shikomi-infra/src"
DAEMON="$ROOT/crates/shikomi-daemon/src"
V2_HANDLER_DIR="$DAEMON/ipc/v2_handler"
CACHE_VEK_RS="$DAEMON/cache/vek.rs"
REQUEST_RS="$CORE/ipc/request.rs"
RESPONSE_RS="$CORE/ipc/response.rs"
ERROR_CODE_RS="$CORE/ipc/error_code.rs"
V2_HANDLER_MOD_RS="$V2_HANDLER_DIR/mod.rs"

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

if [[ ! -d "$CORE" ]] || [[ ! -d "$DAEMON" ]]; then
    echo "FATAL: shikomi crates not found at $ROOT" >&2
    exit 1
fi

# ======================================================================
# TC-E-S01: V2 handler 配下に bare wildcard `_ =>` arm が無い (C-22)
# ======================================================================
# v2_handler 配下の handler ファイルで `match cache.state()` / `match state` /
# `match VaultUnlockState` / `match VaultUnlockState::*` などの分岐に bare
# wildcard arm が含まれていないか機械検証。`#[non_exhaustive]` は defining
# crate 内では無効、`Locked` / `Unlocked { .. }` 全列挙時点で exhaustive。
# `_` arm を残すと将来 variant 追加時に test が先に壊れず、構造防衛が骨抜き。
#
# 注: dispatch_v2 (mod.rs) の `IpcRequest` match の `_` 防御 arm は cross-crate
# `#[non_exhaustive]` 対応として正当 (Rust では cross-crate 時 exhaustive 不可)。
# よって本 gate は v2_handler/{unlock,lock,change_password,rotate_recovery,rekey}.rs
# のみを対象とし、mod.rs の cross-crate 防御は除外する (TC-E-U05 / Sub-D
# Rev3 凍結方針継承)。
target_files=(
    "$V2_HANDLER_DIR/unlock.rs"
    "$V2_HANDLER_DIR/lock.rs"
    "$V2_HANDLER_DIR/change_password.rs"
    "$V2_HANDLER_DIR/rotate_recovery.rs"
    "$V2_HANDLER_DIR/rekey.rs"
)
wildcard_violations=()
for f in "${target_files[@]}"; do
    if [[ ! -f "$f" ]]; then
        continue
    fi
    # bare `_ =>` のみを抽出 (`{ .. } =>` や `Locked =>` 等は識別子始まりで該当しない)。
    hits=$(grep -nE '^[[:space:]]+_[[:space:]]*=>' "$f" 2>/dev/null || true)
    if [[ -n "$hits" ]]; then
        wildcard_violations+=("$f:")
        while IFS= read -r line; do wildcard_violations+=("  $line"); done <<< "$hits"
    fi
done

if [[ ${#wildcard_violations[@]} -eq 0 ]]; then
    emit "TC-E-S01" "PASS" "v2_handler 配下に bare wildcard '_ =>' arm 無し (C-22 maintain)"
    detail "checked: ${#target_files[@]} handler files"
else
    emit "TC-E-S01" "FAIL" "v2_handler に bare wildcard arm が混入 — C-22 構造防衛違反"
    for v in "${wildcard_violations[@]}"; do detail "$v"; done
    detail "remediation: Locked / Unlocked { .. } 全列挙の exhaustive match に修正、'_ =>' を削除せよ"
fi

# ======================================================================
# TC-E-S02: VaultUnlockState variant 集合整合 (EC-7)
# ======================================================================
if [[ -f "$CACHE_VEK_RS" ]]; then
    impl_variants=$(awk '
        /^pub enum VaultUnlockState/ { in_enum=1; next }
        in_enum && /^}/ { in_enum=0; exit }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*[(,{]/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*,?[[:space:]]*$/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
    ' "$CACHE_VEK_RS" | sort -u)
    impl_count=$(echo "$impl_variants" | grep -c .)

    expected=("Locked" "Unlocked")
    expected_set=$(printf '%s\n' "${expected[@]}" | sort -u)
    expected_count=${#expected[@]}

    if [[ "$impl_count" -eq "$expected_count" ]] && [[ "$impl_variants" == "$expected_set" ]]; then
        emit "TC-E-S02" "PASS" "VaultUnlockState has expected $expected_count variants matching grep-extracted impl set"
        detail "variants: $(echo "$impl_variants" | tr '\n' ' ')"
    else
        emit "TC-E-S02" "FAIL" "VaultUnlockState variant set drift (impl=$impl_count, expected=$expected_count)"
        detail "impl set:     $(echo "$impl_variants" | tr '\n' ' ')"
        detail "expected set: $(echo "$expected_set" | tr '\n' ' ')"
    fi
else
    emit "TC-E-S02" "FAIL" "$CACHE_VEK_RS not found"
fi

# ======================================================================
# TC-E-S03: IpcRequest variant 集合整合 (V1 5 + V2 5 = 10)
# ======================================================================
if [[ -f "$REQUEST_RS" ]]; then
    impl_variants=$(awk '
        /^pub enum IpcRequest/ { in_enum=1; next }
        in_enum && /^}/ { in_enum=0; exit }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*[(,{]/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*,?[[:space:]]*$/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
    ' "$REQUEST_RS" | sort -u)
    impl_count=$(echo "$impl_variants" | grep -c .)

    expected=("AddRecord" "ChangePassword" "EditRecord" "Handshake" "ListRecords" "Lock" "Rekey" "RemoveRecord" "RotateRecovery" "Unlock")
    expected_set=$(printf '%s\n' "${expected[@]}" | sort -u)
    expected_count=${#expected[@]}

    if [[ "$impl_count" -eq "$expected_count" ]] && [[ "$impl_variants" == "$expected_set" ]]; then
        emit "TC-E-S03" "PASS" "IpcRequest has expected $expected_count variants (V1 5 + V2 5)"
        detail "variants: $(echo "$impl_variants" | tr '\n' ' ')"
    else
        emit "TC-E-S03" "FAIL" "IpcRequest variant set drift (impl=$impl_count, expected=$expected_count)"
        detail "impl set:     $(echo "$impl_variants" | tr '\n' ' ')"
        detail "expected set: $(echo "$expected_set" | tr '\n' ' ')"
    fi
else
    emit "TC-E-S03" "FAIL" "$REQUEST_RS not found"
fi

# ======================================================================
# TC-E-S04: IpcResponse variant 集合整合 (V1 7 + V2 5 = 12)
# ======================================================================
if [[ -f "$RESPONSE_RS" ]]; then
    impl_variants=$(awk '
        /^pub enum IpcResponse/ { in_enum=1; next }
        in_enum && /^}/ { in_enum=0; exit }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*[(,{]/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*,?[[:space:]]*$/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
    ' "$RESPONSE_RS" | sort -u)
    impl_count=$(echo "$impl_variants" | grep -c .)

    expected=("Added" "Edited" "Error" "Handshake" "Locked" "PasswordChanged" "ProtocolVersionMismatch" "Records" "RecoveryRotated" "Rekeyed" "Removed" "Unlocked")
    expected_set=$(printf '%s\n' "${expected[@]}" | sort -u)
    expected_count=${#expected[@]}

    if [[ "$impl_count" -eq "$expected_count" ]] && [[ "$impl_variants" == "$expected_set" ]]; then
        emit "TC-E-S04" "PASS" "IpcResponse has expected $expected_count variants (V1 7 + V2 5)"
        detail "variants: $(echo "$impl_variants" | tr '\n' ' ')"
    else
        emit "TC-E-S04" "FAIL" "IpcResponse variant set drift (impl=$impl_count, expected=$expected_count)"
        detail "impl set:     $(echo "$impl_variants" | tr '\n' ' ')"
        detail "expected set: $(echo "$expected_set" | tr '\n' ' ')"
    fi
else
    emit "TC-E-S04" "FAIL" "$RESPONSE_RS not found"
fi

# ======================================================================
# TC-E-S05: IpcErrorCode V2 4 新 variant 含有
# ======================================================================
if [[ -f "$ERROR_CODE_RS" ]]; then
    impl_variants=$(awk '
        /^pub enum IpcErrorCode/ { in_enum=1; next }
        in_enum && /^}/ { in_enum=0; exit }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*[(,{]/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
        in_enum && /^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*,?[[:space:]]*$/ {
            match($0, /[A-Z][A-Za-z0-9_]*/)
            print substr($0, RSTART, RLENGTH)
        }
    ' "$ERROR_CODE_RS" | sort -u)

    required_v2=("VaultLocked" "BackoffActive" "RecoveryRequired" "ProtocolDowngrade" "Crypto")
    missing=()
    for v in "${required_v2[@]}"; do
        if ! echo "$impl_variants" | grep -qx "$v"; then
            missing+=("$v")
        fi
    done

    if [[ ${#missing[@]} -eq 0 ]]; then
        emit "TC-E-S05" "PASS" "IpcErrorCode contains all 5 V2 variants (VaultLocked / BackoffActive / RecoveryRequired / ProtocolDowngrade / Crypto)"
        detail "all variants: $(echo "$impl_variants" | tr '\n' ' ')"
    else
        emit "TC-E-S05" "FAIL" "IpcErrorCode V2 variants missing: ${missing[*]}"
        detail "impl set: $(echo "$impl_variants" | tr '\n' ' ')"
    fi
else
    emit "TC-E-S05" "FAIL" "$ERROR_CODE_RS not found"
fi

# ======================================================================
# TC-E-S06: Clean Arch (OS API は shikomi-daemon 限定) — EC-8
# ======================================================================
forbidden_os_apis=(
    "DistributedNotificationCenter"
    "WTSRegisterSessionNotification"
    "use zbus::"
    "use dbus::"
    "use objc::"
    "windows::Win32::System::RemoteDesktop"
)
clean_arch_violations=()
for pat in "${forbidden_os_apis[@]}"; do
    # shikomi-core / shikomi-infra に混入していないか確認
    for dir in "$CORE" "$INFRA"; do
        hits=$(grep -rEn "$pat" --include='*.rs' "$dir" 2>/dev/null \
               | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" \
               | grep -v "^[^:]*:[0-9]*:[[:space:]]*\*" \
               | grep -v "^[^:]*:[0-9]*:[[:space:]]*///" || true)
        if [[ -n "$hits" ]]; then
            clean_arch_violations+=("[$pat] in $dir:")
            while IFS= read -r line; do clean_arch_violations+=("  $line"); done <<< "$hits"
        fi
    done
done

if [[ ${#clean_arch_violations[@]} -eq 0 ]]; then
    emit "TC-E-S06" "PASS" "shikomi-core / shikomi-infra free of OS API imports (Clean Arch maintain)"
    detail "checked patterns: ${forbidden_os_apis[*]}"
else
    emit "TC-E-S06" "FAIL" "OS API leaked outside shikomi-daemon — EC-8 Clean Arch violation"
    for v in "${clean_arch_violations[@]}"; do detail "$v"; done
fi

# ======================================================================
# TC-E-S07: handshake 許可リスト境界 (C-28 / C-29)
# ======================================================================
if [[ -f "$V2_HANDLER_MOD_RS" ]]; then
    failures=()

    # (a) check_request_allowed 関数が存在
    if ! grep -qE "^pub fn check_request_allowed\b|^fn check_request_allowed\b" "$V2_HANDLER_MOD_RS"; then
        failures+=("(a) function 'check_request_allowed' not found")
    fi

    # (b) PreHandshake で Handshake のみ許可する分岐が存在
    if ! grep -qE "ClientState::PreHandshake" "$V2_HANDLER_MOD_RS"; then
        failures+=("(b) ClientState::PreHandshake branch not found (C-29 handshake 必須)")
    fi
    if ! grep -qE "matches!\(.*IpcRequest::Handshake" "$V2_HANDLER_MOD_RS"; then
        failures+=("(b2) PreHandshake で IpcRequest::Handshake のみ許可する判定が見当たらない")
    fi

    # (c) is_v2_only() ベースの V1 拒否経路
    if ! grep -qE "is_v2_only\(\)" "$V2_HANDLER_MOD_RS"; then
        failures+=("(c) is_v2_only() による V1 拒否経路が見当たらない (C-28)")
    fi

    # (d) ProtocolDowngrade を返す経路
    if ! grep -qE "IpcErrorCode::ProtocolDowngrade" "$V2_HANDLER_MOD_RS"; then
        failures+=("(d) IpcErrorCode::ProtocolDowngrade 拒否経路が見当たらない")
    fi

    # (e) IpcProtocolVersion::V1 / V2 / Unknown が分岐に列挙されている
    for v in V1 V2 Unknown; do
        if ! grep -qE "IpcProtocolVersion::$v" "$V2_HANDLER_MOD_RS"; then
            failures+=("(e) IpcProtocolVersion::$v が check_request_allowed 周辺で列挙されていない")
        fi
    done

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-E-S07" "PASS" "check_request_allowed 関数 + handshake 必須 + V1 拒否 + ProtocolDowngrade + V1/V2/Unknown 列挙 全要件 OK"
    else
        emit "TC-E-S07" "FAIL" "handshake 許可リスト境界に欠落あり (${#failures[@]} 件)"
        for f in "${failures[@]}"; do detail "$f"; done
        detail "remediation: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md §14.9 TC-E-S07 を SSoT として実装直読修正"
    fi
else
    emit "TC-E-S07" "FAIL" "$V2_HANDLER_MOD_RS not found (Sub-E impl 未配備)"
fi

# ======================================================================
# TC-E-S08: Bug-E-001 方針B解決経路 (verify_header_aead → WrongPassword 変換 + 改竄ログ)
# ======================================================================
VAULT_MIGRATION_SERVICE_RS="$ROOT/crates/shikomi-infra/src/persistence/vault_migration/service.rs"
if [[ -f "$VAULT_MIGRATION_SERVICE_RS" ]]; then
    failures=()

    # (a) ヘルパ関数の存在
    if ! grep -qE "fn map_aead_failure_in_unlock_to_wrong_password" "$VAULT_MIGRATION_SERVICE_RS"; then
        failures+=("(a) map_aead_failure_in_unlock_to_wrong_password 関数が見当たらない")
    fi

    # (b) AeadTagMismatch → WrongPassword 変換が含まれる
    #     awk で関数本体内に AeadTagMismatch arm + WrongPassword 経路が両方あるか確認
    body=$(awk '
        /^fn map_aead_failure_in_unlock_to_wrong_password/ { in_fn=1; print; next }
        in_fn && /^}[[:space:]]*$/ { print; in_fn=0; exit }
        in_fn { print }
    ' "$VAULT_MIGRATION_SERVICE_RS")
    if ! echo "$body" | grep -qE "AeadTagMismatch"; then
        failures+=("(b) ヘルパ関数本体に AeadTagMismatch arm が見当たらない")
    fi
    if ! echo "$body" | grep -qE "WrongPassword"; then
        failures+=("(b2) ヘルパ関数本体に WrongPassword への変換が見当たらない")
    fi

    # (c) tracing::warn! で改竄通知を残す経路 (運用診断維持)
    if ! echo "$body" | grep -qE "tracing::warn!"; then
        failures+=("(c) ヘルパ関数本体に tracing::warn! 改竄通知ログが見当たらない")
    fi

    # (d) target に shikomi_infra::vault_migration が指定されている
    if ! echo "$body" | grep -qE 'target:[[:space:]]*"shikomi_infra::vault_migration"'; then
        failures+=("(d) tracing::warn! の target が 'shikomi_infra::vault_migration' に設定されていない")
    fi

    # (e) unlock_internal_with_password から本ヘルパが呼ばれている
    #     verify_header_aead 呼出と map_err(map_aead_failure_in_unlock_to_wrong_password) が
    #     近接して存在することを確認 (実装は改行分割されているため単一行 grep ではなく
    #     `unlock_internal_with_password` 関数本体内に両方が出現することで判定)。
    unlock_fn_body=$(awk '
        /fn unlock_internal_with_password/ { in_fn=1; print; next }
        in_fn && /^[[:space:]]{4}\}[[:space:]]*$/ { print; in_fn=0; exit }
        in_fn { print }
    ' "$VAULT_MIGRATION_SERVICE_RS")
    if ! echo "$unlock_fn_body" | grep -qE "verify_header_aead"; then
        failures+=("(e1) unlock_internal_with_password 関数本体に verify_header_aead 呼出が見当たらない")
    fi
    if ! echo "$unlock_fn_body" | grep -qE "map_aead_failure_in_unlock_to_wrong_password"; then
        failures+=("(e2) unlock_internal_with_password 関数本体に map_aead_failure_in_unlock_to_wrong_password 呼出が見当たらない (Bug-E-001 回帰経路)")
    fi

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-E-S08" "PASS" "Bug-E-001 方針B解決経路 (AeadTagMismatch→WrongPassword 変換 + tracing::warn 改竄通知 + unlock 経路で map_err 呼出) 全要件 OK"
    else
        emit "TC-E-S08" "FAIL" "Bug-E-001 方針B解決経路に欠落あり (${#failures[@]} 件) — REQ-S11 / C-26 brute force backoff 機能不全の回帰経路"
        for f in "${failures[@]}"; do detail "$f"; done
        detail "remediation: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md §14.13 Bug-E-001 解決経路 SSoT 参照"
    fi
else
    emit "TC-E-S08" "FAIL" "$VAULT_MIGRATION_SERVICE_RS not found"
fi

# ======================================================================
# TC-E-S09: cache_relocked: false 経路 + fault-injection seam の cfg(test) 限定
# ======================================================================
REKEY_RS="$DAEMON/ipc/v2_handler/rekey.rs"
ROTATE_RS="$DAEMON/ipc/v2_handler/rotate_recovery.rs"

if [[ -f "$REKEY_RS" ]] && [[ -f "$ROTATE_RS" ]]; then
    failures=()

    # (a) rekey.rs / rotate_recovery.rs 双方に cache_relocked = false 経路が存在
    for f in "$REKEY_RS" "$ROTATE_RS"; do
        # `false` を返す arm (= false, または false$ パターン)
        if ! grep -qE 'cache_relocked[[:space:]]*=[[:space:]]*false|false,?\s*$|=>[[:space:]]*false' "$f"; then
            failures+=("(a) $f に cache_relocked=false 経路が見当たらない (Lie-Then-Surprise 構造防衛)")
        fi
        # tracing::warn の cache_relocked=false 通知ログが存在
        if ! grep -qE "cache_relocked=false" "$f"; then
            failures+=("(a2) $f に cache_relocked=false の tracing::warn 通知が見当たらない")
        fi
    done

    # (b) FORCE_RELOCK_FAILURE が cfg(debug_assertions) 限定であること
    #     (release build には実体がコンパイルされない、攻撃面ゼロ)
    if ! grep -qE '#\[cfg\(debug_assertions\)\]' "$REKEY_RS"; then
        failures+=("(b) rekey.rs に #[cfg(debug_assertions)] 属性が見当たらない (FORCE_RELOCK_FAILURE 露出経路)")
    fi
    # FORCE_RELOCK_FAILURE 宣言が cfg(debug_assertions) 直下にあるか確認
    cfg_dbg_line=$(grep -n '#\[cfg(debug_assertions)\]' "$REKEY_RS" | head -n1 | cut -d: -f1)
    if [[ -n "$cfg_dbg_line" ]]; then
        # cfg(debug_assertions) の直後数行内に FORCE_RELOCK_FAILURE 宣言が出現するか
        range=$(awk -v start="$cfg_dbg_line" 'NR>=start && NR<=start+5' "$REKEY_RS")
        if ! echo "$range" | grep -qE "FORCE_RELOCK_FAILURE"; then
            failures+=("(b2) rekey.rs の #[cfg(debug_assertions)] 直後に FORCE_RELOCK_FAILURE 宣言が見当たらない")
        fi
    fi

    # (c) release ビルド (cfg(not(debug_assertions))) で force_failure が常に false
    for f in "$REKEY_RS" "$ROTATE_RS"; do
        if ! grep -qE '#\[cfg\(not\(debug_assertions\)\)\]' "$f"; then
            failures+=("(c) $f に #[cfg(not(debug_assertions))] フォールバック経路が見当たらない (release で force_failure 強制 false)")
        fi
    done

    if [[ ${#failures[@]} -eq 0 ]]; then
        emit "TC-E-S09" "PASS" "Pegasus 致命指摘④解消経路 (cache_relocked=false 経路 + cfg(test) 限定 fault-injection seam) 全要件 OK"
    else
        emit "TC-E-S09" "FAIL" "cache_relocked=false 経路 / cfg(test) 限定 seam に欠落あり (${#failures[@]} 件) — Lie-Then-Surprise 回帰経路"
        for fmsg in "${failures[@]}"; do detail "$fmsg"; done
        detail "remediation: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md §14.13.4 TC-E-I06c SSoT 参照"
    fi
else
    emit "TC-E-S09" "FAIL" "rekey.rs または rotate_recovery.rs not found"
fi

# ======================================================================
# Summary
# ======================================================================
echo ""
echo "Sub-E static checks (#43):"
echo ""
for line in "${RESULTS[@]}"; do
    echo "$line"
done
echo ""
TOTAL=$((PASS + FAIL))
echo "Summary: $PASS/$TOTAL static checks passed."
[[ $FAIL -eq 0 ]] && exit 0 || exit 1
