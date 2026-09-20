# テストの考え方

利用者の入口から登録・貼り付け・編集・削除までを通します。入力と期待結果は[ケース](../tests/acceptance/cases.json)が正本で、各行を独立して実行します。[受入例](110_requirements/シナリオ/SC-001-文字列を使う/受入例.md)は実行先だけを参照します。

`./scripts/test-desktop.sh` は別のD-Busセッション、GNOME、保存先を用意します。検証専用の設定領域でアニメーションを無効にし、起動完了を確かめてから実際の入力装置でキーを押し、CLIの表示とGTK入力欄に届いた文字列を確認します。GNOMEの代用品や内部関数のモックは使いません。`tests/integration` は競合、取消し、切断、保存、再起動を確認します。

`just check` は文書・参照・認証情報混入・構文・保存・実画面の検査を行います。未確認事項はPRに明記し、対象外へ移して完了にしません。録画方法とロック画面の検証は[手動確認書](../tests/acceptance/manual/README.md)を参照します。

GUIの入力と期待結果は [gui-cases.json](../tests/acceptance/gui-cases.json) が正本です。Flutterの実画面で操作し、同じ隔離GNOMEの実際の入力装置と別のGTK入力欄でキー取得・貼り付けを検査します。`TestGui` の各行からFlutterの同名ケースへ対応します。
