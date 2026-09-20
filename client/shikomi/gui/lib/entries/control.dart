import 'dart:async';
import 'dart:convert';

import 'package:dbus/dbus.dart';

import 'entry.dart';

final class ControlFailure implements Exception {
  const ControlFailure(
    this.message, {
    this.uncertain = false,
    this.offline = false,
  });
  final String message;
  final bool uncertain;
  final bool offline;
  @override
  String toString() => message;
}

abstract interface class EntryControl {
  Future<List<EntrySummary>> list();
  Future<Entry> get(String label);
  Future<void> save(Entry entry, Entry? previous);
  Future<void> remove(Entry previous);
  Future<String> capture();
  Future<void> cancelCapture();
  Future<void> close();
}

final class ShellControl implements EntryControl {
  ShellControl({DBusClient? client}) : _client = client ?? DBusClient.session();
  final DBusClient _client;

  Future<DBusMethodSuccessResponse> _call(
    String method,
    List<DBusValue> values,
    String signature, {
    bool mutation = false,
  }) async {
    try {
      return await _client
          .callMethod(
            destination: 'org.gnome.Shell',
            path: DBusObjectPath('/io/github/shikomi/Control'),
            interface: 'io.github.shikomi.Control',
            name: method,
            values: values,
            replySignature: DBusSignature(signature),
          )
          .timeout(Duration(seconds: method == 'Capture' ? 65 : 8));
    } on Exception catch (error) {
      if (error is DBusMethodResponseException &&
          error.errorName == 'io.github.shikomi.Busy') {
        throw const ControlFailure('別のキー入力待ち、または画面ロック中です。解除してからもう一度お試しください。');
      }
      throw ControlFailure(
        mutation
            ? '変更結果を確認できません。再接続して保存済みの内容を確認してください。'
            : 'GNOME拡張に接続できません。拡張を有効にして、再接続してください。',
        uncertain: mutation,
        offline: true,
      );
    }
  }

  Future<Object?> _request(
    String operation,
    Map<String, Object?> fields, {
    bool mutation = false,
  }) async {
    final response = await _call(
      'Request',
      [
        DBusString(jsonEncode({'operation': operation, ...fields})),
      ],
      's',
      mutation: mutation,
    );
    try {
      final Object? decoded = jsonDecode(response.values.single.asString());
      switch (decoded) {
        case {'ok': true, 'result': final Object? result}:
          return result;
        case {'ok': false, 'error': final String message}:
          throw ControlFailure(message);
        default:
          throw const FormatException();
      }
    } on FormatException {
      throw ControlFailure(
        '応答を読み取れません。再接続して内容を確認してください。',
        uncertain: mutation,
        offline: true,
      );
    }
  }

  @override
  Future<List<EntrySummary>> list() async {
    final result = await _request('list', {});
    if (result is! List<Object?>) {
      throw const ControlFailure('一覧の応答が不正です。', offline: true);
    }
    try {
      return result.map(EntrySummary.fromJson).toList();
    } on FormatException catch (error) {
      throw ControlFailure(error.message, offline: true);
    }
  }

  @override
  Future<Entry> get(String label) async {
    try {
      return Entry.fromJson(await _request('get', {'label': label}));
    } on FormatException catch (error) {
      throw ControlFailure(error.message, offline: true);
    }
  }

  @override
  Future<void> save(Entry entry, Entry? previous) async {
    final result = await _request(previous == null ? 'add' : 'edit', {
      'entry': entry.toJson(),
      if (previous != null) ...{
        'label': previous.label,
        'previous': previous.toJson(),
      },
    }, mutation: true);
    if (result != null) {
      throw const ControlFailure(
        '保存の応答が不正です。再接続して確認してください。',
        uncertain: true,
        offline: true,
      );
    }
  }

  @override
  Future<void> remove(Entry previous) async {
    final result = await _request('remove', {
      'label': previous.label,
      'previous': previous.toJson(),
    }, mutation: true);
    if (result != null) {
      throw const ControlFailure(
        '削除の応答が不正です。再接続して確認してください。',
        uncertain: true,
        offline: true,
      );
    }
  }

  @override
  Future<String> capture() async =>
      (await _call('Capture', [], 's')).values.single.asString();

  @override
  Future<void> cancelCapture() async {
    await _call('CancelCapture', [], '');
  }

  @override
  Future<void> close() => _client.close();
}
