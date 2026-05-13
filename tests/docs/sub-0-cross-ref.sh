#!/usr/bin/env bash
# Sub-0 (#38) cross-reference integrity test — TC-DOC-I01..I08.
#
# Verifies cross-document consistency between:
#   - docs/features/vault-encryption/requirements-analysis.md   (RA)
#   - docs/features/vault-encryption/requirements.md            (REQ)
#   - docs/architecture/context/threat-model.md                 (TM)
#   - docs/architecture/tech-stack.md                           (TS)
#
# This script is the "integration" tier of document quality verification
# defined in test-design.md §5.
#
# Exit codes:
#   0 = all 8 checks passed
#   1 = at least one check failed

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RA="$ROOT/docs/features/vault-encryption/requirements-analysis.md"
REQ="$ROOT/docs/features/vault-encryption/requirements.md"
TM="$ROOT/docs/architecture/context/threat-model.md"
TS="$ROOT/docs/architecture/tech-stack.md"

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

# Sanity checks
for f in "$RA" "$REQ" "$TM" "$TS"; do
    if [[ ! -f "$f" ]]; then
        echo "FATAL: $f not found" >&2
        exit 1
    fi
done

# ======================================================================
# TC-DOC-I01: requirements-analysis.md → threat-model.md section refs
# ======================================================================
# Extract section numbers referenced from RA and verify they exist in TM.
ref_sections=$(grep -oE 'threat-model\.md\b[^.]*§[0-9A-Z][0-9A-Z.]*' "$RA" \
               | grep -oE '§[0-9A-Z][0-9A-Z.]*' | sort -u)
# Also pick references like 「§7.0 既定」 directly (without explicit threat-model.md prefix nearby)
explicit_refs=$(echo "$ref_sections")

missing=()
while IFS= read -r ref; do
    [[ -z "$ref" ]] && continue
    num="${ref#§}"
    # threat-model.md uses headings like "## 7." / "### 7.0 ..." / "### 7.1 ..."
    if ! grep -qE "^#{1,6} +${num}([. ]|$)" "$TM"; then
        missing+=("$ref")
    fi
done <<< "$explicit_refs"

if [[ ${#missing[@]} -eq 0 ]]; then
    emit "TC-DOC-I01" "PASS" "all threat-model.md section refs in RA exist"
    detail "checked: $(echo "$explicit_refs" | tr '\n' ' ')"
else
    emit "TC-DOC-I01" "FAIL" "missing threat-model.md sections: ${missing[*]}"
fi

# ======================================================================
# TC-DOC-I02: requirements-analysis.md → tech-stack.md section refs
# ======================================================================
ts_refs=$(grep -oE 'tech-stack\.md\b[^.]*§[0-9.]+' "$RA" \
          | grep -oE '§[0-9.]+' | sort -u)
missing=()
while IFS= read -r ref; do
    [[ -z "$ref" ]] && continue
    num="${ref#§}"
    if ! grep -qE "^#{1,6} +${num}([. ]|$)" "$TS"; then
        missing+=("$ref")
    fi
done <<< "$ts_refs"

if [[ ${#missing[@]} -eq 0 ]]; then
    emit "TC-DOC-I02" "PASS" "all tech-stack.md section refs in RA exist"
    detail "checked: $(echo "$ts_refs" | tr '\n' ' ')"
else
    emit "TC-DOC-I02" "FAIL" "missing tech-stack.md sections: ${missing[*]}"
fi

# ======================================================================
# TC-DOC-I03: frozen value parity
# ======================================================================
# Two scopes:
#   (a) tech-stack-frozen values: must appear in both RA and TS
#       m=19456, t=2, p=1, nonce 12B, 2^{32}, VEK 32B, kdf_salt 16B
#   (b) feature-local frozen values: must appear in RA only
#       (tech-stack does not cover feature-local sizes)
#       AAD 26B, idle 15min
# Bug-DOC-003 (Sub-0 test execution): TC-DOC-I03 was originally written to
# check all 9 values in both documents, but AAD-size and cache-idle are
# feature-local and never appear in tech-stack.md. Test design was
# narrowed to "tech-stack frozen" + "feature-local in RA".
declare -a TS_FROZEN=("m=19456" "t=2" "p=1" "12B" "2^{32}" "32B" "16B")
declare -a FEATURE_FROZEN=("26B" "15min")
mismatch=()
for needle in "${TS_FROZEN[@]}"; do
    in_ra=$(grep -cF "$needle" "$RA" || true)
    in_ts=$(grep -cF "$needle" "$TS" || true)
    if [[ "$in_ra" -lt 1 || "$in_ts" -lt 1 ]]; then
        mismatch+=("$needle (RA=$in_ra, TS=$in_ts)")
    fi
done
for needle in "${FEATURE_FROZEN[@]}"; do
    in_ra=$(grep -cF "$needle" "$RA" || true)
    if [[ "$in_ra" -lt 1 ]]; then
        mismatch+=("$needle (feature-local, RA=$in_ra)")
    fi
done

if [[ ${#mismatch[@]} -eq 0 ]]; then
    emit "TC-DOC-I03" "PASS" "tech-stack frozen values match RA, feature-local values present in RA"
    detail "TS-frozen: ${TS_FROZEN[*]}"
    detail "feature-local: ${FEATURE_FROZEN[*]}"
else
    emit "TC-DOC-I03" "FAIL" "frozen value mismatch: ${mismatch[*]}"
fi

# ======================================================================
# TC-DOC-I04: DAG integrity (functional list 担当 Sub <-> Sub-issue split DAG)
# ======================================================================
# Extract from RA §機能一覧 the (REQ-S, 担当 Sub) pairs and from
# §Sub-issue分割計画 the (Sub-issue, 依存関係) pairs.  Then assert the
# DAG declared at the top of the analysis (0 -> A -> {B,C} -> D -> E -> F)
# matches the dependency column.
python3 - <<'PY' "$RA"
import re, sys
ra = open(sys.argv[1], encoding='utf-8').read()
# Sub-issue split table after "## Sub-issue分割計画"
m = re.search(r'## Sub-issue分割計画.*?(\| Sub-issue.*?)\n## ', ra, re.DOTALL)
if not m:
    print('FAIL'); sys.exit(1)
table = m.group(1)
deps = {}
for row in table.splitlines():
    cells = [c.strip() for c in row.strip('|').split('|')]
    if len(cells) < 3 or cells[0] in ('Sub-issue', '----------', ''):
        continue
    if '---' in cells[0]:
        continue
    sub = cells[0]
    dep = cells[2]
    deps[sub] = dep

expected = {
    '0': 'なし',
    'A': 'Sub-0',
    'B': 'Sub-A',
    'C': 'Sub-A',
    'D': ['Sub-B', 'Sub-C'],
    'E': 'Sub-D',
    'F': 'Sub-E',
}
errs = []
for sub_key, dep in deps.items():
    letter = sub_key.replace('**Sub-0 #38**', '0').replace('Sub-0', '0')
    for ch in 'ABCDEF0':
        if f'Sub-{ch}' in sub_key or f'**Sub-{ch}' in sub_key:
            letter = ch; break
    exp = expected.get(letter)
    if exp is None:
        errs.append(f'unknown sub: {sub_key}')
        continue
    if isinstance(exp, list):
        ok = all(e in dep for e in exp)
    else:
        ok = exp in dep
    if not ok:
        errs.append(f'Sub-{letter}: expected {exp}, got {dep!r}')

if errs:
    print('FAIL\n' + '\n'.join(errs))
    sys.exit(1)
print('PASS')
PY
rc=$?
if [[ $rc -eq 0 ]]; then
    emit "TC-DOC-I04" "PASS" "DAG 0 -> A -> {B,C} -> D -> E -> F integrity verified"
else
    emit "TC-DOC-I04" "FAIL" "DAG integrity violation (see python output above)"
fi

# ======================================================================
# TC-DOC-I05: REQ-S* 担当 Sub bidirectional mapping (RA §機能一覧 <-> REQ §REQ-S*)
# ======================================================================
python3 - <<'PY' "$RA" "$REQ"
import re, sys
ra = open(sys.argv[1], encoding='utf-8').read()
req = open(sys.argv[2], encoding='utf-8').read()

# RA §機能一覧 table: extract (REQ-Sxx, 担当 Sub)
m = re.search(r'## 機能一覧.*?(\| 機能ID.*?)\n## ', ra, re.DOTALL)
ra_map = {}
if m:
    for row in m.group(1).splitlines():
        cells = [c.strip() for c in row.strip('|').split('|')]
        if len(cells) < 5: continue
        rid = cells[0]
        if not re.match(r'REQ-S\d+', rid): continue
        ra_map[rid] = cells[3]  # 担当 Sub

# REQ: each ### REQ-Sxx section has a `| 担当 Sub | ... |` row
req_map = {}
for m in re.finditer(r'### (REQ-S\d{2}):.*?(?=\n### REQ-S|\Z)', req, re.DOTALL):
    rid = m.group(1)
    sec = m.group(0)
    row = re.search(r'\| 担当 Sub \| (.+?) \|', sec)
    if row:
        req_map[rid] = row.group(1).strip()

errs = []
for rid in sorted(set(ra_map.keys()) | set(req_map.keys())):
    a = ra_map.get(rid, '<missing>')
    b = req_map.get(rid, '<missing>')
    # Allow "Sub-X (#NN)" vs "Sub-X" inclusion both ways
    a_norm = re.sub(r'\s*\(#\d+\)', '', a).strip()
    b_norm = re.sub(r'\s*\(#\d+\)', '', b).strip()
    a_letters = sorted(set(re.findall(r'Sub-[0-9A-F]+', a_norm)))
    b_letters = sorted(set(re.findall(r'Sub-[0-9A-F]+', b_norm)))
    if a_letters != b_letters or not a_letters:
        errs.append(f'{rid}: RA={a_norm!r} vs REQ={b_norm!r}')

if errs:
    print('FAIL\n' + '\n'.join(errs))
    sys.exit(1)
print(f'PASS ({len(ra_map)} REQ-S* matched)')
PY
rc=$?
if [[ $rc -eq 0 ]]; then
    emit "TC-DOC-I05" "PASS" "REQ-S01..S17 担当 Sub mapping consistent across RA and REQ"
else
    emit "TC-DOC-I05" "FAIL" "REQ-S* Sub mapping mismatch (see python output)"
fi

# ======================================================================
# TC-DOC-I06: data model entities <-> protected asset inventory
# ======================================================================
python3 - <<'PY' "$RA" "$REQ"
import re, sys
ra = open(sys.argv[1], encoding='utf-8').read()
req = open(sys.argv[2], encoding='utf-8').read()

# REQ data model: 9 entities
m = re.search(r'## データモデル.*?(\| エンティティ.*?)\n## ', req, re.DOTALL)
entities = []
if m:
    for row in m.group(1).splitlines():
        cells = [c.strip() for c in row.strip('|').split('|')]
        if len(cells) < 2: continue
        if 'エンティティ' in cells[0] or '---' in cells[0]: continue
        # extract first backtick or word after stripping markdown bold
        ent = re.sub(r'`', '', cells[0]).split('（')[0].strip().lstrip('*').rstrip('*').strip()
        if ent:
            entities.append(ent)

# RA §3 protected asset names (look for asset names like VEK, KEK_pw, KEK_recovery,
# wrapped_VEK_*, kdf_salt, AAD, kdf_params, master password, recovery, plaintext records)
ra_section_match = re.search(r'### 3\. 保護資産インベントリ.*?(?=\n### 4\.)', ra, re.DOTALL)
ra_section = ra_section_match.group(0) if ra_section_match else ''

# Build correspondence rules between data model entity names and asset inventory anchors
expected_anchors = {
    # Sub-0 凍結エンティティ
    'VaultEncryptedHeader': ['ヘッダ', 'wrapped_VEK', 'kdf_params', 'kdf_salt'],
    'WrappedVek': ['wrapped_VEK_by_pw', 'wrapped_VEK_by_recovery'],
    'KdfSalt': ['kdf_salt'],
    'KdfParams': ['kdf_params'],
    'NonceCounter': ['nonce'],
    'EncryptedRecord': ['records.ciphertext', 'AAD', 'nonce'],
    'MasterPassword': ['マスターパスワード'],
    'RecoveryMnemonic': ['リカバリ', 'BIP-39'],
    'Vek': ['VEK'],
    # Sub-A 詳細化エンティティ（Sub-0 の Tier-1〜3 資産から派生する型）
    'NonceBytes': ['nonce'],            # per-record nonce 12B の値オブジェクト
    'AuthTag': ['AEAD', 'tag'],         # AES-256-GCM 認証タグ
    'Kek<KekKindPw>': ['KEK_pw'],       # phantom-typed KEK
    'Kek<KekKindRecovery>': ['KEK_recovery'],
    'HeaderAeadKey': ['ヘッダ', 'wrapped_VEK', 'kdf_params'],  # ヘッダ独立 AEAD タグの鍵
    'Plaintext': ['平文レコード'],
    'Verified<T>': ['平文レコード', 'AEAD'],  # 検証済み平文の newtype
    'WeakPasswordFeedback': ['マスターパスワード'],  # zxcvbn 入口ゲート Feedback
    'CryptoOutcome<T>': ['AEAD'],       # 暗号操作の結果列挙
}

errs = []
for ent in entities:
    keys = expected_anchors.get(ent)
    if keys is None:
        errs.append(f'{ent}: no expected anchor (data-model entity without inventory mapping)')
        continue
    if not any(k in ra_section for k in keys):
        errs.append(f'{ent}: none of {keys} found in §3 inventory')

if errs:
    print('FAIL\n' + '\n'.join(errs))
    sys.exit(1)
print(f'PASS ({len(entities)} entities mapped)')
PY
rc=$?
if [[ $rc -eq 0 ]]; then
    emit "TC-DOC-I06" "PASS" "data-model entities map onto §3 protected-asset inventory"
else
    emit "TC-DOC-I06" "FAIL" "data-model entity / inventory mismatch (see python output)"
fi

# ======================================================================
# TC-DOC-I07: 依存 crate <-> tech-stack §4.7 (RUSTSEC ignore-禁止 list)
# ======================================================================
python3 - <<'PY' "$REQ" "$TS"
import re, sys
req = open(sys.argv[1], encoding='utf-8').read()
ts = open(sys.argv[2], encoding='utf-8').read()

# Crates listed in REQ dependencies (filter to those marked as crate)
m = re.search(r'## 依存関係.*?(\| 依存先.*?)(?:\n## |\Z)', req, re.DOTALL)
crates = []
if m:
    for row in m.group(1).splitlines():
        cells = [c.strip() for c in row.strip('|').split('|')]
        if len(cells) < 4: continue
        if '依存先' in cells[0] or '---' in cells[0]: continue
        if 'crate' not in cells[1]: continue
        name = cells[0].strip().strip('`')
        if name:
            crates.append(name)

missing = []
for c in crates:
    # tech-stack §4.7 lists crates within backticks; just check substring presence
    if f'`{c}`' not in ts and c not in ts:
        missing.append(c)

if missing:
    print('FAIL\nmissing in tech-stack: ' + ', '.join(missing))
    sys.exit(1)
print(f'PASS ({len(crates)} crates verified)')
PY
rc=$?
if [[ $rc -eq 0 ]]; then
    emit "TC-DOC-I07" "PASS" "all REQ依存関係 crates present in tech-stack.md §4.7"
else
    emit "TC-DOC-I07" "FAIL" "crate present in REQ but absent from tech-stack (see python output)"
fi

# ======================================================================
# TC-DOC-I08: broken-link check (internal repo paths only; we do not hit network)
# ======================================================================
python3 - <<'PY' "$ROOT" "$RA" "$REQ"
import os, re, sys
root, ra_path, req_path = sys.argv[1], sys.argv[2], sys.argv[3]
broken = []
for p in (ra_path, req_path):
    text = open(p, encoding='utf-8').read()
    # Markdown link [text](path) where path looks like a relative or repo-root path
    for m in re.finditer(r'\]\(([^)]+)\)', text):
        target = m.group(1).split('#')[0].split(' ')[0]
        if not target: continue
        if target.startswith(('http://', 'https://', 'mailto:')):
            continue
        # Resolve relative to file, then to repo root
        if target.startswith('/'):
            tgt = os.path.join(root, target.lstrip('/'))
        else:
            tgt = os.path.normpath(os.path.join(os.path.dirname(p), target))
        if not os.path.exists(tgt):
            broken.append(f'{os.path.basename(p)} -> {target}')
    # Also check inline backtick paths like `docs/architecture/...`
    for m in re.finditer(r'`(docs/[A-Za-z0-9_./-]+(?:\.md|\.toml))`', text):
        target = m.group(1)
        tgt = os.path.join(root, target)
        if not os.path.exists(tgt):
            broken.append(f'{os.path.basename(p)} -> {target}')

if broken:
    print('FAIL')
    for b in broken[:30]:
        print(' ', b)
    sys.exit(1)
print('PASS')
PY
rc=$?
if [[ $rc -eq 0 ]]; then
    emit "TC-DOC-I08" "PASS" "no broken internal links in RA / REQ"
else
    emit "TC-DOC-I08" "FAIL" "broken internal links (see python output)"
fi

# ======================================================================
# DRIFT-CHECK-01: split-aware reference drift (Sub-A Rev1 で
# detailed-design.md → detailed-design/{index,...}.md に分割された後、
# 兄弟ドキュメント (basic-design.md / requirements.md / test-design.md /
# requirements-analysis.md) 内で `detailed-design.md` への裸参照が残存
# していないことを検証する。
#
# Bug-DOC-007 (Sub-A Rev1 review): セルがファイル分割時に basic-design /
# requirements の参照は更新したが、test-design.md TC-A-U18 内 2 箇所の
# `detailed-design.md` 言及を見落とし、マユリの TC-A-U18 修正もファイル
# 分割前の表記のままだった。両者がかりの参照ドリフト。
#
# このチェックは split が起きた後でないと意味を持たないため、
# detailed-design/ ディレクトリの存在を前提条件とする。
# ======================================================================
SPLIT_DIR="$ROOT/docs/features/vault-encryption/detailed-design"
if [[ -d "$SPLIT_DIR" ]]; then
    drift=()
    for f in "$ROOT/docs/features/vault-encryption/test-design/index.md" \
             "$ROOT/docs/features/vault-encryption/test-design/sub-0-threat-model.md" \
             "$ROOT/docs/features/vault-encryption/test-design/sub-a-crypto-types.md" \
             "$ROOT/docs/features/vault-encryption/test-design/sub-b-kdf-rng-zxcvbn.md" \
             "$ROOT/docs/features/vault-encryption/test-design/sub-c-aead.md" \
             "$ROOT/docs/features/vault-encryption/test-design/sub-d-repository-migration.md" \
             "$ROOT/docs/features/vault-encryption/basic-design/index.md" \
             "$ROOT/docs/features/vault-encryption/basic-design/architecture.md" \
             "$ROOT/docs/features/vault-encryption/basic-design/processing-flows.md" \
             "$ROOT/docs/features/vault-encryption/basic-design/security.md" \
             "$ROOT/docs/features/vault-encryption/basic-design/ux-and-msg.md" \
             "$ROOT/docs/features/vault-encryption/requirements.md" \
             "$ROOT/docs/features/vault-encryption/requirements-analysis.md"; do
        [[ -f "$f" ]] || continue
        # Look for `detailed-design.md` token in code blocks / inline backticks /
        # markdown links. Allow it inside HTML comments (history reference).
        # Strategy: strip <!-- ... --> blocks then grep.
        clean=$(python3 -c '
import re, sys, pathlib
text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
text = re.sub(r"<!--.*?-->", "", text, flags=re.DOTALL)
sys.stdout.write(text)
' "$f")
        if echo "$clean" | grep -qE 'detailed-design\.md'; then
            count=$(echo "$clean" | grep -cE 'detailed-design\.md' || true)
            drift+=("$(basename "$f"): $count occurrence(s) of bare detailed-design.md")
        fi
    done
    if [[ ${#drift[@]} -eq 0 ]]; then
        emit "DRIFT-CHECK-01" "PASS" "no stale 'detailed-design.md' refs in sibling docs (split-aware)"
    else
        emit "DRIFT-CHECK-01" "FAIL" "stale 'detailed-design.md' references survive after split: ${drift[*]}"
    fi
fi

# ======================================================================
# Summary
# ======================================================================
echo ""
for line in "${RESULTS[@]}"; do
    echo "$line"
done
echo ""
TOTAL=$((PASS + FAIL))
echo "Summary: $PASS/$TOTAL checks passed."
if [[ $FAIL -eq 0 ]]; then
    exit 0
else
    exit 1
fi
