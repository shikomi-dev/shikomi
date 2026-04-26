#!/usr/bin/env bash
# Sub-D (#42) static contract checks — TC-D-U18 / DC-13 / atomic write delegation.
#
# Complements cargo test (unit + integration + property) by enforcing
# integrity invariants via source-text grep, runnable without Rust toolchain.
# Pattern follows sub-{a,b,c}-static-checks.sh.
#
# TC-D-S01 (TC-D-U18 / DC-13): shikomi-core remains pure Rust / no-I/O —
#          aes_gcm / OsRng / rand_core::OsRng / getrandom / SystemTime /
#          Instant::now / std::fs / std::net / tokio:: / std::env::var
#          must NOT be imported in shikomi-core/src/. Sub-D should not
#          regress the Sub-A/B/C累積契約.
# TC-D-S02 (Sub-D Clean Arch): VaultMigration is implemented in
#          shikomi-infra ONLY (not shikomi-core). The repository-and-
#          migration impl files must reside under shikomi-infra/src/
#          persistence/vault_migration/ once Sub-D impl PR lands.
# TC-D-S03 (DC-9 information hiding): MSG-S11 i18n catalog (en / ja)
#          must NOT contain raw NonceCounter::current() values
#          ("あと N 回" pattern, "remaining N operations" pattern, etc.)
#          — design freeze from Sub-C Rev1.
# TC-D-S04 (DC-13 / cross-Sub regression): aes-gcm crate is imported
#          ONLY under shikomi-infra/src/crypto/aead/ (Sub-C single-entry)
#          and NOT pulled into shikomi-core for VaultMigration glue.
#
# Exit codes: 0 all pass / 1 at least one fail.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CORE="$ROOT/crates/shikomi-core/src"
INFRA="$ROOT/crates/shikomi-infra/src"
AEAD_DIR="$INFRA/crypto/aead"
VAULT_MIGRATION_DIR="$INFRA/persistence/vault_migration"

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

if [[ ! -d "$CORE" ]] || [[ ! -d "$INFRA" ]]; then
    echo "FATAL: shikomi crates not found" >&2
    exit 1
fi

# ======================================================================
# TC-D-S01: shikomi-core no-I/O purity (Sub-A/B/C 累積契約の Sub-D 段階回帰)
# ======================================================================
forbidden=(
    "use aes_gcm::"
    "aes_gcm::Aes"
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
for pat in "${forbidden[@]}"; do
    hits=$(grep -rEn "$pat" --include='*.rs' "$CORE" 2>/dev/null \
           | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" \
           | grep -v "^[^:]*:[0-9]*:[[:space:]]*\*" \
           | grep -v "^[^:]*:[0-9]*:[[:space:]]*///" || true)
    if [[ -n "$hits" ]]; then
        violations+=("$pat:")
        while IFS= read -r line; do violations+=("  $line"); done <<< "$hits"
    fi
done

if [[ ${#violations[@]} -eq 0 ]]; then
    emit "TC-D-S01" "PASS" "shikomi-core remains pure Rust / no-I/O (Sub-A/B/C accumulated契約 維持)"
    detail "checked patterns: ${forbidden[*]}"
else
    emit "TC-D-S01" "FAIL" "shikomi-core no-I/O contract regressed by Sub-D"
    for v in "${violations[@]}"; do detail "$v"; done
fi

# ======================================================================
# TC-D-S02: VaultMigration in shikomi-infra only (Clean Arch)
# ======================================================================
# When Sub-D impl lands, VaultMigration MUST be under shikomi-infra/src/
# persistence/vault_migration/ — not in shikomi-core. Graceful skip if
# impl not yet merged.
if [[ -d "$VAULT_MIGRATION_DIR" ]]; then
    # Verify no VaultMigration leaks into shikomi-core
    core_leak=$(grep -rEn "struct VaultMigration|impl VaultMigration|fn encrypt_vault|fn decrypt_vault" \
                --include='*.rs' "$CORE" 2>/dev/null \
                | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)
    if [[ -z "$core_leak" ]]; then
        emit "TC-D-S02" "PASS" "VaultMigration lives only under shikomi-infra/persistence/vault_migration/ (Clean Arch)"
    else
        emit "TC-D-S02" "FAIL" "VaultMigration symbols leaked into shikomi-core"
        while IFS= read -r line; do detail "$line"; done <<< "$core_leak"
    fi
else
    emit "TC-D-S02" "SKIP" "vault_migration/ subdir not present (Sub-D impl not yet merged)"
fi

# ======================================================================
# TC-D-S03: MSG-S11 information hiding (no raw NonceCounter::current)
# ======================================================================
# i18n message catalog files: shikomi-cli/i18n/*.toml or similar.
# Heuristic search for ja/en MSG-S11 fragments containing raw counter values.
i18n_candidates=(
    "$ROOT/crates/shikomi-cli/i18n"
    "$ROOT/crates/shikomi-gui/i18n"
    "$ROOT/i18n"
)
i18n_found=false
msg_violations=()
for d in "${i18n_candidates[@]}"; do
    if [[ -d "$d" ]]; then
        i18n_found=true
        # Look for files matching MSG-S11 / NONCE_LIMIT / nonce_limit_exceeded
        while IFS= read -r f; do
            # Heuristic: if line mentions MSG-S11 / nonce_limit and contains digit
            # patterns matching "あと N 回" / "remaining N operations" / "N 回"
            bad=$(grep -inE "(あと|remaining|残り|あと)[[:space:]]*[0-9]+[[:space:]]*(回|operation|times)" "$f" 2>/dev/null || true)
            if [[ -n "$bad" ]]; then
                msg_violations+=("$f: $bad")
            fi
        done < <(find "$d" -type f \( -name '*.toml' -o -name '*.txt' -o -name '*.md' -o -name '*.fluent' \) 2>/dev/null)
    fi
done

if [[ "$i18n_found" == false ]]; then
    emit "TC-D-S03" "SKIP" "i18n message catalog not present (Sub-D impl phase will create it)"
elif [[ ${#msg_violations[@]} -eq 0 ]]; then
    emit "TC-D-S03" "PASS" "MSG-S11 i18n catalog free of raw NonceCounter::current() values (DC-9 information hiding)"
else
    emit "TC-D-S03" "FAIL" "MSG-S11 may leak NonceCounter::current()"
    for v in "${msg_violations[@]}"; do detail "$v"; done
fi

# ======================================================================
# TC-D-S04: aes-gcm only under shikomi-infra/src/crypto/aead/ (Sub-C 累積)
# ======================================================================
if [[ -d "$AEAD_DIR" ]]; then
    aead_imports=$(grep -rEn "use aes_gcm::|aes_gcm::Aes" --include='*.rs' "$INFRA" 2>/dev/null \
                   | grep -v "^[^:]*:[0-9]*:[[:space:]]*//" || true)
    bad_aead=()
    while IFS= read -r line; do
        [[ -z "$line" ]] && continue
        file="${line%%:*}"
        if [[ "$file" != "$AEAD_DIR"* ]]; then
            bad_aead+=("$line")
        fi
    done <<< "$aead_imports"

    if [[ ${#bad_aead[@]} -eq 0 ]]; then
        emit "TC-D-S04" "PASS" "aes_gcm imported only under crypto/aead/ (Sub-C single-entry maintained, Sub-D no leak)"
    else
        emit "TC-D-S04" "FAIL" "Sub-D impl leaked aes_gcm imports outside crypto/aead/"
        for v in "${bad_aead[@]}"; do detail "$v"; done
    fi
else
    emit "TC-D-S04" "SKIP" "crypto/aead/ subdir absent (impossible — Sub-C should be merged)"
fi

# ======================================================================
# Bonus: Sub-D contracts coverage map
# ======================================================================
coverage_table=(
    "DC-1..2 (encrypt/decrypt round trip)|persistence/vault_migration/{encrypt_flow,decrypt_flow}.rs + tests/vault_migration_integration.rs"
    "DC-3..4 (unlock 2 paths + rekey)|persistence/vault_migration/{unlock_flow,rekey_flow}.rs"
    "DC-5 (change_password O(1))|persistence/vault_migration/change_password_flow.rs"
    "C-17..18 (header AEAD tag + canonical_bytes)|persistence/vault_migration/header_aead.rs"
    "C-19 (RecoveryDisclosure::disclose move semantics)|shikomi-core::vault::recovery_disclosure"
    "C-20 (DecryptConfirmation _private)|shikomi-core::vault::decrypt_confirmation"
    "C-21 (atomic write原状復帰)|vault-persistence delegation, TC-D-I04 integration test"
    "DC-7 (MigrationError 5 variants + non_exhaustive)|shikomi-core::error::MigrationError"
    "DC-8..11 (MSG-S10/S11/S13/S14 文言)|i18n catalog + sub-d-static-checks TC-D-S03"
    "DC-12 (REQ-P11 v1受入/v999拒否)|cross-feature: vault-persistence TC-I03/I04/I04a"
    "DC-13 (no aes_gcm in shikomi-core)|TC-D-S01 above"
    "Sub-D Clean Arch (VaultMigration in infra only)|TC-D-S02 above"
)

# ======================================================================
# Summary
# ======================================================================
echo ""
echo "Sub-D static checks (#42):"
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
