#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(cat "$root/VERSION")"
tag="${1:-v$version}"
output="${2:-$root/dist}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ || "$tag" != "v$version" ]]; then
  echo 'ReleaseタグとVERSIONが一致していません。' >&2
  exit 1
fi
stage="$(mktemp -d -t shikomi-deb.XXXXXX)"
trap 'rm -rf "$stage"' EXIT
install -d "$stage/DEBIAN" "$stage/usr/bin" "$stage/usr/share/doc/shikomi" \
  "$stage/usr/share/gnome-shell/extensions/shikomi@shikomi-dev.github.io"
install -m 755 "$root/client/shikomi/shikomi" "$stage/usr/bin/shikomi"
install -m 644 "$root"/client/shikomi/extension/{extension.js,entries.js,metadata.json} \
  "$stage/usr/share/gnome-shell/extensions/shikomi@shikomi-dev.github.io/"
install -m 644 "$root/LICENSE" "$stage/usr/share/doc/shikomi/copyright"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: shikomi
Version: $version
Section: utils
Priority: optional
Architecture: all
Maintainer: shikomi maintainers <shikomi@users.noreply.github.com>
Depends: python3 (>= 3.12), python3-gi, gnome-shell (>= 46), gnome-shell (<< 51)
Homepage: https://github.com/shikomi-dev/shikomi
Description: Insert saved text using a GNOME keyboard shortcut
 Register text interactively, and edit or remove it by label.
 Tested on Ubuntu 24.04 and Ubuntu 26.04 with GNOME Wayland.
CONTROL
cat > "$stage/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
  echo 'shikomi: 初回導入・更新後はログインし直してください。'
  echo '初回は gnome-extensions enable shikomi@shikomi-dev.github.io を実行してください。'
fi
POSTINST
chmod 755 "$stage/DEBIAN/postinst"
mkdir -p "$output"
dpkg-deb --root-owner-group --build "$stage" "$output/shikomi_${version}_all.deb"
