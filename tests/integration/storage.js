import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Entries} from '../../client/shikomi/extension/entries.js';

class StorageChecks {
    run() {
        const directory = GLib.dir_make_tmp('shikomi-storage-XXXXXX');
        const entries = new Entries(directory);
        const item = {label: '日本語', text: '改行\n保存', key: '<Control><Alt>j'};
        entries.save(entries.changed('add', {entry: item}));
        if (new Entries(directory).find('日本語').text !== item.text)
            throw new Error('保存と読み取りが一致しません');
        const mode = entries.file.query_info('unix::mode', Gio.FileQueryInfoFlags.NONE, null)
            .get_attribute_uint32('unix::mode') & 0o777;
        if (mode !== 0o600)
            throw new Error(`ファイル権限が広すぎます: ${mode}`);
        entries.file.replace_contents(new TextEncoder().encode('{"version":99,"entries":[]}'),
            null, false, Gio.FileCreateFlags.PRIVATE, null);
        let refused = false;
        try { new Entries(directory); } catch { refused = true; }
        if (!refused)
            throw new Error('未知の保存形式を受理しました');
        entries.file.delete(null);
        Gio.File.new_for_path(directory).delete(null);
        print('保存・再読込・所有者限定権限・未知版拒否: OK');
    }
}
new StorageChecks().run();
