# shikomi（仕込み）

任意のグローバルホットキーを押下すると、事前登録した文字列をクリップボード経由でフォアグラウンドアプリへ即時投入する、マルチプラットフォーム対応（Windows / macOS / Linux）のクリップボード管理ツール。Windows 専用の [Clibor](https://chigusa-web.com/clibor/) の OSS 代替を志向する。

## 特徴

- **ホットキー 1 回で投入**: `Ctrl+Alt+1` などのグローバルホットキーに文字列を登録し、任意のアプリへ即時貼り付け
- **機密文字列ファースト**: パスワード・トークン等を安全に扱う（自動クリア・OS キーチェーン連携）
- **デフォルト平文 vault**: OS ファイルパーミッション（Unix `0600` / Windows ACL 所有者のみ）で保護。技術知識なしに即利用可能
- **暗号化はオプトイン**: `shikomi vault encrypt` で Argon2id + AES-256-GCM による保護に切替可能
- **CLI + GUI**: `shikomi-cli` で全操作完結。`shikomi-gui`（Tauri v2）で設定 UI も提供

## 動作環境

| OS | 対応バージョン |
|----|--------------|
| Windows | 10 21H2 以降、11 |
| macOS | 12 Monterey 以降（Apple Silicon / Intel 両対応）|
| Linux | X11 / Wayland 両対応（Ubuntu 22.04+、Arch、その他 glibc 2.35+ ディストリ）|

## インストール（Install）

### Windows

```powershell
winget install shikomi-dev.shikomi
```

または [GitHub Releases](https://github.com/shikomi-dev/shikomi/releases) から `shikomi-windows-x86_64.msi` をダウンロードして実行。

> **Windows SmartScreen について**: shikomi のインストーラは EV/OV コード署名済みです。インストール時に「Windows によって PC が保護されました」という SmartScreen の警告が表示された場合は、「詳細情報」をクリックし「実行」を選択してください。署名元が `shikomi-dev` であることを確認してから実行してください。警告が出た場合の詳細は [SmartScreen 対処方法](https://github.com/shikomi-dev/shikomi/wiki/SmartScreen) を参照してください。

### macOS

```bash
brew install --cask shikomi
```

または [GitHub Releases](https://github.com/shikomi-dev/shikomi/releases) から `shikomi-macos-universal.dmg` をダウンロードして開く。

**Apple Developer ID 署名済み・Notarization 済みのため、Gatekeeper の警告は発生しない。**

### Linux

```bash
# apt（Ubuntu / Debian）
sudo apt install shikomi

# AppImage（汎用）
chmod +x shikomi-linux-x86_64.AppImage
./shikomi-linux-x86_64.AppImage
```

または [GitHub Releases](https://github.com/shikomi-dev/shikomi/releases) から各パッケージをダウンロード。

#### Linux 追加セットアップ

Wayland 環境でグローバルホットキーを使用するには、XDG Portal `org.freedesktop.impl.portal.GlobalShortcuts` をサポートするデスクトップ環境（GNOME 44+ / KDE Plasma 6+）が必要です。

X11 環境では追加セットアップは不要です。

## 権限要件（Permissions）

shikomi はユーザー権限のみで動作します。**管理者権限・`setuid`・`sudo` は一切不要です。**

### vault ファイルのパーミッション（Permission / ACL）

| OS | デフォルト設定 | 説明 |
|----|-------------|------|
| Unix（Linux / macOS） | `0600`（所有者のみ読み書き可） | `~/.local/share/shikomi/vault.json` に自動適用 |
| Windows | NTFS ACL: 所有者 SID のみフルコントロール | `%APPDATA%\shikomi\vault.json` に自動適用 |

> **注意（平文モード）**: デフォルトの平文 vault は OS のファイルパーミッションのみで保護されています。同一ユーザー権限で動作する他プロセス・マルウェアからは読取・書換が可能です。パスワード等の高度な機密情報を保管する場合は `shikomi vault encrypt` で暗号化保護を有効化してください。

### 暗号化モード（オプトイン）

```bash
shikomi vault encrypt
```

マスターパスワードを設定し、Argon2id + AES-256-GCM による暗号化保護に切り替えます。BIP-39 形式の 24 語リカバリフレーズが生成されます。安全な場所に保管してください。

## 使い方（Usage）

### デーモン起動

shikomi はバックグラウンドデーモンとしてホットキーを監視します。インストール時に OS の自動起動に登録されます。

```bash
# 手動起動
shikomi daemon start

# 状態確認
shikomi daemon status

# 停止
shikomi daemon stop
```

### エントリ管理（CLI）

```bash
# エントリ一覧表示（[plaintext] / [encrypted] を常時表示）
shikomi list

# エントリ登録（ホットキー: Ctrl+Alt+1、ラベル: メールアドレス）
shikomi add --hotkey "ctrl+alt+1" --label "メールアドレス" --value "user@example.com"

# パスワード等の機密文字列（自動クリア有効）
shikomi add --hotkey "ctrl+alt+2" --label "パスワード" --secret

# エントリ編集
shikomi edit 1

# エントリ削除
shikomi remove 1
```

### GUI 設定

```bash
shikomi gui
```

Tauri v2 ベースの設定 UI が起動します。エントリの管理、ホットキー設定、暗号化オプトインが可能です。

### 自動クリア

機密エントリ（`--secret`）をホットキーで投入すると、設定秒数（デフォルト: 30 秒）後にクリップボードを自動クリアします。トレイアイコンにカウントダウンが表示されます。

## ビルド方法（開発者向け）

開発環境のセットアップは [CONTRIBUTING.md](CONTRIBUTING.md) を参照してください。

```bash
git clone https://github.com/shikomi-dev/shikomi.git
cd shikomi
cargo build --workspace
```

## セキュリティ

脆弱性を発見した場合は [SECURITY.md](SECURITY.md) の手順に従ってご報告ください。

## ライセンス

[MIT License](LICENSE) — Copyright © 2026 shikomi Contributors
