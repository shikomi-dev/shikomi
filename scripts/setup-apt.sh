#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
base=https://shikomi-dev.github.io/shikomi/apt
key="$(mktemp -t shikomi-public-key.XXXXXX)"
trap 'rm -f "$key"' EXIT
curl -fsSL "$base/shikomi-archive-keyring.gpg" -o "$key"
fingerprint="$(gpg --batch --with-colons --show-keys "$key" | awk -F: '$1 == "fpr" {print $10; exit}')"
if [[ "$fingerprint" != "$(cat "$root/packaging/apt/fingerprint.txt")" ]]; then
  echo '公開鍵が一致しないため、配布元を登録していません。' >&2
  exit 1
fi
sudo install -m 644 "$key" /usr/share/keyrings/shikomi-archive-keyring.gpg
printf '%s\n' "deb [arch=amd64 signed-by=/usr/share/keyrings/shikomi-archive-keyring.gpg] $base stable main" \
  | sudo tee /etc/apt/sources.list.d/shikomi.list >/dev/null
echo '配布元を登録しました。sudo apt update && sudo apt install shikomi で導入できます。'
