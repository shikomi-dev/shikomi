#!/usr/bin/env bash
# Sub-C (#41) static contract checks — TC-C-I01..I04 (cargo-free).
#
# Complements cargo test (KAT + property) by enforcing integrity invariants
# via source-text grep, runnable without Rust toolchain. Pattern follows
# sub-a-static-checks.sh / sub-b-static-checks.sh.
#
# TC-C-I01 (CC-4 / C-15): expose_within_crate must NOT appear under
#          shikomi-infra/src/crypto/aead/ — adapter accesses key bytes
#          only via AeadKey::with_secret_bytes closure injection (kept
#          separate from KDF input accessors expose_secret_bytes /
#          expose_words which are pub by Sub-B Rev2 visibility policy).
# TC-C-I02 (CC-5 / C-16): aead/aes_gcm.rs must use Zeroizing<Vec<u8>>
#          for intermediate buffers; no bare `let mut buf: Vec<u8>` for
#          ciphertext/plaintext intermediates.
# TC-C-I03 (CC-11): aes_gcm crate must only be imported under
#          crypto/aead/ subtree (single entry point).
# TC-C-I04 (CC-8 / CC-12): no self-rolled byte-array equality on tags
#          or 32B keys under crypto/aead/ — comparison must delegate
#          to aes-gcm's constant-time path.
#
# Exit codes: 0 all pass / 1 at least one fail.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INFRA="$ROOT/crates/shikomi-infra/src"
AEAD_DIR="$INFRA/crypto/aead"

PASS=0
FAIL=0
RESULTS=()

emit() {
    local id="$1" status="$2" msg="$3"
    RESULTS+=("[$status] $id: $msg")
    if [[ "$status" == PASS ]]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
}

detail() {
    RESULTS+=("        $1")
}

if [[ ! -d "$INFRA" ]]; then
    echo "FATAL: $INFRA not found (workspace layout changed?)" >&2
    exit 1
fi

if [[ ! -d "$AEAD_DIR" ]]; then
    # Graceful skip: Sub-C 実装前は本スクリプトを fail させない。
    # impl PR で aead/ が出来た瞬間から各チェックが PASS/FAIL 判定する。
    echo "[SKIP] all Sub-C static checks (crates/shikomi-infra/src/crypto/aead absent)"
    exit 0
fi

# ======================================================================
# TC-C-I01 (CC-4 / C-15): expose_within_crate not used inside aead/
# ======================================================================
expose_hits=$(grep -rEn "expose_within_crate" "$AEAD_DIR" 2>/dev/null \
              | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)
if [[ -z "$expose_hits" ]]; then
    emit "TC-C-I01" "PASS" "no expose_within_crate calls inside crypto/aead/ (CC-4 / C-15: AeadKey closure path)"
else
    emit "TC-C-I01" "FAIL" "expose_within_crate leaked into crypto/aead/"
    while IFS= read -r line; do detail "$line"; done <<< "$expose_hits"
fi

# ======================================================================
# TC-C-I02 (CC-5 / C-16): Zeroizing<Vec<u8>> for intermediates
# ======================================================================
# Heuristic: detect `let mut <ident>: Vec<u8>` declarations in aes_gcm.rs
# that are NOT immediately followed (within the same line) by a Zeroizing
# wrapping. Tests blocks (#[cfg(test)] mod ...) are excluded heuristically
# by skipping files that match `*tests*` (none currently) and by stripping
# test-cfg modules with awk like sub-b-static-checks.sh.
aesgcm_file="$AEAD_DIR/aes_gcm.rs"
bad_intermediate=()
if [[ -f "$aesgcm_file" ]]; then
    prod=$(awk '
        BEGIN { in_test=0; depth=0 }
        /#\[cfg\(test\)\]/ { in_test=1; next }
        in_test && /^[[:space:]]*mod[[:space:]]+tests/ {
            depth=0
            if (match($0, /\{/)) depth=1
            next
        }
        in_test && depth>0 {
            for (i=1; i<=length($0); i++) {
                c=substr($0, i, 1)
                if (c=="{") depth++
                else if (c=="}") depth--
            }
            if (depth==0) in_test=0
            next
        }
        !in_test { print FILENAME ":" FNR ":" $0 }
    ' "$aesgcm_file" 2>/dev/null)
    bad=$(echo "$prod" | grep -E "let mut [a-zA-Z_][a-zA-Z0-9_]*[[:space:]]*:[[:space:]]*Vec<u8>" \
          | grep -vE "Zeroizing" || true)
    if [[ -n "$bad" ]]; then
        while IFS= read -r line; do bad_intermediate+=("$line"); done <<< "$bad"
    fi
fi
if [[ ${#bad_intermediate[@]} -eq 0 ]]; then
    emit "TC-C-I02" "PASS" "intermediate Vec<u8> in aes_gcm.rs is Zeroizing-wrapped (CC-5 / C-16)"
else
    emit "TC-C-I02" "FAIL" "bare Vec<u8> intermediate(s) without Zeroizing"
    for b in "${bad_intermediate[@]}"; do detail "$b"; done
fi

# ======================================================================
# TC-C-I03 (CC-11): aes_gcm imported only under crypto/aead/
# ======================================================================
aesgcm_import_hits=$(grep -rEn "use aes_gcm::|aes_gcm::Aes|aes_gcm::AeadCore|aes_gcm::Aead" \
                     --include='*.rs' "$INFRA" 2>/dev/null \
                     | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)
violations=()
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    file="${line%%:*}"
    if [[ "$file" != "$AEAD_DIR"* ]]; then
        violations+=("$line")
    fi
done <<< "$aesgcm_import_hits"

if [[ ${#violations[@]} -eq 0 ]]; then
    emit "TC-C-I03" "PASS" "aes_gcm crate imported only under crypto/aead/ (CC-11 single-entry)"
else
    emit "TC-C-I03" "FAIL" "aes_gcm crate imported outside crypto/aead/"
    for v in "${violations[@]}"; do detail "$v"; done
fi

# ======================================================================
# TC-C-I04 (CC-8 / CC-12): no self-rolled byte-array equality
# ======================================================================
# Detect patterns like `tag.as_array() == ...`, `[u8; 16] == ...` inside
# crypto/aead/ — must delegate to aes-gcm constant-time path.
ct_eq_hits=$(grep -rEn "as_array\(\)[[:space:]]*==|\[u8;[[:space:]]*(16|32)\][[:space:]]*==" \
             --include='*.rs' "$AEAD_DIR" 2>/dev/null \
             | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)
if [[ -z "$ct_eq_hits" ]]; then
    emit "TC-C-I04" "PASS" "no self-rolled tag/key equality inside crypto/aead/ (CC-8 / CC-12)"
else
    emit "TC-C-I04" "FAIL" "self-rolled byte-array equality detected"
    while IFS= read -r line; do detail "$line"; done <<< "$ct_eq_hits"
fi

# ======================================================================
# Bonus: contracts coverage map (CC-1..CC-14 → impl files)
# ======================================================================
coverage_table=(
    "CC-1 (NIST CAVP KAT)|crypto/aead/kat.rs + crypto/aead/aes_gcm.rs"
    "CC-2 (Aad 26B)|shikomi-core::vault::crypto_data::Aad reused"
    "CC-3 / C-14 (Verified構築禁止 on AEAD fail)|crypto/aead/aes_gcm.rs decrypt path"
    "CC-4 / C-15 (AeadKey closure)|TC-C-I01 above + shikomi-core::crypto::aead_key"
    "CC-5 / C-16 (Zeroizing buffers)|TC-C-I02 above"
    "CC-6 (wrap/unwrap roundtrip)|crypto/aead/aes_gcm.rs wrap_vek/unwrap_vek + TC-C-P02"
    "CC-7 (AAD swap attack)|TC-C-P01 property"
    "CC-8 (constant-time tag)|TC-C-I04 above"
    "CC-9 (no NonceCounter in adapter)|crypto/aead/aes_gcm.rs signatures"
    "CC-10 (no unwrap/expect)|sub-b-static-checks.sh shares pattern"
    "CC-11 (aes_gcm single-entry)|TC-C-I03 above"
    "CC-12 (subtle policy)|TC-C-I04 above"
    "CC-13 (AeadKey impl on Vek/Kek)|shikomi-core::crypto::{key, aead_key}"
    "CC-14 (VekProvider derive_new_wrapped_*)|TC-C-I05 (cargo check)"
)

# ======================================================================
# Summary
# ======================================================================
echo ""
echo "Sub-C static checks (#41):"
echo ""
for line in "${RESULTS[@]}"; do
    echo "$line"
done
echo ""
echo "Contracts coverage map (informational):"
for line in "${coverage_table[@]}"; do
    echo "  - ${line//|/ -> }"
done
echo ""
TOTAL=$((PASS + FAIL))
echo "Summary: $PASS/$TOTAL static checks passed."
[[ $FAIL -eq 0 ]] && exit 0 || exit 1
