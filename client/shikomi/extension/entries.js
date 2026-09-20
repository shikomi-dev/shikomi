import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export class Entries {
    constructor(directory = GLib.build_filenamev([GLib.get_user_data_dir(), 'shikomi'])) {
        this.directory = directory;
        this.file = Gio.File.new_for_path(GLib.build_filenamev([directory, 'entries.json']));
        this.items = this.load();
    }

    load() {
        try {
            const [, bytes] = this.file.load_contents(null);
            const data = JSON.parse(new TextDecoder().decode(bytes));
            if (data.version !== 1 || !Array.isArray(data.entries))
                throw new Error('保存形式を読み取れません。保存ファイルは変更していません。');
            this.validateAll(data.entries);
            return data.entries;
        } catch (error) {
            if (error.matches?.(Gio.io_error_quark(), Gio.IOErrorEnum.NOT_FOUND))
                return [];
            throw error;
        }
    }

    validateAll(items) {
        const labels = new Set();
        const keys = new Set();
        for (const item of items) {
            this.validate(item);
            if (labels.has(item.label) || keys.has(item.key))
                throw new Error('ラベルまたはキーが重複しています。');
            labels.add(item.label);
            keys.add(item.key);
        }
    }

    validate(item) {
        if (!item || typeof item.label !== 'string' || !item.label.trim() ||
            item.label !== item.label.trim() || /[\x00-\x1f\x7f]/.test(item.label))
            throw new Error('ラベルは前後の空白や制御文字を含まない文字列にしてください。');
        if (typeof item.text !== 'string' || !item.text.length || item.text.includes('\0'))
            throw new Error('文字列は空またはNULを含む値にできません。');
        if (typeof item.key !== 'string' || !/<(Control|Alt|Super)>/.test(item.key) ||
            item.key.length > 200 || /[\x00-\x1f\x7f]/.test(item.key))
            throw new Error('Ctrl・Alt・Superのいずれかを含むキーにしてください。');
    }

    find(label) {
        const item = this.items.find(entry => entry.label === label);
        if (!item)
            throw new Error('指定したラベルはありません。');
        return item;
    }

    changed(operation, request) {
        const next = this.items.slice();
        if (operation === 'add') {
            next.push(request.entry);
        } else {
            const current = this.find(request.label);
            if ((operation === 'edit' || request.previous !== undefined) &&
                (!request.previous || ['label', 'text', 'key'].some(field => current[field] !== request.previous[field])))
                throw new Error('別の操作で登録が変わりました。もう一度編集してください。');
            const index = next.indexOf(current);
            if (operation === 'edit')
                next[index] = request.entry;
            else if (operation === 'remove')
                next.splice(index, 1);
            else
                throw new Error('未対応の操作です。');
        }
        this.validateAll(next);
        return next;
    }

    save(items) {
        if (GLib.mkdir_with_parents(this.directory, 0o700) !== 0)
            throw new Error('保存先を作成できません。');
        const bytes = new TextEncoder().encode(JSON.stringify({version: 1, entries: items}));
        this.file.replace_contents(bytes, null, false,
            Gio.FileCreateFlags.PRIVATE | Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        this.items = items;
    }
}
