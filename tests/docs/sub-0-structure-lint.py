#!/usr/bin/env python3
"""Sub-0 (#38) document structure lint — TC-DOC-U01..U10 + META-01..04.

Verifies the structural completeness of:
  - docs/features/vault-encryption/requirements-analysis.md
  - docs/features/vault-encryption/requirements.md
  - docs/features/vault-encryption/test-design.md (self-integrity)

This script is the "unit" tier of the document quality verification
defined in test-design.md §6 / §6.1.

Exit codes:
  0 = all checks pass
  1 = at least one check failed
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
RA_PATH = REPO_ROOT / "docs/features/vault-encryption/requirements-analysis.md"
REQ_PATH = REPO_ROOT / "docs/features/vault-encryption/requirements.md"
TD_PATH = REPO_ROOT / "docs/features/vault-encryption/test-design.md"


@dataclass
class Result:
    tc_id: str
    passed: bool
    message: str
    detail: str = ""


@dataclass
class Report:
    results: list[Result] = field(default_factory=list)

    def add(self, tc_id: str, passed: bool, message: str, detail: str = "") -> None:
        self.results.append(Result(tc_id, passed, message, detail))

    @property
    def all_passed(self) -> bool:
        return all(r.passed for r in self.results)

    def render(self) -> str:
        lines = []
        for r in self.results:
            icon = "PASS" if r.passed else "FAIL"
            lines.append(f"[{icon}] {r.tc_id}: {r.message}")
            if r.detail:
                for d in r.detail.splitlines():
                    lines.append(f"        {d}")
        passed = sum(1 for r in self.results if r.passed)
        total = len(self.results)
        lines.append("")
        lines.append(f"Summary: {passed}/{total} checks passed.")
        return "\n".join(lines)


# ----------------------------------------------------------------- helpers


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def extract_section(text: str, start_pat: str, end_pat: str | None = None) -> str:
    """Return slice between the first occurrence of start_pat and end_pat (exclusive)."""
    sm = re.search(start_pat, text)
    if not sm:
        return ""
    body = text[sm.start():]
    if end_pat:
        em = re.search(end_pat, body[len(sm.group(0)):])
        if em:
            return body[: len(sm.group(0)) + em.start()]
    return body


def extract_l_block(text: str, level: str) -> str:
    """Slice the L1/L2/L3/L4 sub-section body up to the next #### or ### heading."""
    pat = rf"#### {level}:.*?(?=\n#### |\n### |\Z)"
    m = re.search(pat, text, flags=re.DOTALL)
    return m.group(0) if m else ""


def parse_md_table(block: str) -> list[list[str]]:
    """Return data rows (header excluded) of the first markdown table in block."""
    rows: list[list[str]] = []
    in_data = False  # True after separator row encountered
    for line in block.splitlines():
        stripped = line.strip()
        if stripped.startswith("|") and stripped.endswith("|"):
            cells = [c.strip() for c in stripped.strip("|").split("|")]
            if all(re.fullmatch(r":?-+:?", c) for c in cells):
                in_data = True
                continue
            if in_data:
                rows.append(cells)
        else:
            if in_data:
                break
    return rows


def normalize_key(s: str) -> str:
    """Strip markdown bold markers, parenthetical modifiers, whitespace."""
    s = s.strip()
    s = re.sub(r"\*+", "", s)
    s = re.sub(r"（.*?）", "", s)  # remove (full-width parentheses) modifier
    s = re.sub(r"\(.*?\)", "", s)
    return s.strip()


def find_key(kv: dict[str, str], key: str) -> str | None:
    """Return value whose normalized key equals/contains the requested key."""
    for k, v in kv.items():
        if normalize_key(k) == key:
            return v
    return None


def is_empty_cell(cell: str) -> bool:
    norm = cell.strip().replace(" ", "").replace("　", "")
    if not norm:
        return True
    placeholders = {"-", "—", "–", "TBD", "TODO", "XXX", "FIXME", "?", "??", "???"}
    return norm.upper() in {p.upper() for p in placeholders}


# --------------------------------------------------------- TC-DOC-U01 / U02 / U03


def check_l_matrices(report: Report, ra_text: str) -> None:
    """L1〜L4 each must have a 7-row vertical table with all cells non-empty.

    Each L block uses a 2-column table (項目 | 内容).  We require seven
    semantic rows: 能力 / 想定具体例 / 対応する STRIDE / 対策 / 残存リスク
    / 平文モード時の扱い / テスト観点.  L4 may add 1 extra row for
    "ユーザ向け約束" — that is permitted but does not count toward the 7.
    """
    required_keys = [
        "能力",
        "想定具体例",
        "対応する STRIDE",
        "対策",
        "残存リスク",
        "平文モード時の扱い",
        "テスト観点",
    ]

    for level in ("L1", "L2", "L3", "L4"):
        block = extract_l_block(ra_text, level)
        if not block:
            report.add("TC-DOC-U01", False, f"{level} sub-section not found in §4 capability matrix")
            continue
        rows = parse_md_table(block)
        # raw kv (full key with markers) so we can locate by normalized key
        kv = {r[0]: r[1] for r in rows if len(r) >= 2}
        missing = [k for k in required_keys if find_key(kv, k) is None]
        empties = [k for k in required_keys if (v := find_key(kv, k)) is not None and is_empty_cell(v)]
        passed = not missing and not empties
        msg = f"{level} 7-row capability matrix structure"
        detail = ""
        if missing:
            detail += f"missing rows: {missing}\n"
        if empties:
            detail += f"empty cells: {empties}\n"
        report.add(f"TC-DOC-U01[{level}]", passed, msg, detail.strip())

        措置 = find_key(kv, "対策") or ""
        if level in ("L1", "L2", "L3"):
            markers = re.findall(r"\([a-z]\)", 措置)
            ok = len(markers) >= 3
            report.add(
                f"TC-DOC-U02[{level}]",
                ok,
                f"{level} 対策 has >=3 enumerated items (a)(b)(c)...",
                f"found markers: {markers}",
            )
        elif level == "L4":
            ok = "**対象外**" in 措置
            report.add(
                "TC-DOC-U03",
                ok,
                "L4 対策 explicitly says **対象外**",
                f"対策 cell: {措置[:80]!r}",
            )


# --------------------------------------------------------- TC-DOC-U04 / U05


def check_protected_assets(report: Report, ra_text: str) -> None:
    sec = extract_section(
        ra_text,
        r"### 3\. 保護資産インベントリ",
        r"\n### 4\. ",
    )
    if not sec:
        report.add("TC-DOC-U04", False, "§3 protected-asset inventory section missing")
        report.add("TC-DOC-U05", False, "§3 protected-asset inventory section missing")
        return
    rows = parse_md_table(sec)
    if not rows:
        report.add("TC-DOC-U04", False, "no protected-asset table rows parsed")
        report.add("TC-DOC-U05", False, "no protected-asset table rows parsed")
        return

    tier_pat = re.compile(r"^[123]$")
    bad_tier = []
    for r in rows:
        if len(r) < 5:
            bad_tier.append(r[0] if r else "<empty>")
            continue
        tier_cell = r[1].strip()
        if not tier_pat.match(tier_cell):
            bad_tier.append(f"{r[0]} (tier={tier_cell!r})")
    u04_ok = not bad_tier and len(rows) >= 13
    report.add(
        "TC-DOC-U04",
        u04_ok,
        f"All {len(rows)} protected-asset rows have Tier 1/2/3",
        f"bad rows: {bad_tier}" if bad_tier else "",
    )

    # NOTE: zeroize trigger may appear in either 「所在」(col 2) or 「寿命」(col 3).
    # 寿命列は時間メトリクス、所在列は保管場所＋ゼロ化契約 — 意味論的に
    # ゼロ化トリガは所在列に書かれる方が自然。test-design.md TC-DOC-U05 の
    # 文言を「所在 or 寿命」に拡張済み（Sub-0 テスト実行時の発見、Boy Scout
    # Rule で同期修正）。
    zero_keywords = ("zeroize", "KDF 完了", "unlock", "Drop", "30 秒", "15min", "アイドル", "保持しない")
    bad_zero = []
    tier1_count = 0
    for r in rows:
        if len(r) < 5:
            continue
        if r[1].strip() != "1":
            continue
        tier1_count += 1
        location = r[2]
        lifespan = r[3]
        if not any(k in lifespan or k in location for k in zero_keywords):
            bad_zero.append(f"{r[0]}: location={location[:50]!r} lifespan={lifespan[:50]!r}")
    u05_ok = tier1_count >= 6 and not bad_zero
    report.add(
        "TC-DOC-U05",
        u05_ok,
        f"All {tier1_count} Tier-1 assets have zeroize-trigger keyword in 所在 or 寿命",
        f"bad rows: {bad_zero}" if bad_zero else "",
    )


# --------------------------------------------------------- TC-DOC-U06


def check_trust_boundaries(report: Report, ra_text: str) -> None:
    sec = extract_section(ra_text, r"### 2\. 信頼境界", r"\n### 3\. ")
    rows = parse_md_table(sec)
    bad = []
    for r in rows:
        if len(r) < 4:
            bad.append(r[0] if r else "<empty>")
            continue
        if any(is_empty_cell(c) for c in r[:4]):
            bad.append(r[0])
    ok = len(rows) >= 5 and not bad
    report.add(
        "TC-DOC-U06",
        ok,
        f"§2 trust-boundary table has >=5 rows with 内側/外側/横断ポイント all non-empty",
        f"rows={len(rows)}, bad={bad}" if not ok else "",
    )


# --------------------------------------------------------- TC-DOC-U07


def check_scope_out(report: Report, ra_text: str) -> None:
    sec = extract_section(ra_text, r"### 5\. スコープ外", r"\n### 6\. ")
    rows = parse_md_table(sec)
    bad = []
    for r in rows:
        if len(r) < 3:
            bad.append(r[0] if r else "<empty>")
            continue
        if any(is_empty_cell(c) for c in r[:3]):
            bad.append(r[0])
    ok = len(rows) >= 10 and not bad
    report.add(
        "TC-DOC-U07",
        ok,
        f"§5 scope-out table has >=10 categories with 受容根拠 non-empty",
        f"rows={len(rows)}, bad={bad}" if not ok else "",
    )


# --------------------------------------------------------- TC-DOC-U08


def check_fail_secure_patterns(report: Report, ra_text: str) -> None:
    sec = extract_section(ra_text, r"### 6\. Fail-Secure 哲学", r"\n## 機能一覧")
    rows = parse_md_table(sec)
    bad = []
    for r in rows:
        if len(r) < 3:
            bad.append(r[0] if r else "<empty>")
            continue
        if any(is_empty_cell(c) for c in r[:3]):
            bad.append(r[0])
    ok = len(rows) >= 5 and not bad
    report.add(
        "TC-DOC-U08",
        ok,
        f"§6 Fail-Secure pattern table has >=5 rows with 適用/効果 non-empty",
        f"rows={len(rows)}, bad={bad}" if not ok else "",
    )


# --------------------------------------------------------- TC-DOC-U09 / U10


def check_req_s_sections(report: Report, req_text: str) -> None:
    section_pat = re.compile(r"^### REQ-S(\d{2}):", re.MULTILINE)
    ids = section_pat.findall(req_text)
    expected = [f"{i:02d}" for i in range(1, 18)]
    u09_ok = ids == expected
    report.add(
        "TC-DOC-U09",
        u09_ok,
        f"REQ-S01..S17 sections present in order",
        f"found ids: {ids}" if not u09_ok else "",
    )

    bad_threat = []
    bad_struct = []
    threat_pat = re.compile(r"L[1-4]|—")
    for i in range(1, 18):
        sid = f"REQ-S{i:02d}"
        m = re.search(rf"### {sid}:.*?(?=\n### REQ-S|\Z)", req_text, flags=re.DOTALL)
        if not m:
            bad_struct.append(sid)
            continue
        block = m.group(0)
        for key in ("担当 Sub", "概要", "関連脅威 ID"):
            row = re.search(rf"\| {re.escape(key)} \| (.+?) \|", block)
            if not row or is_empty_cell(row.group(1)):
                bad_struct.append(f"{sid}:{key}")
        threat_row = re.search(r"\| 関連脅威 ID \| (.+?) \|", block)
        if threat_row and not threat_pat.search(threat_row.group(1)):
            bad_threat.append(f"{sid}: {threat_row.group(1)!r}")
    report.add(
        "TC-DOC-U09[cells]",
        not bad_struct,
        "All REQ-S* sections have 担当 Sub / 概要 / 関連脅威 ID non-empty",
        f"bad: {bad_struct}" if bad_struct else "",
    )
    report.add(
        "TC-DOC-U10",
        not bad_threat,
        "All REQ-S* 関連脅威 ID rows contain L1..L4 or `—`",
        f"bad: {bad_threat}" if bad_threat else "",
    )


# --------------------------------------------------------- META-01 / 02 / 03 / 04


def check_self_integrity(report: Report, td_text: str) -> None:
    # META-01: §1 overview "TC 総数" cell declares 25
    m1 = re.search(r"TC 総数.*?\|\s*\*\*?(\d+)\s*件", td_text)
    meta1_ok = bool(m1 and m1.group(1) == "25")
    report.add(
        "META-01",
        meta1_ok,
        "§1 overview table declares TC 総数 = 25",
        f"found: {m1.group(0) if m1 else 'no match'}" if not meta1_ok else "",
    )

    # META-02: §3 matrix heading declares 25
    m2 = re.search(r"## 3\. テストマトリクス.*?TC 総数:\s*(\d+)\s*件", td_text, flags=re.DOTALL)
    meta2_ok = bool(m2 and m2.group(1) == "25")
    report.add(
        "META-02",
        meta2_ok,
        "§3 matrix heading declares TC 総数 = 25",
        f"found: {m2.group(1) if m2 else 'no match'}" if not meta2_ok else "",
    )

    # META-03: unique TC-DOC-(U|I|E)NN ids count == 25
    ids = sorted(set(re.findall(r"TC-DOC-([UIE])(\d{2})", td_text)))
    series = {"U": [], "I": [], "E": []}
    for letter, num in ids:
        series[letter].append(int(num))
    total = sum(len(v) for v in series.values())
    meta3_ok = total == 25 and len(series["U"]) == 10 and len(series["I"]) == 8 and len(series["E"]) == 7
    report.add(
        "META-03",
        meta3_ok,
        "Unique TC-DOC ids: 10(U) + 8(I) + 7(E) = 25",
        f"got U={len(series['U'])} I={len(series['I'])} E={len(series['E'])} total={total}",
    )

    # META-04: contiguous numbering (no gaps)
    bad_runs = []
    for letter, expected in (("U", 10), ("I", 8), ("E", 7)):
        nums = sorted(series[letter])
        if nums != list(range(1, expected + 1)):
            bad_runs.append(f"{letter}: {nums}")
    report.add(
        "META-04",
        not bad_runs,
        "Each TC-DOC series is contiguous from 01 (U10/I08/E07 last)",
        f"gaps: {bad_runs}" if bad_runs else "",
    )


# --------------------------------------------------------- main


def main() -> int:
    report = Report()

    if not RA_PATH.exists():
        print(f"FATAL: {RA_PATH} not found", file=sys.stderr)
        return 1
    if not REQ_PATH.exists():
        print(f"FATAL: {REQ_PATH} not found", file=sys.stderr)
        return 1
    if not TD_PATH.exists():
        print(f"FATAL: {TD_PATH} not found", file=sys.stderr)
        return 1

    ra_text = read(RA_PATH)
    req_text = read(REQ_PATH)
    td_text = read(TD_PATH)

    check_l_matrices(report, ra_text)              # U01 / U02 / U03
    check_protected_assets(report, ra_text)        # U04 / U05
    check_trust_boundaries(report, ra_text)        # U06
    check_scope_out(report, ra_text)               # U07
    check_fail_secure_patterns(report, ra_text)    # U08
    check_req_s_sections(report, req_text)         # U09 / U10
    check_self_integrity(report, td_text)          # META-01 .. META-04

    print(report.render())
    return 0 if report.all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
