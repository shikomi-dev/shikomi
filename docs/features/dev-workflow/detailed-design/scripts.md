# 詳細設計書 — dev-workflow / CI スクリプト契約

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- 配置先: docs/features/dev-workflow/detailed-design/scripts.md -->
<!-- 兄弟: ./index.md, ./classes.md, ./messages.md, ./setup.md, ./data-structures.md -->

## CI ワークフローで追加する secret scan ステップ

`audit.yml` に**二重防護**のため `just audit-secrets` ステップを追加する（T8 脅威対応）。ローカル pre-commit が `lefthook.yml` 改変で無効化された場合でも CI 側で独立に検知。

| ステップ順 | `run:` | 目的 |
|---------|-------|------|
| 1 | `actions/checkout@v4`（`fetch-depth: 0`）| gitleaks に履歴全体を渡すため全履歴取得 |
| 2 | `dtolnay/rust-toolchain@stable` | `cargo install` 前提 |
| 3 | `Swatinem/rust-cache@v2` | キャッシュ |
| 4 | `cargo install --locked just cargo-deny` | `just` / `cargo deny` 導入 |
| 5 | `just audit` | `cargo deny` + `audit-secret-paths.sh` |
| 6 | **gitleaks setup: `bash scripts/setup.sh --tools-only` を CI から呼び出す**（設計時確定） | setup ロジックを CI inline に複製すると DRY 違反。**`setup.sh` に `--tools-only` オプション**（`lefthook install` ステップを skip、ツール配置のみ実施）を追加し、CI も開発者ローカルも同一コード経路で Go 製バイナリを SHA256 検証つきで導入する。Sub-issue C の setup.sh 実装時にこのオプションを含める。判断は本設計書で凍結 |
| 7 | `just audit-secrets`（または `gitleaks detect --no-banner` を直接） | 履歴全体に対する secret 検知 |

## `audit-secret-paths.sh` の `unsafe` ブロック検出契約（TC-CI-019 / TC-CI-026 共通仕様）

**経緯**: Issue #75 (PR #82) で `crates/shikomi-cli/src/io/ipc_vault_repository.rs` の doc コメントに「`` `unsafe { std::env::remove_var(...) }` は Rust 2024 edition の env 操作 unsafe 化 ``」という解説文字列が導入された。当時の grep gate（`grep -rnE 'unsafe[[:space:]]*\{' ... --include='*.rs'`）はソース行とコメント行を区別しないため、**doc コメント内の `unsafe { ... }` 解説文字列を実 `unsafe` ブロックと誤検出**して TC-CI-026 が FAIL し、PR #82 / #84 で連続して `--admin` バイパスを誘発した。本設計書ではこの誤検出を **grep の表層仕様で構造的に塞ぐ**契約を確定する（Issue #85）。

### 検出契約（TC-CI-019 / TC-CI-026 共通）

| 項目 | 仕様 |
|-----|------|
| 検査対象パス | TC-CI-019: `crates/shikomi-daemon/src/` / TC-CI-026: `crates/shikomi-cli/src/`（再帰、`*.rs` のみ） |
| 検査対象行 | **実コード行のみ**。**コメント行**（行頭から最初の非空白文字が `//` で始まる行、`///` / `//!` を含む）は対象外 |
| 検出パターン | `unsafe[[:space:]]*\{`（拡張正規表現、`unsafe` キーワードと開きブレースの間に空白 0 個以上） |
| 許可リスト | TC-CI-019: `permission/{unix,windows,windows_acl}.rs` / TC-CI-026: `io/windows_sid.rs`、`hardening/core_dump.rs` |
| 失敗条件 | 検査対象行のうち、許可リスト外のファイルで検出パターンに合致する行が 1 件以上存在 |
| 出力 | 失敗時のみ該当 `file:line:content` を stderr に列挙し非 0 で exit。成功時は `[TC-CI-026] PASS` の 1 行のみ |

### コメント行の判定規則（実装契約）

- 「行頭から最初の非空白文字が `//` で始まる行」を一律にコメント行として除外する。`///`（doc コメント）/`//!`（モジュール doc コメント）/`//`（通常コメント）はすべてこの規則で吸収される
- **行内コメント**（コード `unsafe { ... } // 解説`）は除外しない。実コードに `unsafe` ブロックが書かれている以上、grep は正しくヒットする必要がある（むしろ検出が責務）
- **複数行ブロックコメント**（`/* ... */`）内の `unsafe { ... }` 文字列は本契約のスコープ外（YAGNI）。shikomi の Rust コードでブロックコメントを使う規約はなく、現状コードベース全体で `/* */` 用例ゼロを `rustfmt` 整形コミット履歴で確認済み。将来 `/* */` 内に `unsafe { ... }` を書くケースが現れたら本設計書を更新して契約拡張する
- **文字列リテラル内の `unsafe { ... }`**（例: `let s = "unsafe { ... }";`）は本契約のスコープ外。これも現状コードベースで用例ゼロ。仮に発生した場合は、文字列定数を別ファイル（`tests/fixtures/` 等）に切り出すかリテラルを分割するなど**コード側で回避**する方針（grep gate 側を AST 化するコストに見合わない）

### 実装方式の選択肢と判定（Issue #85 修正方針）

| 案 | 概要 | 採否 | 根拠 |
|----|------|------|------|
| A | grep のパイプで `grep -vE '^[[:space:]]*//'` を挟みコメント行を除外 | **採用** | 既存 `bash + grep` の構成を保ち、依存追加なし。文字列処理の透過性が高く CI ログで挙動を目視可能 |
| B | `cargo-geiger` で AST レベル `unsafe` 検出に置換 | 不採用 | 依存追加・実行時間増・許可リスト機構の再設計が必要。本 Issue の射程（コメント誤検出の是正）を超える |
| C | `ripgrep` の `--type rust` + `-U` マルチライン正則で `unsafe[[:space:]]*\{` のみ厳密照合 | 不採用 | rg 依存追加（CI / 開発者ローカル両方に setup 工数）。grep のみで決着するなら追加依存は YAGNI |

### 回帰防止のテスト契約

テスト設計担当への要請（`test-design.md` で TC-CI-026 のサブケースとして反映）:

| サブ TC | 検査対象 fixture | 期待結果 |
|--------|-----------------|---------|
| TC-CI-026-a | 実 `unsafe { ... }` ブロックが許可リスト外ファイルに存在する fixture | FAIL（exit 非 0、該当 `file:line` 出力） |
| TC-CI-026-b | doc コメント `/// unsafe { ... }` のみ存在し実 `unsafe` ブロック無しの fixture（PR #82 で混入した実例の再現） | **PASS**（誤検出しないこと） |
| TC-CI-026-c | 通常コメント `// unsafe { ... }` のみ存在し実 `unsafe` ブロック無しの fixture | **PASS** |
| TC-CI-026-d | 行内コメント `unsafe { ... } // 解説` の実コードが許可リスト外ファイルに存在する fixture | FAIL（実 `unsafe` ブロックは検出されるべき） |
| TC-CI-026-e | 許可リストファイル（`io/windows_sid.rs` 等）に実 `unsafe { ... }` ブロックが存在 | PASS（許可リストにより除外） |

### 設計判断の凍結

本契約は `scripts/ci/audit-secret-paths.sh` の TC-CI-019 / TC-CI-026 ステップに**コメント行除外の grep -v パイプを 1 段挟む**ことで実装する。Sub-issue 実装側での再判定は行わない。文字列リテラル内 / ブロックコメント内のスコープ外事項が将来発生した場合のみ本設計書を更新して再凍結する。
