#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root/client/shikomi/gui"
"$root/scripts/flutter.sh" pub get --enforce-lockfile
"$root/scripts/flutter.sh" build linux --release
