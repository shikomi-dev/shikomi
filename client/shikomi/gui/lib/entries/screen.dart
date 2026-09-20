import 'dart:async';
import 'dart:ui' show AppExitResponse;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../appearance.dart';
import 'editor.dart';
import 'key_caps.dart';

class EntryScreen extends StatefulWidget {
  const EntryScreen({
    super.key,
    required this.editor,
    required this.appearance,
  });
  final EntryEditor editor;
  final Appearance appearance;

  @override
  State<EntryScreen> createState() => _EntryScreenState();
}

class _EntryScreenState extends State<EntryScreen> with WidgetsBindingObserver {
  static const window = MethodChannel('io.github.shikomi/window');
  final label = TextEditingController();
  final body = TextEditingController();
  final search = TextEditingController();
  final labelFocus = FocusNode();
  final searchFocus = FocusNode();
  final scaffold = GlobalKey<ScaffoldState>();
  Timer? poll;
  late final AppLifecycleListener exitListener;
  int revision = -1;
  bool dialogOpen = false;
  EntryEditor get editor => widget.editor;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    editor.addListener(changed);
    search.addListener(changed);
    unawaited(editor.refresh());
    poll = Timer.periodic(
      const Duration(seconds: 5),
      (_) => editor.refresh(background: true),
    );
    exitListener = AppLifecycleListener(
      onExitRequested: () async => !editor.busy && await mayLeave()
          ? AppExitResponse.exit
          : AppExitResponse.cancel,
    );
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    unawaited(
      window.invokeMethod<void>(
        'setDark',
        Theme.of(context).brightness == Brightness.dark,
      ),
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      unawaited(editor.refresh(background: true));
    }
  }

  void changed() {
    if (!mounted) return;
    if (revision != editor.revision) {
      revision = editor.revision;
      label.text = editor.draft.label;
      body.text = editor.draft.text;
    }
    setState(() {});
  }

  @override
  void dispose() {
    poll?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    exitListener.dispose();
    editor.removeListener(changed);
    search.dispose();
    label.dispose();
    body.dispose();
    labelFocus.dispose();
    searchFocus.dispose();
    super.dispose();
  }

  Future<bool> confirm(String title, String message, String action) async {
    if (dialogOpen) return false;
    dialogOpen = true;
    try {
      return await showDialog<bool>(
            context: context,
            builder: (context) => AlertDialog(
              title: Text(title),
              content: Text(message),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context, false),
                  child: const Text('戻る'),
                ),
                FilledButton(
                  onPressed: () => Navigator.pop(context, true),
                  child: Text(action),
                ),
              ],
            ),
          ) ??
          false;
    } finally {
      dialogOpen = false;
    }
  }

  Future<bool> mayLeave() async {
    if (editor.uncertain) {
      return confirm(
        '変更結果が未確認です',
        '保存されたか確認できていません。閉じると編集中の入力は失われます。先に「保存内容を読み直す」で確認できます。',
        '閉じる',
      );
    }
    if (!editor.dirty) return true;
    return confirm('変更を破棄しますか？', '入力した変更はまだ保存されていません。', '変更を破棄');
  }

  Future<void> create() async {
    if (editor.busy || editor.uncertain || !await mayLeave() || !mounted) {
      return;
    }
    editor.create();
    scaffold.currentState?.closeDrawer();
    labelFocus.requestFocus();
  }

  Future<void> select(String value) async {
    if (editor.busy || editor.uncertain || !await mayLeave() || !mounted) {
      return;
    }
    await editor.select(value);
    scaffold.currentState?.closeDrawer();
  }

  Future<void> reload() async {
    if ((editor.dirty || editor.uncertain) &&
        !await confirm(
          '保存内容を読み直しますか？',
          '編集中の入力を破棄し、現在保存されている内容を表示します。',
          '読み直す',
        )) {
      return;
    }
    await editor.reload();
  }

  Future<void> remove() async {
    final saved = editor.original;
    if (saved == null) return;
    if (await confirm(
      '「${saved.label}」を削除しますか？',
      '登録とショートカットを解除します。この操作は元に戻せません。',
      '削除する',
    )) {
      await editor.remove();
    }
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.keyN, control: true): () =>
          unawaited(create()),
      const SingleActivator(LogicalKeyboardKey.keyS, control: true): () =>
          unawaited(editor.save()),
      const SingleActivator(LogicalKeyboardKey.keyF, control: true): () {
        if (MediaQuery.sizeOf(context).width < 780) {
          scaffold.currentState?.openDrawer();
        }
        searchFocus.requestFocus();
      },
    },
    child: LayoutBuilder(
      builder: (context, constraints) {
        final narrow = constraints.maxWidth < 780;
        return Scaffold(
          key: scaffold,
          drawer: narrow
              ? Drawer(width: 300, child: SafeArea(child: sidebar()))
              : null,
          body: SafeArea(
            child: Column(
              children: [
                header(narrow),
                const Divider(),
                if (!editor.connected) connectionBanner(),
                if (editor.error != null || editor.uncertain)
                  message(
                    editor.error ?? '変更結果が未確認です。保存内容を読み直してください。',
                    error: true,
                  ),
                if (editor.notice != null) message(editor.notice!),
                Expanded(
                  child: Row(
                    children: [
                      if (!narrow)
                        SizedBox(
                          width: constraints.maxWidth > 1150 ? 320 : 280,
                          child: sidebar(),
                        ),
                      if (!narrow) const VerticalDivider(width: 1),
                      Expanded(
                        child: editor.opened
                            ? editingPane(narrow)
                            : emptyPane(),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    ),
  );

  Widget header(bool narrow) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
    child: Row(
      children: [
        if (narrow)
          IconButton(
            tooltip: '定型文の一覧',
            onPressed: () => scaffold.currentState?.openDrawer(),
            icon: const Icon(Icons.menu),
          ),
        Image.asset(
          'assets/app-icon.png',
          width: 34,
          height: 34,
          excludeFromSemantics: true,
        ),
        const SizedBox(width: 10),
        const Text(
          'shikomi',
          style: TextStyle(fontSize: 24, fontWeight: FontWeight.w700),
        ),
        const Spacer(),
        if (!narrow) ...[
          Icon(
            editor.connected ? Icons.check_circle : Icons.warning_amber_rounded,
            size: 15,
            color: editor.connected
                ? const Color(0xff57a84a)
                : Theme.of(context).colorScheme.error,
          ),
          const SizedBox(width: 8),
          Text(editor.connected ? 'ショートカット有効' : '拡張に接続できません'),
          const Spacer(),
        ],
        PopupMenuButton<ThemeMode>(
          tooltip: '外観',
          icon: Icon(
            Theme.of(context).brightness == Brightness.dark
                ? Icons.dark_mode_outlined
                : Icons.light_mode_outlined,
          ),
          initialValue: widget.appearance.mode,
          onSelected: (value) async {
            final saved = await widget.appearance.select(value);
            if (!saved && mounted) {
              ScaffoldMessenger.of(context).showSnackBar(
                const SnackBar(content: Text('外観を切り替えましたが、次回用の設定を保存できませんでした。')),
              );
            }
          },
          itemBuilder: (context) => const [
            PopupMenuItem(value: ThemeMode.system, child: Text('システムに合わせる')),
            PopupMenuItem(value: ThemeMode.light, child: Text('ライト')),
            PopupMenuItem(value: ThemeMode.dark, child: Text('ダーク')),
          ],
        ),
        IconButton(
          tooltip: '一覧を更新',
          onPressed: editor.busy ? null : () => editor.refresh(),
          icon: const Icon(Icons.refresh),
        ),
      ],
    ),
  );

  Widget connectionBanner() => Container(
    color: Theme.of(context).colorScheme.errorContainer,
    padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 10),
    child: Wrap(
      crossAxisAlignment: WrapCrossAlignment.center,
      spacing: 16,
      runSpacing: 8,
      children: [
        const Text('GNOME拡張との接続を確認してください。'),
        TextButton(
          onPressed: editor.busy ? null : () => editor.refresh(),
          child: const Text('再接続'),
        ),
        TextButton(
          onPressed: () => showDialog<void>(
            context: context,
            builder: (context) => AlertDialog(
              title: const Text('拡張を有効にする'),
              content: const SelectableText(
                '初回導入・更新後はログインし直してください。\n端末で次を実行すると拡張を有効にできます。\n\ngnome-extensions enable shikomi@shikomi-dev.github.io\n\n画面ロック中は操作できません。',
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('閉じる'),
                ),
              ],
            ),
          ),
          child: const Text('接続方法'),
        ),
      ],
    ),
  );

  Widget message(String text, {bool error = false}) => Semantics(
    liveRegion: true,
    child: Container(
      width: double.infinity,
      color: error
          ? Theme.of(context).colorScheme.errorContainer
          : Theme.of(context).colorScheme.primaryContainer,
      padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 10),
      child: Wrap(
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 16,
        children: [
          Text(text),
          if (error || editor.uncertain)
            TextButton(
              onPressed: editor.busy ? null : reload,
              child: const Text('保存内容を読み直す'),
            ),
        ],
      ),
    ),
  );

  Widget sidebar() {
    final filtered = editor.entries
        .where(
          (entry) =>
              entry.label.toLowerCase().contains(search.text.toLowerCase()),
        )
        .toList();
    return ColoredBox(
      color: Theme.of(context).brightness == Brightness.dark
          ? const Color(0xff202123)
          : const Color(0xfff3f2f0),
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 22, 20, 16),
            child: Row(
              children: [
                const Text(
                  '定型文',
                  style: TextStyle(fontSize: 23, fontWeight: FontWeight.w700),
                ),
                const Spacer(),
                OutlinedButton.icon(
                  onPressed: editor.busy || editor.uncertain ? null : create,
                  icon: const Icon(Icons.add, size: 19),
                  label: const Text('追加'),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: TextField(
              controller: search,
              focusNode: searchFocus,
              decoration: const InputDecoration(
                hintText: 'ラベルで検索',
                prefixIcon: Icon(Icons.search),
                isDense: true,
              ),
            ),
          ),
          const SizedBox(height: 12),
          Expanded(
            child: filtered.isEmpty
                ? Center(
                    child: Text(
                      search.text.isEmpty ? 'まだ登録がありません' : '見つかりませんでした',
                    ),
                  )
                : ListView.builder(
                    itemCount: filtered.length,
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    itemBuilder: (context, index) {
                      final entry = filtered[index];
                      final selected = editor.original?.label == entry.label;
                      return Padding(
                        padding: const EdgeInsets.only(bottom: 5),
                        child: Material(
                          color: selected
                              ? Theme.of(context).colorScheme.primaryContainer
                              : Colors.transparent,
                          borderRadius: BorderRadius.circular(8),
                          child: ListTile(
                            selected: selected,
                            enabled: !editor.busy && !editor.uncertain,
                            selectedColor: Theme.of(context)
                                .colorScheme
                                .onSurface,
                            shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(8),
                            ),
                            title: Text(
                              entry.label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontWeight: selected
                                    ? FontWeight.w700
                                    : FontWeight.w500,
                              ),
                            ),
                            subtitle: Padding(
                              padding: const EdgeInsets.only(top: 6),
                              child: KeyCaps(value: entry.key, small: true),
                            ),
                            contentPadding: const EdgeInsets.symmetric(
                              horizontal: 18,
                              vertical: 7,
                            ),
                            onTap: () => select(entry.label),
                          ),
                        ),
                      );
                    },
                  ),
          ),
          Padding(
            padding: const EdgeInsets.all(20),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Text(
                '${editor.entries.length}件の定型文',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget emptyPane() => Center(
    child: Padding(
      padding: const EdgeInsets.all(32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.keyboard_alt_outlined,
            size: 42,
            color: Theme.of(context).colorScheme.primary,
          ),
          const SizedBox(height: 20),
          Text(
            editor.entries.isEmpty ? 'よく使う言葉を、ひと押しで。' : '定型文を選んで編集',
            textAlign: TextAlign.center,
            style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 12),
          const Text(
            '本文とショートカットを登録すると、\nほかのアプリへキー操作で貼り付けられます。',
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          FilledButton.icon(
            onPressed: editor.busy || editor.uncertain ? null : create,
            icon: const Icon(Icons.add),
            label: const Text('定型文を追加'),
          ),
        ],
      ),
    ),
  );

  Widget editingPane(bool narrow) => Column(
    children: [
      Expanded(
        child: SingleChildScrollView(
          padding: EdgeInsets.all(narrow ? 20 : 32),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      editor.original?.label ?? '新しい定型文',
                      style: const TextStyle(
                        fontSize: 28,
                        fontWeight: FontWeight.w700,
                      ),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (editor.dirty || editor.uncertain)
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 6,
                      ),
                      decoration: BoxDecoration(
                        border: Border.all(
                          color: Theme.of(context).colorScheme.primary,
                        ),
                        borderRadius: BorderRadius.circular(5),
                      ),
                      child: Text(
                        editor.uncertain ? '変更結果が未確認' : '未保存の変更',
                        style: TextStyle(
                          fontSize: 12,
                          color: Theme.of(context).colorScheme.primary,
                        ),
                      ),
                    ),
                  if (editor.original != null)
                    PopupMenuButton<String>(
                      tooltip: '定型文の操作',
                      enabled: !editor.busy && !editor.uncertain,
                      onSelected: (value) =>
                          value == 'remove' ? remove() : reload(),
                      itemBuilder: (context) => const [
                        PopupMenuItem(
                          value: 'reload',
                          child: Text('保存内容を読み直す'),
                        ),
                        PopupMenuItem(value: 'remove', child: Text('定型文を削除')),
                      ],
                    ),
                ],
              ),
              const SizedBox(height: 26),
              fieldHeading('ラベル'),
              Semantics(
                label: 'ラベル',
                child: TextField(
                  key: const ValueKey('label'),
                  controller: label,
                  focusNode: labelFocus,
                  readOnly: editor.busy || editor.uncertain,
                  decoration: const InputDecoration(hintText: '例：あいさつ'),
                  onChanged: (value) =>
                      editor.update(editor.draft.copyWith(label: value)),
                ),
              ),
              const SizedBox(height: 24),
              fieldHeading('貼り付ける本文'),
              Semantics(
                label: '貼り付ける本文',
                child: TextField(
                  key: const ValueKey('body'),
                  controller: body,
                  minLines: 6,
                  maxLines: 12,
                  readOnly: editor.busy || editor.uncertain,
                  decoration: const InputDecoration(
                    hintText: 'ここに、よく使う文章を入力してください。',
                  ),
                  onChanged: (value) =>
                      editor.update(editor.draft.copyWith(text: value)),
                ),
              ),
              const SizedBox(height: 24),
              fieldHeading('ショートカット'),
              Container(
                padding: const EdgeInsets.all(14),
                decoration: BoxDecoration(
                  border: Border.all(
                    color: Theme.of(context).colorScheme.outline,
                  ),
                  borderRadius: BorderRadius.circular(9),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: editor.capturing
                          ? const Text('使いたいキーを押して、離してください…')
                          : editor.draft.key.isEmpty
                          ? const Text('まだ設定されていません')
                          : KeyCaps(value: editor.draft.key),
                    ),
                    const SizedBox(width: 10),
                    OutlinedButton(
                      onPressed:
                          editor.busy || editor.uncertain || !editor.connected
                          ? null
                          : editor.capture,
                      child: Text(editor.draft.key.isEmpty ? 'キーを設定' : '変更'),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 10),
              Text(
                editor.capturing
                    ? 'Ctrl・Alt・Superのいずれかを含めてください。Escで取消し、1分で終了します。'
                    : '変更を押して、使いたいキーを実際に押してください。',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(height: 28),
              Text(
                '保存すると、Shift + Insertで貼り付けられるアプリで使えます。',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
          ),
        ),
      ),
      const Divider(),
      Padding(
        padding: EdgeInsets.all(narrow ? 16 : 24),
        child: Row(
          children: [
            if (!narrow)
              Expanded(
                child: Text(
                  editor.uncertain
                      ? '変更結果を確認してください'
                      : editor.dirty
                      ? '変更はまだ保存されていません'
                      : editor.original == null
                      ? 'ラベル・本文・キーを入力してください'
                      : '保存済み',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              )
            else
              const Spacer(),
            OutlinedButton(
              onPressed:
                  editor.busy ||
                      editor.uncertain ||
                      (!editor.dirty && editor.original != null)
                  ? null
                  : () async {
                      if (await mayLeave()) editor.cancel();
                    },
              child: const Text('キャンセル'),
            ),
            const SizedBox(width: 12),
            FilledButton(
              onPressed: editor.canSave ? editor.save : null,
              child: Text(
                editor.busy && !editor.capturing
                    ? '処理中…'
                    : editor.original == null
                    ? '登録する'
                    : '変更を保存',
              ),
            ),
          ],
        ),
      ),
    ],
  );

  Widget fieldHeading(String value) => Padding(
    padding: const EdgeInsets.only(bottom: 10),
    child: Text(
      value,
      style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
    ),
  );
}
