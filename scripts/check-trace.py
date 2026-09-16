#!/usr/bin/env python3
"""現在の受入例と実行ケースを対応させる。過去の増分は採番だけ確かめる。"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class TraceCheck:
    ENTRY = re.compile(r"^\| (EX-SC\d{3}-\d{2}) \| [^|]+ \| `([^`]+)` \|$")
    CASE = re.compile(r"\[(EX-SC\d{3}-\d{2})\]$")
    ISSUE = re.compile(r"^対応Issue: #(\d+)\s*$", re.MULTILINE)

    def __init__(self, root: Path = ROOT) -> None:
        self.root = root

    def run(self) -> list[str]:
        entries, failures = self.read_entries()
        failures.extend(self.check_increment_numbers())
        if failures:
            return failures
        collected, errors = self.collect_tests()
        return [*errors, *self.check_targets(entries, collected)]

    def read_entries(self) -> tuple[dict[str, str], list[str]]:
        entries: dict[str, str] = {}
        failures: list[str] = []
        for scenario in sorted(
            (self.root / "docs/110_requirements/シナリオ").glob("SC-*")
        ):
            if not scenario.is_dir():
                continue
            number = scenario.name.split("-", 2)[1]
            path = scenario / "受入例.md"
            local_count = 0
            for line in path.read_text(encoding="utf-8").splitlines():
                match = self.ENTRY.match(line)
                if match is None:
                    if line.startswith("| EX-"):
                        failures.append(
                            f"{path.relative_to(self.root)}: 受入例の行形式が不正です"
                        )
                    continue
                case_id, target = match.groups()
                local_count += 1
                if not case_id.startswith(f"EX-SC{number}-"):
                    failures.append(f"{case_id}: シナリオの番号と一致しません")
                if case_id in entries:
                    failures.append(f"{case_id}: 受入例が重複しています")
                entries[case_id] = target
            if local_count == 0:
                failures.append(f"{path.relative_to(self.root)}: 受入例がありません")
        return entries, failures

    def check_increment_numbers(self) -> list[str]:
        failures: list[str] = []
        for path in (self.root / "docs/210_increments").glob("INC-*.md"):
            match = re.fullmatch(r"INC-(\d+)(?:-\d+)?", path.stem)
            issue = self.ISSUE.search(path.read_text(encoding="utf-8"))
            if match is None or issue is None or match.group(1) != issue.group(1):
                failures.append(f"{path.name}: ファイル名と対応Issueが一致しません")
        return failures

    def collect_tests(self) -> tuple[set[str], list[str]]:
        result = subprocess.run(
            [
                "/usr/bin/python3",
                "-m",
                "pytest",
                "--rootdir=.",
                "--collect-only",
                "-q",
                "tests",
            ],
            cwd=self.root,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            return set(), [
                "テストの収集に失敗しました:\n" + result.stdout + result.stderr
            ]
        return {
            line.strip()
            for line in result.stdout.splitlines()
            if "::" in line and line.startswith(("backend/", "tests/"))
        }, []

    def check_targets(self, entries: dict[str, str], collected: set[str]) -> list[str]:
        failures: list[str] = []
        for case_id, target in entries.items():
            if ".md::" in target:
                if not self.manual_exists(case_id, target):
                    failures.append(
                        f"{case_id}: 手動確認書の見出しがありません: {target}"
                    )
            elif target not in collected or not target.endswith(f"[{case_id}]"):
                failures.append(
                    f"{case_id}: 実行できる個別ケースがありません: {target}"
                )
        for target in collected:
            match = self.CASE.search(target)
            if "/acceptance/" in target and match is None:
                failures.append(f"受入テストにEXのケース名がありません: {target}")
            if match is not None and entries.get(match.group(1)) != target:
                failures.append(
                    f"{match.group(1)}: 現在の受入例一覧に対応する実行物がありません"
                )
        return failures

    def manual_exists(self, case_id: str, target: str) -> bool:
        path_text, anchor = target.split("::", 1)
        path = (self.root / path_text).resolve()
        if not path.is_relative_to(self.root.resolve()) or not path.is_file():
            return False
        return (
            anchor == case_id
            and re.search(
                rf"^#{{1,6}} {re.escape(anchor)}(?:\s|$)",
                path.read_text(encoding="utf-8"),
                re.MULTILINE,
            )
            is not None
        )


if __name__ == "__main__":
    failures = TraceCheck().run()
    if failures:
        print("\n".join(failures), file=sys.stderr)
        raise SystemExit(1)
    print("要求から受入例とテストの対応: OK")
