# shikomi

Ubuntu 24.04 / 26.04 の標準GNOMEデスクトップで、登録した文字列をキーから貼り付けます。端末で登録・編集・削除し、GNOME Shell拡張がキーの読み取りと貼り付けを受け持ちます。

## aptでインストール

対象はUbuntu 24.04 / 26.04の標準GNOME（amd64）。初回だけ署名用の公開鍵と配布元を登録します。

```sh
curl -fsSL https://shikomi-dev.github.io/shikomi/apt/shikomi-archive-keyring.gpg \
  | sudo tee /usr/share/keyrings/shikomi-archive-keyring.gpg >/dev/null

echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/shikomi-archive-keyring.gpg] https://shikomi-dev.github.io/shikomi/apt stable main' \
  | sudo tee /etc/apt/sources.list.d/shikomi.list >/dev/null
sudo apt update
sudo apt install shikomi
```

ソースを取得済みなら `./scripts/setup-apt.sh` でも登録できます。このスクリプトは公開鍵の指紋（鍵を一意に指す値）をリポジトリ内の正本と照合してから登録します。

初回導入・更新後は一度ログアウトしてログインし直してください。初回は次を実行して拡張を有効にします。

```sh
gnome-extensions enable shikomi@shikomi-dev.github.io
```

次の版への更新も `sudo apt update && sudo apt upgrade shikomi` です。ソースからユーザー専用に入れる場合は `./scripts/install.sh` を使えますが、apt版と混在させないでください。

## 使い方

```console
$ shikomi add 'よろしくお願いします'
割り当てるキーを押してください（Ctrl / Alt / Super を含む組み合わせ、Escで取消し）:
読み取り: <Control><Alt>j
ラベル: あいさつ
登録しました。
```

入力待ちになったら、キーの名前をタイプするのではなく、割り当てたい組み合わせを実際に押します。全てのキーを離すとラベル入力へ進みます。Esc、入力終了、または1分の待機で取り消します。

アプリの入力欄で登録したキーを押して離すと、その文字列を貼り付けます。貼り付けはShift+Insertを使うため、この操作を受け付ける入力欄が対象です。クリップボードと選択文字列用の貼り付け領域は、登録文字列へ置き換わります。

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
sudo apt install gnome-shell gnome-terminal python3-gi python3-pytest python3-pexpect gir1.2-gtk-4.0 gir1.2-atspi-2.0 gjs nodejs reprepro gnupg
./scripts/setup-dev.sh # lefthookを導入済みの環境でフックを設定
./scripts/check-all.sh
```

実際のGNOMEを別セッションで起動し、利用者のデスクトップや保存先と分けて検証します。テストはキー入力からGTK入力欄への貼り付けまで通します。

[MIT License](LICENSE)

## リリースを発行する

`VERSION` を更新して検査を通し、PRをmainへ取り込みます。同じ版の `v<版>` タグでGitHub Releaseを公開すると、debの作成・Releaseへの添付・署名付きapt配布元の更新が自動実行されます。タグとVERSION、署名鍵の指紋が一致しないと公開しません。事前にActionsの `APT_SIGNING_KEY` とGitHub PagesのActions配備を設定します。

開発時の `git push` ではlefthookのpre-pushから同じ自動検査を実行し、失敗時はpushを止めます。
