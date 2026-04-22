# Contributing to shikomi

## 目次

1. [はじめに](#はじめに)
2. [ブランチ戦略（GitFlow）](#ブランチ戦略gitflow)
3. [コミット規約（Conventional Commits）](#コミット規約conventional-commits)
4. [マージ戦略](#マージ戦略)
5. [PR 規約](#pr-規約)
6. [開発環境セットアップ](#開発環境セットアップ)
7. [コーディング規約](#コーディング規約)

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

# 全ワークスペースビルド
cargo build --workspace

# CLI のみ
cargo build -p shikomi-cli

# テスト
cargo nextest run --workspace

# lint / format
cargo fmt --check
cargo clippy -- -D warnings
```

---

## コーディング規約

- **Clean Architecture / SOLID**: トップダウンに読めるコード。責務はクラス・モジュールに閉じる
- **Fail Fast**: 不正な入力・状態は早期に失敗させる。エラーを `_` で握り潰さない
- **秘密情報**: `secrecy` crate の `Secret<T>` でラップ。スコープ終了時に `zeroize` で消去
- **unsafe**: 原則禁止。使用する場合は `// SAFETY:` コメントで根拠を明示
- **公開 API**: `pub` は最小限。クレート外に公開する必要がない関数は `pub(crate)` または非公開

詳細は各クレートの README および設計書（`docs/architecture/`）を参照してください。
