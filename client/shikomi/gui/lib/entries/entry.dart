final class EntrySummary {
  const EntrySummary(this.label, this.key);

  final String label;
  final String key;

  factory EntrySummary.fromJson(Object? value) => switch (value) {
    {'label': final String label, 'key': final String key}
        when label.isNotEmpty && key.isNotEmpty =>
      EntrySummary(label, key),
    _ => throw const FormatException('一覧の応答を読み取れません。'),
  };
}

final class Entry {
  const Entry({required this.label, required this.text, required this.key});
  const Entry.empty() : label = '', text = '', key = '';

  final String label;
  final String text;
  final String key;

  factory Entry.fromJson(Object? value) => switch (value) {
    {
      'label': final String label,
      'text': final String text,
      'key': final String key,
    }
        when label.isNotEmpty && text.isNotEmpty && key.isNotEmpty =>
      Entry(label: label, text: text, key: key),
    _ => throw const FormatException('定型文の応答を読み取れません。'),
  };

  Entry copyWith({String? label, String? text, String? key}) => Entry(
    label: label ?? this.label,
    text: text ?? this.text,
    key: key ?? this.key,
  );

  Map<String, String> toJson() => {'label': label, 'text': text, 'key': key};

  bool sameAs(Entry other) =>
      label == other.label && text == other.text && key == other.key;

  String? get problem {
    if (label.trim().isEmpty ||
        label != label.trim() ||
        RegExp(r'[\x00-\x1f\x7f]').hasMatch(label)) {
      return 'ラベルは前後の空白や改行を含めずに入力してください。';
    }
    if (text.isEmpty || text.contains('\u0000')) {
      return '貼り付ける本文を入力してください。NUL文字は使えません。';
    }
    if (key.isEmpty) return '使いたいショートカットを設定してください。';
    return null;
  }
}
