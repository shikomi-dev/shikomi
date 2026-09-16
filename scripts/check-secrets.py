#!/usr/bin/env python3
"""作業ツリーへ誤って置かれた代表的な認証情報を検出する。"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SKIP_DIRECTORIES = {
    ".git",
    ".pytest_cache",
    ".venv",
    "__pycache__",
    "bin",
    "coverage",
    "dist",
}
SKIP_SUFFIXES = {".gif", ".ico", ".jpeg", ".jpg", ".mp4", ".pdf", ".png", ".webp"}
PATTERNS = (
    re.compile("BEGIN " + "(?:RSA |EC |OPENSSH )?PRIVATE KEY"),
    re.compile("gh" + r"p_[A-Za-z0-9]{30,}"),
    re.compile("github" + r"_pat_[A-Za-z0-9_]{30,}"),
    re.compile("AKIA" + r"[A-Z0-9]{16}"),
    re.compile("sk" + r"-[A-Za-z0-9]{32,}"),
)


def text_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or any(part in SKIP_DIRECTORIES for part in path.parts):
            continue
        if path.suffix.lower() in SKIP_SUFFIXES:
            continue
        files.append(path)
    return files


def main() -> int:
    matches: list[str] = []
    for path in text_files():
        try:
            content = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for line_number, line in enumerate(content.splitlines(), start=1):
            if any(pattern.search(line) for pattern in PATTERNS):
                matches.append(f"{path.relative_to(ROOT)}:{line_number}")
    if matches:
        print("機密らしい文字列を検出しました:", file=sys.stderr)
        print("\n".join(matches), file=sys.stderr)
        return 1
    print("機密らしい文字列: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
