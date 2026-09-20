import subprocess
import time
from pathlib import Path

import pexpect
import pytest
from gi.repository import GLib

ROOT = Path(__file__).resolve().parents[2]
CLI = str(ROOT / 'client/shikomi/shikomi')


class TestGuards:
    @pytest.mark.parametrize('replacement', [
        {'label': 'existing', 'text': 'new', 'key': '<Control><Alt>k'},
        {'label': 'other', 'text': 'new', 'key': '<Control><Alt>j'},
        {'label': '', 'text': 'new', 'key': '<Control><Alt>k'},
        {'label': 'other', 'text': '', 'key': '<Control><Alt>k'},
        {'label': 'other', 'text': 'new', 'key': 'j'},
    ])
    def test_invalid_add_preserves_existing(self, desktop, replacement):
        entry = {'label': 'existing', 'text': 'original', 'key': '<Control><Alt>j'}
        assert desktop.request('add', entry=entry)['ok']
        try:
            result = desktop.request('add', entry=replacement)
            assert not result['ok']
            assert desktop.request('get', label='existing')['result'] == entry
            assert len(desktop.request('list')['result']) == 1
        finally:
            desktop.request('remove', label='existing')

    def test_escape_cancels_without_saving(self, desktop, target):
        desktop.evaluate('global.previousTestInputMethod = global.shikomiTestBackend.get_input_method(); true')
        child = pexpect.spawn(CLI, ['add', 'cancel me'], encoding='utf-8', timeout=8)
        try:
            child.expect('Escで取消し）:')
            time.sleep(.2)
            desktop.chord([0xff1b])
            child.expect('取り消しました。')
            child.expect(pexpect.EOF)
            child.close()
            assert child.exitstatus == 130
            assert desktop.evaluate('global.shikomiTestBackend.get_input_method() === global.previousTestInputMethod') == 'true'
            assert desktop.request('list')['result'] == []
        finally:
            child.close(force=True)

    def test_disconnect_releases_capture(self, desktop, target):
        desktop.evaluate('global.disconnectTestInputMethod = global.shikomiTestBackend.get_input_method(); true')
        child = pexpect.spawn(CLI, ['add', 'disconnect'], encoding='utf-8', timeout=8)
        child.expect('Escで取消し）:')
        time.sleep(.2)
        child.close(force=True)
        time.sleep(.2)
        assert desktop.evaluate('global.shikomiTestBackend.get_input_method() === global.disconnectTestInputMethod') == 'true'
        self.test_escape_cancels_without_saving(desktop, target)

    def test_stale_edit_preserves_new_value(self, desktop):
        old = dict(label='version', text='original', key='<Control><Alt>j')
        new = dict(old, text='new')
        assert desktop.request('add', entry=old)['ok']
        try:
            assert desktop.request('edit', label='version', entry=new, previous=old)['ok']
            assert not desktop.request('edit', label='version', entry=old, previous=old)['ok']
            assert desktop.request('get', label='version')['result'] == new
        finally:
            desktop.request('remove', label='version')

    def test_missing_label_fails(self, desktop):
        result = subprocess.run([CLI, 'remove', 'missing'], capture_output=True, text=True)
        assert result.returncode == 1
        assert '指定したラベルはありません' in result.stderr
        assert result.stdout == ''

    def test_stale_delete_preserves_new_value(self, desktop):
        old = dict(label='delete-version', text='original', key='<Control><Alt>j')
        new = dict(old, text='changed elsewhere')
        assert desktop.request('add', entry=old)['ok']
        try:
            assert desktop.request('edit', label=old['label'], entry=new, previous=old)['ok']
            assert not desktop.request('remove', label=old['label'], previous=old)['ok']
            assert desktop.request('get', label=old['label'])['result'] == new
            reordered = dict(key=new['key'], text=new['text'], label=new['label'])
            assert desktop.request('remove', label=old['label'], previous=reordered)['ok']
        finally:
            desktop.request('remove', label=old['label'])

    def test_restart_restores_entries(self, desktop):
        entry = dict(label='restore', text='restart', key='<Control><Alt>j')
        assert desktop.request('add', entry=entry)['ok']
        try:
            for command in ('disable', 'enable'):
                desktop.call('org.gnome.Shell', '/org/gnome/Shell', 'org.gnome.Shell.Extensions',
                             command.title() + 'Extension', GLib.Variant('(s)', ('shikomi@shikomi-dev.github.io',)))
                time.sleep(.3)
            assert desktop.request('get', label='restore')['result'] == entry
        finally:
            desktop.request('remove', label='restore')

    def test_change_key_releases_old_key(self, desktop):
        old = dict(label='move', text='text', key='<Control><Alt>j')
        new = dict(label='renamed', text='new', key='<Control><Alt>k')
        assert desktop.request('add', entry=old)['ok']
        try:
            assert desktop.request('edit', label='move', entry=new, previous=old)['ok']
            assert not desktop.request('get', label='move')['ok']
            assert desktop.request('get', label='renamed')['result'] == new
            assert desktop.request('add', entry=old)['ok']
        finally:
            desktop.request('remove', label='move')
            desktop.request('remove', label='renamed')

    def test_save_failure_releases_new_key(self, desktop):
        import os
        data = Path(os.environ['XDG_DATA_HOME']) / 'shikomi'
        path = data / 'entries.json'
        backup = data / 'entries.saved'
        old = dict(label='keep', text='original', key='<Control><Alt>j')
        other = dict(label='new', text='replacement', key='<Control><Alt>k')
        assert desktop.request('add', entry=old)['ok']
        try:
            path.rename(backup)
            path.mkdir()
            try:
                assert not desktop.request('add', entry=other)['ok']
                assert desktop.request('get', label='keep')['result'] == old
                assert not desktop.request('get', label='new')['ok']
            finally:
                path.rmdir()
                backup.rename(path)
            assert desktop.request('add', entry=other)['ok']
        finally:
            desktop.request('remove', label='keep')
            desktop.request('remove', label='new')
