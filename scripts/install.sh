#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. /etc/os-release
if [[ "$ID" != ubuntu || ( "$VERSION_ID" != 24.04 && "$VERSION_ID" != 26.04 ) ]]; then
  echo 'Ubuntu 24.04 / 26.04 が必要です。' >&2
  exit 1
fi
/usr/bin/python3 -c 'from gi.repository import Gio' || {
  echo 'sudo apt install python3-gi を実行してください。' >&2
  exit 1
}
uuid=shikomi@shikomi-dev.github.io
"$root/scripts/build-gui.sh"
extension="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$uuid"
install -d -m 700 "$extension" "$HOME/.local/bin"
install -m 644 "$root"/client/shikomi/extension/{extension.js,entries.js,metadata.json} "$extension/"
install -m 755 "$root/client/shikomi/shikomi" "$HOME/.local/bin/shikomi"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
install -d "$data/shikomi/gui" "$data/applications" "$data/icons"
cp -R "$root/client/shikomi/gui/build/linux/x64/release/bundle/." "$data/shikomi/gui/"
ln -sfn "$data/shikomi/gui/shikomi_gui" "$HOME/.local/bin/shikomi-gui"
sed "s|^Exec=.*|Exec=\"$HOME/.local/bin/shikomi-gui\"|" \
  "$root/packaging/io.github.shikomi.shikomi_gui.desktop" > "$data/applications/io.github.shikomi.shikomi_gui.desktop"
install -m 644 "$root/client/shikomi/gui/assets/app-icon.png" "$data/icons/io.github.shikomi.shikomi_gui.png"
echo 'インストールしました。ログアウトしてログインし直した後、次を実行してください。'
echo "gnome-extensions enable $uuid"
echo 'その後 shikomi add "登録したい文字列" を実行できます。'
echo 'アプリ一覧から shikomi を開くか、shikomi-gui を実行すると画面で登録できます。'
