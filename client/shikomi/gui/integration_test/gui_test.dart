import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shikomi_gui/appearance.dart';
import 'package:shikomi_gui/entries/control.dart';
import 'package:shikomi_gui/entries/editor.dart';
import 'package:shikomi_gui/entries/entry.dart';
import 'package:shikomi_gui/main.dart';

const root = String.fromEnvironment('SHIKOMI_ROOT');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  final Object? data = jsonDecode(
    File('$root/tests/acceptance/gui-cases.json').readAsStringSync(),
  );
  if (data is! List<Object?>) throw const FormatException('ケースは配列にしてください');
  for (final value in data) {
    final scenario = GuiScenario.fromJson(value);
    testWidgets(scenario.id, (tester) async {
      final journey = GuiJourney(tester, scenario);
      try {
        await journey.prepare();
        await journey.run();
      } finally {
        await journey.close();
      }
    });
  }
}

final class GuiScenario {
  const GuiScenario(
    this.id,
    this.journey,
    this.entry,
    this.edited,
    this.keys,
    this.changedKeys,
  );
  final String id;
  final String journey;
  final Entry entry;
  final String edited;
  final List<int> keys;
  final List<int> changedKeys;
  factory GuiScenario.fromJson(Object? value) => switch (value) {
    {
      'id': final String id,
      'journey': final String journey,
      'label': final String label,
      'text': final String text,
      'edited': final String edited,
      'key': final String key,
      'keys': final List<Object?> keys,
      'changedKeys': final List<Object?> changedKeys,
    } =>
      GuiScenario(
        id,
        journey,
        Entry(label: label, text: text, key: key),
        edited,
        keys
            .map(
              (key) => switch (key) {
                final int key => key,
                _ => throw const FormatException(),
              },
            )
            .toList(),
        changedKeys
            .map(
              (key) => switch (key) {
                final int key => key,
                _ => throw const FormatException(),
              },
            )
            .toList(),
      ),
    _ => throw const FormatException('ケースの値が不正です'),
  };
}

final class GuiJourney {
  GuiJourney(this.tester, this.scenario);
  final WidgetTester tester;
  final GuiScenario scenario;
  final control = ShellControl();
  late final EntryEditor editor = EntryEditor(control);
  late final Directory temp;
  late final Appearance appearance;
  Process? recording;

  Future<void> desktop(
    String operation, [
    List<String> arguments = const [],
  ]) async {
    final result = await Process.run('/usr/bin/python3', [
      '$root/tests/support/gui_desktop.py',
      operation,
      ...arguments,
    ]);
    expect(result.exitCode, 0, reason: '${result.stdout}\n${result.stderr}');
  }

  Future<void> prepare() async {
    temp = await Directory.systemTemp.createTemp('shikomi-gui-');
    appearance = Appearance(file: File('${temp.path}/appearance'));
    for (final item in await control.list()) {
      await control.remove(await control.get(item.label));
    }
    if (scenario.journey != 'register') {
      await control.save(scenario.entry, null);
    }
    await tester.pumpWidget(ShikomiApp(editor: editor, appearance: appearance));
    await settle();
    await desktop('focus');
    if (scenario.journey != 'register') await tap(scenario.entry.label);
  }

  Future<void> settle() async {
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    for (var count = 0; editor.busy && count < 150; count++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    await tester.pumpAndSettle();
    expect(editor.busy, false, reason: '操作が完了しません');
  }

  Future<void> tap(String text) async {
    final finder = find.text(text).last;
    await tester.ensureVisible(finder);
    await tester.tap(finder);
    await settle();
  }

  Future<void> fill(String key, String text) async {
    final finder = find.byKey(ValueKey(key));
    await tester.ensureVisible(finder);
    await tester.tap(finder);
    await tester.pumpAndSettle();
    await desktop('input', [text]);
    await tester.pumpAndSettle();
  }

  Future<void> run() async {
    switch (scenario.journey) {
      case 'register':
        await register();
      case 'cancel':
        await cancel();
      case 'conflict':
        await conflict();
      case 'themes':
        await themes();
      case 'offline':
        await offline();
      default:
        throw StateError('未対応のシナリオ');
    }
  }

  Future<void> register() async {
    final media = Platform.environment['SHIKOMI_GUI_RECORDING'];
    if (media != null) {
      recording = await Process.start('/usr/bin/python3', [
        '$root/tests/support/gui_desktop.py',
        'record',
        media,
      ]);
      recording!.stdout.drain<void>();
      recording!.stderr.transform(utf8.decoder).listen(debugPrint);
      while (!File('$media/recording').existsSync()) {
        await tester.pump(const Duration(milliseconds: 100));
      }
    }
    await tap('定型文を追加');
    await fill('label', scenario.entry.label);
    await fill('body', scenario.entry.text);
    final capture = find.text('キーを設定');
    await tester.ensureVisible(capture);
    await tester.tap(capture);
    await tester.pump(const Duration(milliseconds: 100));
    await desktop('chord', [jsonEncode(scenario.keys)]);
    await settle();
    expect(find.text('Ctrl'), findsWidgets);
    await tap('登録する');
    expect(find.textContaining('保存しました。'), findsOneWidget);
    await desktop('paste', [jsonEncode(scenario.keys), scenario.entry.text]);
    await fill('body', scenario.edited);
    await desktop('chord', ['[65507,115]']);
    await settle();
    expect(editor.error, isNull);
    expect(
      (await control.get(scenario.entry.label)).text,
      scenario.edited,
      reason: '編集後の保存内容。入力=${editor.draft.text} 通知=${editor.notice}',
    );
    await desktop('paste', [jsonEncode(scenario.keys), scenario.edited]);
    await tester.ensureVisible(find.text('変更').last);
    await tester.tap(find.text('変更').last);
    await tester.pump(const Duration(milliseconds: 100));
    await desktop('chord', [jsonEncode(scenario.changedKeys)]);
    await settle();
    await tap('変更を保存');
    await desktop('paste', [jsonEncode(scenario.keys), '']);
    await desktop('paste', [jsonEncode(scenario.changedKeys), scenario.edited]);
    await tester.ensureVisible(find.byTooltip('定型文の操作'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('定型文の操作'));
    await settle();
    await tap('定型文を削除');
    await tap('削除する');
    expect(find.textContaining('削除しました。'), findsOneWidget);
    await desktop('paste', [jsonEncode(scenario.changedKeys), '']);
    if (media != null) {
      await File('$media/stop').writeAsString('');
      expect(await recording!.exitCode, 0);
    }
  }

  Future<void> cancel() async {
    await fill('body', scenario.edited);
    await desktop('close');
    await settle();
    expect(find.text('変更を破棄しますか？'), findsOneWidget);
    await tap('戻る');
    await tap('追加');
    expect(find.text('変更を破棄しますか？'), findsOneWidget);
    await tap('戻る');
    expect(find.text(scenario.edited), findsOneWidget);
    await tap('キャンセル');
    await tap('変更を破棄');
    expect(find.text(scenario.entry.text), findsOneWidget);
    expect((await control.get(scenario.entry.label)).text, scenario.entry.text);
    await tester.tap(find.text('変更').last);
    await tester.pump(const Duration(milliseconds: 100));
    await desktop('chord', ['[65307]']);
    await settle();
    expect(find.text('キーの読み取りを取り消しました。'), findsOneWidget);
  }

  Future<void> conflict() async {
    await control.save(
      scenario.entry.copyWith(text: scenario.edited),
      scenario.entry,
    );
    await fill('body', 'GUIからの変更');
    await tap('変更を保存');
    expect(find.textContaining('別の操作で登録が変わりました'), findsOneWidget);
    expect((await control.get(scenario.entry.label)).text, scenario.edited);
    await tap('保存内容を読み直す');
    await tap('読み直す');
    await tap('追加');
    await fill('label', scenario.entry.label);
    await fill('body', scenario.entry.text);
    await tester.tap(find.text('キーを設定'));
    await tester.pump(const Duration(milliseconds: 100));
    await desktop('chord', [jsonEncode(scenario.keys)]);
    await settle();
    await tap('登録する');
    expect(find.textContaining('重複'), findsOneWidget);
    expect((await control.get(scenario.entry.label)).text, scenario.edited);
  }

  Future<void> themes() async {
    for (final theme in ['ライト', 'ダーク']) {
      await tester.tap(find.byTooltip('外観'));
      await settle();
      await tap(theme);
      expect(tester.takeException(), isNull);
      final media = Platform.environment['SHIKOMI_GUI_SCREENSHOTS'];
      if (media != null) {
        await desktop('snapshot', [
          '$media/${theme == 'ライト' ? 'light' : 'dark'}.png',
        ]);
      }
    }
    final restored = Appearance(file: File('${temp.path}/appearance'));
    await restored.load();
    expect(restored.mode, ThemeMode.dark);
    await desktop('resize', ['600', '700']);
    await tester.pumpAndSettle();
    expect(find.byTooltip('定型文の一覧'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await fill('body', scenario.edited);
    await tap('変更を保存');
    await tester.tap(find.byTooltip('定型文の一覧'));
    await settle();
    expect(find.text('定型文'), findsOneWidget);
    await tap(scenario.entry.label);
    await desktop('resize', ['1120', '780']);
    await settle();
  }

  Future<void> offline() async {
    await fill('body', scenario.edited);
    await desktop('disable');
    await tester.tap(find.byTooltip('一覧を更新'));
    await settle();
    expect(find.text('ショートカット有効'), findsNothing);
    expect(find.text('再接続'), findsOneWidget);
    expect(find.text(scenario.edited), findsOneWidget);
    await desktop('enable');
    await tap('再接続');
    expect(find.text('ショートカット有効'), findsOneWidget);
    expect(find.text(scenario.edited), findsOneWidget);
  }

  Future<void> close() async {
    if (recording != null) recording!.kill();
    await desktop('enable');
    await tester.pumpWidget(const SizedBox.shrink());
    editor.dispose();
    for (final item in await control.list()) {
      await control.remove(await control.get(item.label));
    }
    await control.close();
    await temp.delete(recursive: true);
  }
}
