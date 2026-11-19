import Clutter from 'gi://Clutter';
import Pango from 'gi://Pango';
import St from 'gi://St';
import * as Util from 'resource:///org/gnome/shell/misc/util.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {ModalDialog} from 'resource:///org/gnome/shell/ui/modalDialog.js';

export class SnippetSearch {
    constructor(items, monitorIndex, finished) {
        this.items = items;
        this.finished = finished;
        this.selected = 0;
        this.confirmed = null;
        this.dialog = new ModalDialog({
            styleClass: 'shikomi-search',
            shouldFadeIn: false,
            shouldFadeOut: false,
        });
        const monitor = Main.layoutManager.monitors[monitorIndex] ?? Main.layoutManager.currentMonitor;
        this.monitorIndex = monitor.index;
        this.dialog.dialogLayout.translation_y = -Math.round(monitor.height * 0.09);
        const content = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'shikomi-search-content',
            width: Math.min(552, monitor.width - 80),
        });
        this.dialog.contentLayout.add_child(content);

        content.add_child(new St.Label({text: '定型文を挿入', style_class: 'shikomi-search-title'}));
        this.input = new St.Entry({
            hint_text: 'ラベルで検索',
            accessible_name: '定型文を検索',
            can_focus: true,
            style_class: 'shikomi-search-entry',
        });
        this.input.set_primary_icon(new St.Icon({icon_name: 'edit-find-symbolic', icon_size: 18}));
        content.add_child(this.input);

        this.scroll = new St.ScrollView({
            style_class: 'shikomi-search-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
        });
        this.rows = new St.BoxLayout({orientation: Clutter.Orientation.VERTICAL, x_expand: true});
        this.scroll.add_child(this.rows);
        content.add_child(this.scroll);
        content.add_child(this.footer());

        const text = this.input.clutter_text;
        this.dialog.setInitialKeyFocus(text);
        text.connect('text-changed', () => this.refresh());
        text.connect('key-press-event', (_actor, event) => this.onPress(event));
        // ClutterTextの確定を使い、IMEが消費するEnterを拾わない。
        text.connect('activate', () => {
            this.confirmed = this.matches[this.selected] ?? null;
        });
        text.connect('key-release-event', (_actor, event) => this.onRelease(event));
        this.dialog.connect('closed', () => this.finished(this.chosen));
        this.refresh();
    }

    open() {
        if (!this.dialog.open())
            return false;
        // 標準のモーダルはポインター側に開く。貼り付け先のモニターへ配置する。
        this.dialog._monitorConstraint.index = this.monitorIndex;
        return true;
    }

    close() {
        this.dialog.close();
    }

    destroy() {
        this.dialog.destroy();
    }

    footer() {
        const footer = new St.BoxLayout({style_class: 'shikomi-search-footer'});
        for (const [key, description] of [['↑ ↓', '選択'], ['Enter', '貼り付け'], ['Esc', '閉じる']]) {
            const hint = new St.BoxLayout({style_class: 'shikomi-search-hint', x_expand: true});
            hint.add_child(new St.Label({text: key, style_class: 'shikomi-search-keycap'}));
            hint.add_child(new St.Label({text: description, y_align: Clutter.ActorAlign.CENTER}));
            footer.add_child(hint);
        }
        return footer;
    }

    refresh() {
        this.selected = 0;
        this.confirmed = null;
        const query = this.input.get_text().trim().toLowerCase();
        this.matches = this.items.filter(item => item.label.toLowerCase().includes(query));
        this.rows.destroy_all_children();
        if (this.matches.length === 0) {
            this.rows.add_child(new St.Label({
                text: '見つかりませんでした',
                style_class: 'shikomi-search-empty',
            }));
            return;
        }
        for (const item of this.matches)
            this.rows.add_child(this.row(item));
        this.select(0);
    }

    row(item) {
        const row = new St.Button({
            style_class: 'shikomi-search-result',
            accessible_name: item.label,
            x_expand: true,
        });
        const text = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
            x_align: Clutter.ActorAlign.FILL,
        });
        const label = new St.Label({text: item.label, style_class: 'shikomi-search-result-label'});
        const preview = new St.Label({
            text: item.text.split(/\r?\n/, 1)[0],
            style_class: 'shikomi-search-preview',
        });
        for (const child of [label, preview]) {
            child.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            text.add_child(child);
        }
        row.set_child(text);
        row.connect('clicked', () => this.choose(item));
        return row;
    }

    select(index) {
        this.rows.get_child_at_index(this.selected)?.remove_style_class_name('shikomi-search-selected');
        this.selected = index;
        const row = this.rows.get_child_at_index(index);
        row.add_style_class_name('shikomi-search-selected');
        if (row.mapped)
            Util.ensureActorVisibleInScrollView(this.scroll, row);
    }

    onPress(event) {
        const key = event.get_key_symbol();
        if (key === Clutter.KEY_Escape) {
            this.close();
            return Clutter.EVENT_STOP;
        }
        if (key !== Clutter.KEY_Up && key !== Clutter.KEY_Down)
            return Clutter.EVENT_PROPAGATE;
        if (this.matches.length) {
            const step = key === Clutter.KEY_Down ? 1 : -1;
            this.select(Math.max(0, Math.min(this.matches.length - 1, this.selected + step)));
        }
        return Clutter.EVENT_STOP;
    }

    onRelease(event) {
        const key = event.get_key_symbol();
        if (this.confirmed && (key === Clutter.KEY_Return || key === Clutter.KEY_KP_Enter)) {
            this.choose(this.confirmed);
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }

    choose(item) {
        this.chosen = item;
        this.close();
    }
}
