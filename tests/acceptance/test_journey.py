import json
import subprocess
import time
from pathlib import Path

import pexpect
import pytest

ROOT = Path(__file__).resolve().parents[2]
CLI = str(ROOT / 'client/shikomi/shikomi')
CASES = json.loads((Path(__file__).parent / 'cases.json').read_text())
KEYS = [0xffe3, 0xffe9, ord('j')]


class TestJourney:
    @pytest.mark.parametrize('case', CASES, ids=lambda row: row['id'])
    def test_register_paste_edit_remove(self, desktop, target, case):
        child = pexpect.spawn(CLI, ['add', case['text']], encoding='utf-8', timeout=8)
        try:
            child.expect('Escで取消し）:')
            time.sleep(.2)
            desktop.chord(KEYS)
            child.expect('ラベル:')
            child.sendline(case['label'])
            child.expect('登録しました。')
            child.expect(pexpect.EOF)
            child.close()
            assert child.exitstatus == 0
            time.sleep(.3)
            desktop.focus_target()
            desktop.chord(KEYS)
            self.expect_text(target, case['text'])
            edit = pexpect.spawn(CLI, ['edit', case['label']], encoding='utf-8', timeout=8)
            edit.expect('新しい文字列（Enterで変更なし）:')
            edit.sendline(case['edited'])
            edit.expect('キーを変更しますか？')
            edit.sendline('n')
            edit.expect('新しいラベル（Enterで変更なし）:')
            edit.sendline('')
            edit.expect('変更しました。')
            edit.expect(pexpect.EOF)
            edit.close()
            assert edit.exitstatus == 0
            desktop.focus_target()
            desktop.chord(KEYS)
            self.expect_text(target, case['text'] + case['edited'])
            result = subprocess.run([CLI, 'remove', case['label']], capture_output=True, text=True)
            assert result.returncode == 0, result.stderr
            assert result.stdout == '削除しました。\n'
            before = target.read_text()
            desktop.chord(KEYS)
            assert target.read_text() == before
            assert desktop.request('list')['result'] == []
        finally:
            child.close(force=True)
            desktop.request('remove', label=case['label'])

    def expect_text(self, path, expected):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if path.exists() and path.read_text() == expected:
                return
            time.sleep(.05)
        assert path.exists() and path.read_text() == expected
