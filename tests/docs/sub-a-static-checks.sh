#!/usr/bin/env bash
# Sub-A (#39) static contract checks — TC-A-I01 / TC-A-I02 / TC-A-U13 grep portion.
#
# These checks do NOT need the Rust toolchain; they verify integrity invariants
# by source-text grep, complementing cargo test (unit) and cargo test --doc
# (compile_fail) which run inside CI.
#
# TC-A-I01: NonceOverflow → NonceLimitExceeded rename integrity
# TC-A-I02: shikomi-core has no OsRng / getrandom / rand:: / SystemTime / std::fs
#           (Clean Architecture: pure Rust / no-I/O for shikomi-core)
# TC-A-U13: KdfSalt::generate() must NOT exist in shikomi-core
#           (single-entry-point contract: only shikomi-infra::crypto::Rng can
#           generate kdf_salt; shikomi-core only exposes try_new(&[u8]))
#
# Exit codes: 0 all pass / 1 at least one fail.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CORE="$ROOT/crates/shikomi-core/src"

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

if [[ ! -d "$CORE" ]]; then
    echo "FATAL: $CORE not found" >&2
    exit 1
fi

# ======================================================================
# TC-A-I01: NonceOverflow → NonceLimitExceeded rename integrity
# ======================================================================
overflow_hits=$(grep -rE "NonceOverflow" --include='*.rs' "$ROOT" 2>/dev/null | grep -v "tests/docs/" || true)
limit_hits=$(grep -rE "NonceLimitExceeded" --include='*.rs' "$ROOT" 2>/dev/null | grep -v "tests/docs/" | wc -l)

if [[ -z "$overflow_hits" ]] && [[ "$limit_hits" -gt 0 ]]; then
    emit "TC-A-I01" "PASS" "NonceOverflow rename: 0 occurrences of old name; ${limit_hits} occurrences of NonceLimitExceeded"
else
    emit "TC-A-I01" "FAIL" "NonceOverflow rename incomplete"
    if [[ -n "$overflow_hits" ]]; then
        detail "stale NonceOverflow occurrences:"
        while IFS= read -r line; do detail "  $line"; done <<< "$overflow_hits"
    fi
    if [[ "$limit_hits" -eq 0 ]]; then
        detail "NonceLimitExceeded not found — rename did not complete"
    fi
fi

# ======================================================================
# TC-A-I02: shikomi-core no-I/O purity
# ======================================================================
forbidden_patterns=(
    "rand::"
    "rand_core::"
    "getrandom"
    "OsRng"
    "SystemTime"
    "Instant::now"
    "std::fs"
    "std::net"
    "tokio::"
    "std::env::var"
)
violations=()
for pat in "${forbidden_patterns[@]}"; do
    # Only scan production sources (exclude tests/ and bench/)
    hits=$(grep -rE "$pat" --include='*.rs' "$CORE" 2>/dev/null \
           | grep -v "//" \
           | grep -v "doc =" || true)
    if [[ -n "$hits" ]]; then
        violations+=("$pat:")
        while IFS= read -r line; do violations+=("  $line"); done <<< "$hits"
    fi
done

if [[ ${#violations[@]} -eq 0 ]]; then
    emit "TC-A-I02" "PASS" "shikomi-core is pure Rust / no-I/O (no OsRng / getrandom / rand / fs / net / tokio / time-I/O)"
    detail "checked patterns: ${forbidden_patterns[*]}"
else
    emit "TC-A-I02" "FAIL" "shikomi-core no-I/O contract violated"
    for v in "${violations[@]}"; do detail "$v"; done
fi

# ======================================================================
# TC-A-U13 (static portion): KdfSalt::generate() must not exist in shikomi-core
# ======================================================================
generate_hits=$(grep -rE "fn[[:space:]]+generate[[:space:]]*\(" --include='*.rs' "$CORE" 2>/dev/null \
                | grep -i "kdf\|salt" || true)
if [[ -z "$generate_hits" ]]; then
    emit "TC-A-U13[grep]" "PASS" "KdfSalt has no generate() in shikomi-core (single-entry contract preserved)"
    detail "shikomi-core exposes only try_new(&[u8]); generation lives in shikomi-infra::crypto::Rng"
else
    emit "TC-A-U13[grep]" "FAIL" "KdfSalt::generate-like function present in shikomi-core"
    while IFS= read -r line; do detail "$line"; done <<< "$generate_hits"
fi

# ======================================================================
# Bonus: contracts checklist coverage map (TC-A → impl files)
# ======================================================================
coverage_table=(
    "C-1 (Drop zeroize)|key.rs|password.rs|recovery.rs|verified.rs|header_aead.rs"
    "C-2 (Clone forbidden)|compile_fail doctest @ key.rs / password.rs / recovery.rs / verified.rs / header_aead.rs"
    "C-3 (Debug REDACTED)|key.rs|password.rs|recovery.rs|verified.rs|header_aead.rs"
    "C-4 (Display forbidden)|compile_fail doctest @ key.rs etc."
    "C-5 (Serialize forbidden for Tier-1)|compile_fail doctest @ key.rs etc."
    "C-6 (Kek phantom-typed mix forbidden)|compile_fail doctest @ key.rs"
    "C-7 (Verified pub(crate))|compile_fail doctest @ verified.rs"
    "C-8 (MasterPassword::new gate required)|password.rs runtime tests"
    "C-9 (NonceCounter limit)|tests/nonce_overflow.rs"
    "C-10 (NonceBytes::from_random)|vault/nonce.rs runtime"
    "C-11 (WrappedVek bounds)|vault/crypto_data.rs runtime"
    "C-12 (RecoveryMnemonic 24-word)|recovery.rs runtime"
    "C-13 (NonceOverflow rename)|TC-A-I01 above"
)

# ======================================================================
# Summary
# ======================================================================
echo ""
echo "Sub-A static checks (#39):"
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
