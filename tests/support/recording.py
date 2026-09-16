"""明示したときだけ、端末ウィンドウの範囲をGNOMEで録画する。"""
import json
from pathlib import Path
import subprocess
import time

from gi.repository import GLib


class Recording:
    def __init__(self, desktop, directory: str) -> None:
        self.desktop = desktop
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True)
        self.area = json.loads(desktop.evaluate(
            '(()=>{const r=global.display.focus_window.get_frame_rect();return [r.x,r.y,r.width,r.height]})()'))
        self.active = False

    def prepare(self) -> None:
        preview = self.start('preview')
        time.sleep(.7)
        self.stop()
        subprocess.run(['ffmpeg', '-loglevel', 'error', '-i', preview, '-ss', '0.3',
                        '-frames:v', '1', str(self.directory / 'preview.png')], check=True)
        (self.directory / 'ready').touch()
        for _ in range(2400):
            if (self.directory / 'continue').exists():
                return
            time.sleep(.1)
        raise TimeoutError('試し撮りの目視確認を待っています')

    def start(self, name: str) -> str:
        if list(self.directory.glob(name + '.*')):
            raise FileExistsError('録画の出力先は新しい場所を使ってください')
        success, filename = self.desktop.call(
            'org.gnome.Shell.Screencast', '/org/gnome/Shell/Screencast',
            'org.gnome.Shell.Screencast', 'ScreencastArea',
            GLib.Variant('(iiiisa{sv})', (*self.area, str(self.directory / name), {})))
        assert success
        self.active = True
        return filename

    def stop(self) -> None:
        if self.active:
            result = self.desktop.call('org.gnome.Shell.Screencast', '/org/gnome/Shell/Screencast',
                                       'org.gnome.Shell.Screencast', 'StopScreencast')
            assert result[0]
            self.active = False
