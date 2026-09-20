import 'package:flutter/material.dart';

class KeyCaps extends StatelessWidget {
  const KeyCaps({super.key, required this.value, this.small = false});
  final String value;
  final bool small;

  @override
  Widget build(BuildContext context) {
    final parts = RegExp(r'<([^>]+)>|([^<>]+)')
        .allMatches(value)
        .map(
          (match) => switch (match.group(1) ?? match.group(2)!) {
            'Control' => 'Ctrl',
            'Primary' => 'Ctrl',
            final String key => key.length == 1 ? key.toUpperCase() : key,
          },
        )
        .toList();
    return Semantics(
      label: parts.join(' + '),
      child: ExcludeSemantics(
        child: Wrap(
          spacing: small ? 4 : 8,
          runSpacing: 5,
          children: parts
              .map(
                (part) => Container(
                  padding: EdgeInsets.symmetric(
                    horizontal: small ? 7 : 13,
                    vertical: small ? 3 : 8,
                  ),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.surface,
                    border: Border.all(
                      color: Theme.of(context).colorScheme.outline,
                    ),
                    borderRadius: BorderRadius.circular(small ? 4 : 6),
                  ),
                  child: Text(
                    part,
                    style: TextStyle(fontSize: small ? 11 : 16),
                  ),
                ),
              )
              .toList(),
        ),
      ),
    );
  }
}
