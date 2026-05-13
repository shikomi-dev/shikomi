#!/usr/bin/env bash
# Sub-B (#40) static contract checks — TC-B-I02 / I03 / I04 (cargo-free).
#
# These checks complement cargo test (KDF/RNG/Gate unit + KAT) by enforcing
# integrity invariants via source-text grep, runnable without Rust toolchain.
#
# TC-B-I02: Rng single-entry-point — OsRng / rand_core / getrandom may
#           appear ONLY under crates/shikomi-infra/src/crypto/rng/.
# TC-B-I03: pbkdf2 crate must NOT be imported directly anywhere in
#           shikomi-infra (use `bip39::Mnemonic::to_seed("")` instead).
# TC-B-I04: Fail-Secure — no `.unwrap()` / `.expect(` on production paths
#           in shikomi-infra/src/crypto/ (test-cfg blocks excluded).
#
# Exit codes: 0 all pass / 1 at least one fail.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INFRA="$ROOT/crates/shikomi-infra/src"
RNG_DIR="$INFRA/crypto/rng"

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
    echo "FATAL: $INFRA not found (Sub-B impl not yet merged?)" >&2
    # Graceful skip: Sub-B 実装前は本スクリプトを fail させない。
    # impl PR で本スクリプトを CI 統合する時に Pass/Fail 判定が走る。
    echo "[SKIP] all Sub-B static checks (shikomi-infra src tree absent)"
    exit 0
fi

# ======================================================================
# TC-B-I02: Rng single-entry-point
# ======================================================================
csprng_patterns=(
    "OsRng"
    "rand_core::OsRng"
    "rand_core::CryptoRng"
    "getrandom::"
    "rand::rngs::OsRng"
)
violations=()
for pat in "${csprng_patterns[@]}"; do
    hits=$(grep -rEn "$pat" --include='*.rs' "$INFRA" 2>/dev/null \
           | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" \
           | grep -v "^[^:]*:[0-9]*:[[:space:]]*\*" || true)
    if [[ -n "$hits" ]]; then
        while IFS= read -r line; do
            file="${line%%:*}"
            # Allow only if file is under crypto/rng/
            if [[ "$file" != "$RNG_DIR"* ]]; then
                violations+=("$pat: $line")
            fi
        done <<< "$hits"
    fi
done

if [[ ${#violations[@]} -eq 0 ]]; then
    emit "TC-B-I02" "PASS" "OsRng/rand_core/getrandom appear only under crypto/rng/ (single-entry-point)"
    detail "checked patterns: ${csprng_patterns[*]}"
else
    emit "TC-B-I02" "FAIL" "CSPRNG entry-point breach"
    for v in "${violations[@]}"; do detail "$v"; done
fi

# ======================================================================
# TC-B-I03: pbkdf2 crate NOT directly imported
# ======================================================================
pbkdf2_hits=$(grep -rEn "use pbkdf2::|pbkdf2::pbkdf2" --include='*.rs' "$INFRA" 2>/dev/null \
              | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)

if [[ -z "$pbkdf2_hits" ]]; then
    emit "TC-B-I03" "PASS" "pbkdf2 crate not imported directly (uses bip39::Mnemonic::to_seed instead, DRY)"
else
    emit "TC-B-I03" "FAIL" "pbkdf2 crate is imported directly (DRY violation)"
    while IFS= read -r line; do detail "$line"; done <<< "$pbkdf2_hits"
fi

# ======================================================================
# TC-B-I04: Fail-Secure — no unwrap/expect on production crypto paths
# ======================================================================
# Strategy: extract only crypto/ subdirectory, exclude #[cfg(test)] mod
# blocks (heuristic: lines after "#[cfg(test)]" until end of file or next
# top-level item). For simplicity, we use: any unwrap/expect in
# crypto/**/*.rs that is NOT inside a `mod tests` block.
crypto_dir="$INFRA/crypto"
if [[ -d "$crypto_dir" ]]; then
    bad_unwrap=()
    while IFS= read -r f; do
        # Strip #[cfg(test)] mod tests { ... } using awk
        prod=$(awk '
            BEGIN { in_test=0; depth=0 }
            /#\[cfg\(test\)\]/ { in_test=1; next }
            in_test && /^[[:space:]]*mod[[:space:]]+tests/ {
                # Enter the test mod, count braces
                depth=0
                # find first { on line
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
        ' "$f" 2>/dev/null)
        # Now grep unwrap/expect on production lines
        bad=$(echo "$prod" | grep -E "\.unwrap\(\)|\.expect\(" \
              | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)
        if [[ -n "$bad" ]]; then
            while IFS= read -r line; do bad_unwrap+=("$line"); done <<< "$bad"
        fi
    done < <(find "$crypto_dir" -name '*.rs' 2>/dev/null)

    if [[ ${#bad_unwrap[@]} -eq 0 ]]; then
        emit "TC-B-I04" "PASS" "no .unwrap()/.expect() on production crypto paths (Fail-Secure)"
    else
        emit "TC-B-I04" "FAIL" "production unwrap/expect found in crypto/"
        for b in "${bad_unwrap[@]}"; do detail "$b"; done
    fi
else
    emit "TC-B-I04" "SKIP" "crypto/ subdir not present (Sub-B impl not yet merged)"
fi

# ======================================================================
# Bonus: contracts coverage map (TC-B → impl files)
# ======================================================================
coverage_table=(
    "BC-1 (Argon2id RFC 9106 KAT)|crypto/kdf/argon2id.rs + crypto/kdf/kat.rs"
    "BC-2 (FROZEN_OWASP_2024_05 const)|crypto/kdf/argon2id.rs"
    "BC-3 (criterion p95<=1.0s)|benches/argon2id.rs (CI bench-kdf job)"
    "BC-4 (BIP-39 trezor KAT)|crypto/kdf/bip39_pbkdf2_hkdf.rs + kat.rs"
    "BC-5 (HKDF info b\"shikomi-kek-v1\")|crypto/kdf/bip39_pbkdf2_hkdf.rs (const)"
    "BC-6 (RFC 5869 HKDF KAT)|crypto/kdf/kat.rs"
    "BC-7 (no direct pbkdf2 use)|TC-B-I03 above"
    "BC-8 (KdfErrorKind variants)|shikomi-core errors-and-contracts.md"
    "BC-9 (Rng single-entry)|TC-B-I02 above + crypto/rng/mod.rs"
    "BC-10 (Zeroizing buf)|crypto/rng/mod.rs"
    "BC-11..16 (ZxcvbnGate)|crypto/password/zxcvbn_gate.rs"
)

# ======================================================================
# Summary
# ======================================================================
echo ""
echo "Sub-B static checks (#40):"
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
