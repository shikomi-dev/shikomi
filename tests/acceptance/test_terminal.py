import json
import os
from pathlib import Path
import shlex
import subprocess
import time

import pytest
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import GLib, Atspi

ROOT = Path(__file__).resolve().parents[2]
CASE = json.loads((Path(__file__).parent / 'terminal-case.json').read_text())


class TestTerminal:
    @pytest.mark.parametrize('case', [CASE], ids=lambda row: row['id'])
    def test_commands_change_and_stop(self, desktop, tmp_path, case):
        cli = os.environ.get('SHIKOMI_TEST_CLI', str(ROOT / 'client/shikomi/shikomi'))
        environment = dict(os.environ, PS1='> ', HISTFILE='/dev/null')
        terminal = subprocess.Popen([
            'gnome-terminal', '--wait', '--hide-menubar', '--title', 'shikomi 操作確認',
            '--geometry=110x24', '--', 'script', '-q', '-f', '-c',
            "env PS1='> ' HISTFILE=/dev/null bash --noprofile --norc", 'terminal.log',
        ], cwd=tmp_path, env=environment)
        log = tmp_path / 'terminal.log'
        result = tmp_path / 'result.txt'
        try:
            self.await_text(log, '> ')
            desktop.evaluate('Main.activateWindow(global.get_window_actors().find(a => a.meta_window.get_title() === "shikomi 操作確認").meta_window)')
            time.sleep(.6)
            assert 'shikomi 操作確認' in desktop.evaluate('global.display.focus_window?.get_title()')
            assert self.focus_terminal(Atspi.get_desktop(0))
            time.sleep(.3)
            desktop.type_text(shlex.join([cli, 'add', case['before']]))
            desktop.chord([0xff0d])
            self.await_text(log, 'Escで取消し）:')
            time.sleep(.2)
            desktop.chord([0xffe3, 0xffe9, ord('j')])
            self.await_text(log, 'ラベル:')
            desktop.type_text(case['label'])
            desktop.chord([0xff0d])
            self.await_text(log, '登録しました。')
            desktop.chord([0xffe3, 0xffe9, ord('j')])
            desktop.chord([0xff0d])
            self.await_text(result, 'before\n')
            desktop.type_text(shlex.join([cli, 'edit', case['label']]))
            desktop.chord([0xff0d])
            self.await_text(log, '新しい文字列')
            desktop.type_text(case['after'])
            desktop.chord([0xff0d])
            self.await_text(log, 'キーを変更しますか')
            desktop.type_text('n')
            desktop.chord([0xff0d])
            self.await_text(log, '新しいラベル')
            desktop.chord([0xff0d])
            self.await_text(log, '変更しました。')
            desktop.chord([0xffe3, 0xffe9, ord('j')])
            desktop.chord([0xff0d])
            self.await_text(result, 'after\n')
            desktop.type_text(shlex.join([cli, 'remove', case['label']]))
            desktop.chord([0xff0d])
            self.await_text(log, '削除しました。')
            original = result.stat().st_mtime_ns
            desktop.chord([0xffe3, 0xffe9, ord('j')])
            desktop.chord([0xff0d])
            time.sleep(.3)
            assert result.stat().st_mtime_ns == original
            assert desktop.request('list')['result'] == []
            desktop.type_text('exit')
            desktop.chord([0xff0d])
            terminal.wait(timeout=5)
        except BaseException:
            print(desktop.evaluate('JSON.stringify([global.display.focus_window?.get_title(), Main.actionMode, Main.overview.visible, global.stage.key_focus?.toString()])'))
            rect = json.loads(desktop.evaluate('(()=>{const r=global.display.focus_window.get_frame_rect();return [r.x,r.y,r.width,r.height]})()'))
            desktop.call('org.gnome.Shell', '/org/gnome/Shell/Screenshot', 'org.gnome.Shell.Screenshot',
                         'ScreenshotArea', GLib.Variant('(iiiibs)', (*rect, False, str(tmp_path / 'failure.png'))))
            raise
        finally:
            desktop.request('remove', label=case['label'])
            if terminal.poll() is None:
                terminal.terminate()
                terminal.wait(timeout=5)

    def focus_terminal(self, accessible):
        if accessible.get_role() == Atspi.Role.TERMINAL:
            assert accessible.get_component_iface().grab_focus()
            return True
        for index in range(accessible.get_child_count()):
            if self.focus_terminal(accessible.get_child_at_index(index)):
                return True
        return False

    def await_text(self, path, text):
        for _ in range(150):
            if path.exists() and text in path.read_text(errors='replace'):
                return
            time.sleep(.05)
        assert path.exists() and text in path.read_text(errors='replace'), path.read_text(errors='replace') if path.exists() else path
