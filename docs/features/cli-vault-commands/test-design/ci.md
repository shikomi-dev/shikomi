# テスト設計書 — cli-vault-commands / CI 検証・配置・証跡

> `index.md` の §2 索引からの分割ファイル。CI 検証ケース、テストファイル配置、実行コマンド、証跡提出方針を扱う。

## 1. CI 検証ケース一覧

服部平次 review の指摘（`expose_secret` 経路監査、panic hook 監査）を踏まえ、**静的監査系 TC を 3 件追加**（TC-CI-013〜015）。

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-001 | 12 | `cargo fmt --check --all` | exit 0 |
| TC-CI-002 | 12 | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| TC-CI-003 | 12 | `cargo deny check` | exit 0 |
| TC-CI-004 | 13 | `cargo test -p shikomi-cli --all-targets` | exit 0、全テスト pass |
| TC-CI-005 | 13 | `cargo llvm-cov -p shikomi-cli --summary-only` | line coverage >= 80% |
| TC-CI-010 | 14 | `grep -E "MVP\s*Phase\s*1\|Phase\s*2" docs/architecture/context/process-model.md` | マッチ行 >= 1 |
| TC-CI-011 | 15 | `find crates/shikomi-cli/src/usecase crates/shikomi-cli/src/presenter crates/shikomi-cli/src/io -type d` がそれぞれ存在 | 3 ディレクトリ存在確認 + `src/lib.rs` 存在確認 |
| TC-CI-012 | 16 | `grep -rn "SqliteVaultRepository" crates/shikomi-cli/src/` の **マッチが `main.rs` と `lib.rs::run` のみ** | `usecase/` `presenter/` `error.rs` `view.rs` `input.rs` に `SqliteVaultRepository` 文字列を含まない |
| **TC-CI-013** | 16（新規、服部平次 ③） | `grep -rn "expose_secret" crates/shikomi-cli/src/` | **マッチ 0 件**（`shikomi-cli/src/` 配下で `SecretString::expose_secret()` を呼ばない契約。Text kind preview は `shikomi_core::Record::text_preview` に委譲） |
| **TC-CI-014** | （新規、服部平次 ①） | `grep -rn 'tracing::' crates/shikomi-cli/src/main.rs crates/shikomi-cli/src/lib.rs` の **panic hook 周辺**（`std::panic::set_hook` を含むブロック内） | panic hook 内に `tracing::error!` / `tracing::warn!` / `tracing::info!` / `tracing::debug!` / `tracing::trace!` が**存在しないこと**を静的に確認（ブロック抽出は手動 or `#[cfg(test)] fn inspect_panic_hook_source()` で文字列検査） |
| **TC-CI-015** | （新規、服部平次 ①） | `grep -rnE '\.payload\(\)\|info\.payload\|PanicHookInfo::payload' crates/shikomi-cli/src/` の **panic hook 内** | panic hook 内で `PanicHookInfo::payload()` を参照しないこと（secret が payload に混入した場合の漏洩回避） |

### 1.1 TC-CI-013〜015 の実装方法

**実装スクリプト案**（`scripts/ci/audit-secret-paths.sh` 相当）:

```bash
#!/usr/bin/env bash
set -euo pipefail

# TC-CI-013: expose_secret 呼び出し 0 件
if grep -rn "expose_secret" crates/shikomi-cli/src/ > /tmp/expose_secret.txt; then
    echo "FAIL TC-CI-013: expose_secret found in crates/shikomi-cli/src/"
    cat /tmp/expose_secret.txt
    exit 1
fi

# TC-CI-014: panic hook 内で tracing 呼び出し禁止
# panic hook のコードブロックを抽出するため、set_hook から次の }) まで awk で切り出す
awk '/std::panic::set_hook/,/\}\)\);/' crates/shikomi-cli/src/main.rs crates/shikomi-cli/src/lib.rs \
    | grep -E "tracing::" && { echo "FAIL TC-CI-014: tracing in panic hook"; exit 1; }

# TC-CI-015: panic hook 内で payload 参照禁止
awk '/std::panic::set_hook/,/\}\)\);/' crates/shikomi-cli/src/main.rs crates/shikomi-cli/src/lib.rs \
    | grep -E "\.payload\(\)|info\.payload|PanicHookInfo::payload" && { echo "FAIL TC-CI-015: payload reference in panic hook"; exit 1; }

echo "PASS: TC-CI-013, TC-CI-014, TC-CI-015"
```

このスクリプトを `cargo test` の前 step として CI job に組み込む（GitHub Actions の `.github/workflows/ci.yml` で `run: bash scripts/ci/audit-secret-paths.sh`）。

**フォールバック**: awk での panic hook ブロック抽出が fragile な場合、`shikomi-cli` のユニットテストから `include_str!("main.rs")` で自ソースを読み込んで文字列検査する Rust ネイティブテストに切り替える（この場合 TC-CI-014/015 は TC-UT 系に移行）。

---

## 2. テスト実装ファイル配置

```
crates/shikomi-cli/
├── Cargo.toml          # [lib] 追加、[dev-dependencies] に assert_cmd, predicates, tempfile, serial_test, (optional) expectrl
├── src/
│   ├── lib.rs          # //! Internal API. Not stable; subject to change.
│   │                   # #[doc(hidden)] pub mod usecase;
│   │                   # #[doc(hidden)] pub mod presenter;
│   │                   # #[doc(hidden)] pub mod io;
│   │                   # #[doc(hidden)] pub mod error;
│   │                   # #[doc(hidden)] pub mod view;
│   │                   # #[doc(hidden)] pub mod input;
│   │                   # #[doc(hidden)] pub mod cli;
│   │                   # pub fn run() -> Result<(), CliError>;  # Composition Root
│   ├── main.rs         # use shikomi_cli::run; fn main() { std::process::exit(run()...) }
│   ├── error.rs        # #[cfg(test)] mod tests { TC-UT-001〜009, 070 }
│   ├── view.rs         # #[cfg(test)] mod tests { TC-UT-010〜012 }
│   ├── input.rs        # ConfirmedRemoveInput の doc-test { TC-UT-110, 111 }
│   ├── cli.rs          # #[cfg(test)] mod tests { TC-UT-090, 091 }
│   ├── usecase/
│   │   ├── mod.rs
│   │   ├── list.rs     # list_records()、結合テストは tests/it_usecase_list.rs 側
│   │   ├── add.rs
│   │   ├── edit.rs
│   │   └── remove.rs
│   ├── presenter/
│   │   ├── mod.rs      # Locale enum + detect_from_lang_env_value tests { TC-UT-080〜085 }
│   │   ├── list.rs     # tests { TC-UT-050〜053 }
│   │   ├── error.rs    # tests { TC-UT-030〜041 }
│   │   ├── success.rs  # tests { TC-UT-060〜062 }
│   │   └── warning.rs  # tests { TC-UT-013, 014 }
│   └── io/
│       ├── mod.rs
│       ├── paths.rs    # tests { TC-UT-020, 022, 023 }
│       └── terminal.rs # TTY 操作のため tests は最小、E2E でカバー
└── tests/
    ├── common/
    │   ├── mod.rs      # fresh_repo(), fixed_time() ヘルパー
    │   └── fixtures.rs # create_encrypted_vault()
    ├── e2e_list.rs           # TC-E2E-001〜003
    ├── e2e_add.rs            # TC-E2E-010〜015
    ├── e2e_edit.rs           # TC-E2E-020〜025
    ├── e2e_remove.rs         # TC-E2E-030〜033（030 は #[ignore]）
    ├── e2e_encrypted.rs      # TC-E2E-040〜041
    ├── e2e_uninitialized.rs  # TC-E2E-050〜052
    ├── e2e_paths.rs          # TC-E2E-060〜062
    ├── e2e_i18n.rs           # TC-E2E-070〜071
    ├── e2e_scenarios.rs      # TC-E2E-100〜102
    ├── it_usecase_list.rs    # TC-IT-001〜003
    ├── it_usecase_add.rs     # TC-IT-010〜013
    ├── it_usecase_edit.rs    # TC-IT-020〜024
    ├── it_usecase_remove.rs  # TC-IT-030, 031, 033
    └── it_usecase_cross.rs   # TC-IT-040, 050
```

**`shikomi-core`** 側の追加:

```
crates/shikomi-core/src/vault/record.rs
  └── pub fn text_preview(&self, max_chars: usize) -> Option<String>
      #[cfg(test)] mod tests { TC-UT-100〜104 }
```

**`shikomi-infra`** 側の追加:

```
crates/shikomi-infra/src/persistence/repository.rs
  └── impl SqliteVaultRepository {
        pub fn from_directory(path: &Path) -> Result<Self, PersistenceError>  # 新規
        pub fn new() -> Result<Self, PersistenceError>  # 既存（内部で resolve + from_directory に委譲へリファクタ）
      }
  └── #[cfg(feature = "test-fixtures")]
      pub mod test_fixtures {
          pub fn create_encrypted_vault(dir: &Path) -> Result<(), anyhow::Error>;
      }
```

各テストファイルの docstring に対応 REQ-ID と Issue 番号を書くこと（テスト戦略ガイド準拠）。

---

## 3. 開発者向け実行手順

### 3.1 全テスト実行

```bash
# 全テスト（ユニット + 結合 + E2E）
cargo test -p shikomi-cli --all-targets

# E2E のみ
cargo test -p shikomi-cli --test 'e2e_*'

# 結合のみ
cargo test -p shikomi-cli --test 'it_usecase_*'

# ユニットのみ（lib テスト + doc-test）
cargo test -p shikomi-cli --lib
cargo test -p shikomi-cli --doc

# 暗号化 vault フィクスチャが必要なテスト（test-fixtures feature 有効化）
cargo test -p shikomi-cli --features "shikomi-infra/test-fixtures"

# 擬似 TTY 必要ケース（CI スキップ、ローカル手動）
cargo test -p shikomi-cli --test e2e_remove -- --ignored

# CI 監査スクリプト
bash scripts/ci/audit-secret-paths.sh

# CI 一式
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo llvm-cov -p shikomi-cli --summary-only
```

### 3.2 人間が動作確認できるタイミング

実装完了後、以下のコマンドで**初めて `shikomi` が実機で動作**する。README "Try it" セクション or PR description に記載すること:

```bash
# ビルド
cargo build -p shikomi-cli --release

# 平文 vault を作成しつつ最初のレコード追加（自動初期化）
./target/release/shikomi --vault-dir ~/shikomi-test add --kind text --label "test" --value "hello"

# 一覧表示
./target/release/shikomi --vault-dir ~/shikomi-test list

# Secret を stdin で投入（シェル履歴汚染なし）
echo "my-secret" | ./target/release/shikomi --vault-dir ~/shikomi-test add --kind secret --label "api-key" --stdin

# 編集
./target/release/shikomi --vault-dir ~/shikomi-test edit --id <uuid> --label "renamed"

# 削除（--yes で確認スキップ）
./target/release/shikomi --vault-dir ~/shikomi-test remove --id <uuid> --yes

# 日本語表示で確認
LANG=ja_JP.UTF-8 ./target/release/shikomi --vault-dir ~/shikomi-test list
```

これが Issue #1（cli-vault-commands）完了後、初めて「動くもの」を実機で触れるマイルストーンとなる。

---

## 4. 証跡提出方針

全て `/app/shared/attachments/マユリ/` に保存して Discord に添付する。**コミットだけ・添付だけは禁止**（テスト戦略ガイド準拠）。

| 種別 | ファイル名 | 内容 |
|------|----------|------|
| E2E 実行ログ | `cli-vault-commands-e2e-report.md` | TC-E2E-001〜102 の `assert_cmd` 出力（stdout/stderr/exit code/diff）+ `SECRET_TEST_VALUE` 不在 grep 結果 |
| 結合・ユニット集計 | `cli-vault-commands-test-summary.md` | `cargo test -p shikomi-cli` の集計（X passed; Y failed の TC 別表）+ doc-test（TC-UT-111）の compile_fail 確認 |
| カバレッジ | `cli-vault-commands-coverage.html` | `cargo llvm-cov --html` のレポート（受入基準 13 検証） |
| CI チェック | `cli-vault-commands-ci-checks.md` | `cargo fmt / clippy / deny` の実行ログ（TC-CI-001〜003）+ `audit-secret-paths.sh` ログ（TC-CI-013〜015） |
| Secret 経路監査 | `cli-vault-commands-secret-audit.md` | `grep -rn "expose_secret" crates/shikomi-cli/src/` 結果（0 件ベースライン）+ panic hook 周辺抽出 |
| バグレポート（発見時） | `cli-vault-commands-bugs.md` | ファイル名・行番号・期待動作・実際動作・再現手順 |

---

## 5. CI job の段階構成（推奨）

`.github/workflows/ci.yml` での実行順（fail fast 原則）:

1. **Stage 1: 静的チェック**（早期 fail）
   - `cargo fmt --check --all`（TC-CI-001）
   - `cargo clippy --workspace --all-targets -- -D warnings`（TC-CI-002）
   - `bash scripts/ci/audit-secret-paths.sh`（TC-CI-013〜015）
   - `bash scripts/ci/audit-architecture.sh`（TC-CI-010〜012、grep 系）
2. **Stage 2: 依存監査**
   - `cargo deny check`（TC-CI-003）
3. **Stage 3: テスト**
   - `cargo test -p shikomi-cli --all-targets --features "shikomi-infra/test-fixtures"`（TC-CI-004、暗号化フィクスチャ込み）
   - `cargo test -p shikomi-cli --doc`（doc-test、TC-UT-111 の compile_fail 確認）
4. **Stage 4: カバレッジ**
   - `cargo llvm-cov -p shikomi-cli --summary-only --fail-under-lines 80`（TC-CI-005）

Stage 1 を通らなければ後続 job を実行しない（`needs:` で依存化）。**Secret 経路監査（TC-CI-013〜015）は Stage 1 に置く**ことで、万が一の secret 漏洩経路が main にマージされる前に早期検出する。

---

## 6. 残課題・未決事項の扱い

本テスト設計書のレビューで追加の指摘があれば:

- 「TC-ID で特定できるもの」はマトリクス（`index.md §4.1`）を更新
- 「設計に及ぶもの」は詳細設計 or 要件定義への差戻しをリーダーに要請
- 「実装でしか決まらないもの」は `unit.md §3 実装担当への引き継ぎ事項` に追記

---

*この文書は `index.md` の分割成果。E2E は `e2e.md`、結合は `integration.md`、ユニットは `unit.md` を参照*
