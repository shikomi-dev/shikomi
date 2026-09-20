import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'appearance.dart';
import 'entries/control.dart';
import 'entries/editor.dart';
import 'entries/screen.dart';

void main() => ShikomiApplication().run();

final class ShikomiApplication {
  Future<void> run() async {
    WidgetsFlutterBinding.ensureInitialized();
    final appearance = Appearance();
    await appearance.load();
    runApp(
      ShikomiApp(editor: EntryEditor(ShellControl()), appearance: appearance),
    );
  }
}

class ShikomiApp extends StatelessWidget {
  const ShikomiApp({super.key, required this.editor, required this.appearance});
  final EntryEditor editor;
  final Appearance appearance;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: appearance,
    builder: (context, child) => MaterialApp(
      title: 'shikomi',
      debugShowCheckedModeBanner: false,
      locale: const Locale('ja'),
      supportedLocales: const [Locale('ja')],
      localizationsDelegates: GlobalMaterialLocalizations.delegates,
      theme: Appearance.theme(Brightness.light),
      darkTheme: Appearance.theme(Brightness.dark),
      themeMode: appearance.mode,
      home: EntryScreen(editor: editor, appearance: appearance),
    ),
  );
}
