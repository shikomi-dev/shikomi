# 詳細設計書 — infra-changes（shikomi-infra への最小変更 / tech-stack.md 反映）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/detailed-design/infra-changes.md -->
<!-- 兄弟: ./index.md, ./data-structures.md, ./public-api.md, ./clap-config.md, ./composition-root.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。変更点は**「現状」「変更後」「根拠」**の 3 段で示す。

## 変更方針の総括

本 feature は「既存 infra を最小変更」を原則とする。変更は以下の 3 点のみ:

1. `shikomi-infra::persistence::repository` に `SqliteVaultRepository::from_directory(path: &Path)` を追加
2. `shikomi-infra::persistence::paths` に内部ヘルパ `resolve_os_default_or_env()` を追加（既存 `new()` 内のロジックを切り出し）
3. `shikomi-core::vault::record` に `Record::text_preview(&self, max_chars: usize) -> Option<String>` を追加

**既存の公開 API を削除 / 改名しない**（Boy Scout Rule の逆に該当しない）。**`VaultPaths` の pub 昇格は行わない**（ペテルギウス指摘 ⑦）。

## 変更 1: `SqliteVaultRepository::from_directory`

**現状**:

- `pub fn SqliteVaultRepository::new() -> Result<Self, PersistenceError>` のみが公開
- `new()` 内部で `std::env::var("SHIKOMI_VAULT_DIR")` か `dirs::data_dir()` を解決し、`VaultPaths::new(dir)` を呼んで `Self { paths }` を構築

**変更後**:

- `pub fn SqliteVaultRepository::from_directory(path: &Path) -> Result<Self, PersistenceError>` を追加
  - 内部で `VaultPaths::new(path.to_path_buf())?` を呼び、`Self { paths }` を構築
  - `VaultPaths::new` の既存検証（`PROTECTED_PATH_PREFIXES` 等 7 ステップ）を流用
- 既存 `pub fn new()` は内部で `paths::resolve_os_default_or_env()?` を呼び、得た `PathBuf` を `from_directory(&path)` に委譲するリファクタ
  - 既存の env / OS default 解決ロジックは `paths::resolve_os_default_or_env` に移動（下記変更 2）
  - `new()` の挙動は**不変**（既存テストは無変更で pass）

**配置**: `crates/shikomi-infra/src/persistence/repository.rs` の `impl SqliteVaultRepository` ブロックに `from_directory` を追加

**公開範囲**: `pub`（crate 外部 = `shikomi-cli` から呼ばれる）

**根拠**:

- CLI の `--vault-dir` フラグ指定を thread-safe に扱うため、`std::env::set_var` を使わずに vault dir を渡す経路が必要
- プリミティブ引数 `&Path` を取ることで、`VaultPaths` 値型を pub 昇格させずに済む（公開 API 契約の最小化、ペテルギウス指摘 ⑦）
- 既存 `new()` の挙動を維持することで既存テストに無影響（Boy Scout Rule の理想的適用）

## 変更 2: `paths::resolve_os_default_or_env`

**現状**:

- `SqliteVaultRepository::new()` 内に env / OS default 解決ロジックが inline

**変更後**:

- `crates/shikomi-infra/src/persistence/paths.rs` に crate-private 関数を追加:
  - `pub(crate) fn resolve_os_default_or_env() -> Result<PathBuf, PersistenceError>`
  - 内部: `std::env::var("SHIKOMI_VAULT_DIR")` が Ok なら `PathBuf::from(s)`、それ以外は `dirs::data_dir().ok_or(CannotResolveVaultDir)?.join("shikomi")`
- 既存 `new()` はこの関数を呼び出すだけのラッパに変わる

**公開範囲**: `pub(crate)`（`shikomi-infra` crate 内のみ、CLI からは見えない）

**根拠**:

- `SqliteVaultRepository::new()` と `from_directory(&path)` の両方から共通ロジックを呼べるようにする
- `shikomi-cli` 側では env / OS default 解決を**clap が env を、`io::paths::resolve_os_default_vault_dir` が OS default を**担当するため、この infra 内部関数は CLI から直接呼ばれない（既存 `new()` の後方互換のみ）

**注意**: 本関数は **`std::env::var` を呼ぶ唯一の場所**として infra 内に保持される。将来 Phase 2 で `shikomi-cli` が `new()` を呼ばなくなれば（`from_directory` のみを使う）、この関数の呼び出し元は既存 `new()` のみとなり、deprecation path を用意する余地ができる（将来 feature）。

## 変更 3: `Record::text_preview`

**現状**:

- `crates/shikomi-core/src/vault/record.rs` に `pub fn Record::payload(&self) -> &RecordPayload` アクセサのみ
- `RecordPayload::Plaintext(SecretString)` から平文を取り出すには呼び出し側で `expose_secret` が必要

**変更後**:

- `impl Record` ブロックに以下を追加:
  - `pub fn text_preview(&self, max_chars: usize) -> Option<String>`
  - 挙動:
    - `self.kind() == RecordKind::Text` かつ `self.payload()` が `RecordPayload::Plaintext(SecretString)` の場合: `Some(s.expose_secret().chars().take(max_chars).collect::<String>())` を返す
    - それ以外（`Secret` kind or `Encrypted` variant）: `None`
  - 内部で `SecretString::expose_secret()` を呼ぶが、**`shikomi-core` 内部で完結**する（`shikomi-cli/src/` の CI grep 対象外）

**配置**: `crates/shikomi-core/src/vault/record.rs` の `impl Record` ブロック

**公開範囲**: `pub`

**根拠**:

- `shikomi-cli/src/view.rs::RecordView::from_record` 内で `SecretString::expose_secret()` を呼ばずに Text kind の preview を生成するため
- **CI 契約「`shikomi-cli/src/` 内で `expose_secret` 呼び出し 0 件」を守るための必要最小の `shikomi-core` API 追加**
- Secret kind は `None` を返すため、Secret 値が Text ルートで露出する経路が存在しない
- 機能拡張としても汎用的（将来の `shikomi-gui` / `shikomi-daemon` でも preview が必要）

**テスト**:

- `crates/shikomi-core/src/vault/record.rs` 末尾の `#[cfg(test)] mod tests` に UT 追加:
  - Text kind + max_chars 0 → `Some("")`
  - Text kind + max_chars = String 長 → `Some(全文)`
  - Text kind + max_chars > String 長 → `Some(全文)`
  - Text kind + 日本語マルチバイト + max_chars 5 → `Some(先頭 5 char)`（grapheme 単位ではなく char 単位）
  - Secret kind → `None`
  - Encrypted variant → `None`

## `tech-stack.md` への反映

本 feature の PR で `docs/architecture/tech-stack.md` に以下を追記する。

**§4.4 `[workspace.dependencies]` への追加項目**:

| crate | バージョン | feature | 用途 | 根拠 |
|-------|----------|--------|------|------|
| `anyhow` | `1` | — | `shikomi-cli` の `run()` 戻り値ラップ | `thiserror` は型別エラー、`anyhow` は CLI 層で総合エラー扱い。業界標準の棲み分け |
| `is-terminal` | `0.4` | — | stdin TTY 判定（`shikomi-cli`）| `std::io::IsTerminal` 直接利用でも可だが MSRV 1.80 で crate 版が広く使われておりテスト差し替え余地を残す |
| `rpassword` | `7` | — | 非エコー stdin 入力（secret 値） | `tauri-plugin-*` のような重量依存を避け、Unix termios / Windows SetConsoleMode を薄くラップする標準 |
| `assert_cmd` | `2`（dev） | — | E2E テストのプロセス起動 | Rust エコシステムの事実標準 E2E helper |
| `predicates` | `3`（dev） | — | `assert_cmd` の assertion 補助 | 同上 |

**§2.1 の CLI パーサ行**: 既記載のまま（`clap` v4 derive を採用、変更なし）。

## PR に含める変更ファイル（設計書 + コード）

本 feature の実装 PR で変更するファイル:

**設計書（本 PR）**:

- `docs/features/cli-vault-commands/requirements-analysis.md`（新規）
- `docs/features/cli-vault-commands/requirements.md`（新規）
- `docs/features/cli-vault-commands/basic-design/index.md`（新規）
- `docs/features/cli-vault-commands/basic-design/security.md`（新規）
- `docs/features/cli-vault-commands/basic-design/error.md`（新規）
- `docs/features/cli-vault-commands/detailed-design/index.md`（新規）
- `docs/features/cli-vault-commands/detailed-design/data-structures.md`（新規）
- `docs/features/cli-vault-commands/detailed-design/public-api.md`（新規）
- `docs/features/cli-vault-commands/detailed-design/clap-config.md`（新規）
- `docs/features/cli-vault-commands/detailed-design/composition-root.md`（新規）
- `docs/features/cli-vault-commands/detailed-design/infra-changes.md`（本ファイル、新規）
- `docs/features/cli-vault-commands/detailed-design/future-extensions.md`（新規）
- `docs/features/cli-vault-commands/test-design.md`（マユリ担当、修正・分割予定）
- `docs/architecture/context/process-model.md`（§4.1.1 追記、済）
- `docs/architecture/tech-stack.md`（§4.4 追記、未実施）

**実装 PR（本 feature の後続、別 PR）**:

- `Cargo.toml`（workspace.dependencies 追加）
- `crates/shikomi-cli/Cargo.toml`（lib + bin ターゲット定義、dependencies 追加）
- `crates/shikomi-cli/src/lib.rs`（新規、`run()` 本体）
- `crates/shikomi-cli/src/main.rs`（既存の 1 行を `fn main() -> ExitCode { shikomi_cli::run() }` に置換）
- `crates/shikomi-cli/src/{cli.rs, error.rs, input.rs, view.rs}`（新規）
- `crates/shikomi-cli/src/{usecase/, presenter/, io/}/*.rs`（新規）
- `crates/shikomi-cli/tests/*.rs`（E2E テスト、テスト設計で定義）
- `crates/shikomi-infra/src/persistence/repository.rs`（`from_directory` 追加）
- `crates/shikomi-infra/src/persistence/paths.rs`（`resolve_os_default_or_env` 追加）
- `crates/shikomi-core/src/vault/record.rs`（`text_preview` メソッド追加 + UT）
- `.github/workflows/*.yml`（TC-CI-013/014/015 の grep チェック追加、テスト設計側で詳細化）

**実装 PR は本 PR（設計）マージ後に別ブランチで起こす**（設計と実装を分離する運用）。
