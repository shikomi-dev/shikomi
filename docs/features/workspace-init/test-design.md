# テスト設計書 — workspace-init（Cargo workspace 初期化）

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | workspace-init（Cargo workspace 初期化と 5 crate 空スケルトン） |
| 対象 Issue | [#4](https://github.com/shikomi-dev/shikomi/issues/4) |
| 対象ブランチ | `feature/issue-4-workspace-init` |
| 設計根拠 | `docs/architecture/tech-stack.md` §1（全体構成）、§2.1（言語・ランタイム）、§2.2（CI/CD・依存監査） |
| テスト実行タイミング | `feature/issue-4-workspace-init` → `develop` へのマージ前 |

## 2. テスト対象と受入基準

| 受入基準ID | 受入基準 | 検証レベル |
|-----------|---------|-----------|
| WS-01 | `cargo build --workspace` が pass する | 結合 |
| WS-02 | `cargo test --workspace` が pass する（テストケース 0 件でも可） | 結合 |
| WS-03 | `cargo fmt --check --all` が pass する（rustfmt.toml 準拠） | 結合 |
| WS-04 | `cargo clippy --workspace -- -D warnings` が pass する（.clippy.toml 準拠） | 結合 |
| WS-05 | `cargo deny check` が pass する（deny.toml 初期設定準拠） | 結合 |
| WS-06 | ライブラリ crate（shikomi-core / infra / gui）が `lib.rs` のみ・バイナリ crate（daemon / cli）が `main.rs` のみの最小構成である | ユニット |
| WS-07 | 各 crate に公開 API が存在しない（`pub fn` / `pub struct` / `pub enum` / `pub trait` / `pub mod` / `pub use` / `pub const` / `pub static` / `pub type` なし） | ユニット |
| WS-08 | `Cargo.lock` がリポジトリにコミット済みである | ユニット |

## 3. テストマトリクス（トレーサビリティ）

| テストID | 受入基準ID | 検証内容 | テストレベル | 種別 |
|---------|-----------|---------|------------|------|
| TC-I01 | WS-01 | `cargo build --workspace` の実行成功 | 結合 | 正常系 |
| TC-I02 | WS-02 | `cargo test --workspace` の実行成功（0 tests でも pass） | 結合 | 正常系 |
| TC-I03 | WS-03 | `cargo fmt --check --all` の実行成功（差分なし） | 結合 | 正常系 |
| TC-I04 | WS-04 | `cargo clippy --workspace -- -D warnings` の実行成功（警告ゼロ） | 結合 | 正常系 |
| TC-I05 | WS-05 | `cargo deny check` の実行成功（ライセンス・advisory・重複 crate 全 pass） | 結合 | 正常系 |
| TC-U01 | WS-06 | `crates/shikomi-core/src/lib.rs` が存在し、`src/` にはそのファイルのみである（ライブラリ crate） | ユニット | 正常系 |
| TC-U02 | WS-06 | `crates/shikomi-infra/src/lib.rs` が存在し、`src/` にはそのファイルのみである（ライブラリ crate） | ユニット | 正常系 |
| TC-U03 | WS-06 | `crates/shikomi-daemon/src/main.rs` が存在し、`src/` にはそのファイルのみである（バイナリ crate） | ユニット | 正常系 |
| TC-U04 | WS-06 | `crates/shikomi-cli/src/main.rs` が存在し、`src/` にはそのファイルのみである（バイナリ crate） | ユニット | 正常系 |
| TC-U05 | WS-06 | `crates/shikomi-gui/src/lib.rs` が存在し、`src/` にはそのファイルのみである（ライブラリ crate） | ユニット | 正常系 |
| TC-U06 | WS-07 | 全 crate のソースファイルに公開 API シンボルが存在しない（検査パターン: `^pub (fn\|struct\|enum\|trait\|mod\|use\|const\|static\|type) `、`pub(crate)` は誤検知対象外） | ユニット | 正常系 |
| TC-U07 | WS-08 | `Cargo.lock` がリポジトリルートに存在する | ユニット | 正常系 |

## 4. E2Eテスト設計

**省略理由**: workspace-init はエンドユーザーが直接操作する UI / CLI / 公開 API を持たない横断的変更。
テスト戦略ガイドの方針「エンドユーザー操作がない場合は結合テストで代替」に従い、E2E は設計対象外とする。
受入基準の検証は §5 の結合テスト（`cargo` コマンド実行）で網羅する。

## 5. 結合テスト設計（cargo コマンドによる CI 検証）

> **ツール選択根拠**: このシステムは Cargo workspace の初期化。インターフェースは `cargo` コマンド群であり、CLIツールとして結合テストに分類する。検証は実際の `cargo` コマンド実行結果（exit code / stderr）で行う。

### TC-I01: cargo build の通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I01 |
| 対応する受入基準ID | WS-01 |
| 対応する工程 | 基本設計（tech-stack.md §1 Cargo workspace 構成） |
| 種別 | 正常系 |
| 前提条件 | Cargo workspace の `Cargo.toml` が存在し、5 crate 全て登録済み |
| 操作 | `cargo build --workspace` を実行 |
| 期待結果 | exit code == 0。5 crate（shikomi-core / infra / daemon / cli / gui）全てのコンパイルが成功する |

### TC-I02: cargo test の通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I02 |
| 対応する受入基準ID | WS-02 |
| 対応する工程 | 基本設計 |
| 種別 | 正常系 |
| 前提条件 | TC-I01 通過済み |
| 操作 | `cargo test --workspace` を実行 |
| 期待結果 | exit code == 0。`0 passed; 0 failed` または空テストが 5 crate 全てで確認できる |

### TC-I03: cargo fmt --check の通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I03 |
| 対応する受入基準ID | WS-03 |
| 対応する工程 | 基本設計（tech-stack.md §2.2 CI/CD） |
| 種別 | 正常系 |
| 前提条件 | `rustfmt.toml` がリポジトリルートに存在する |
| 操作 | `cargo fmt --check --all` を実行 |
| 期待結果 | exit code == 0。フォーマット差分なし（`Diff in ...` 行が出力されない） |

### TC-I04: cargo clippy の通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I04 |
| 対応する受入基準ID | WS-04 |
| 対応する工程 | 基本設計（tech-stack.md §2.2 CI/CD） |
| 種別 | 正常系 |
| 前提条件 | `.clippy.toml` または `Cargo.toml` の `[lints.clippy]` 設定が存在する |
| 操作 | `cargo clippy --workspace -- -D warnings` を実行 |
| 期待結果 | exit code == 0。`warning:` / `error:` が出力されない |

### TC-I05: cargo deny check の通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I05 |
| 対応する受入基準ID | WS-05 |
| 対応する工程 | 基本設計（tech-stack.md §2.2 依存監査） |
| 種別 | 正常系 |
| 前提条件 | `deny.toml` がリポジトリルートに存在し、ライセンス許可リスト初期設定済み |
| 操作 | `cargo deny check` を実行 |
| 期待結果 | exit code == 0。`error[` が出力されない（ライセンス / advisory / 重複バージョン / 禁止 crate のいずれも fail しない） |

## 6. ユニットテスト設計（コード構造検証）

> **ツール**: `find` / `grep` によるファイル構造・ソース内容の静的確認。Rust コンパイラ自体がコード構造チェックを担うため、別途テストコードは作成しない。

### TC-U01〜TC-U05: crate ソースファイル構成確認

> **crate 種別の区別**: ライブラリ crate（core / infra / gui）は `lib.rs`、バイナリ crate（daemon / cli）は `main.rs` が正しいエントリポイント。daemon は常駐プロセス、cli は実行バイナリであり、将来 `fn main()` を実装する前提の正しい構造（tech-stack.md §4.1 準拠）。

| テストID | crate | 種別 | 期待エントリポイント | 期待結果 |
|---------|-------|------|-----------------|---------|
| TC-U01 | shikomi-core | ライブラリ | `crates/shikomi-core/src/lib.rs` | `lib.rs` のみ存在する（他 `.rs` ファイルなし） |
| TC-U02 | shikomi-infra | ライブラリ | `crates/shikomi-infra/src/lib.rs` | `lib.rs` のみ存在する |
| TC-U03 | shikomi-daemon | バイナリ | `crates/shikomi-daemon/src/main.rs` | `main.rs` のみ存在する（`lib.rs` なし） |
| TC-U04 | shikomi-cli | バイナリ | `crates/shikomi-cli/src/main.rs` | `main.rs` のみ存在する（`lib.rs` なし） |
| TC-U05 | shikomi-gui | ライブラリ | `crates/shikomi-gui/src/lib.rs` | `lib.rs` のみ存在する |

**前提条件**: workspace 実装コードが `feature/issue-4-workspace-init` ブランチにコミット済み
**操作**: 各 crate の `src/` を確認

### TC-U06: 公開 API 不存在確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U06 |
| 対応する受入基準ID | WS-07 |
| 対応する工程 | 詳細設計（設計原則: 公開 API ゼロ） |
| 種別 | 正常系 |
| 前提条件 | 全 crate のソースファイルが存在する |
| 操作 | 全 `.rs` ファイルに対して `grep -rEn "^pub (fn\|struct\|enum\|trait\|mod\|use\|const\|static\|type) " --include="*.rs"` を実行（`target/` 配下は除外） |
| 期待結果 | マッチ行ゼロ。外部公開 API シンボルが存在しない。なお `pub(crate)` / `pub(super)` は公開 API ではないため検査対象外とする（grep パターンが `pub ` + スペースで区切ることで自動排除） |

### TC-U07: Cargo.lock コミット確認

| 項目 | 内容 |
|------|------|
| テストID | TC-U07 |
| 対応する受入基準ID | WS-08 |
| 対応する工程 | 基本設計 |
| 種別 | 正常系 |
| 前提条件 | ブランチがコミット済み |
| 操作 | `git ls-files Cargo.lock` を実行 |
| 期待結果 | `Cargo.lock` が出力される（git 管理下にある） |

## 7. モック方針

| 検証対象 | モック要否 | 理由 |
|---------|---------|------|
| cargo コマンド | **不要**（本物を使用） | 実際のコンパイル・ビルドツールチェーンを使う。モックでは意味をなさない |
| ファイル構造確認 | **不要** | リポジトリの実ファイルを直接確認する |
| 外部 crate（依存 crate） | **対象外** | スケルトンのため外部依存は `deny.toml` の初期許可リスト確認のみ |

外部依存はすべて本物を使用する。assumed mock は禁止。

## 8. 実行手順と証跡

### 実行環境

- Rust toolchain（`rustup`、`cargo`、`rustfmt`、`clippy` インストール済み）
- `cargo-deny`（`cargo install cargo-deny`）
- ローカルまたは CI 環境

### 実行コマンド例（結合テスト TC-I01〜TC-I05）

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

echo "=== TC-I01: cargo build ==="
cargo build --workspace 2>&1
echo "TC-I01: PASS"

echo "=== TC-I02: cargo test ==="
cargo test --workspace 2>&1
echo "TC-I02: PASS"

echo "=== TC-I03: cargo fmt --check ==="
cargo fmt --check --all 2>&1
echo "TC-I03: PASS"

echo "=== TC-I04: cargo clippy ==="
cargo clippy --workspace -- -D warnings 2>&1
echo "TC-I04: PASS"

echo "=== TC-I05: cargo deny check ==="
cargo deny check 2>&1
echo "TC-I05: PASS"

echo "=== 全結合テスト PASS ==="
```

### 実行コマンド例（ユニットテスト TC-U01〜TC-U07）

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"
PASS=true

echo "=== TC-U01〜TC-U05: crate ソースファイル構成確認 ==="

# ライブラリ crate: lib.rs のみ（crates/ 配下に配置）
LIB_CRATES=(shikomi-core shikomi-infra shikomi-gui)
for crate in "${LIB_CRATES[@]}"; do
    FILES=$(find "crates/$crate/src" -name "*.rs" | sort)
    EXPECTED="crates/$crate/src/lib.rs"
    if [ "$FILES" = "$EXPECTED" ]; then
        echo "TC: $crate → PASS (lib.rs のみ)"
    else
        echo "TC: $crate → FAIL (想定外ファイルあり: $FILES)"
        PASS=false
    fi
done

# バイナリ crate: main.rs のみ（crates/ 配下に配置）
BIN_CRATES=(shikomi-daemon shikomi-cli)
for crate in "${BIN_CRATES[@]}"; do
    FILES=$(find "crates/$crate/src" -name "*.rs" | sort)
    EXPECTED="crates/$crate/src/main.rs"
    if [ "$FILES" = "$EXPECTED" ]; then
        echo "TC: $crate → PASS (main.rs のみ)"
    else
        echo "TC: $crate → FAIL (想定外ファイルあり: $FILES)"
        PASS=false
    fi
done

echo "=== TC-U06: 公開 API 不存在確認 ==="
# ^pub (fn|struct|enum|trait|mod|use|const|static) で外部公開シンボルを検査
# pub(crate) / pub(super) は "pub " の後に "(" が来るため自動排除される
PUB_MATCHES=$(grep -rEn "^pub (fn|struct|enum|trait|mod|use|const|static|type) " \
    --include="*.rs" . \
    --exclude-dir=target || true)
if [ -z "$PUB_MATCHES" ]; then
    echo "TC-U06: PASS (公開 API なし)"
else
    echo "TC-U06: FAIL (公開 API が存在します):"
    echo "$PUB_MATCHES"
    PASS=false
fi

echo "=== TC-U07: Cargo.lock コミット確認 ==="
if git ls-files Cargo.lock | grep -q "Cargo.lock"; then
    echo "TC-U07: PASS"
else
    echo "TC-U07: FAIL (Cargo.lock が git 管理外)"
    PASS=false
fi

$PASS && echo "=== 全ユニットテスト PASS ===" || { echo "=== FAIL ==="; exit 1; }
```

### 証跡

- テスト実行結果（stdout/stderr/exit code）を Markdown で記録する
- 結果ファイルを `/app/shared/attachments/マユリ/` に保存して Discord に添付する
- `cargo build` / `cargo clippy` / `cargo deny` の出力ログを証跡として保存する

## 9. カバレッジ基準

| 観点 | 基準 |
|------|------|
| 受入基準の網羅 | WS-01 〜 WS-08 が全テストケース（TC-I01〜TC-I05 / TC-U01〜TC-U07）で網羅されていること |
| 正常系 | 全ケース必須（結合テスト 5 件 + ユニットテスト 7 件 = 合計 12 件） |
| 異常系 | スケルトン段階のため意図的なエラーパスは設けない（公開 API が誤って追加された場合の検知は TC-U06 で担保） |
| 境界値 | `cargo deny` で許可リスト外ライセンスの crate が混入した場合に TC-I05 が fail することを確認 |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — WS-06/TC-U01〜U05 を daemon/cli バイナリ crate（main.rs）・core/infra/gui ライブラリ crate（lib.rs）に区別修正。TC-U06 の grep パターンを `^pub (fn|struct|enum|trait|mod|use|const|static) ` に精密化し pub(crate) 誤検知を排除*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — WS-07/TC-U06 に `pub type` を追加（型エイリアスの見逃し修正）。TC-U01〜U05 の期待パスとbashスクリプトの find/EXPECTED を `crates/{name}/src/` プレフィックスに統一（tech-stack.md §4.1 の crates/ 配置との整合）*
*対応 Issue: #4 feat(workspace): Cargo workspace 初期化と 5 crate 空スケルトン*
