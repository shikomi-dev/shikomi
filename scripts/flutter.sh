#!/usr/bin/env bash
set -euo pipefail
version=3.47.5
checksum=2132e990f236f8d22e7c6314b29a191a95b10d7cbcfec9b4e2e303d996652cbb
if [[ -n "${FLUTTER_BIN:-}" ]]; then
  exec "$FLUTTER_BIN" "$@"
fi
sdk="$HOME/.cache/shikomi/flutter-$version"
if [[ ! -x "$sdk/flutter/bin/flutter" ]]; then
  mkdir -p "$sdk"
  archive="$(mktemp -t shikomi-flutter.XXXXXX)"
  trap 'rm -f "$archive"' EXIT
  curl -fsSL --retry 3 "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_${version}-stable.tar.xz" -o "$archive"
  printf '%s  %s\n' "$checksum" "$archive" | sha256sum -c - >&2
  tar --no-same-owner -xf "$archive" -C "$sdk"
  "$sdk/flutter/bin/flutter" --disable-analytics >&2
fi
exec "$sdk/flutter/bin/flutter" "$@"
