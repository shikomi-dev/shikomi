# 詳細設計書 — dev-workflow / ユーザー向けメッセージ

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- 配置先: docs/features/dev-workflow/detailed-design/messages.md -->
<!-- 兄弟: ./index.md, ./classes.md, ./setup.md, ./data-structures.md, ./scripts.md -->

## ユーザー向けメッセージの確定文言

`requirements.md` §ユーザー向けメッセージ一覧 で ID のみ定義した MSG-DW-001〜012 の **正確な文言**を本節で凍結する。実装者・Sub-issue が勝手に改変できない契約として扱う。変更は本設計書の更新 PR のみで許可される。

### プレフィックス統一

全メッセージは 5 種類のプレフィックスのいずれかで始まる。色非対応端末でもプレフィックスのテキストだけで重要度が識別可能（A09 対策）。

| プレフィックス | 意味 | 色（対応端末時） |
|--------------|-----|--------------|
| `[FAIL]` | 処理中止を伴う失敗 | 赤 |
| `[OK]` | 成功完了 | 緑 |
| `[SKIP]` | 冪等実行による省略 | 灰 |
| `[WARN]` | 警告（処理は継続） | 黄 |
| `[INFO]` | 情報提供（処理は継続） | 既定色 |

色付けは `just` / `lefthook` / `cargo` のデフォルトに従い、TTY 非検出時は自動で無効化される（既存ツールの振る舞い）。**本 feature で独自に ANSI エスケープを出力しない**（KISS）。

### 2 行構造ルール

失敗メッセージ（`[FAIL]` プレフィックス）は常に **2 行構造**とする。検証は受入基準 11 で assertion される。

```
[FAIL] <何が失敗したかを日本語 1 文で要約>
次のコマンド: <実行すべき復旧コマンド 1 つ>
```

改行は LF 固定（`.gitattributes` で強制済み）。`fail_text` 内で動的変数（ファイル名・ユーザ名等）は使わない（T7 対策）。

### MSG 確定文言表

| ID | 出力先 | 文言（改行区切りで上下 2 行） |
|----|------|------------------------------|
| MSG-DW-001 | lefthook `fail_text` | `[FAIL] cargo fmt 違反を検出しました。` / `次のコマンド: just fmt` |
| MSG-DW-002 | lefthook `fail_text` | `[FAIL] cargo clippy 違反を検出しました。` / `次のコマンド: just clippy` |
| MSG-DW-003 | lefthook `fail_text` | `[FAIL] cargo test に失敗しました。` / `次のコマンド: just test` |
| MSG-DW-004 | lefthook `fail_text` | `[FAIL] コミットメッセージが Conventional Commits 1.0 に準拠していません。` / `規約: CONTRIBUTING.md §コミット規約 または https://www.conventionalcommits.org/ja/v1.0.0/` |
| MSG-DW-005 | setup stdout | `[OK] Setup complete. Git フックが有効化されました。`（1 行のみ、成功は 2 行構造ルールの例外） |
| MSG-DW-006 | setup stdout | `[SKIP] {tool} は既にインストール済みです。` / `バージョン: {version}`（`{tool}` / `{version}` は setup スクリプトが動的挿入、ただし **stdout のみ・CI ログ共有なし**のため T7 対象外） |
| MSG-DW-007 | CONTRIBUTING.md 静的記載 | `[WARN] --no-verify の使用は規約で原則禁止です。` / `PR 本文に理由を明記してください。CI が同一チェックを再実行します。` |
| MSG-DW-008 | setup stderr | `[FAIL] Rust toolchain が未検出です。` / `次のコマンド: https://rustup.rs/ の手順に従って rustup を導入してください。` |
| MSG-DW-009 | setup stderr | `[FAIL] .git/ ディレクトリが見つかりません。リポジトリルートで実行してください。` / `現在のディレクトリ: {cwd}`（`{cwd}` は setup stdout のみ、CI ログには流れない） |
| MSG-DW-010 | lefthook `fail_text` | `[FAIL] secret の混入が検出されました。該当行を除去後、git add を再実行してください。` / `既に push 済みの場合: CONTRIBUTING.md §Secret 混入時の緊急対応` |
| MSG-DW-011 | setup stderr | `[FAIL] PowerShell 7 以上が必要です（検出: {version}）。` / `次のコマンド: winget install Microsoft.PowerShell` |
| MSG-DW-012 | setup stderr | `[FAIL] {tool} バイナリの SHA256 検証に失敗しました。サプライチェーン改ざんの可能性があります。` / `次のコマンド: 一時ファイルを削除後にネットワーク状況を確認し再実行。繰り返し失敗する場合は Issue で報告してください。` |
| MSG-DW-013 | lefthook `fail_text` | `[FAIL] コミットメッセージに AI 生成フッターが含まれています（🤖 Generated with Claude Code / Co-Authored-By: Claude 等）。` / `次のコマンド: 該当行を削除して再コミット。許可されない trailer のポリシーは CONTRIBUTING.md §AI 生成フッターの禁止 を参照。` |

**gitleaks / audit-secret-paths.sh の `file:line` 出力**: これらのツール自体の stdout に検出箇所が出力される。`fail_text` には載せず、ツール出力との「縦の情報階層」（ツール出力 → 空行 → `[FAIL]` 2 行）を形成する。
