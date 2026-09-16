#!/usr/bin/env python3
"""必要な文書と、Markdown内の相対リンクを検査する。"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
SKIP_DIRECTORIES = {".git", ".pytest_cache", ".venv", "__pycache__"}
REQUIRED_FILES = (
    "README.md",
    "AGENTS.md",
    "CONTRIBUTING.md",
    "LICENSE",
    "docs/000_process.md",
    "docs/010_design-principles.md",
    "docs/020_code-structure.md",
    "docs/030_testing.md",
    "docs/040_completion.md",
    "docs/110_requirements/README.md",
    "docs/110_requirements/01-背景と課題.md",
    "docs/110_requirements/02-利用者.md",
    "docs/110_requirements/03-要求.md",
    "docs/110_requirements/04-横断的な受入条件.md",
    "docs/110_requirements/シナリオ/README.md",
    "docs/110_requirements/シナリオ/_template.md",
    "docs/110_requirements/シナリオ/_acceptance-template.md",
    "docs/150_system/README.md",
    "docs/150_system/全体構造.md",
    "docs/150_system/データの構造.md",
    "docs/150_system/シーケンス/README.md",
    "docs/210_increments/README.md",
    "docs/210_increments/_template.md",
    "docs/decisions/_template.md",
    "client/shikomi/shikomi",
    "client/shikomi/extension/extension.js",
    "contracts/control.md",
    ".github/workflows/check.yml",
    "scripts/check-all.sh",
    "scripts/check-trace.py",
    "tests/acceptance/manual/README.md",
    "tests/acceptance/manual/_template.md",
    ".github/ISSUE_TEMPLATE/change.yml",
    ".github/pull_request_template.md",
)
REQUIRED_DIRECTORIES = (
    "docs/110_requirements/シナリオ",
    "docs/150_system/シーケンス",
    "docs/210_increments",
    "tests/acceptance",
)
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
ISSUE_FORM_TITLE = re.compile(r"^title:\s*(.*)$", re.MULTILINE)


def missing_required_files() -> list[str]:
    return [name for name in REQUIRED_FILES if not (ROOT / name).is_file()]


def missing_required_directories() -> list[str]:
    return [name for name in REQUIRED_DIRECTORIES if not (ROOT / name).is_dir()]


def broken_links() -> list[str]:
    broken: list[str] = []
    for document in sorted(ROOT.rglob("*.md")):
        if any(part in SKIP_DIRECTORIES for part in document.parts):
            continue
        if document.parent == ROOT / "docs/210_increments" and document.name.startswith(
            "INC-"
        ):
            continue
        for raw_target in LINK.findall(document.read_text(encoding="utf-8")):
            target = raw_target.strip().strip("<>")
            if target.startswith(("https://", "http://", "mailto:", "#")):
                continue
            path_part = unquote(target.split("#", 1)[0])
            if not path_part:
                continue
            resolved = (document.parent / path_part).resolve()
            if not resolved.exists():
                relative_document = document.relative_to(ROOT)
                broken.append(f"{relative_document}: {target}")
    return broken


def invalid_issue_form_titles() -> list[str]:
    invalid: list[str] = []
    forms = ROOT / ".github/ISSUE_TEMPLATE"
    for form in sorted(forms.glob("*.yml")):
        if form.name == "config.yml":
            continue
        match = ISSUE_FORM_TITLE.search(form.read_text(encoding="utf-8"))
        title = match.group(1).strip().strip("\"'") if match else ""
        if not title:
            invalid.append(str(form.relative_to(ROOT)))
    return invalid


def orphan_sequences() -> list[str]:
    missing: list[str] = []
    sequences = ROOT / "docs/150_system/シーケンス"
    scenarios = ROOT / "docs/110_requirements/シナリオ"
    for sequence in sorted(sequences.glob("SC-*.md")):
        scenario = scenarios / sequence.stem / "README.md"
        if not scenario.is_file():
            missing.append(str(sequence.relative_to(ROOT)))
    return missing


def unlisted_scenarios() -> list[str]:
    scenarios = ROOT / "docs/110_requirements/シナリオ"
    index = (scenarios / "README.md").read_text(encoding="utf-8")
    return [
        str(scenario.relative_to(ROOT))
        for scenario in sorted(scenarios.glob("SC-*"))
        if scenario.is_dir() and f"{scenario.name}/README.md" not in index
    ]


def incomplete_scenarios() -> list[str]:
    scenarios = ROOT / "docs/110_requirements/シナリオ"
    incomplete: list[str] = []
    for scenario in sorted(scenarios.glob("SC-*")):
        if not scenario.is_dir():
            continue
        for required in ("README.md", "受入例.md"):
            if not (scenario / required).is_file():
                incomplete.append(str((scenario / required).relative_to(ROOT)))
    return incomplete


def main() -> int:
    failures = [
        *(f"必要なファイルがありません: {name}" for name in missing_required_files()),
        *(
            f"必要なディレクトリがありません: {name}"
            for name in missing_required_directories()
        ),
        *(f"リンク先がありません: {link}" for link in broken_links()),
        *(
            f"Issueフォームのtitleが空です: {form}"
            for form in invalid_issue_form_titles()
        ),
        *(
            f"対応する要求シナリオが無いシーケンスです: {sequence}"
            for sequence in orphan_sequences()
        ),
        *(
            f"シナリオ一覧から参照されていません: {scenario}"
            for scenario in unlisted_scenarios()
        ),
        *(
            f"シナリオに必要な文書がありません: {document}"
            for document in incomplete_scenarios()
        ),
    ]
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print("文書の構造: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
