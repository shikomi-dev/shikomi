import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Entries} from './entries.js';
import {MODIFIERS, Shortcuts} from './shortcuts.js';

const XML = `<node><interface name="io.github.shikomi.Control">
<method name="Request"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
<method name="Capture"><arg type="s" direction="out"/></method>
<method name="CancelCapture"/>
</interface></node>`;
const REQUIRED = MODIFIERS & ~Clutter.ModifierType.SHIFT_MASK;

class KeyCapture {
    constructor(invocation, done) {
        this.invocation = invocation;
        this.done = done;
        this.sender = invocation.get_sender();
        this.key = '';
        this.watch = Gio.bus_watch_name_on_connection(Gio.DBus.session, this.sender,
            Gio.BusNameWatcherFlags.NONE, null, () => this.finish(''));
        this.actor = new St.Widget({reactive: true, can_focus: true, width: global.stage.width, height: global.stage.height});
        Main.uiGroup.add_child(this.actor);
        this.grab = Main.pushModal(this.actor, {actionMode: Shell.ActionMode.SYSTEM_MODAL});
        this.actor.connect('captured-event', (_actor, event) => this.onEvent(event));
        this.stageSignal = global.stage.connect('captured-event', (_stage, event) => this.onEvent(event));
        // キーそのものを読む間は、日本語入力などの文字変換を通さない。
        this.backend = Clutter.get_default_backend();
        this.inputMethod = this.backend.get_input_method();
        this.backend.set_input_method(null);
        this.timeout = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 60, () => {
            this.timeout = 0;
            this.finish('');
            return GLib.SOURCE_REMOVE;
        });
    }

    onEvent(event) {
        if (global.stage.get_key_focus() !== this.actor)
            return Clutter.EVENT_PROPAGATE;
        if (event.type() === Clutter.EventType.KEY_PRESS) {
            const symbol = event.get_key_symbol();
            if (symbol === Clutter.KEY_Escape) {
                this.finish('');
            } else if (!this.key && ![
                Clutter.KEY_Control_L, Clutter.KEY_Control_R,
                Clutter.KEY_Alt_L, Clutter.KEY_Alt_R,
                Clutter.KEY_Super_L, Clutter.KEY_Super_R,
                Clutter.KEY_Shift_L, Clutter.KEY_Shift_R,
                Clutter.KEY_Caps_Lock, Clutter.KEY_Num_Lock,
            ].includes(symbol)) {
                const modifiers = event.get_state() & MODIFIERS;
                if (modifiers & REQUIRED)
                    this.key = Meta.accelerator_name(modifiers, symbol);
            }
        } else if (event.type() === Clutter.EventType.KEY_RELEASE && this.key && !this.released) {
            this.released = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 25, () => {
                if (global.get_pointer()[2] & MODIFIERS)
                    return GLib.SOURCE_CONTINUE;
                this.released = 0;
                this.finish(this.key);
                return GLib.SOURCE_REMOVE;
            });
        }
        return Clutter.EVENT_STOP;
    }

    finish(key) {
        if (!this.invocation)
            return;
        if (this.timeout)
            GLib.Source.remove(this.timeout);
        if (this.released)
            GLib.Source.remove(this.released);
        if (this.stageSignal)
            global.stage.disconnect(this.stageSignal);
        Gio.bus_unwatch_name(this.watch);
        Main.popModal(this.grab);
        this.backend.set_input_method(this.inputMethod);
        this.inputMethod = null;
        this.actor.destroy();
        this.invocation.return_value(new GLib.Variant('(s)', [key]));
        this.invocation = null;
        this.done();
    }
}

export default class Shikomi extends Extension {
    enable() {
        this.entries = new Entries();
        this.shortcuts = new Shortcuts(this.entries);
        this.object = Gio.DBusExportedObject.wrapJSObject(XML, this);
        this.object.export(Gio.DBus.session, '/io/github/shikomi/Control');
    }

    Request(json) {
        try {
            const request = JSON.parse(json);
            if (Main.sessionMode.isLocked)
                throw new Error('画面ロック中は操作できません。');
            return JSON.stringify({ok: true, result: this.request(request)});
        } catch (error) {
            return JSON.stringify({ok: false, error: error.message});
        }
    }

    request(request) {
        switch (request.operation) {
        case 'list':
            return this.entries.items.map(({label, key}) => ({label, key}));
        case 'get':
            return this.entries.find(request.label);
        case 'checkKey':
            this.entries.validateKey(request.key);
            return this.shortcuts.check(request.key, request.owner);
        case 'getSearchKey':
            return this.shortcuts.searchSetting();
        case 'setSearchKey':
            this.shortcuts.setSearchKey(request.key, request.previous);
            return null;
        case 'add':
        case 'edit':
        case 'remove':
            this.shortcuts.change(this.entries.changed(request.operation, request));
            return null;
        default:
            throw new Error('未対応の操作です。');
        }
    }

    CaptureAsync(_parameters, invocation) {
        if (this.capture || this.shortcuts.searchOpen || Main.sessionMode.isLocked) {
            invocation.return_dbus_error('io.github.shikomi.Busy', '別の入力待ちか画面ロック中です。');
            return;
        }
        this.capture = new KeyCapture(invocation, () => { this.capture = null; });
    }

    CancelCaptureAsync(_parameters, invocation) {
        if (this.capture?.sender === invocation.get_sender())
            this.capture.finish('');
        invocation.return_value(null);
    }

    disable() {
        this.capture?.finish('');
        this.capture = null;
        this.object?.unexport();
        this.object = null;
        this.shortcuts?.destroy();
        this.shortcuts = null;
        this.entries = null;
    }
}
