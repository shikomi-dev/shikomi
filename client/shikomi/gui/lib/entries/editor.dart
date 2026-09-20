import 'package:flutter/foundation.dart';

import 'control.dart';
import 'entry.dart';

final class EntryEditor extends ChangeNotifier {
  EntryEditor(this.control);
  final EntryControl control;
  List<EntrySummary> entries = [];
  Entry? original;
  Entry draft = const Entry.empty();
  bool opened = false;
  bool busy = false;
  bool capturing = false;
  bool connected = false;
  bool uncertain = false;
  String? error;
  String? notice;
  int revision = 0;
  bool _disposed = false;

  bool get dirty => opened && !draft.sameAs(original ?? const Entry.empty());
  bool get canSave => opened && dirty && !busy && !uncertain && connected;

  void update(Entry value) {
    draft = value;
    notice = null;
    notifyListeners();
  }

  void _replace(Entry? entry, {bool open = true}) {
    original = entry;
    draft = entry ?? const Entry.empty();
    opened = open;
    revision++;
  }

  void create() {
    if (busy || uncertain) return;
    _replace(null);
    error = null;
    notice = null;
    notifyListeners();
  }

  Future<void> _perform(Future<void> Function() action) async {
    if (busy || _disposed) return;
    busy = true;
    error = null;
    notice = null;
    notifyListeners();
    try {
      await action();
    } on ControlFailure catch (failure) {
      error = failure.message;
      if (failure.offline) connected = false;
      uncertain |= failure.uncertain;
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  Future<void> refresh({bool background = false}) async {
    if (busy || _disposed) return;
    if (!background) {
      await _perform(() async {
        entries = await control.list();
        connected = true;
      });
      return;
    }
    // 定期確認は編集中の値や操作中のメッセージを置き換えない。
    try {
      final latest = await control.list();
      if (busy || _disposed) return;
      entries = latest;
      connected = true;
    } on ControlFailure {
      if (busy || _disposed) return;
      connected = false;
    }
    if (!_disposed) notifyListeners();
  }

  Future<void> select(String label) async {
    if (uncertain) return;
    await _perform(() async {
      final entry = await control.get(label);
      connected = true;
      _replace(entry);
    });
  }

  Future<void> reload() => _perform(() async {
    final latest = await control.list();
    final label = original?.label ?? draft.label;
    final exists = latest.any((entry) => entry.label == label);
    final saved = exists ? await control.get(label) : null;
    entries = latest;
    connected = true;
    uncertain = false;
    _replace(saved, open: exists);
    notice = exists ? '保存済みの内容を読み直しました。' : 'このラベルの登録はありません。';
  });

  void cancel() {
    if (busy || uncertain) return;
    _replace(original, open: original != null);
    error = null;
    notice = null;
    notifyListeners();
  }

  Future<void> save() async {
    if (!canSave) return;
    if (draft.problem case final String problem) {
      error = problem;
      notifyListeners();
      return;
    }
    await _perform(() async {
      await control.save(draft, original);
      _replace(draft);
      notice = '保存しました。ほかのアプリでショートカットを使えます。';
      entries = await control.list();
      connected = true;
    });
  }

  Future<void> remove() async {
    final saved = original;
    if (saved == null || uncertain || !connected) return;
    await _perform(() async {
      await control.remove(saved);
      _replace(null, open: false);
      notice = '削除しました。ショートカットも解除しました。';
      entries = await control.list();
      connected = true;
    });
  }

  Future<void> capture() async {
    if (busy || uncertain || !connected) return;
    capturing = true;
    await _perform(() async {
      final key = await control.capture();
      if (key.isNotEmpty) draft = draft.copyWith(key: key);
      notice = key.isEmpty ? 'キーの読み取りを取り消しました。' : null;
    });
    capturing = false;
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
