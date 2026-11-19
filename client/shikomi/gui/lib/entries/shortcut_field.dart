import 'package:flutter/material.dart';

import 'key_caps.dart';
import 'shortcut.dart';

class ShortcutField extends StatelessWidget {
  const ShortcutField({
    super.key,
    required this.value,
    required this.capturing,
    required this.check,
    required this.onCapture,
    required this.onChoose,
  });

  final String value;
  final bool capturing;
  final ShortcutCheck? check;
  final VoidCallback? onCapture;
  final ValueChanged<String>? onChoose;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final result = check;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            border: Border.all(color: theme.colorScheme.outline),
            borderRadius: BorderRadius.circular(9),
          ),
          child: Row(
            children: [
              Expanded(
                child: capturing
                    ? const Text('使いたいキーを押して、離してください…')
                    : value.isEmpty
                    ? const Text('まだ設定されていません')
                    : KeyCaps(value: value),
              ),
              const SizedBox(width: 10),
              OutlinedButton(
                onPressed: onCapture,
                child: Text(value.isEmpty ? 'キーを設定' : '変更'),
              ),
            ],
          ),
        ),
        const SizedBox(height: 10),
        if (capturing)
          Text(
            'Ctrl・Alt・Superのいずれかを含めてください。Escで取消し、1分で終了します。',
            style: theme.textTheme.bodySmall,
          )
        else if (result != null)
          _status(context, result)
        else
          Text('使いたいキーを実際に押してください。', style: theme.textTheme.bodySmall),
      ],
    );
  }

  Widget _status(BuildContext context, ShortcutCheck result) {
    final theme = Theme.of(context);
    final color = result.blocked
        ? theme.colorScheme.error
        : theme.colorScheme.onSurface;
    return Semantics(
      liveRegion: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                result.blocked
                    ? Icons.warning_rounded
                    : Icons.check_circle_outline,
                color: color,
                size: 17,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(result.message, style: TextStyle(color: color)),
              ),
            ],
          ),
          if (result.candidates.isNotEmpty) ...[
            const SizedBox(height: 12),
            const Text(
              '空いている候補',
              style: TextStyle(fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 10,
              runSpacing: 8,
              children: [
                for (final key in result.candidates)
                  OutlinedButton(
                    style: OutlinedButton.styleFrom(
                      side: BorderSide(color: theme.colorScheme.primary),
                    ),
                    onPressed: onChoose == null ? null : () => onChoose!(key),
                    child: Text(KeyCaps.parts(key).join(' + ')),
                  ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}
