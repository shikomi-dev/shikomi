import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shikomi_gui/appearance.dart';
import 'package:shikomi_gui/entries/editor.dart';
import 'package:shikomi_gui/entries/entry.dart';
import 'package:shikomi_gui/main.dart';

import 'editor_test.dart' show InterruptedControl;

void main() {
  testWidgets('返事を失ったあと一覧を更新しても、確認して再開する入口を失わない', (tester) async {
    final temp = Directory.systemTemp.createTempSync('shikomi-recovery-');
    final control = InterruptedControl()..loseReply = true;
    final editor = EntryEditor(control);
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('io.github.shikomi/window'),
      (call) async => null,
    );
    await tester.pumpWidget(
      ShikomiApp(
        editor: editor,
        appearance: Appearance(file: File('${temp.path}/theme')),
      ),
    );
    await tester.pumpAndSettle();
    editor.create();
    editor.update(
      const Entry(label: 'あいさつ', text: '本文', key: '<Control><Alt>j'),
    );
    await editor.save();
    await tester.pumpAndSettle();
    expect(find.text('変更結果が未確認'), findsOneWidget);
    await tester.tap(find.byTooltip('一覧を更新'));
    await tester.pumpAndSettle();
    expect(find.text('保存内容を読み直す'), findsOneWidget);
    final save = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, '登録する'),
    );
    expect(save.onPressed, isNull);
    await tester.tap(find.text('保存内容を読み直す'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('読み直す'));
    await tester.pumpAndSettle();
    expect(find.text('変更結果が未確認'), findsNothing);
    expect(control.writes, 1);
    await tester.pumpWidget(const SizedBox.shrink());
    editor.dispose();
    temp.deleteSync(recursive: true);
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('io.github.shikomi/window'),
      null,
    );
  });
}
