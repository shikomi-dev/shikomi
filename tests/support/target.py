"""隔離GNOME上で、実際のGTK入力欄に届いた文字列を観測する。"""
import os
from pathlib import Path

import gi

gi.require_version('Gtk', '4.0')
from gi.repository import Gtk, GLib


class Target(Gtk.Application):
    def do_activate(self) -> None:
        window = Gtk.ApplicationWindow(
            application=self, title='shikomi 貼り付け確認',
            default_width=800, default_height=240,
        )
        self.entry = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        for side in ('top', 'bottom', 'start', 'end'):
            getattr(self.entry, f'set_margin_{side}')(30)
        self.entry.get_buffer().connect('changed', self.changed)
        window.set_child(self.entry)
        window.present()
        self.entry.grab_focus()
        GLib.idle_add(self.ready)

    def ready(self) -> bool:
        Path(os.environ['SHIKOMI_OBSERVATION']).write_text('')
        return GLib.SOURCE_REMOVE

    def changed(self, buffer: Gtk.TextBuffer) -> None:
        Path(os.environ['SHIKOMI_OBSERVATION']).write_text(buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), True))


Target(application_id='io.github.shikomi.TestTarget').run()
