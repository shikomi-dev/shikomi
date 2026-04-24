# Contributing to shikomi

## 目次

1. [はじめに](#はじめに)
2. [ブランチ戦略（GitFlow）](#ブランチ戦略gitflow)
3. [コミット規約（Conventional Commits）](#コミット規約conventional-commits)
4. [AI 生成フッターの禁止](#ai-生成フッターの禁止)
5. [マージ戦略](#マージ戦略)
6. [PR 規約](#pr-規約)
7. [開発環境セットアップ](#開発環境セットアップ)
8. [ローカル品質ゲート（lefthook / just）](#ローカル品質ゲートlefthook--just)
9. [Secret 混入時の緊急対応](#secret-混入時の緊急対応)
10. [コーディング規約](#コーディング規約)

---

## はじめに

shikomi への貢献を歓迎します。バグ報告・機能提案・ドキュメント改善・コード PR、いずれも大歓迎です。

セキュリティ脆弱性を発見した場合は、Issue ではなく [SECURITY.md](SECURITY.md) の手順に従って非公開で報告してください。

---

## ブランチ戦略（GitFlow）

本プロジェクトは **GitFlow** を採用します。

### ブランチ構成

| ブランチ | 役割 | 起点 | マージ先 |
|---------|------|------|---------|
| `main` | リリース済みの唯一の真実源。各コミットはタグ付きリリースに対応 | — | — |
| `develop` | 次期リリースの統合ブランチ。全 `feature` がここに集約 | `main`（初回のみ） | `release/*` 経由で `main` へ |
| `feature/*` | 単一機能・単一 Issue の作業ブランチ | **`develop`** | **`develop`** |
| `release/*` | RC 期間。バージョン bump / CHANGELOG 確定 / 署名動作確認のみ | `develop` | `main`（tag 付与）+ `develop`（back-merge） |
| `hotfix/*` | リリース済み版への緊急修正 | `main` | `main`（tag 付与）+ `develop`（back-merge） |

### feature ブランチの命名規則

```
feature/{issue-number}-{slug}
feature/{slug}

例:
  feature/12-hotkey-registrar
  feature/vault-encryption
```

### release / hotfix ブランチの命名規則

```
release/{version}   例: release/0.1.0    （v 接頭辞なし。タグ側に v を付ける）
hotfix/{version}    例: hotfix/0.1.1
```

### 作業フロー（feature）

1. `develop` から `feature/{slug}` を切る
2. 作業・コミット（Conventional Commits 必須）
3. `develop` への PR を作成（squash merge）
4. CODEOWNERS レビュー 1 名 + 必須 CI 通過でマージ

### リリースフロー（release）

1. `develop` から `release/X.Y.Z` を切る
2. `release/X.Y.Z` 上でバージョン bump / CHANGELOG 確定のみ
3. `main` への PR を作成（merge commit）— **2 名レビュー必須**
4. マージ後に `vX.Y.Z` タグを付与
5. **同じ `release/X.Y.Z` を `develop` へも back-merge する（24h 以内）**

### hotfix フロー

1. `main` から `hotfix/X.Y.(Z+1)` を切る
2. バグ修正のみ実施、バージョン bump（patch のみ）
3. `main` への PR を作成（merge commit）— **2 名レビュー必須**
4. マージ後に `vX.Y.(Z+1)` タグを付与
5. **同じ `hotfix/X.Y.(Z+1)` を `develop` へも back-merge する（24h 以内）**

> **back-merge の重要性**: release/hotfix を `main` にマージした後、同ブランチを `develop` にも merge commit で戻さないと、次回リリースで `develop` が `main` より古い状態になり衝突します。CI の `back-merge-check` が 24h 以内の back-merge 未実施を検知し、担当者に Issue で通知します。

---

## コミット規約（Conventional Commits）

全コミットメッセージは [Conventional Commits](https://www.conventionalcommits.org/) に従います。PR タイトルが squash merge 時のコミットメッセージになるため、**PR タイトルも同規約に従う必要があります**（CI の `pr-title-check` で検証）。

### フォーマット

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### 使用可能な type

| type | 用途 |
|------|------|
| `feat` | 新機能 |
| `fix` | バグ修正 |
| `docs` | ドキュメントのみの変更 |
| `chore` | ビルド・ツール・設定変更（本番コードに影響なし） |
| `refactor` | リファクタリング（バグ修正・機能追加なし） |
| `test` | テストの追加・修正 |
| `ci` | CI/CD 設定変更 |
| `build` | ビルドシステム・依存関係変更 |
| `perf` | パフォーマンス改善 |

### Breaking Change

```
feat!: remove deprecated --plaintext flag
# または
feat(vault): new encryption API

BREAKING CHANGE: vault format v1 is no longer supported
```

---

## AI 生成フッターの禁止

コミットメッセージには、AI コーディングアシスタント（Claude Code / ChatGPT / Copilot 等）が挿入する**自動署名 trailer / 広告フッターを一切含めない**。`commit-msg` フック（`just commit-msg-no-ai-footer`）が以下 3 パターンを検出し、該当時はコミットを中止します。

| # | 検出パターン（case-insensitive ERE） | 例 |
|---|----------------------------------|-----|
| P1 | `🤖.*Generated with.*Claude` | `🤖 Generated with [Claude Code](https://claude.com/claude-code)` |
| P2 | `Co-Authored-By:.*@anthropic\.com` | `Co-Authored-By: Claude <noreply@anthropic.com>` |
| P3 | `Co-Authored-By:.*\bClaude\b` | `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` |

### なぜ禁止するか

- 履歴純度: `main` / `develop` の履歴を人間の意図のみで追えるようにする
- コントリビュータ帰属の正確性: AI ツールは Contributor 契約（CLA / DCO）の対象外
- 将来の監査経路: `git log` 解析で共作者を抽出する際に AI が混ざると集計がブレる

### Agent への明示的教示

Claude Code / Cursor / その他のエージェントをローカル環境で使う場合、**AI 生成フッターの自動挿入を抑止する設定で起動**してください。抑止方法はツール提供元のドキュメントに従います。

### `--no-verify` での回避は禁止

`git commit --no-verify` で commit-msg フックをバイパスする行為は本規約で原則禁止です（下記 [ローカル品質ゲート](#ローカル品質ゲートlefthook--just) の注意書き参照）。やむを得ずバイパスした PR は、レビュワーが `git log` 目視で trailer 混入を検出し、差し戻します。

---

## マージ戦略

| PR 種別 | マージ方法 | 理由 |
|--------|----------|------|
| `feature/*` → `develop` | **squash merge** | feature 内の作業コミットを 1 commit に集約。PR タイトルがコミットメッセージになる |
| `release/*` → `main` | **merge commit**（No fast-forward） | リリース分岐の履歴を `main` に残す |
| `release/*` → `develop` | **merge commit**（No fast-forward） | back-merge 痕跡を残す |
| `hotfix/*` → `main` | **merge commit**（No fast-forward） | 同上 |
| `hotfix/*` → `develop` | **merge commit**（No fast-forward） | 同上 |

> **rebase merge は使用禁止です。** GitHub リポジトリ設定で無効化されています。

---

## PR 規約

### PR ブランチ制限

- `main` への PR は `release/*` または `hotfix/*` からのみ許可（`branch-policy` CI で強制）
- `develop` への PR は `feature/*` / `release/*` / `hotfix/*` からのみ許可

### PR チェックリスト

- [ ] PR タイトルが Conventional Commits に従っている
- [ ] 関連する Issue 番号を本文に記載している（`Closes #123`）
- [ ] `CHANGELOG.md` の更新が必要な場合は更新済み
- [ ] `release/*` / `hotfix/*` → `main` PR の場合: **24h 以内に `develop` への back-merge PR を作成する**

### lock file の変更

`Cargo.lock` / `pnpm-lock.yaml` のみが変更されている PR には `deps-lockfile-only` ラベルを付与し、意図的な更新である旨を本文に記載してください。

---

## 開発環境セットアップ

### 必要なツール

- [Rust](https://rustup.rs/) 最新 stable
- [Node.js](https://nodejs.org/) 20 LTS 以上（GUI 開発時のみ）
- [pnpm](https://pnpm.io/) 9 以上（GUI 開発時のみ）

### Linux 追加依存

```bash
# Ubuntu / Debian
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  libxdo-dev libxcb1-dev libxcb-keysyms1-dev
```

### ビルド

```bash
git clone https://github.com/shikomi-dev/shikomi.git
cd shikomi

# ローカル品質ゲートの導入（Rust toolchain 済み前提。冪等）
bash scripts/setup.sh          # POSIX (Linux / macOS)
pwsh scripts/setup.ps1         # Windows (PowerShell 7+)

# 以降は just 経由で統一
just fmt-check   # cargo fmt --all -- --check
just clippy      # cargo clippy --all-targets --all-features
just test        # cargo test --workspace
just audit       # cargo deny + 静的 secret 経路監査
just check-all   # 全チェックを順次実行
```

**Windows 要件**: PowerShell 7+ が必須です（5.1 は非サポート）。未導入なら `winget install Microsoft.PowerShell` で導入してください。setup.ps1 は冒頭でバージョン検査し、未満なら MSG-DW-011 を出力して中止します。

---

## ローカル品質ゲート（lefthook / just）

`scripts/setup.sh` / `setup.ps1` 実行後、以下のフックが `.git/hooks/` に配線されます（`lefthook install` が担当）。

| hook | 実行コマンド | 失敗時の挙動 |
|------|------------|-----------|
| `pre-commit` | `just fmt-check` / `just clippy` / `just audit-secrets`（parallel） | 該当失敗の MSG を stderr に出してコミット中止 |
| `commit-msg` | `just commit-msg-check <file>` / `just commit-msg-no-ai-footer <file>`（parallel） | Conventional Commits 非準拠 / AI 生成フッター検出でコミット中止 |
| `pre-push` | `just test` | テスト失敗で push 中止 |

失敗メッセージは常に以下の 2 行構造です:

```
[FAIL] <何が失敗したか 1 文で要約>
次のコマンド: <実行すべき復旧コマンド 1 つ>
```

### `--no-verify` バイパスの扱い

`git commit --no-verify` / `git push --no-verify` によるフックバイパスは**本規約で原則禁止**です。

> `[WARN] --no-verify の使用は規約で原則禁止です。`
> `PR 本文に理由を明記してください。CI が同一チェックを再実行します。`

やむを得ずバイパスした場合、PR 本文に理由を明記してください。CI 側で同一チェックを独立に再実行するため、バイパスしても PR は統合ゲートで同じ判定を受けます（DRY: `justfile` が単一真実源）。

---

## Secret 混入時の緊急対応

`gitleaks` / `audit-secret-paths.sh` が secret（AWS キー / API トークン / 秘密鍵 / `.env` 混入等）を検出した場合:

### ローカルで pre-commit が弾いた場合

1. 指摘された行を該当ファイルから除去、または対象ファイルを `.gitignore` に追加
2. `git reset HEAD <file>` でステージ解除 → 修正 → `git add -p` で差分単位に再確認
3. `just audit-secrets` で単独再検証
4. 再コミット

### 既に commit / push してしまった場合

1. **直ちに対象 secret を無効化する**（AWS キーなら IAM で失効、API トークンなら発行元で rotate）
2. リポジトリ管理者（`@kkm-horikawa`）に Issue **非公開**で連絡。SECURITY.md の手順に従う
3. 履歴からの除去は [`git filter-repo`](https://github.com/newren/git-filter-repo) を使う。`git filter-branch` は使わない
4. force push は管理者のみ。共同作業者全員に `git reset --hard origin/<branch>` の再同期を依頼

**重要**: `git push --force` で履歴を書き換えても、GitHub の push-back リフで既に外部にコピーされている可能性があります。secret の無効化 (1) を必ず最優先してください。

---

## コーディング規約

- **Clean Architecture / SOLID**: トップダウンに読めるコード。責務はクラス・モジュールに閉じる
- **Fail Fast**: 不正な入力・状態は早期に失敗させる。エラーを `_` で握り潰さない
- **秘密情報**: `secrecy` crate の `Secret<T>` でラップ。スコープ終了時に `zeroize` で消去
- **unsafe**: 原則禁止。使用する場合は `// SAFETY:` コメントで根拠を明示
- **公開 API**: `pub` は最小限。クレート外に公開する必要がない関数は `pub(crate)` または非公開

詳細は各クレートの README および設計書（`docs/architecture/`）を参照してください。
