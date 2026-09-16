"""公開D-Busと入力装置を通して、隔離したGNOMEを操作する。"""
import json
import time

from gi.repository import Gio, GLib


class Desktop:
    def __init__(self) -> None:
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        for _ in range(100):
            if self.evaluate('!Main.layoutManager._startingUp') == 'true':
                break
            time.sleep(.05)
        else:
            raise TimeoutError('GNOMEの画面準備が完了していません')
        self.evaluate('global.shikomiPrepareTestInput()')

    def call(self, destination: str, path: str, interface: str,
             method: str, parameters: GLib.Variant | None = None) -> tuple:
        return self.bus.call_sync(destination, path, interface, method, parameters,
                                  None, 0, 5000, None).unpack()

    def key(self, symbol: int, pressed: bool) -> None:
        self.evaluate(f'global.shikomiTestKeyboard.notify_keyval(GLib.get_monotonic_time(), {symbol}, {1 if pressed else 0})')

    def chord(self, keys: list[int]) -> None:
        for key in keys:
            self.key(key, True)
            time.sleep(.06)
        for key in reversed(keys):
            self.key(key, False)
            time.sleep(.06)
        time.sleep(.15)

    def type_text(self, text: str) -> None:
        for character in text:
            symbol = ord(character)
            if symbol > 127:
                symbol |= 0x01000000
            self.key(symbol, True)
            time.sleep(.025)
            self.key(symbol, False)
            time.sleep(.015)
        time.sleep(.1)

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

