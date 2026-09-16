import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Entries} from './entries.js';

const XML = `<node><interface name="io.github.shikomi.Control">
<method name="Request"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
<method name="Capture"><arg type="s" direction="out"/></method>
<method name="CancelCapture"/>
</interface></node>`;
const MODIFIERS = Clutter.ModifierType.CONTROL_MASK | Clutter.ModifierType.MOD1_MASK |
    Clutter.ModifierType.SUPER_MASK | Clutter.ModifierType.SHIFT_MASK;
const REQUIRED = MODIFIERS & ~Clutter.ModifierType.SHIFT_MASK;

class Shortcuts {
    constructor(entries) {
        this.entries = entries;
        this.bindings = new Map();
        this.pending = 0;
        this.keyboard = Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this.signal = global.display.connect('accelerator-activated', (_display, action) => this.activate(action));
        try {
            this.prepare(entries.items);
        } catch (error) {
            this.destroy();
            throw error;
        }
    }

    prepare(items) {
        const added = [];
        try {
            for (const item of items) {
                if (this.bindings.has(item.key))
                    continue;
                const action = global.display.grab_accelerator(item.key, Meta.KeyBindingFlags.NONE);
                if (action === Meta.KeyBindingAction.NONE)
                    throw new Error('そのキーは使用中、または登録できません。別のキーを選んでください。');
                Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NORMAL);
                this.bindings.set(item.key, action);
                added.push(item.key);
            }
        } catch (error) {
            this.release(added);
            throw error;
        }
        return added;
    }

    change(items) {
        const added = this.prepare(items);
        try {
            this.entries.save(items);
        } catch (error) {
            this.release(added);
            throw error;
        }
        this.cancelPaste();
        this.release([...this.bindings.keys()].filter(key => !items.some(item => item.key === key)));
    }

    release(keys) {
        for (const key of keys) {
            const action = this.bindings.get(key);
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NONE);
            global.display.ungrab_accelerator(action);
            this.bindings.delete(key);
        }
    }

    activate(action) {
        if (Main.sessionMode.isLocked || this.pending)
            return;
        const item = this.entries.items.find(entry => this.bindings.get(entry.key) === action);
        const focus = global.display.focus_window;
        if (!item || !focus)
            return;
        const deadline = GLib.get_monotonic_time() + 3000000;
        this.pending = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 25, () => {
            if (Main.sessionMode.isLocked || global.display.focus_window !== focus ||
                GLib.get_monotonic_time() > deadline) {
                this.pending = 0;
                return GLib.SOURCE_REMOVE;
            }
            if (global.get_pointer()[2] & MODIFIERS)
                return GLib.SOURCE_CONTINUE;
            this.pending = 0;
            this.paste(item.text);
            return GLib.SOURCE_REMOVE;
        });
    }

    paste(text) {
        const clipboard = St.Clipboard.get_default();
        clipboard.set_text(St.ClipboardType.CLIPBOARD, text);
        clipboard.set_text(St.ClipboardType.PRIMARY, text);
        // GNOME TerminalはShift+InsertでPRIMARYを読むため、両方を同じ内容にする。
        for (const [key, state] of [
            [Clutter.KEY_Shift_L, Clutter.KeyState.PRESSED],
            [Clutter.KEY_Insert, Clutter.KeyState.PRESSED],
            [Clutter.KEY_Insert, Clutter.KeyState.RELEASED],
            [Clutter.KEY_Shift_L, Clutter.KeyState.RELEASED],
        ])
            this.keyboard.notify_keyval(GLib.get_monotonic_time(), key, state);
    }

    cancelPaste() {
        if (this.pending)
            GLib.Source.remove(this.pending);
        this.pending = 0;
    }

    destroy() {
        this.cancelPaste();
        this.release([...this.bindings.keys()]);
        if (this.signal)
            global.display.disconnect(this.signal);
        this.signal = 0;
        this.keyboard = null;
    }
}

class KeyCapture {
    constructor(invocation, done) {
        this.invocation = invocation;
        this.done = done;
        this.sender = invocation.get_sender();
        this.key = '';
        this.watch = Gio.bus_watch_name_on_connection(Gio.DBus.session, this.sender,
            Gio.BusNameWatcherFlags.NONE, null, () => this.finish(''));
        this.actor = new St.Widget({reactive: true, can_focus: true, opacity: 0, width: global.stage.width, height: global.stage.height});
        Main.uiGroup.add_child(this.actor);
        this.grab = Main.pushModal(this.actor, {actionMode: Shell.ActionMode.SYSTEM_MODAL});
        this.actor.connect('key-press-event', (_actor, event) => this.onEvent(event));
        this.actor.connect('key-release-event', (_actor, event) => this.onEvent(event));
        this.timeout = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 60, () => {
            this.timeout = 0;
            this.finish('');
            return GLib.SOURCE_REMOVE;
        });
    }

    onEvent(event) {
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
        Gio.bus_unwatch_name(this.watch);
        Main.popModal(this.grab);
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
        if (this.capture || Main.sessionMode.isLocked) {
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
