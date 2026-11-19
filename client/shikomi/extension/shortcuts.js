import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {SnippetSearch} from './search.js';

export const MODIFIERS = Clutter.ModifierType.CONTROL_MASK | Clutter.ModifierType.MOD1_MASK |
    Clutter.ModifierType.SUPER_MASK | Clutter.ModifierType.SHIFT_MASK;
const CANDIDATE_LETTERS = 'kmuiophjlnbv';

export class Shortcuts {
    constructor(entries) {
        this.entries = entries;
        this.bindings = new Map();
        this.pending = 0;
        this.keyboard = null;
        this.search = null;
        this.signal = global.display.connect('accelerator-activated', (_display, action) => this.activate(action));
        this.sessionSignal = Main.sessionMode.connect('updated', () => {
            if (Main.sessionMode.isLocked)
                this.cancelInteraction();
        });
        try {
            this.prepare(entries.items.map(item => item.key));
            // 共通キーを取得できなくても、既存の定型文は利用できる。
            if (entries.searchKey)
                this.grab(entries.searchKey);
        } catch (error) {
            this.destroy();
            throw error;
        }
    }

    get searchOpen() {
        return this.search !== null;
    }

    searchSetting() {
        const key = this.entries.searchKey;
        return {key, active: this.bindings.has(key)};
    }

    setSearchKey(key, previous) {
        if (previous !== this.entries.searchKey)
            throw new Error('別の操作で検索ショートカットが変わりました。開き直してください。');
        this.change(this.entries.items, key, true);
    }

    check(key, owner) {
        if (!owner || !['entry', 'search'].includes(owner.kind) ||
            (owner.kind === 'entry' && typeof owner.label !== 'string'))
            throw new Error('キーの割り当て先を確認できません。');

        const result = this.availability(key, owner);
        if (result.status === 'available')
            return {...result, candidates: []};

        const modifiers = key.slice(0, key.lastIndexOf('>') + 1);
        const candidates = [];
        for (const letter of CANDIDATE_LETTERS) {
            const candidate = modifiers + letter;
            if (candidate !== key && this.availability(candidate, owner).status === 'available')
                candidates.push(candidate);
            if (candidates.length === 2)
                break;
        }
        return {...result, candidates};
    }

    availability(key, owner) {
        const item = this.entries.items.find(entry => entry.key === key);
        if (item)
            return owner.kind === 'entry' && item.label === owner.label
                ? {status: 'available'} : {status: 'entry', label: item.label};
        if (key === this.entries.searchKey) {
            if (owner.kind !== 'search')
                return {status: 'search'};
            if (this.bindings.has(key))
                return {status: 'available'};
        }
        const action = this.probe(key);
        if (action === Meta.KeyBindingAction.NONE)
            return {status: 'unavailable'};
        global.display.ungrab_accelerator(action);
        return {status: 'available'};
    }

    probe(key) {
        return global.display.grab_accelerator(key, Meta.KeyBindingFlags.NONE);
    }

    grab(key) {
        const action = this.probe(key);
        if (action === Meta.KeyBindingAction.NONE)
            return false;
        Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NORMAL);
        this.bindings.set(key, action);
        return true;
    }

    prepare(keys) {
        const added = [];
        try {
            for (const key of keys) {
                if (this.bindings.has(key))
                    continue;
                if (!this.grab(key))
                    throw new Error('そのキーは使用中、または登録できません。別のキーを選んでください。');
                added.push(key);
            }
        } catch (error) {
            this.release(added);
            throw error;
        }
        return added;
    }

    change(items, searchKey = this.entries.searchKey, changingSearch = false) {
        this.entries.validateSearchKey(searchKey, items);
        const keys = items.map(item => item.key);
        if (searchKey && (changingSearch || this.bindings.has(searchKey)))
            keys.push(searchKey);
        const added = this.prepare(keys);
        try {
            this.entries.save(items, searchKey);
        } catch (error) {
            this.release(added);
            throw error;
        }
        this.cancelInteraction();
        this.release([...this.bindings.keys()].filter(key => !keys.includes(key)));
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
        if (Main.sessionMode.isLocked || this.pending || this.search)
            return;
        const focus = global.display.focus_window;
        if (!focus)
            return;
        if (this.bindings.get(this.entries.searchKey) === action) {
            this.whenReady(focus, () => this.openSearch(focus));
            return;
        }
        const item = this.entries.items.find(entry => this.bindings.get(entry.key) === action);
        if (item)
            this.whenReady(focus, () => this.paste(item.text));
    }

    openSearch(focus) {
        this.search = new SnippetSearch(this.entries.items, focus.get_monitor(), item => {
            this.search = null;
            if (item)
                this.whenReady(focus, () => this.paste(item.text));
        });
        if (!this.search.open()) {
            this.search.destroy();
            this.search = null;
        }
    }

    whenReady(focus, action) {
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
            action();
            return GLib.SOURCE_REMOVE;
        });
    }

    paste(text) {
        this.keyboard ??= Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
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

    cancelInteraction() {
        this.search?.close();
        this.search = null;
        if (this.pending)
            GLib.Source.remove(this.pending);
        this.pending = 0;
    }

    destroy() {
        this.cancelInteraction();
        this.release([...this.bindings.keys()]);
        global.display.disconnect(this.signal);
        Main.sessionMode.disconnect(this.sessionSignal);
        this.keyboard = null;
    }
}
