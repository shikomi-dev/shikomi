# 同じログイン内の通信

バス名 `org.gnome.Shell`、パス `/io/github/shikomi/Control`、インターフェース `io.github.shikomi.Control`。利用者と同じユーザー権限のプロセスが呼べます。ネットワークには公開しません。

`Capture()` は実際のキーを押して離すまで待ち、GNOMEのキー表記を返します。取消し・時間切れは空文字です。同時に二件は読み取れません。`CancelCapture()` は同じ接続の入力待ちだけを取り消します。呼出元が消えたら入力取得を解放します。

`Request(JSON文字列)` の `operation` は `add / edit / remove / list / get`。addは `entry: {label,text,key}`、editは `label, entry, previous`、removeとgetは `label` を受け取ります。previousは編集前の一件です。

成功は `{"ok":true,"result":...}`、拒否は `{"ok":false,"error":"理由"}`。listはラベルとキーの配列、getは一件、変更はnullを返します。接続断では変更結果が不明なため、listで確認してから再操作します。
