# shikomi

Ubuntu 24.04 / 26.04 の標準GNOMEデスクトップで、登録した文字列をキーから貼り付けます。端末で登録・編集・削除し、GNOME Shell拡張がキーの読み取りと貼り付けを受け持ちます。

## インストール

標準GNOMEとPython 3 / python3-giが必要です。専用サーバーや管理者権限は不要です。

```sh
git clone https://github.com/shikomi-dev/shikomi.git
cd shikomi
./scripts/install.sh
```

一度ログアウトしてログインし直し、次を実行します。

```sh
gnome-extensions enable shikomi@shikomi-dev.github.io
```

`~/.local/bin` がPATHに含まれない場合は、`~/.local/bin/shikomi` を使ってください。更新時もインストール後にログインし直します。

## 使い方

```console
$ shikomi add 'よろしくお願いします'
割り当てるキーを押してください（Ctrl / Alt / Super を含む組み合わせ、Escで取消し）:
読み取り: <Control><Alt>j
ラベル: あいさつ
登録しました。
```

入力待ちになったら、キーの名前をタイプするのではなく、割り当てたい組み合わせを実際に押します。全てのキーを離すとラベル入力へ進みます。Esc、入力終了、または1分の待機で取り消します。

アプリの入力欄で登録したキーを押して離すと、その文字列を貼り付けます。貼り付けはShift+Insertを使うため、この操作を受け付ける入力欄が対象です。クリップボードの内容は登録文字列へ置き換わります。

```sh
shikomi list
shikomi edit あいさつ
shikomi remove あいさつ
```

編集では文字列、キー、ラベルを順に変更できます。文字列とラベルはEnterで現在値を維持します。削除は指定ラベルの登録を即時に解除します。ラベルとキーの重複、OS等で使用中のキーは登録できません。画面ロック中には動作しません。

登録は `~/.local/share/shikomi/entries.json`（XDG_DATA_HOME設定時はその配下）へ、所有者だけが読める平文で保存します。暗号化はありません。登録文字列をコマンド引数にするとシェル履歴にも残るため、初版は通常の定型文向けです。一覧には文字列を表示しません。

Windows、macOS、GNOME以外のデスクトップ、旧版のデータ移行、Homebrew配布は対象外です。

## 開発

[開発の進め方](docs/000_process.md)、[要求](docs/110_requirements/README.md)、[構造](docs/150_system/README.md)を参照してください。基盤は [software-development-template](https://github.com/kkm-horikawa/software-development-template) から作成しています。テンプレートのサンプル製品は含めません。

```sh
sudo apt install gnome-shell python3-gi python3-pytest python3-pexpect gir1.2-gtk-4.0 gjs nodejs
./scripts/check-all.sh
```

実際のGNOMEを別セッションで起動し、利用者のデスクトップや保存先と分けて検証します。テストはキー入力からGTK入力欄への貼り付けまで通します。

[MIT License](LICENSE)
