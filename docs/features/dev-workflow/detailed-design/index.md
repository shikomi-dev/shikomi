# 詳細設計書 — dev-workflow / 索引

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- 本ディレクトリは PR #84 / Issue #85 のレビュー（ペガサス指摘）で `detailed-design.md` 単一ファイル 516 行が 500 行ルール超過したため分割した結果。 -->
<!-- 配置先: docs/features/dev-workflow/detailed-design/ -->
<!-- 兄弟: ./classes.md, ./messages.md, ./setup.md, ./data-structures.md, ./scripts.md -->

## 記述ルール（必ず守ること）

詳細設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

## 分割構成

`detailed-design.md` 単一ファイルの 500 行超えを避けるため、本詳細設計は次の 6 ファイルに分割する（PR #84 / Issue #85 レビューでペガサス指摘）。サフィックスファイル（`detailed-design-scripts.md` 等）は禁止、必ずディレクトリ化:

| ファイル | 内容 |
|---------|------|
| `index.md` | 本ファイル。記述ルール / 分割構成 / 全体構造図 / ビジュアルデザイン / 出典・参考の集約 |
| `classes.md` | クラス設計（設定ファイルの構造契約 Mermaid 図、レビュー指摘で確定した 5 項目（先送り撤廃）、設計判断の補足） |
| `messages.md` | ユーザー向けメッセージの確定文言（プレフィックス統一 / 2 行構造ルール / MSG-DW-001〜013 確定文言表） |
| `setup.md` | lefthook / gitleaks の配布経路と SHA256 検証（REQ-DW-015 詳細）、CODEOWNERS 保護 5 パス（REQ-DW-016 詳細） |
| `data-structures.md` | データ構造（`lefthook.yml` / `justfile` / `setup.sh` / `setup.ps1` のキー・ステップ契約、AI 生成フッター検出パターン、`.github/workflows/*.yml` 編集契約） |
| `scripts.md` | CI スクリプト系契約（`audit-secret-paths.sh` の `unsafe` ブロック検出契約、`audit.yml` への secret scan ステップ追加契約） |

## 読む順序（推奨）

1. `index.md`（本ファイル）で全体構造と分割マップを把握
2. `classes.md` で設定ファイル群の構造契約・確定済み 5 項目を確認
3. `messages.md` で UX 文言契約を確認
4. `setup.md` で setup スクリプトのバイナリ取得経路と CODEOWNERS 保護を確認
5. `data-structures.md` で各設定ファイルのキー / レシピ / ステップ契約を確認
6. `scripts.md` で CI スクリプトの個別検出契約を確認

## ビジュアルデザイン

該当なし — 理由: 本 feature は CLI のみで GUI 要素を持たない。フック失敗時のテキスト出力は `lefthook` / `just` / `cargo` のデフォルト配色・書式に従う。

---

## 出典・参考

- lefthook 公式ドキュメント: https://lefthook.dev/ / https://github.com/evilmartians/lefthook
- lefthook Configuration Reference (`Commands` / `fail_text` / `parallel`): https://lefthook.dev/configuration/
- lefthook Releases（SHA256 ピン元）: https://github.com/evilmartians/lefthook/releases
- just 公式ドキュメント: https://just.systems/
- just `windows-shell` 設定: https://just.systems/man/en/chapter_33.html
- convco: https://github.com/convco/convco README §Git hooks
- gitleaks: https://github.com/gitleaks/gitleaks
- gitleaks `protect` サブコマンド（staged 対象）: https://github.com/gitleaks/gitleaks#scan-commands
- gitleaks Releases（SHA256 ピン元）: https://github.com/gitleaks/gitleaks/releases
- Git `core.hooksPath` ドキュメント: https://git-scm.com/docs/githooks#_core_hookspath
- `cargo install --locked` 挙動: https://doc.rust-lang.org/cargo/commands/cargo-install.html
- Conventional Commits 1.0 仕様: https://www.conventionalcommits.org/ja/v1.0.0/
- `cargo-deny` 公式: https://embarkstudios.github.io/cargo-deny/
- `git filter-repo` 公式（履歴書換え推奨手段）: https://github.com/newren/git-filter-repo
- GitHub Secret scanning: https://docs.github.com/en/code-security/secret-scanning/about-secret-scanning
- GitHub Actions `Swatinem/rust-cache@v2`: https://github.com/Swatinem/rust-cache
- Microsoft Learn "Installing PowerShell on Windows": https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows
- OWASP Top 10 2021: https://owasp.org/Top10/
