# 定型文の画面

Flutterで一覧と編集を描き、GNOME拡張へ[公開契約](../../../contracts/control.md)で接続します。保存先ファイルは読み書きしません。`entries` が定型文の操作、`appearance.dart` が外観、編集中の画面が終了時の確認を受け持ちます。

リポジトリのルートで `./scripts/build-gui.sh` を実行すると、`build/linux/x64/release/bundle/shikomi_gui` を作ります。必要なライブラリは `bundle` ごと配布します。SDKは `scripts/flutter.sh` が指定版を取得して照合します。既存のSDKを使う場合は実行ファイルの絶対パスを `FLUTTER_BIN` に指定します。

`./scripts/check-all.sh` で画面の状態と、隔離したGNOMEに接続した実画面を確認します。GUIの具体的な入力は [gui-cases.json](../../../tests/acceptance/gui-cases.json) が正本です。
