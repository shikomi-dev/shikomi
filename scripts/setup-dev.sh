#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
command -v lefthook >/dev/null || {
  echo 'lefthookを導入してから再実行してください。' >&2
  exit 1
}
lefthook install
