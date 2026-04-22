# テスト設計書 — repo-setup（リポジトリ整備）

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | repo-setup（GitFlow ブランチ整備・リポジトリガバナンスファイル整備） |
| 対象ブランチ | `feature/repo-setup` |
| 設計根拠 | `docs/architecture/dev.md` §7（GitFlow ブランチ戦略・保護ルール）|
| テスト実行タイミング | `feature/repo-setup` → `develop` へのマージ前、および `develop` → `main` マージ前 |

## 2. テスト対象と受入基準

| 受入基準ID | 受入基準 | 検証レベル |
|-----------|---------|-----------|
| REPO-01 | 全対象ファイルがリポジトリルートまたは所定パスに存在する | ユニット |
| REPO-02 | ブランチ保護ルールが §7.2 の仕様通りに GitHub に設定されている | 結合 |
| REPO-03 | LICENSE に MIT 文言と著作権表記が含まれる | ユニット |
| REPO-04 | README.md にインストール手順・権限要件・使い方が含まれる | ユニット |
| REPO-05 | `develop` ブランチが存在し、`feature/*` のデフォルトマージ先が `develop` である | 結合 |
| REPO-06 | `.github/` テンプレート群が存在し、Issue/PR 作成時に自動適用される | E2E |

## 3. テストマトリクス（トレーサビリティ）

| テストID | 受入基準ID | 検証対象ファイル/設定 | テストレベル | 種別 |
|---------|-----------|---------------------|------------|------|
| TC-U01 | REPO-01 | `README.md` の存在 | ユニット | 正常系 |
| TC-U02 | REPO-01 | `LICENSE` の存在 | ユニット | 正常系 |
| TC-U03 | REPO-01 | `CONTRIBUTING.md` の存在 | ユニット | 正常系 |
| TC-U04 | REPO-01 | `SECURITY.md` の存在 | ユニット | 正常系 |
| TC-U05 | REPO-01 | `CODEOWNERS` の存在 | ユニット | 正常系 |
| TC-U06 | REPO-01 | `.github/ISSUE_TEMPLATE/*.yml` の存在（1件以上）| ユニット | 正常系 |
| TC-U07 | REPO-01 | `.github/pull_request_template.md` の存在 | ユニット | 正常系 |
| TC-U08 | REPO-03 | `LICENSE` に「MIT License」文言を含む | ユニット | 正常系 |
| TC-U09 | REPO-03 | `LICENSE` に著作権年・著作者名を含む（`Copyright`） | ユニット | 正常系 |
| TC-U10 | REPO-04 | `README.md` にインストール手順セクションを含む | ユニット | 正常系 |
| TC-U11 | REPO-04 | `README.md` に権限要件の記載を含む | ユニット | 正常系 |
| TC-U12 | REPO-04 | `README.md` に使い方（Usage）の記載を含む | ユニット | 正常系 |
| TC-U13 | REPO-01 | `CONTRIBUTING.md` に GitFlow ブランチモデルの記載を含む | ユニット | 正常系 |
| TC-U14 | REPO-01 | `SECURITY.md` に脆弱性報告窓口の記載を含む | ユニット | 正常系 |
| TC-U15 | REPO-01 | `CODEOWNERS` に `docs/architecture/` の責務割当を含む | ユニット | 正常系 |
| TC-I01 | REPO-02 | `main` ブランチ保護: PR 必須・2名レビュー・status checks・force push 禁止 | 結合 | 正常系 |
| TC-I02 | REPO-02 | `develop` ブランチ保護: PR 必須・1名レビュー・status checks・force push 禁止 | 結合 | 正常系 |
| TC-I03 | REPO-02 | `main` ブランチ: signed commits 必須設定 | 結合 | 正常系 |
| TC-I04 | REPO-02 | `develop` ブランチ: signed commits 必須設定 | 結合 | 正常系 |
| TC-I05 | REPO-02 | `main`: PR ソース制限（`release/*` / `hotfix/*` のみ許可する branch-policy ワークフローが存在する） | 結合 | 正常系 |
| TC-I06 | REPO-05 | `develop` ブランチが存在する | 結合 | 正常系 |
| TC-I07 | REPO-02 | `main` / `develop`: include_administrators = true（管理者も保護ルール適用） | 結合 | 正常系 |
| TC-E01 | REPO-01, REPO-03, REPO-04 | 新規コントリビューターが必要情報を取得できるシナリオ | E2E | 正常系 |
| TC-E02 | REPO-05, REPO-06 | コアメンバーが GitFlow に従い develop へ PR を作成できるシナリオ | E2E | 正常系 |
| TC-E03 | REPO-02 | `feature/*` から `main` への直接 PR が branch-policy チェックで失敗するシナリオ | E2E | 異常系 |

## 4. E2Eテスト設計（受入基準 REPO-01, 02, 05, 06 の振る舞い検証）

> **ツール選択根拠**: このシステムは GitHub リポジトリそのもの（CLI/公開 API が主インターフェース）。
> Playwright は不要。`gh` CLI + `gh api` でブラックボックス検証する。

### TC-E01: 新規コントリビューターのオンボーディングシナリオ

**ペルソナ**: 外部コントリビューター（プロジェクトにはじめて貢献しようとする開発者）

| 項目 | 内容 |
|------|------|
| テストID | TC-E01 |
| 対応する受入基準ID | REPO-01, REPO-03, REPO-04 |
| 対応する工程 | 要件定義（キャプテン・アメリカからの要求） |
| 種別 | 正常系 |
| 前提条件 | `feature/repo-setup` が `develop` にマージ済み。GitHub 上でリポジトリが公開状態 |
| 操作 | 1. `gh repo view shikomi-dev/shikomi` でリポジトリ概要を取得<br>2. `gh api repos/shikomi-dev/shikomi/contents/LICENSE` で LICENSE を確認<br>3. `gh api repos/shikomi-dev/shikomi/contents/README.md` で README を確認<br>4. `gh api repos/shikomi-dev/shikomi/contents/CONTRIBUTING.md` で CONTRIBUTING を確認<br>5. `gh api repos/shikomi-dev/shikomi/contents/SECURITY.md` で SECURITY を確認 |
| 期待結果 | - 全ファイルの取得が成功する（HTTP 200、content フィールドに base64 データあり）<br>- LICENSE の内容に「MIT License」と「Copyright」を含む<br>- README の内容にインストール手順・権限要件・使い方のキーワードを含む<br>- CONTRIBUTING の内容に GitFlow / feature / develop のキーワードを含む<br>- SECURITY の内容に脆弱性報告（security / vulnerability / report 等）のキーワードを含む |

### TC-E02: コアメンバーの GitFlow ワークフローシナリオ

**ペルソナ**: コアメンバー（定常的に開発する内部メンバー）

| 項目 | 内容 |
|------|------|
| テストID | TC-E02 |
| 対応する受入基準ID | REPO-05, REPO-06 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `develop` ブランチが存在する。`.github/pull_request_template.md` が存在する |
| 操作 | 1. `gh api repos/shikomi-dev/shikomi/branches/develop` で develop ブランチの存在確認<br>2. `gh api repos/shikomi-dev/shikomi/contents/.github/pull_request_template.md` で PR テンプレート取得<br>3. `gh api repos/shikomi-dev/shikomi/contents/.github/ISSUE_TEMPLATE` で Issue テンプレート一覧取得 |
| 期待結果 | - develop ブランチが HTTP 200 で取得できる<br>- PR テンプレートが取得でき、content が空でない<br>- Issue テンプレートが 1 件以上存在する（配列が空でない） |

### TC-E03: main への直接 PR がブランチポリシーで拒否されるシナリオ

**ペルソナ**: コアメンバー（誤って main に直接 PR を作成しようとした）

| 項目 | 内容 |
|------|------|
| テストID | TC-E03 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 要件定義 |
| 種別 | 異常系 |
| 前提条件 | `.github/workflows/branch-policy.yml` が存在する。`main` ブランチ保護が設定済み |
| 操作 | 1. `gh api repos/shikomi-dev/shikomi/contents/.github/workflows/branch-policy.yml` でワークフローファイルの存在確認<br>2. ワークフロー内容に PR の source branch を検査する記述が含まれることを確認<br>3. `gh api repos/shikomi-dev/shikomi/branches/main/protection` で保護設定を確認し、required_status_checks に branch-policy が含まれることを確認 |
| 期待結果 | - `branch-policy.yml` が存在し、内容に `github.base_ref == 'main'` かつ source branch 検査の記述を含む<br>- `main` の保護設定の required_status_checks に branch-policy 相当のチェックが含まれる |

## 5. 結合テスト設計（GitHub API による設定値の契約検証）

> **検証スタイル**: `gh api` で実際の GitHub 設定値を取得し、§7.2 の仕様と具体的に照合する（contract testing）。

### TC-I01: main ブランチ保護ルール確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I01 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `main` ブランチ保護が GitHub 上で設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/main/protection` を実行 |
| 期待結果 | - `required_pull_request_reviews.required_approving_review_count` == 2<br>- `required_pull_request_reviews.require_code_owner_reviews` == true<br>- `required_pull_request_reviews.dismiss_stale_reviews` == true<br>- `required_status_checks.checks` に `lint`, `unit-core`, `test-infra`, `audit`, `build-preview`, `back-merge-check` が含まれる<br>- `enforce_admins.enabled` == true<br>- `allow_force_pushes.enabled` == false<br>- `allow_deletions.enabled` == false |

### TC-I02: develop ブランチ保護ルール確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I02 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `develop` ブランチ保護が GitHub 上で設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/develop/protection` を実行 |
| 期待結果 | - `required_pull_request_reviews.required_approving_review_count` == 1<br>- `required_pull_request_reviews.require_code_owner_reviews` == true<br>- `required_status_checks.checks` に `lint`, `unit-core`, `test-infra`, `audit` が含まれる<br>- `enforce_admins.enabled` == true<br>- `allow_force_pushes.enabled` == false<br>- `allow_deletions.enabled` == false |

### TC-I03: main ブランチの signed commits 必須確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I03 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `main` ブランチ保護が設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/main/protection/required_signatures` を実行 |
| 期待結果 | `enabled` == true |

### TC-I04: develop ブランチの signed commits 必須確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I04 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `develop` ブランチ保護が設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/develop/protection/required_signatures` を実行 |
| 期待結果 | `enabled` == true |

### TC-I05: branch-policy ワークフローの存在確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I05 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `feature/repo-setup` → `develop` マージ済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/contents/.github/workflows/branch-policy.yml` |
| 期待結果 | HTTP 200、content に `github.base_ref` の検査ロジックを含む |

### TC-I06: develop ブランチの存在確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I06 |
| 対応する受入基準ID | REPO-05 |
| 対応する工程 | 基本設計（dev.md §7.1） |
| 種別 | 正常系 |
| 前提条件 | なし |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/develop` |
| 期待結果 | HTTP 200、`name` == `"develop"` |

### TC-I07: 管理者保護ルール適用確認（include_administrators）

| 項目 | 内容 |
|------|------|
| テストID | TC-I07 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `main` / `develop` 保護設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/main/protection` および `develop/protection` |
| 期待結果 | `enforce_admins.enabled` == true（両ブランチとも） |

## 6. ユニットテスト設計（ファイル存在・内容検証）

> **ツール**: `gh api repos/shikomi-dev/shikomi/contents/{path}` で取得し base64 デコードして文字列検索。
> または git clone 後のローカルファイルを直接確認。

### TC-U01〜TC-U07: ファイル存在確認群

| テストID | 検証対象パス | 期待結果 |
|---------|------------|---------|
| TC-U01 | `README.md` | HTTP 200 / ファイルが存在する |
| TC-U02 | `LICENSE` | HTTP 200 / ファイルが存在する |
| TC-U03 | `CONTRIBUTING.md` | HTTP 200 / ファイルが存在する |
| TC-U04 | `SECURITY.md` | HTTP 200 / ファイルが存在する |
| TC-U05 | `CODEOWNERS` | HTTP 200 / ファイルが存在する |
| TC-U06 | `.github/ISSUE_TEMPLATE/` | ディレクトリに `.yml` ファイルが 1 件以上存在する |
| TC-U07 | `.github/pull_request_template.md` | HTTP 200 / ファイルが存在する |

**前提条件**: `feature/repo-setup` が `develop` にマージ済み（または feature ブランチ上で検証）
**操作**: `gh api repos/shikomi-dev/shikomi/contents/{path}?ref=develop`
**対応する受入基準ID**: REPO-01

### TC-U08〜TC-U09: LICENSE 内容確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U08 |
| 対応する受入基準ID | REPO-03 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `LICENSE` が存在する |
| 操作 | `LICENSE` を取得し内容を確認 |
| 期待結果 | 「MIT License」の文字列を含む |

| テストID | TC-U09 |
|---------|--------|
| 期待結果 | 「Copyright」の文字列を含む（著作権年・著作者名が記載されている） |

### TC-U10〜TC-U12: README 必須記載確認

| テストID | 検証キーワード | 目的 | 対応する受入基準ID |
|---------|-------------|------|-----------------|
| TC-U10 | `install` または `インストール` または `## Install` | インストール手順セクションの存在 | REPO-04 |
| TC-U11 | `permission` または `権限` または `0600` または `ACL` | 権限要件の記載 | REPO-04 |
| TC-U12 | `usage` または `使い方` または `## Usage` | 使い方セクションの存在 | REPO-04 |

**前提条件**: `README.md` が存在する
**操作**: README.md を取得し、各キーワードが含まれているか確認（大文字小文字区別なし）

### TC-U13: CONTRIBUTING.md GitFlow 記載確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U13 |
| 対応する受入基準ID | REPO-01 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `CONTRIBUTING.md` が存在する |
| 操作 | `CONTRIBUTING.md` を取得し内容確認 |
| 期待結果 | 「feature」「develop」「GitFlow」または「ブランチ」のいずれかを含む |

### TC-U14: SECURITY.md 脆弱性報告窓口確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U14 |
| 対応する受入基準ID | REPO-01 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `SECURITY.md` が存在する |
| 操作 | `SECURITY.md` を取得し内容確認 |
| 期待結果 | 「report」または「報告」または「vulnerability」または「脆弱性」を含む |

### TC-U15: CODEOWNERS 責務割当確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U15 |
| 対応する受入基準ID | REPO-01 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `CODEOWNERS` が存在する |
| 操作 | `CODEOWNERS` を取得し内容確認 |
| 期待結果 | `docs/architecture/` のパターンに対する所有者割当が存在する |

## 7. モック方針

| 検証対象 | モック要否 | 理由 |
|---------|---------|------|
| GitHub API（ブランチ保護・ファイル取得） | **不要**（本物を使用） | このテストの目的は「GitHub 上に正しく設定されているか」の確認。モックでは意味をなさない |
| ファイルの内容 | **不要** | リポジトリの実ファイルを直接確認する |
| GitHub Actions の実行 | **対象外** | branch-policy.yml の動作は TC-E03 で「ファイルの存在と内容」として確認する。CI 実行そのものはテスト設計の対象外 |

外部依存はすべて本物を使用する。assumed mock は禁止。

## 8. 実行手順と証跡

### 実行環境

- `gh` CLI（認証済み、`GH_TOKEN` 設定済み）
- `jq` によるレスポンス解析
- `python3` または `bash` によるファイル内容検査

### 実行コマンド例（結合テスト TC-I01）

```bash
# main ブランチ保護ルール確認
gh api repos/shikomi-dev/shikomi/branches/main/protection | jq '{
  required_approving_review_count: .required_pull_request_reviews.required_approving_review_count,
  require_code_owner_reviews: .required_pull_request_reviews.require_code_owner_reviews,
  dismiss_stale_reviews: .required_pull_request_reviews.dismiss_stale_reviews,
  enforce_admins: .enforce_admins.enabled,
  allow_force_pushes: .allow_force_pushes.enabled,
  allow_deletions: .allow_deletions.enabled,
  required_status_checks: [.required_status_checks.checks[].context]
}'
```

### 実行コマンド例（ユニットテスト TC-U08 〜 TC-U12）

```bash
# LICENSE 内容確認
gh api repos/shikomi-dev/shikomi/contents/LICENSE | jq -r '.content' | base64 -d | grep -c "MIT License"

# README 必須記載確認
gh api repos/shikomi-dev/shikomi/contents/README.md | jq -r '.content' | base64 -d | grep -iE "install|インストール"
gh api repos/shikomi-dev/shikomi/contents/README.md | jq -r '.content' | base64 -d | grep -iE "permission|権限|0600|ACL"
gh api repos/shikomi-dev/shikomi/contents/README.md | jq -r '.content' | base64 -d | grep -iE "usage|使い方"
```

### 証跡

- テスト実行結果（stdout/stderr/exit code）を Markdown で記録
- 結果ファイルを `/app/shared/attachments/マユリ/` に保存して Discord に添付
- ブランチ保護設定の JSON レスポンスを証跡として保存

## 9. カバレッジ基準

| 観点 | 基準 |
|------|------|
| 受入基準の網羅 | REPO-01 〜 REPO-06 が全テストケースで網羅されていること |
| 正常系 | 全ケース必須 |
| 異常系 | TC-E03（branch-policy による拒否）を必須で確認 |
| 境界値 | ISSUE_TEMPLATE が 0 件の場合（TC-U06 異常系）は必要に応じて追加 |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
