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
| REPO-05 | `develop` ブランチが存在し、`feature/*` のデフォルトマージ先が `develop` であることが CONTRIBUTING.md に明記されている | 結合 + ユニット |
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
| TC-U13 | REPO-01 | `CONTRIBUTING.md` に GitFlow 4ブランチ（feature/develop/release/hotfix）**全て**の記載を含む | ユニット | 正常系 |
| TC-U14 | REPO-01 | `SECURITY.md` に脆弱性報告窓口の記載を含む | ユニット | 正常系 |
| TC-U15 | REPO-01 | `CODEOWNERS` に `docs/architecture/` の責務割当を含む | ユニット | 正常系 |
| TC-U16 | REPO-05 | `CONTRIBUTING.md` に `feature/*` → `develop` のマージ先が明記されている | ユニット | 正常系 |
| TC-I01 | REPO-02 | `main` ブランチ保護: PR 必須・2名レビュー・status checks（branch-policy/pr-title-check含む）・force push 禁止・会話解決必須・bypass list | 結合 | 正常系 |
| TC-I02 | REPO-02 | `develop` ブランチ保護: PR 必須・1名レビュー・status checks（branch-policy/pr-title-check含む）・force push 禁止 | 結合 | 正常系 |
| TC-I03 | REPO-02 | `main` ブランチ: signed commits 必須設定 | 結合 | 正常系 |
| TC-I04 | REPO-02 | `develop` ブランチ: signed commits 必須設定 | 結合 | 正常系 |
| TC-I05 | REPO-02 | `main`: PR ソース制限（`release/*` / `hotfix/*` のみ許可する branch-policy ワークフローが存在する） | 結合 | 正常系 |
| TC-I06 | REPO-05 | `develop` ブランチが存在する | 結合 | 正常系 |
| TC-I07 | REPO-02 | `main` / `develop`: enforce_admins = true + bypass list にセキュリティ緊急例外アクターが設定されている | 結合 | 正常系 |
| TC-I08 | REPO-02 | `main` への直接 PR を許可しない branch-policy ワークフローの静的確認（ファイル存在・ソース制限ロジック） | 結合 | 異常系 |
| TC-I09 | REPO-02 | `main` マージ方法規約の文書化確認（CONTRIBUTING.md に merge commit 使用規約が明記されているか） | 結合 | 正常系 |
| TC-I10 | REPO-02 | `develop` マージ戦略: squash merge + merge commit 許可（rebase merge 禁止） | 結合 | 正常系 |
| TC-I11 | REPO-02 | `main` / `develop`: required_conversation_resolution == true | 結合 | 正常系 |
| TC-I12 | REPO-02 | bypass list にセキュリティ緊急対応アクター（チームまたはユーザー）が 1 件以上設定されている | 結合 | 正常系 |
| TC-I13 | REPO-02 | `.github/workflows/pr-title-check.yml` の存在確認 | 結合 | 正常系 |
| TC-I14 | REPO-02 | `.github/workflows/back-merge-check.yml` の存在確認 | 結合 | 正常系 |
| TC-E01 | REPO-01, REPO-03, REPO-04 | 新規コントリビューターが必要情報を取得できるシナリオ | E2E | 正常系 |
| TC-E02 | REPO-05, REPO-06 | コアメンバーが GitFlow に従い develop へ PR を作成できるシナリオ | E2E | 正常系 |

## 4. E2Eテスト設計（受入基準 REPO-01, 05, 06 の振る舞い検証）

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
| 期待結果 | - 全ファイルの取得が成功する（HTTP 200、content フィールドに base64 データあり）<br>- LICENSE の内容に「MIT License」と「Copyright」を含む<br>- README の内容にインストール手順・権限要件・使い方のキーワードを含む<br>- CONTRIBUTING の内容に feature / develop / release / hotfix の全ワードを含む<br>- SECURITY の内容に脆弱性報告（security / vulnerability / report 等）のキーワードを含む |

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
| 期待結果 | - `required_pull_request_reviews.required_approving_review_count` == 2<br>- `required_pull_request_reviews.require_code_owner_reviews` == true<br>- `required_pull_request_reviews.dismiss_stale_reviews` == true<br>- `required_pull_request_reviews.bypass_pull_request_allowances` に 1 件以上のアクター（teams または users）が設定されている<br>- `required_status_checks.checks` に `lint`, `unit-core`, `test-infra`, `audit`, `build-preview`, `back-merge-check`, `branch-policy`, `pr-title-check` が含まれる<br>- `enforce_admins.enabled` == true<br>- `allow_force_pushes.enabled` == false<br>- `allow_deletions.enabled` == false<br>- `required_conversation_resolution.enabled` == true |

### TC-I02: develop ブランチ保護ルール確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I02 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `develop` ブランチ保護が GitHub 上で設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/develop/protection` を実行 |
| 期待結果 | - `required_pull_request_reviews.required_approving_review_count` == 1<br>- `required_pull_request_reviews.require_code_owner_reviews` == true<br>- `required_status_checks.checks` に `lint`, `unit-core`, `test-infra`, `audit`, `branch-policy`, `pr-title-check` が含まれる<br>- `enforce_admins.enabled` == true<br>- `allow_force_pushes.enabled` == false<br>- `allow_deletions.enabled` == false |

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

### TC-I07: 管理者保護ルール適用 + bypass list 確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I07 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `main` / `develop` 保護設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/main/protection` および `develop/protection` |
| 期待結果 | - `enforce_admins.enabled` == true（両ブランチとも）<br>- `main` の `required_pull_request_reviews.bypass_pull_request_allowances.teams` または `.users` に 1 件以上のアクターが設定されている（セキュリティ hotfix 緊急例外ルート確保） |

### TC-I08: main への直接 PR を拒否する branch-policy の静的確認（旧 TC-E03）

> **分類変更理由**: 「実際に PR を投げて CI が fail する」のは完全 E2E だが、CI 実行は制御外（外部副作用）のため静的ファイル検査として結合テストに分類する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I08 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 異常系 |
| 前提条件 | `.github/workflows/branch-policy.yml` が存在する。`main` ブランチ保護が設定済み |
| 操作 | 1. `gh api repos/shikomi-dev/shikomi/contents/.github/workflows/branch-policy.yml` でワークフローファイル取得<br>2. base64 デコードし内容確認<br>3. `gh api repos/shikomi-dev/shikomi/branches/main/protection` で required_status_checks を確認 |
| 期待結果 | - `branch-policy.yml` が存在し、`github.base_ref == 'main'`（または同等条件）と source branch のパターン検査（`release/*` / `hotfix/*` 以外を拒否）の記述を含む<br>- `main` の保護設定の `required_status_checks.checks` に branch-policy の check context が含まれる |

### TC-I09: main マージ方法規約の文書化確認

> **置換理由**: GitHub はリポジトリ単位でのみマージ方法を設定するため、main 専用の「merge commit のみ」制限は `allow_squash_merge` フラグでは技術的に実現不能（TC-I10 と矛盾する）。main での merge commit 使用は規約として CONTRIBUTING.md に文書化し、TC-I08 の branch-policy による PR ソース制限（`release/*` / `hotfix/*` のみ）と組み合わせることで実質的な運用制約を実現する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I09 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.5） |
| 種別 | 正常系 |
| 前提条件 | `CONTRIBUTING.md` が存在する |
| 操作 | `gh api repos/shikomi-dev/shikomi/contents/CONTRIBUTING.md` を取得し base64 デコードして内容確認 |
| 期待結果 | `grep -iP "merge.commit"` または `grep -i "merge commit"` がマッチする行が 1 件以上存在する（main への PR は merge commit を使用するという規約が文書化されている） |

### TC-I10: develop マージ戦略確認（squash + merge commit 許可）

| 項目 | 内容 |
|------|------|
| テストID | TC-I10 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.5） |
| 種別 | 正常系 |
| 前提条件 | リポジトリ設定が完了済み |
| 操作 | `gh api repos/shikomi-dev/shikomi` でリポジトリ設定を取得（リポジトリ全体設定で squash/merge commit 両方許可かつ rebase 禁止） |
| 期待結果 | - `allow_merge_commit` == true（release/hotfix back-merge に使用）<br>- `allow_squash_merge` == true（feature → develop の squash に使用）<br>- `allow_rebase_merge` == false<br>（§7.5「feature→develop は squash、release/hotfix は merge commit」に準拠） |

> **注意**: GitHub はリポジトリ単位でマージ戦略を設定するため、「develop のみ squash merge」という per-branch 制限は技術的に不可能。TC-I10 は「squash + merge commit 両方が有効（rebase は禁止）」を確認する。feature→develop での squash 使用・release/hotfix→main での merge commit 使用はいずれも TC-I09 で文書化規約として補完する。

### TC-I11: required_conversation_resolution 確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I11 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.2） |
| 種別 | 正常系 |
| 前提条件 | `main` / `develop` 保護設定済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/main/protection` および `develop/protection` |
| 期待結果 | `required_conversation_resolution.enabled` == true（両ブランチとも） |

### TC-I12: bypass list アクター設定確認（セキュリティ緊急例外ルート）

| 項目 | 内容 |
|------|------|
| テストID | TC-I12 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §7.4） |
| 種別 | 正常系 |
| 前提条件 | `main` 保護設定済み、セキュリティ緊急対応チームが GitHub 上に存在する |
| 操作 | `gh api repos/shikomi-dev/shikomi/branches/main/protection` を実行し `bypass_pull_request_allowances` を確認 |
| 期待結果 | `required_pull_request_reviews.bypass_pull_request_allowances.teams` または `.users` に 1 件以上のアクターが存在する<br>（セキュリティ脆弱性 hotfix 時に `enforce_admins=true` を維持しつつ特定チームのみバイパス可能とする §7.4 緊急例外ルートが技術的に実現されている） |

### TC-I13: pr-title-check.yml の存在確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I13 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §3, §7.5） |
| 種別 | 正常系 |
| 前提条件 | `feature/repo-setup` → `develop` マージ済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/contents/.github/workflows/pr-title-check.yml` |
| 期待結果 | HTTP 200。content に Conventional Commits パターン（`feat`/`fix`/`docs` 等）を検証する正規表現の記述を含む |

### TC-I14: back-merge-check.yml の存在確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I14 |
| 対応する受入基準ID | REPO-02 |
| 対応する工程 | 基本設計（dev.md §3, §7.6） |
| 種別 | 正常系 |
| 前提条件 | `feature/repo-setup` → `develop` マージ済み |
| 操作 | `gh api repos/shikomi-dev/shikomi/contents/.github/workflows/back-merge-check.yml` |
| 期待結果 | HTTP 200。content に `develop` への back-merge PR 存在確認ロジック（`release/*` または `hotfix/*` のマージ後に develop への PR を検査する記述）を含む |

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

### TC-U13: CONTRIBUTING.md GitFlow 4ブランチ記載確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U13 |
| 対応する受入基準ID | REPO-01 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `CONTRIBUTING.md` が存在する |
| 操作 | `CONTRIBUTING.md` を取得し内容確認 |
| 期待結果 | 「feature」「develop」「release」「hotfix」の**4ワード全て**を含む（いずれか 1 つでは不合格。GitFlow の全ブランチ種別が説明されていることを保証する） |

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

### TC-U16: CONTRIBUTING.md への feature/* → develop マージ先明記確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U16 |
| 対応する受入基準ID | REPO-05 |
| 対応する工程 | 要件定義 |
| 種別 | 正常系 |
| 前提条件 | `CONTRIBUTING.md` が存在する |
| 操作 | `CONTRIBUTING.md` を取得し内容確認 |
| 期待結果 | `grep -P "feature.*develop"` がマッチする行が 1 件以上存在する。または `grep -P "feature/"` と `grep -P "develop"` が前後 3 行以内に共起する記述が存在する<br>具体的な合格例: `feature/* → develop`、`feature ブランチは develop にマージ`、`feature/xxx を develop にマージ` 等の記述 |

## 7. モック方針

| 検証対象 | モック要否 | 理由 |
|---------|---------|------|
| GitHub API（ブランチ保護・ファイル取得） | **不要**（本物を使用） | このテストの目的は「GitHub 上に正しく設定されているか」の確認。モックでは意味をなさない |
| ファイルの内容 | **不要** | リポジトリの実ファイルを直接確認する |
| GitHub Actions の実行 | **対象外** | branch-policy.yml の動作は TC-I08 で「ファイルの存在と内容」の静的確認として検証する。CI 実行そのものはテスト設計の対象外 |

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
  bypass_actors: .required_pull_request_reviews.bypass_pull_request_allowances,
  enforce_admins: .enforce_admins.enabled,
  allow_force_pushes: .allow_force_pushes.enabled,
  allow_deletions: .allow_deletions.enabled,
  conversation_resolution: .required_conversation_resolution.enabled,
  required_status_checks: [.required_status_checks.checks[].context]
}'
```

### 実行コマンド例（結合テスト TC-I09: merge commit 規約確認）

```bash
# CONTRIBUTING.md の merge commit 規約明記確認
gh api repos/shikomi-dev/shikomi/contents/CONTRIBUTING.md \
  | jq -r '.content' | base64 -d \
  | grep -iP "merge.commit" && echo "PASS" || echo "FAIL"
```

### 実行コマンド例（結合テスト TC-I10: マージ戦略確認）

```bash
# リポジトリ設定のマージ戦略確認（squash + merge commit 両方有効・rebase 禁止）
gh api repos/shikomi-dev/shikomi | jq '{
  allow_merge_commit: .allow_merge_commit,
  allow_squash_merge: .allow_squash_merge,
  allow_rebase_merge: .allow_rebase_merge
}'
```

### 実行コマンド例（結合テスト TC-I13/TC-I14: ワークフロー存在確認）

```bash
# pr-title-check.yml 存在確認
gh api repos/shikomi-dev/shikomi/contents/.github/workflows/pr-title-check.yml \
  | jq '.name'

# back-merge-check.yml 存在確認
gh api repos/shikomi-dev/shikomi/contents/.github/workflows/back-merge-check.yml \
  | jq '.name'
```

### 実行コマンド例（ユニットテスト TC-U08 〜 TC-U13）

```bash
# LICENSE 内容確認
gh api repos/shikomi-dev/shikomi/contents/LICENSE | jq -r '.content' | base64 -d | grep -c "MIT License"

# README 必須記載確認
gh api repos/shikomi-dev/shikomi/contents/README.md | jq -r '.content' | base64 -d | grep -iE "install|インストール"
gh api repos/shikomi-dev/shikomi/contents/README.md | jq -r '.content' | base64 -d | grep -iE "permission|権限|0600|ACL"
gh api repos/shikomi-dev/shikomi/contents/README.md | jq -r '.content' | base64 -d | grep -iE "usage|使い方"

# CONTRIBUTING GitFlow 4ブランチ全確認（全て含む場合のみ合格）
CONTENT=$(gh api repos/shikomi-dev/shikomi/contents/CONTRIBUTING.md | jq -r '.content' | base64 -d)
echo "$CONTENT" | grep -q "feature"  && \
echo "$CONTENT" | grep -q "develop"  && \
echo "$CONTENT" | grep -q "release"  && \
echo "$CONTENT" | grep -q "hotfix"   && echo "PASS" || echo "FAIL"
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
| 異常系 | TC-I08（branch-policy による拒否の静的確認）を必須で確認 |
| 境界値 | ISSUE_TEMPLATE が 0 件の場合（TC-U06 異常系）は必要に応じて追加 |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — TC-E03→TC-I08再分類、TC-I09〜TC-I12追加、TC-U13強化、TC-U16追加、bypass list検証追記*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — TC-I09を矛盾する allow_squash_merge==false から merge commit 規約文書化確認に置換、TC-I01/I02の必須 status checks に branch-policy/pr-title-check 追加、TC-I13/I14（ワークフロー存在確認）新設、TC-U16 期待結果を具体的 grep パターンに変更*
