"""隔離GNOME内の入力先と、実際の入力装置をGUI受入テストから使う。"""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from gi.repository import GLib

from gnome import Desktop
from recording import Recording


class GuiDesktop:
    def __init__(self):
        if os.environ.get('SHIKOMI_ISOLATED_TEST') != '1':
            raise RuntimeError('隔離した検証セッションでのみ実行できます')
        self.desktop = Desktop()

    def run(self, operation, arguments):
        if operation == 'chord':
            time.sleep(.4)
            self.desktop.chord(json.loads(arguments[0]))
        elif operation == 'paste':
            self.paste(json.loads(arguments[0]), arguments[1])
        elif operation == 'focus':
            self.focus()
        elif operation == 'close':
            self.desktop.evaluate('global.get_window_actors().find(a=>a.meta_window.get_title()==="shikomi").meta_window.delete(global.get_current_time())')
        elif operation == 'input':
            self.desktop.evaluate('imports.gi.St.Clipboard.get_default().set_text(imports.gi.St.ClipboardType.CLIPBOARD,' + json.dumps(arguments[0]) + ')')
            self.desktop.chord([0xffe3, ord('a')])
            self.desktop.chord([0xffe3, ord('v')])
        elif operation == 'resize':
            width, height = map(int, arguments)
            self.desktop.evaluate('(()=>{const w=global.get_window_actors().find(a=>a.meta_window.get_title()==="shikomi").meta_window;'
                                  f'w.unmaximize(3);w.move_resize_frame(true,20,40,{width},{height});return true;}})()')
            time.sleep(.3)
        elif operation in ('disable', 'enable'):
            self.desktop.call('org.gnome.Shell', '/org/gnome/Shell', 'org.gnome.Shell.Extensions',
                              'DisableExtension' if operation == 'disable' else 'EnableExtension',
                              GLib.Variant('(s)', ('shikomi@shikomi-dev.github.io',)))
        elif operation == 'snapshot':
            self.focus()
            area = json.loads(self.desktop.evaluate('(()=>{const r=global.display.focus_window.get_frame_rect();return [r.x,r.y,r.width,r.height]})()'))
            self.desktop.call('org.gnome.Shell.Screenshot', '/org/gnome/Shell/Screenshot',
                              'org.gnome.Shell.Screenshot', 'ScreenshotArea',
                              GLib.Variant('(iiiibs)', (*area, False, arguments[0])))
        elif operation == 'record':
            self.focus()
            recording = Recording(self.desktop, arguments[0])
            try:
                recording.prepare()
                recording.start('gui-journey')
                Path(arguments[0], 'recording').touch()
                for _ in range(1800):
                    if Path(arguments[0], 'stop').exists():
                        break
                    time.sleep(.1)
                else:
                    raise TimeoutError('録画の終了通知がありません')
            finally:
                recording.stop()

    def focus(self):
        self.desktop.evaluate('Main.activateWindow(global.get_window_actors().find(a=>a.meta_window.get_title()==="shikomi").meta_window)')
        time.sleep(.2)

    def paste(self, keys, expected):
        path = Path(os.environ['SHIKOMI_SESSION']) / 'gui-pasted.txt'
        if path.exists():
            path.unlink()
        process = subprocess.Popen([sys.executable, str(Path(__file__).with_name('target.py'))],
                                   env=dict(os.environ, SHIKOMI_OBSERVATION=str(path)))
        try:
            for _ in range(100):
                if path.exists():
                    break
                time.sleep(.05)
            self.desktop.focus_target()
            self.desktop.chord(keys)
            for _ in range(100):
                if path.exists() and path.read_text() == expected:
                    break
                time.sleep(.05)
            assert path.exists() and path.read_text() == expected, path.read_text() if path.exists() else '入力先なし'
        finally:
            process.terminate()
            process.wait(timeout=5)
            self.focus()


if __name__ == '__main__':
    GuiDesktop().run(sys.argv[1], sys.argv[2:])
