# 同じログイン内の通信

バス名 `org.gnome.Shell`、パス `/io/github/shikomi/Control`、インターフェース `io.github.shikomi.Control`。利用者と同じユーザー権限のプロセスが呼べます。ネットワークには公開しません。

`Capture()` は実際のキーを押して離すまで待ち、GNOMEのキー表記を返します。取消し・時間切れは空文字です。同時に二件は読み取れません。`CancelCapture()` は同じ接続の入力待ちだけを取り消します。呼出元が消えたら入力取得を解放します。

`Request(JSON文字列)` の `operation` は `add / edit / remove / list / get`。addは `entry: {label,text,key}`、editは `label, entry, previous`、removeとgetは `label` を受け取ります。previousは編集前の一件です。

removeには任意で `previous` を渡せます。GUIは画面で確認した一件を渡し、現在のラベル・本文・キーと一致しない場合は削除を拒みます。editのpreviousも同じ三項目で比較し、JSONの項目順には依存しません。previousを渡さない既存CLIの削除は従来どおりです。

`checkKey` は `key` と割り当て先 `owner` を受け取ります。ownerは `{kind:"entry",label:"編集中の元のラベル"}`（新規は空ラベル）または `{kind:"search"}`。結果は `{status,candidates}` で、statusは `available / entry / search / unavailable`。entryの場合は競合相手の `label` を返します。candidatesはその時点で取得・解除して確かめた候補の配列です。確認では割り当てを予約しません。

`getSearchKey` は `{key,active}` を返します。未設定は空キー、画面の競合などで現在取れない保存済みキーはactiveがfalseです。`setSearchKey` は `key` と変更前の `previous` を受け取り、競合を再確認して設定します。空キーで設定を解除します。

成功は `{"ok":true,"result":...}`、拒否は `{"ok":false,"error":"理由"}`。listはラベルとキーの配列、getは一件、変更はnullを返します。接続断では変更結果が不明なため、listで確認してから再操作します。
