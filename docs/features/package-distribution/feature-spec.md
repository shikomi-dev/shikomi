# feature-spec: package-distribution

<!-- 階層 2 — Feature 業務概念 | Issue #154 -->

## メタデータ

| 項目 | 内容 |
|---|---|
| Feature ID | PKG |
| Feature 名 | `package-distribution` |
| Issue | #154 |
| Status | 実装完了 · 設計確定 |
| 依存 Feature | `release-signing` (Issue #130) — コード署名対応後に GPG 署名 APT/RPM repo へ昇格予定 |

## 概要

winget (Windows) / Homebrew tap (macOS) / APT (Linux Debian/Ubuntu) / DNF/YUM (Linux Fedora/RHEL) の各パッケージマネージャーを通じて `shikomi` をインストール可能にする。

GitHub Releases に既存の成果物（.msi / .exe / .dmg / .deb / .rpm）を活用し、GitHub Release の `published` イベントをトリガに各パッケージマネージャーのインデックス・メタデータを自動生成・公開する CI パイプライン (`package-publish.yml`) を構築する。

## ユースケース

| ID | アクター | ストーリー |
|---|---|---|
| UC-PKG-001 | エンドユーザー (Windows) | `winget install shikomi-dev.shikomi` で shikomi を Windows にインストールできる |
| UC-PKG-002 | エンドユーザー (macOS) | `brew tap shikomi-dev/homebrew-shikomi && brew install --cask shikomi` で shikomi を macOS にインストールできる |
| UC-PKG-003 | エンドユーザー (Linux Debian/Ubuntu) | APT ソース追加後に `sudo apt install shikomi` でインストール・更新できる |
| UC-PKG-004 | エンドユーザー (Linux Fedora/RHEL) | DNF リポジトリ設定後に `sudo dnf install shikomi` でインストール・更新できる |
| UC-PKG-005 | 開発チーム | 新バージョンリリース時、CI が自動で全パッケージマネージャーのメタデータを更新する |

## 機能要件一覧

| ID | 区分 | 要件 |
|---|---|---|
| R-PKG-001 | winget | GitHub Release published 時に `winget-pkgs` へ自動 PR を送信する |
| R-PKG-002 | winget | `.github/winget/` に winget マニフェスト 3 点セット（version / installer / locale）を格納する |
| R-PKG-003 | brew | GitHub Release published 時に `homebrew-shikomi` tap の Cask formula を自動更新する |
| R-PKG-004 | brew | `brew tap shikomi-dev/homebrew-shikomi && brew install --cask shikomi` でインストールできる |
| R-PKG-005 | apt | GitHub Release published 時に GitHub Pages の APT リポジトリを自動更新する |
| R-PKG-006 | apt | APT ソース追加後に `sudo apt install shikomi` でインストールできる |
| R-PKG-007 | rpm | GitHub Release published 時に GitHub Pages の RPM リポジトリを自動更新する |
| R-PKG-008 | rpm | DNF/YUM リポジトリ設定後に `sudo dnf install shikomi` でインストールできる |
| R-PKG-009 | 自動化 | `WINGET_TOKEN` 未設定時は winget ジョブを `continue-on-error` でスキップする（graceful degradation）|
| R-PKG-010 | ドキュメント | README のインストール手順が全 OS で実際に機能するコマンドを正確に記載している |

## 非機能要件・既知の制約

| 項目 | 現状 (v0.1.x) | 解消方針 |
|---|---|---|
| GPG 署名（apt） | `[trusted=yes]` による未署名配布 | Issue #130 完了後に GPG 鍵を生成し `signed-by=` へ移行 |
| GPG 署名（rpm） | `gpgcheck=0` による未署名配布 | Issue #130 完了後に同様に移行 |
| コード署名（Windows） | SmartScreen 警告が発生する | Issue #130 (EV/OV 証明書) で解消 |
| コード署名（macOS） | Gatekeeper 警告が発生する | Issue #130 (Apple Developer ID) で解消 |
| Apple Silicon 専用 | v0.1.0 Cask は aarch64 DMG のみ対応 | Intel Mac / Universal Binary は別 Issue で追跡 |
| winget 審査 | microsoft/winget-pkgs への自動 PR 後に人的審査が必要 | 外部依存（Microsoft プロセス）のため制御不可 |

## 受入基準

| # | 基準 | 検証方法 |
|---|---|---|
| AC-PKG-001 | winget-pkgs マージ後に `winget install shikomi-dev.shikomi` が機能する | Windows 10/11 実機 |
| AC-PKG-002 | `brew tap shikomi-dev/homebrew-shikomi && brew install --cask shikomi` が機能する | macOS Monterey+ Apple Silicon 実機 |
| AC-PKG-003 | APT ソース追加後に `sudo apt install shikomi` が機能する | Ubuntu 22.04 実機 |
| AC-PKG-004 | DNF リポジトリ追加後に `sudo dnf install shikomi` が機能する | Fedora 40 実機 |
| AC-PKG-005 | GitHub Release published 時に `package-publish.yml` が自動実行される | CI ログ |
| AC-PKG-006 | GitHub Pages に apt / rpm ディレクトリが公開されている | `https://shikomi-dev.github.io/shikomi/` 確認 |
| AC-PKG-007 | README のインストール手順が全 OS で正確に記載されている | ドキュメントレビュー |

## 関連成果物

| 成果物 | パス / URL |
|---|---|
| CI ワークフロー | `.github/workflows/package-publish.yml` |
| winget マニフェスト | `.github/winget/shikomi-dev.shikomi*.yaml` |
| Homebrew tap | `https://github.com/shikomi-dev/homebrew-shikomi` |
| APT / RPM リポジトリ | `https://shikomi-dev.github.io/shikomi/` (gh-pages) |
| sub-feature 基本設計 | `docs/features/package-distribution/ci/basic-design.md` |
