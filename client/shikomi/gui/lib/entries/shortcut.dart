enum ShortcutAvailability { available, entry, search, unavailable }

final class ShortcutCheck {
  const ShortcutCheck(this.availability, this.label, this.candidates);
  final ShortcutAvailability availability;
  final String? label;
  final List<String> candidates;

  bool get blocked => availability != ShortcutAvailability.available;

  String get message => switch (availability) {
    ShortcutAvailability.available => 'GNOMEで登録可能',
    ShortcutAvailability.entry => '「$label」で使用中',
    ShortcutAvailability.search => '検索ショートカットで使用中',
    ShortcutAvailability.unavailable => 'この組み合わせは登録できません',
  };

  factory ShortcutCheck.fromJson(Object? json) {
    if (json case {
      'status': final String status,
      'candidates': final List<Object?> candidates,
    }) {
      final label = json is Map<String, Object?> ? json['label'] : null;
      final availability = switch (status) {
        'available' => ShortcutAvailability.available,
        'entry' when label is String && label.isNotEmpty =>
          ShortcutAvailability.entry,
        'search' => ShortcutAvailability.search,
        'unavailable' => ShortcutAvailability.unavailable,
        _ => throw const FormatException('キー確認の応答が不正です。'),
      };
      return ShortcutCheck(
        availability,
        label is String ? label : null,
        candidates.map((key) {
          if (key is! String || key.isEmpty) {
            throw const FormatException('キー候補の応答が不正です。');
          }
          return key;
        }).toList(),
      );
    }
    throw const FormatException('キー確認の応答が不正です。');
  }
}

final class SearchShortcut {
  const SearchShortcut(this.key, {required this.active});
  final String key;
  final bool active;

  factory SearchShortcut.fromJson(Object? json) => switch (json) {
    {'key': final String key, 'active': final bool active}
        when key.isNotEmpty || !active =>
      SearchShortcut(key, active: active),
    _ => throw const FormatException('検索ショートカットの応答が不正です。'),
  };
}
