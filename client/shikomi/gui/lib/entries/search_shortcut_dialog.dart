import 'package:flutter/material.dart';

import 'control.dart';
import 'shortcut.dart';
import 'shortcut_field.dart';

class SearchShortcutDialog extends StatefulWidget {
  const SearchShortcutDialog({super.key, required this.control});
  final EntryControl control;

  @override
  State<SearchShortcutDialog> createState() => _SearchShortcutDialogState();
}

class _SearchShortcutDialogState extends State<SearchShortcutDialog> {
  SearchShortcut? setting;
  String keyValue = '';
  ShortcutCheck? check;
  String? error;
  bool busy = false;
  bool capturing = false;

  bool get canSave {
    final saved = setting;
    return saved != null &&
        !busy &&
        !(check?.blocked ?? false) &&
        (saved.key != keyValue || (!saved.active && keyValue.isNotEmpty));
  }

  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> run(Future<void> Function() action) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await action();
    } on ControlFailure catch (failure) {
      error = failure.message;
      if (failure.uncertain || failure.offline) setting = null;
    } finally {
      busy = false;
      capturing = false;
      if (mounted) setState(() {});
    }
  }

  Future<void> load() => run(() async {
    final saved = await widget.control.getSearchShortcut();
    setting = saved;
    keyValue = saved.key;
    check = null;
  });

  Future<void> capture() => run(() async {
    setState(() => capturing = true);
    final selected = await widget.control.capture();
    if (selected.isNotEmpty) await inspect(selected);
  });

  Future<void> inspect(String key) async {
    keyValue = key;
    check = null;
    check = await widget.control.checkShortcut(key);
  }

  Future<void> save() => run(() async {
    final previous = setting;
    if (previous == null) return;
    await widget.control.setSearchShortcut(keyValue, previous.key);
    if (mounted) Navigator.of(context).pop(true);
  });

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !busy,
    child: AlertDialog(
      title: const Text('検索ショートカット'),
      scrollable: true,
      content: SizedBox(
        width: 448,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text('入力中のアプリから定型文の検索を開くキーを設定します。'),
            const SizedBox(height: 16),
            if (setting != null) ...[
              if (setting!.key.isNotEmpty && !setting!.active && check == null)
                const Padding(
                  padding: EdgeInsets.only(bottom: 10),
                  child: Text('保存済みのキーは現在利用できません。'),
                ),
              ShortcutField(
                value: keyValue,
                capturing: capturing,
                check: check,
                onCapture: busy ? null : capture,
                onChoose: busy ? null : (key) => run(() => inspect(key)),
              ),
              if (keyValue.isNotEmpty)
                TextButton(
                  onPressed: busy
                      ? null
                      : () => setState(() {
                          keyValue = '';
                          check = null;
                          error = null;
                        }),
                  child: const Text('割り当てを解除'),
                ),
            ],
            if (setting == null && busy) const LinearProgressIndicator(),
            if (error != null) ...[
              const SizedBox(height: 10),
              Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
              if (setting == null)
                TextButton(
                  onPressed: busy ? null : load,
                  child: const Text('読み直す'),
                ),
            ],
          ],
        ),
      ),
      actions: [
        OutlinedButton(
          onPressed: busy ? null : () => Navigator.of(context).pop(),
          child: const Text('キャンセル'),
        ),
        FilledButton(
          onPressed: canSave ? save : null,
          child: const Text('保存する'),
        ),
      ],
    ),
  );
}
