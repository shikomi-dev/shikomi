#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 3 ]]; then
  echo '使い方: build-apt-repository.sh <deb> <公開鍵> <新しい出力先>' >&2
  exit 2
fi
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
deb="$1"; public_key="$2"; output="$3"
if [[ -e "$output" || ! -f "$public_key" ]]; then
  echo '公開鍵、または出力先を確認してください。' >&2
  exit 1
fi
if [[ "$(dpkg-deb -f "$deb" Package)" != shikomi || \
      "$(dpkg-deb -f "$deb" Version)" != "$(cat "$root/VERSION")" || \
      "$(dpkg-deb -f "$deb" Architecture)" != amd64 ]]; then
  echo 'shikomiの配布物とVERSIONが一致していません。' >&2
  exit 1
fi
fingerprint="$(gpg --batch --with-colons --show-keys "$public_key" | awk -F: '$1 == "fpr" {print $10; exit}')"
[[ -n "$fingerprint" ]] || { echo '公開鍵を読み取れません。' >&2; exit 1; }
state="$(mktemp -d -t shikomi-apt.XXXXXX)"
trap 'rm -rf "$state"' EXIT
mkdir -p "$state/conf"
cat "$root/packaging/apt/distributions" > "$state/conf/distributions"
printf 'SignWith: %s\n' "$fingerprint" >> "$state/conf/distributions"
reprepro --basedir "$state" --section utils --priority optional includedeb stable "$deb"
mkdir -p "$output"
cp -R "$state/dists" "$state/pool" "$output/"
install -m 644 "$public_key" "$output/shikomi-archive-keyring.gpg"
touch "$output/.nojekyll"
