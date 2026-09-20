import subprocess
import time
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture
def gui_process(desktop):
    subprocess.run([str(ROOT / 'scripts/build-gui.sh')], check=True, capture_output=True)
    process = subprocess.Popen([str(ROOT / 'client/shikomi/gui/build/linux/x64/release/bundle/shikomi_gui')],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            if desktop.evaluate('global.get_window_actors().some(a=>a.meta_window.get_title()==="shikomi")') == 'true':
                break
            time.sleep(.1)
        else:
            raise TimeoutError('GUIの起動を確認できません')
        time.sleep(.5)
        yield process
    finally:
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)


class TestGuiLifetime:
    def test_saved_shortcut_works_after_closing_the_window(self, desktop, target, gui_process):
        entry = dict(label='閉じた後', text='画面を閉じても貼り付ける', key='<Control><Alt>j')
        assert desktop.request('add', entry=entry)['ok']
        try:
            desktop.evaluate('global.get_window_actors().find(a=>a.meta_window.get_title()==="shikomi").meta_window.delete(global.get_current_time())')
            assert gui_process.wait(timeout=5) == 0
            desktop.focus_target()
            desktop.chord([0xffe3, 0xffe9, ord('j')])
            for _ in range(60):
                if target.read_text() == entry['text']:
                    break
                time.sleep(.05)
            assert target.read_text() == entry['text']
        finally:
            desktop.request('remove', label=entry['label'])
