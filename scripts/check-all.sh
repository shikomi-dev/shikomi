#!/usr/bin/env bash
set -euo pipefail
export LANG=C.UTF-8 LC_ALL=C.UTF-8
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
python3 scripts/check-docs.py
python3 scripts/check-trace.py
python3 scripts/check-secrets.py
PYTHONPYCACHEPREFIX="$(mktemp -d -t shikomi-pycache.XXXXXX)" python3 -m py_compile client/shikomi/shikomi tests/support/*.py
node --input-type=module --check < client/shikomi/extension/extension.js
node --input-type=module --check < client/shikomi/extension/entries.js
gjs -m tests/integration/storage.js
(cd client/shikomi/gui && "$root/scripts/flutter.sh" pub get --enforce-lockfile && \
  "$root/scripts/flutter.sh" analyze && "$root/scripts/flutter.sh" test test)
./scripts/test-desktop.sh
