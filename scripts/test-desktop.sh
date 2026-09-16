#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${SHIKOMI_ISOLATED_TEST:-}" != 1 ]]; then
  session="$(mktemp -d -t shikomi-desktop.XXXXXX)"
  export SHIKOMI_ISOLATED_TEST=1 SHIKOMI_SESSION="$session"
  exec dbus-run-session -- "$0" "$@"
fi
export XDG_DATA_HOME="$SHIKOMI_SESSION/data" XDG_CONFIG_HOME="$SHIKOMI_SESSION/config"
export XDG_RUNTIME_DIR="$SHIKOMI_SESSION/runtime" XDG_CACHE_HOME="$SHIKOMI_SESSION/cache"
export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME GDK_BACKEND=wayland
export GSETTINGS_BACKEND=memory LIBGL_ALWAYS_SOFTWARE=1
export WAYLAND_DISPLAY=shikomi-test
unset DISPLAY
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME/gnome-shell/extensions/shikomi@shikomi-dev.github.io"
chmod 700 "$XDG_RUNTIME_DIR"
cp "$root"/client/shikomi/extension/* "$XDG_DATA_HOME/gnome-shell/extensions/shikomi@shikomi-dev.github.io/"
mkdir -p "$XDG_DATA_HOME/gnome-shell/extensions/test-observer@shikomi"
cp "$root"/tests/support/observer/* "$XDG_DATA_HOME/gnome-shell/extensions/test-observer@shikomi/"
gnome-shell --headless --wayland --no-x11 --virtual-monitor 1280x800 --wayland-display "$WAYLAND_DISPLAY" > "$SHIKOMI_SESSION/shell.log" 2>&1 &
shell_pid=$!
trap 'kill "$shell_pid" 2>/dev/null || true; wait "$shell_pid" 2>/dev/null || true' EXIT
for attempt in $(seq 1 100); do
  if gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Extensions.EnableExtension shikomi@shikomi-dev.github.io >/dev/null 2>&1 &&
     "$root/client/shikomi/shikomi" list >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$shell_pid" 2>/dev/null; then
    cat "$SHIKOMI_SESSION/shell.log" >&2; exit 1
  fi
  sleep .2
done
gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Extensions.EnableExtension test-observer@shikomi >/dev/null
sleep .3
"$root/client/shikomi/shikomi" list
cd "$root"
/usr/bin/python3 -m pytest tests/acceptance tests/integration -v -o "cache_dir=$SHIKOMI_SESSION/pytest-cache" "$@"
