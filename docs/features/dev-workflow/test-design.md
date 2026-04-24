# テスト設計書

<!-- feature: dev-workflow / Issue #22 / PR #23 (設計) + PR #24 (実装) -->
<!-- 配置先: docs/features/dev-workflow/test-design.md -->
<!-- 対象範囲: REQ-DW-001〜018 / MSG-DW-001〜013 / 脅威 T1〜T9 / 受入基準 1〜17 -->

本 feature は Rust コードを追加せず、設定ファイル（`lefthook.yml` / `justfile`）とシェル/PowerShell スクリプト（`scripts/setup.sh` / `scripts/setup.ps1` / `scripts/ci/audit-pin-sync.sh`）、CI ワークフロー（`.github/workflows/*.yml` 5 本）と文書（`CONTRIBUTING.md`）で構成される。テスト粒度は「ユニット＝設定/スクリプトの単体契約」「結合＝レシピ/フック間連携」「E2E＝ペルソナシナリオ」で定義する。

## テストマトリクス

| 要件ID | 実装アーティファクト | テストケースID | テストレベル | 種別 | 受入基準 |
|--------|-------------------|---------------|------------|------|---------|
| REQ-DW-001 | `lefthook.yml` / `.git/hooks/` | TC-UT-001 | ユニット | 正常系 | 1 |
| REQ-DW-002 | `lefthook.yml::pre-commit` + `justfile::fmt-check` / `clippy` | TC-IT-001, TC-UT-010 | 結合/ユニット | 正常系/異常系 | 2, 11 |
| REQ-DW-003 | `lefthook.yml::pre-push` + `justfile::test` | TC-IT-002 | 結合 | 異常系 | 3 |
| REQ-DW-004 | `lefthook.yml::commit-msg.convco` + `justfile::commit-msg-check` | TC-IT-003, TC-UT-011 | 結合/ユニット | 正常系/異常系 | 4 |
| REQ-DW-005 | `justfile` 全レシピ | TC-UT-002 | ユニット | 正常系 | 10 |
| REQ-DW-006 | `.github/workflows/{lint,unit-core,test-infra,audit,windows}.yml` | TC-UT-003 | ユニット | 正常系 | 5 |
| REQ-DW-007 | `scripts/setup.sh` | TC-IT-004 | 結合 | 正常系 | 1, 6, 7 |
| REQ-DW-008 | `scripts/setup.ps1` | TC-IT-005 | 結合 | 正常系 | 1, 7 |
| REQ-DW-009 | `scripts/setup.{sh,ps1}` 冪等 | TC-IT-006 | 結合 | 正常系 | 6 |
| REQ-DW-010 | `README.md` / `CONTRIBUTING.md` | TC-UT-004 | ユニット | 正常系 | 9 |
| REQ-DW-011 | `.github/workflows/*.yml` | TC-E2E-005 | E2E | 異常系 | 8 |
| REQ-DW-012 | `lefthook.yml::fail_text` 全箇所 | TC-UT-005 | ユニット | 正常系 | 11 |
| REQ-DW-013 | `lefthook.yml::pre-commit.audit-secrets` + `justfile::audit-secrets` | TC-IT-007, TC-UT-012 | 結合/ユニット | 異常系 | 12 |
| REQ-DW-014 | `scripts/setup.ps1` PS7 検査 | TC-IT-008 | 結合 | 異常系 | 13 |
| REQ-DW-015 | `scripts/setup.{sh,ps1}` SHA256 検証 | TC-IT-009, TC-UT-013 | 結合/ユニット | 異常系 | 14 |
| REQ-DW-016 | `.github/CODEOWNERS` | TC-UT-006 | ユニット | 正常系 | 15 |
| REQ-DW-017 | `CONTRIBUTING.md §Secret 混入時の緊急対応` | TC-UT-007 | ユニット | 正常系 | 16 |
| REQ-DW-018 | `lefthook.yml::commit-msg.no-ai-footer` + `justfile::commit-msg-no-ai-footer` | TC-IT-010, TC-UT-014〜016 | 結合/ユニット | 異常系/正常系 | 17 |
| REQ-DW-006（追加契約） | `scripts/ci/audit-pin-sync.sh` | TC-UT-008, TC-UT-009 | ユニット | 正常系/異常系 | — |
| T9 補助 | ピン定数 upstream 同期 | TC-UT-017 | ユニット | 正常系 | — |

## 外部I/O依存マップ

| 外部I/O | 用途 | raw fixture | factory | characterization状態 |
|--------|-----|------------|---------|---------------------|
| GitHub Releases `evilmartians/lefthook v2.1.6` checksums + 全プラットフォーム `.gz` | setup.sh / setup.ps1 のピン値照合対象 | `tests/fixtures/characterization/raw/lefthook_releases_v2.1.6_checksums.json`（要起票） | — | **要起票 (Issue TBD-1)**：upstream の実 SHA256 を 5 プラットフォーム分 `gh release view` で取得して固定し、ピン転記ミスを CI で検出可能にする |
| GitHub Releases `gitleaks/gitleaks v8.30.1` checksums | 同上 | `tests/fixtures/characterization/raw/gitleaks_releases_v8.30.1_checksums.json`（要起票） | — | **要起票 (Issue TBD-2)** |
| `convco` CLI 出力（`--help` / サブコマンド一覧） | commit-msg フック用サブコマンドが実在するかの契約 | `tests/fixtures/characterization/raw/convco_v0.6.3_help.txt`（要起票） | — | **要起票 (Issue TBD-3)**：PR #24 で `convco check-message` と存在しないサブコマンドを呼んでいるバグ（BUG-3）の根本要因。公式 `--help` を固定しないと仕様書の誤引用が実装に流れ込む |
| `gitleaks protect --staged` のデフォルトルール挙動 | secret 混入検知の精度基準 | `tests/fixtures/characterization/raw/gitleaks_default_rules_v8.30.1.json`（要起票） | `tests/factories/secret_sample.py`（要起票、言語ヒアリング要） | **要起票 (Issue TBD-4)**：AWS `AKIAIOSFODNN7EXAMPLE` は allowlist 扱い、実在パターン（AKIA + 16 桁 + secret 40 桁）のみ reject する挙動を実観測で固定 |
| Git 実コマンド（`git commit` / `git push` / `git filter-repo`） | lefthook 経由のフック連携 | `tests/fixtures/characterization/raw/git_hook_invocation.txt`（任意） | — | 済（Git 標準仕様、大きな変動なし。ただしバージョン依存があれば要起票） |
| crates.io（`cargo install --locked just / convco / cargo-deny`） | setup / CI のツール導入経路 | 不要（公式 registry、`--locked` で再現性担保） | — | 不要（lockfile で固定、Cargo 側契約） |
| `pwsh` (PowerShell 7+) の `$PSVersionTable` / `Invoke-WebRequest` / `Get-FileHash` | setup.ps1 動作 | — | `tests/factories/powershell_version.py`（要起票） | **要起票 (Issue TBD-5)**：Windows 10 21H2 既定 5.1 / 7.x それぞれの `$PSVersionTable` 出力形式を固定 |
| `uname -s` / `uname -m` | setup.sh `detect_platform()` | — | `tests/factories/platform_stub.sh`（要起票） | **要起票 (Issue TBD-6)**：5 プラットフォーム × aarch64/x86_64 の正規組み合わせと、未サポート条件の境界 |

**空欄（要起票）の扱い**: 上記 Issue TBD-1〜6 が起票・完了するまで、該当項目に関わる unit/integration は「assumed mock」を禁じる。外部観測値に代わる raw fixture が未整備のまま unit を書くと、本 PR #24 の `convco check-message` 呼び出しのような**仕様誤引用に対する検出力ゼロ**のテストになる（まさに今回の実害）。

## E2Eテストケース

「開発者ペルソナの受入基準 1〜17 をブラックボックスで検証する」層。DB 直接確認・内部状態参照・テスト用裏口は禁止。本 feature は CLI/Git 操作が主なので、Playwright ではなく **bash/pwsh スクリプト + 実コミット発行**で検証する。証跡として stdout/stderr/exit code と `.git/hooks/` 内の生成物を保存する。

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---------|---------|---------|---------|---------|
| TC-E2E-001 | 新田 圭介（Linux x86_64 新規参画者） | clone 直後の setup → 通常コミット成功（受入基準 1, 2, 10） | 1. `git clone` 空ディレクトリ 2. `bash scripts/setup.sh` 3. `just --list` 4. 通常ファイル編集 → `git commit -m "feat(x): add"` | 1〜3. exit 0、MSG-DW-005 表示、14 レシピ全てに 1 行説明 4. コミット成功（pre-commit 全 3 検査 pass + commit-msg 2 検査 pass） |
| TC-E2E-002 | 新田 圭介 | fmt 違反コミットを pre-commit が遮断（受入基準 2, 11） | 1. `*.rs` に fmt 違反を意図的に挿入 2. `git add` → `git commit -m "feat(x): break fmt"` | exit 非 0、stderr 末尾に**静的 2 行構造**で `[FAIL] cargo fmt 違反を検出しました。` / `次のコマンド: just fmt` が出力（MSG-DW-001 確定文言完全一致） |
| TC-E2E-003 | 新田 圭介 | Conventional Commits 違反を commit-msg が遮断（受入基準 4, 11） | メッセージ本文を `random nonsense` として `git commit -m "random nonsense"` | exit 非 0、stderr に MSG-DW-004 が 2 行構造で表示。**lefthook のログではなく convco 側の usage error ではないこと**（現 PR #24 では convco usage error が混入している、BUG-3） |
| TC-E2E-004 | 新田 圭介 | テスト失敗を pre-push が遮断（受入基準 3） | 1. `cargo test` で落ちる変更を入れて `git commit --no-verify` 2. `git push` | push 拒否、stderr に MSG-DW-003 |
| TC-E2E-005 | 倉田 美保（レビュワー） | `--no-verify` バイパスを CI 側再実行で検知（受入基準 8） | 1. fmt 違反を `--no-verify` でコミット 2. `git push --no-verify` 3. GitHub Actions の `lint.yml` 結果を確認 | `lint.yml` job が `just fmt-check` ステップで exit 非 0 になり PR チェックが赤 |
| TC-E2E-006 | 新田 圭介 | `setup.sh` の 2 回連続実行で差分が発生しない（受入基準 6） | 1 回目 setup → 2 回目 setup を連続実行 | 2 回目も exit 0、`[SKIP] <tool> は既にインストール済みです` を各ツールで表示 |
| TC-E2E-007 | Windows 開発者（非 PowerShell 7） | PowerShell 5.1 起動で即 Fail Fast（受入基準 13） | Windows 10 21H2 既定 `powershell.exe` で `.\scripts\setup.ps1` | exit 非 0、MSG-DW-011 表示、`winget install Microsoft.PowerShell` 案内 |
| TC-E2E-008 | 新田 圭介 | secret 混入コミットを pre-commit が遮断（受入基準 12） | `AKIAZ5KQ3ZXNGR4T4BXK` 相当の実在パターン + 40 桁 secret を staged → `git commit` | exit 非 0、MSG-DW-010 表示、gitleaks 側 stdout に file:line 出力 |
| TC-E2E-009 | 新田 圭介 | SHA256 改ざんバイナリを setup が拒否（受入基準 14） | setup.sh 冒頭のピン定数を意図的に 1 文字ズラして再実行（`lefthook` 未導入状態で） | exit 非 0、MSG-DW-012 表示、一時ファイル削除（実装上 `trap RETURN` の発火条件も検証） |
| TC-E2E-010 | Agent-C（Claude Code） | AI 生成フッター付きコミットを commit-msg が遮断（受入基準 17、3 パターン） | 3 ケース個別: (a) `🤖 Generated with [Claude Code](...)` (b) `Co-Authored-By: Claude <noreply@anthropic.com>` (c) `Co-Authored-By: Claude Opus 4.7 <...>` | 3 ケースとも exit 非 0、MSG-DW-013 stderr 表示 |
| TC-E2E-011 | Agent-C 境界（body 位置の Claude 言及） | `Claude Shannon` を body 位置で引用した正規コミット | `feat(x): cite Claude Shannon in info theory` | exit 0 でコミット成功（P3 の `Co-Authored-By:` 接頭辞必須契約） |

## 結合テストケース

「フック配線 × レシピ呼び出し」層。lefthook の `.git/hooks/` ラッパが justfile レシピを呼び、期待通り exit code / stderr を返すかを検証する。外部 API（GitHub Releases / crates.io）は**実接続**ではなく raw fixture を使用。

| テストID | 対象モジュール連携 | 使用 raw fixture | 前提条件 | 操作 | 期待結果 |
|---------|------------------|----------------|---------|------|---------|
| TC-IT-001 | `lefthook::pre-commit` → `justfile::fmt-check` | — | fmt 違反ファイルを staged | `git commit -m "feat: x"` | exit 非 0、lefthook が fmt-check/clippy/audit-secrets を parallel 実行、MSG-DW-001 が 2 行構造で出力 |
| TC-IT-002 | `lefthook::pre-push` → `justfile::test` | — | 落ちるテストを含む commit | `git push` | exit 非 0、MSG-DW-003 stderr |
| TC-IT-003 | `lefthook::commit-msg.convco` → `justfile::commit-msg-check` → `convco` | `convco_v0.6.3_help.txt`（要起票、現 PR #24 では characterization 未実施） | 正規 Conventional Commits メッセージ | `git commit -m "feat(x): valid"` | **convco CLI がその引数形式を受理し exit 0 を返すこと**。`unrecognized subcommand` が混入しないこと（BUG-3 再発防止） |
| TC-IT-004 | `scripts/setup.sh` 全 step（Linux x86_64） | `lefthook_releases_v2.1.6_checksums.json` / `gitleaks_releases_v8.30.1_checksums.json` | 空の作業ディレクトリ + `.git/` + rustc/cargo 済 | `bash scripts/setup.sh` | exit 0、MSG-DW-005、`~/.cargo/bin/{just,convco,lefthook,gitleaks}` 配置、`.git/hooks/{pre-commit,pre-push,commit-msg}` 配線、**正規バイナリが SHA256 検証を pass する**（BUG-1 再発防止、対象プラットフォーム 5 種全て） |
| TC-IT-005 | `scripts/setup.ps1` 全 step（Windows PowerShell 7+） | 同上 | 同上 | `pwsh scripts/setup.ps1` | 同上 |
| TC-IT-006 | `setup.{sh,ps1}` 冪等 | 同上 | 1 回目 setup 済 | 2 回目 setup を連続実行 | 4 ツール全てで MSG-DW-006 表示、exit 0 |
| TC-IT-007 | `lefthook::pre-commit.audit-secrets` → `justfile::audit-secrets` → `gitleaks` + `audit-secret-paths.sh` | `gitleaks_default_rules_v8.30.1.json` + secret factory | 実在パターンの AWS/API トークンを staged | `git commit` | exit 非 0、MSG-DW-010 |
| TC-IT-008 | `setup.ps1` step 0（PS7 検査） | `powershell_version` factory | `$PSVersionTable.PSVersion.Major = 5` | `setup.ps1` 起動 | exit 非 0、MSG-DW-011、以降 step 非実行 |
| TC-IT-009 | `setup.{sh,ps1}` SHA256 検証の改ざん拒否 | 改ざん `.gz` を模した raw fixture | `lefthook` 未導入 + ピン定数を正値に戻す | `setup.sh`（ダウンロード成果物の 1 byte を書換えて検証関数を直接呼ぶ、または外部ミラーの拒否を模擬） | exit 非 0、MSG-DW-012、一時ファイル削除 |
| TC-IT-010 | `lefthook::commit-msg.no-ai-footer` → `justfile::commit-msg-no-ai-footer` | — | 3 パターン個別の AI フッター付き COMMIT_EDITMSG | `git commit -m "..."` | 3 パターンとも exit 非 0、MSG-DW-013 |

## ユニットテストケース

「静的設定ファイル・スクリプト単体の契約」層。factory 経由で入力バリエーションを網羅する。入力は factory（raw fixture 直読は[却下]）。

| テストID | 対象 | 種別 | 入力（factory） | 期待結果 |
|---------|-----|------|---------------|---------|
| TC-UT-001 | `lefthook.yml` 構造 | 正常系 | YAML parser | `pre-commit.parallel: true` / `commit-msg.parallel: true` / `pre-commit.commands.{fmt-check,clippy,audit-secrets}.run == "just <name>"` / `commit-msg.commands.{convco,no-ai-footer}.run == "just commit-msg-check {1}"` 等、キー構造が detailed-design §lefthook キー構造表と完全一致 |
| TC-UT-002 | `justfile` レシピ全 14 本 | 正常系 | `just --summary` / `just --list` 出力 | 14 レシピ名が `default / fmt / fmt-check / clippy / test / test-core / test-infra / test-cli / audit / audit-secrets / audit-pin-sync / check-all / commit-msg-check / commit-msg-no-ai-footer` と完全一致、各レシピに**有意な 1 行説明**が付与（`test-core` / `test-infra` / `test-cli` に要約コメント欠落は BUG-4） |
| TC-UT-003 | `.github/workflows/*.yml` 5 本 | 正常系 | YAML parser | `lint.yml` / `unit-core.yml` / `test-infra.yml` / `audit.yml` / `windows.yml` の `run:` 行に直接 `cargo ...` が残っていないこと、`just <recipe>` 呼び出しのみ |
| TC-UT-004 | `CONTRIBUTING.md` / `README.md` | 正常系 | Markdown 目次 | §開発環境セットアップに `bash scripts/setup.sh` / `pwsh scripts/setup.ps1` 1 ステップ表記 + §AI 生成フッターの禁止節の存在 |
| TC-UT-005 | `lefthook.yml::fail_text` 全 5 箇所 | 正常系 | YAML + 文字列照合 | 5 箇所（fmt-check/clippy/audit-secrets/convco/no-ai-footer/test）全てが MSG-DW-001〜004,010,013 確定文言と文字単位で一致、`{variables}` / `{files}` 等の動的展開が含まれない（T7 対策） |
| TC-UT-006 | `.github/CODEOWNERS` | 正常系 | grep | `/lefthook.yml` / `/justfile` / `/scripts/setup.sh` / `/scripts/setup.ps1` / `/scripts/ci/` の 5 パスが `@kkm-horikawa` 所有で登録 |
| TC-UT-007 | `CONTRIBUTING.md §Secret 混入時の緊急対応` | 正常系 | Markdown 節抽出 + 3 項目 grep | (a) 該当キーを発行元で即 revoke (b) **`git filter-repo --path <file> --invert-paths` の具体コマンド + feature ブランチ限定 force-push + `main`/`develop` への force-push 禁止の明記** (c) **GitHub Support への cache purge 依頼と secret scanning alert の resolve の明記** — 3 項目全てが存在（受入基準 16、現 PR #24 は (b) 具体コマンド・force-push 禁止明記・(c) 全欠落、BUG-2） |
| TC-UT-008 | `audit-pin-sync.sh` positive | 正常系 | setup.sh / setup.ps1 が同期済み | exit 0、`[OK] pin 定数の sh/ps1 同期を確認しました（12 件）` |
| TC-UT-009 | `audit-pin-sync.sh` negative | 異常系 | 12 定数の 1 箇所を意図的に乖離 | exit 1、`[FAIL] <VAR> が setup.sh / setup.ps1 で乖離しています` / 2 ファイルの値が diff 表示 |
| TC-UT-010 | `justfile::fmt-check` 単体 | 異常系 | fmt 違反 factory | exit 非 0（`cargo fmt --all -- --check` の exit code を素通し） |
| TC-UT-011 | `justfile::commit-msg-check` 単体 | 異常系 | convco が受理するメッセージ factory + 受理しないメッセージ factory | convco の実 CLI（`check --from-stdin` or `commit-msg`）が受理する引数形式で呼ばれていること。**`check-message` サブコマンドが実在しない convco 0.6.3 で exit 2 を返さないこと**（BUG-3 根本対策）|
| TC-UT-012 | `justfile::audit-secrets` 単体 | 異常系 | 実在 AWS キーパターン factory | `gitleaks protect --staged --no-banner` が exit 1、`bash scripts/ci/audit-secret-paths.sh` は独立に exit 0（本 feature の改変禁止範囲）|
| TC-UT-013 | `setup.{sh,ps1}::sha256_of / Get-Sha256` + ピン照合 | 異常系 | 改ざんバイナリ factory | SHA256 不一致で exit 1 + MSG-DW-012、一時ファイル削除（bash の `trap RETURN + exit` の既知問題含む、BUG-6 の是正範囲） |
| TC-UT-014 | `justfile::commit-msg-no-ai-footer` P1 | 異常系 | `🤖 + Generated with + Claude` を含むファイル factory（大小文字・改行位置バリエーション） | 全バリエーションで exit 1 |
| TC-UT-015 | 同 P2 | 異常系 | `Co-Authored-By: + @anthropic.com` ドメイン factory（大小文字・トレーラ前後空白バリエーション） | 全バリエーションで exit 1 |
| TC-UT-016 | 同 P3 | 異常系 | `Co-Authored-By: + \bClaude\b` factory（モデル名揺れ / Claude 単体） | 全バリエーションで exit 1。**注記**: `Co-Authored-By: Claude Shannon <...>` も P3 にヒットして reject される（設計意図通り、requirements-analysis L408-409 の境界記述と整合） |
| TC-UT-017 | ピン定数 ↔ upstream checksums 同期 | 正常系 | `lefthook_releases_v2.1.6_checksums.json` / `gitleaks_releases_v8.30.1_checksums.json`（要起票） | 10 SHA256 定数（lefthook 5 + gitleaks 5）が upstream の公式 checksums.txt と**文字単位で一致**（現 PR #24 では `LEFTHOOK_SHA256_LINUX_X86_64` が Windows 用の値に誤転記されており fail、BUG-1 の根本対策） |

## カバレッジ基準

本 feature は Rust コードを持たないため C0/C1 等の伝統的カバレッジ指標は取らない。代わりに以下のトレーサビリティ充足を必須とする:

- REQ-DW-001〜018 の各要件が最低 1 件のテストケース（ユニット/結合/E2E のいずれか）で検証されている
- MSG-DW-001〜013 の 13 文言が全て静的文字列で照合されている（TC-UT-005 + TC-E2E 各種）
- 受入基準 1〜17 の各々が最低 1 件の E2E テストケースで検証されている
- T1〜T9 の各脅威に対する対策が最低 1 件のテストケースで有効性を確認されている

## 人間が動作確認できるタイミング

- CI 統合後: `gh pr checks` / `gh run list` で 5 ワークフロー全てが緑であること
- ローカル: `bash scripts/setup.sh` → `just check-all` → `just --list` の順でワンショット確認
- Windows ローカル: `pwsh scripts/setup.ps1` → `just check-all`

## テストディレクトリ構造（将来）

```
tests/
  fixtures/
    characterization/
      raw/
        lefthook_releases_v2.1.6_checksums.json     # 要起票 TBD-1
        gitleaks_releases_v8.30.1_checksums.json    # 要起票 TBD-2
        convco_v0.6.3_help.txt                      # 要起票 TBD-3
        gitleaks_default_rules_v8.30.1.json         # 要起票 TBD-4
      schema/
        (raw の型 + 統計。factory 設計ソース)
  factories/
    secret_sample.py / platform_stub.sh / powershell_version.py  # 要起票
  e2e/
    (TC-E2E-001〜011 を bash / pwsh で実装、実コミット発行、証跡を /app/shared/attachments/ に出力)
  integration/
    (TC-IT-001〜010。rawfixture を使用)
  unit/
    (TC-UT-001〜017。factory を使用。YAML/bash/PowerShell の単体契約テスト)
```

**ただし言語慣習**: 本 feature は Rust コードを追加しないため上記は**スクリプトテスト**として扱う。Rust crate 側テストとは独立したディレクトリに置く。

## 未決課題・要起票 characterization task

| # | タスク | 起票先 |
|---|-------|--------|
| TBD-1 | lefthook v2.1.6 upstream SHA256 の raw fixture 化 + CI 定期照合 | Issue（本 PR #24 差戻後に着手） |
| TBD-2 | gitleaks v8.30.1 同上 | 同上 |
| TBD-3 | convco v0.6.3 `--help` / サブコマンド一覧の raw fixture 化 | 同上 |
| TBD-4 | gitleaks デフォルトルール allowlist の実観測固定 | 同上 |
| TBD-5 | PowerShell 5.1 / 7.x `$PSVersionTable` 出力の factory 化 | 同上 |
| TBD-6 | `uname -s` / `uname -m` 5 プラットフォーム × 2 arch の factory 化 | 同上 |
