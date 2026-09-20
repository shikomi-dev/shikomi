import 'dart:io';

import 'package:flutter/material.dart';

final class Appearance extends ChangeNotifier {
  Appearance({File? file})
    : _file =
          file ??
          File(
            '${Platform.environment['XDG_CONFIG_HOME'] ?? '${Platform.environment['HOME']}/.config'}/shikomi/appearance',
          );
  final File _file;
  ThemeMode mode = ThemeMode.system;

  Future<void> load() async {
    try {
      final stored = await _file.readAsString();
      mode = ThemeMode.values.firstWhere(
        (value) => value.name == stored,
        orElse: () => ThemeMode.system,
      );
    } on FileSystemException {
      // 初回や設定を読めない場合はデスクトップの外観に従う。
    }
    notifyListeners();
  }

  Future<bool> select(ThemeMode value) async {
    mode = value;
    notifyListeners();
    try {
      await _file.parent.create(recursive: true);
      await _file.writeAsString(value.name, flush: true);
      return true;
    } on FileSystemException {
      return false;
    }
  }

  static ThemeData theme(Brightness brightness) {
    final dark = brightness == Brightness.dark;
    final background = Color(dark ? 0xff27282b : 0xfffdfcfb);
    final foreground = Color(dark ? 0xfff1f1f0 : 0xff252629);
    final border = Color(dark ? 0xff62646a : 0xffbfc1c6);
    final colors = ColorScheme.fromSeed(
      seedColor: const Color(0xffc94a08),
      brightness: brightness,
      surface: background,
      onSurface: foreground,
      primary: Color(dark ? 0xffffa565 : 0xffaa3900),
      primaryContainer: Color(dark ? 0xff4a3020 : 0xffffebde),
      onPrimaryContainer: foreground,
      outline: border,
    );
    final outline = OutlineInputBorder(
      borderRadius: BorderRadius.circular(9),
      borderSide: BorderSide(color: border),
    );
    return ThemeData(
      colorScheme: colors,
      useMaterial3: true,
      brightness: brightness,
      fontFamily: 'Noto Sans CJK JP',
      scaffoldBackgroundColor: background,
      textTheme: Typography.material2021().black.apply(
        bodyColor: foreground,
        displayColor: foreground,
        fontFamily: 'Noto Sans CJK JP',
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: Color(dark ? 0xff222326 : 0xffffffff),
        border: outline,
        enabledBorder: outline,
        focusedBorder: outline.copyWith(
          borderSide: BorderSide(color: colors.primary, width: 2),
        ),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: 16,
          vertical: 15,
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          backgroundColor: const Color(0xffb94000),
          foregroundColor: Colors.white,
          minimumSize: const Size(48, 44),
          padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 14),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(9)),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          foregroundColor: foreground,
          minimumSize: const Size(48, 44),
          side: BorderSide(color: border),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(9)),
        ),
      ),
      dividerTheme: DividerThemeData(
        color: border.withValues(alpha: .4),
        thickness: 1,
        space: 1,
      ),
      tooltipTheme: const TooltipThemeData(
        waitDuration: Duration(milliseconds: 400),
      ),
    );
  }
}
