import 'package:flutter_test/flutter_test.dart';
import 'package:shikomi_gui/entries/control.dart';
import 'package:shikomi_gui/entries/editor.dart';
import 'package:shikomi_gui/entries/entry.dart';

// 外部通信の境界。正常契約はintegration_testで実際のGNOMEにも接続する。
final class InterruptedControl implements EntryControl {
  Entry? saved;
  bool loseReply = false;
  bool refuse = false;
  int writes = 0;
  @override
  Future<List<EntrySummary>> list() async => [
    if (saved case final Entry entry) EntrySummary(entry.label, entry.key),
  ];
  @override
  Future<Entry> get(String label) async =>
      saved ?? (throw const ControlFailure('指定したラベルはありません。'));
  @override
  Future<void> save(Entry entry, Entry? previous) async {
    writes++;
    if (refuse) throw const ControlFailure('保存先を作成できません。');
    saved = entry;
    if (loseReply) {
      throw const ControlFailure(
        '変更結果を確認できません。',
        uncertain: true,
        offline: true,
      );
    }
  }

  @override
  Future<void> remove(Entry previous) async {
    saved = null;
  }

  @override
  Future<String> capture() async => '';
  @override
  Future<void> cancelCapture() async {}
  @override
  Future<void> close() async {}
}

void main() {
  const input = Entry(
    label: 'あいさつ',
    text: 'よろしくお願いいたします。',
    key: '<Control><Alt>j',
  );
  group('通信結果を確認できない場合', () {
    test('保存済みでも返事を失ったら再送せず、読み直して確定する', () async {
      final connection = InterruptedControl()..loseReply = true;
      final editor = EntryEditor(connection);
      addTearDown(editor.dispose);
      await editor.refresh();
      editor.create();
      editor.update(input);
      await editor.save();
      expect(editor.notice, isNull);
      expect(editor.draft.sameAs(input), isTrue);
      expect(editor.uncertain, isTrue);
      await editor.refresh();
      await editor.save();
      expect(connection.writes, 1);
      await editor.reload();
      expect(editor.uncertain, isFalse);
      expect(editor.dirty, isFalse);
      expect(editor.draft.sameAs(input), isTrue);
    });
    test('明示した保存拒否では入力を保ち、修正して再試行できる', () async {
      final connection = InterruptedControl()..refuse = true;
      final editor = EntryEditor(connection);
      addTearDown(editor.dispose);
      await editor.refresh();
      editor.create();
      editor.update(input);
      await editor.save();
      expect(editor.error, '保存先を作成できません。');
      expect(editor.draft.sameAs(input), isTrue);
      expect(editor.canSave, isTrue);
      expect(connection.saved, isNull);
      connection.refuse = false;
      await editor.save();
      expect(editor.dirty, isFalse);
    });
  });
}
