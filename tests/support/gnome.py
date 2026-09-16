"""公開D-Busと入力装置を通して、隔離したGNOMEを操作する。"""
import json
import time

from gi.repository import Gio, GLib


class Desktop:
    def __init__(self) -> None:
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.path = self.call(
            'org.gnome.Mutter.RemoteDesktop', '/org/gnome/Mutter/RemoteDesktop',
            'org.gnome.Mutter.RemoteDesktop', 'CreateSession',
        )[0]
        self.remote('Start')

    def call(self, destination: str, path: str, interface: str,
             method: str, parameters: GLib.Variant | None = None) -> tuple:
        return self.bus.call_sync(destination, path, interface, method, parameters,
                                  None, 0, 5000, None).unpack()

    def remote(self, method: str, parameters: GLib.Variant | None = None) -> tuple:
        return self.call('org.gnome.Mutter.RemoteDesktop', self.path,
                         'org.gnome.Mutter.RemoteDesktop.Session', method, parameters)

    def chord(self, keys: list[int]) -> None:
        for key in keys:
            self.remote('NotifyKeyboardKeysym', GLib.Variant('(ub)', (key, True)))
            time.sleep(.06)
        for key in reversed(keys):
            self.remote('NotifyKeyboardKeysym', GLib.Variant('(ub)', (key, False)))
            time.sleep(.06)
        time.sleep(.15)

    def request(self, operation: str, **values: object) -> dict:
        result = self.call('org.gnome.Shell', '/io/github/shikomi/Control',
                           'io.github.shikomi.Control', 'Request', GLib.Variant(
                               '(s)', (json.dumps({'operation': operation, **values}),)))
        return json.loads(result[0])

    def evaluate(self, code: str) -> str:
        ok, value = self.call('org.gnome.Shell', '/org/gnome/Shell', 'org.gnome.Shell',
                              'Eval', GLib.Variant('(s)', (code,)))
        assert ok, value
        return value

    def focus_target(self) -> None:
        self.evaluate('Main.activateWindow(global.get_window_actors().find(a => a.meta_window.get_title() === "shikomi 貼り付け確認").meta_window)')
        time.sleep(.3)
        self.chord([0xffe3, 0xff57])

    def close(self) -> None:
        self.remote('Stop')
