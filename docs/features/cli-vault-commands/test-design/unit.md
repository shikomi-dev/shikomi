# テスト設計書 — cli-vault-commands / ユニットテスト

> `index.md` の §2 索引からの分割ファイル。pure function のユニットテストと実装担当への引き継ぎ事項を扱う。

## 1. 設計方針

- **対象**: 全 public メソッドのうち pure function または I/O 抽象化済み関数
- **粒度**: 1 テスト 1 アサーションを目指す。何が失敗したか即わかること
- **モック**: I/O バウンダリのみ。pure function は素のまま検証
- **配置**: Rust 慣習に従い `#[cfg(test)] mod tests` でソースモジュール内（テスト戦略ガイド準拠）
- **命名**: `test_<対象>_<状況>_<期待>`（例: `test_from_record_text_short_returns_plain`）

---

## 2. テストケース一覧

### 2.1 `error::ExitCode::from(&CliError)`（`CliError` 9 バリアント × ExitCode マッピング）

| TC-ID | 入力 | 期待結果 |
|-------|------|---------|
| TC-UT-001 | `CliError::UsageError(_)` | `ExitCode::UserError` (1) |
| TC-UT-002 | `CliError::InvalidLabel(_)` | `ExitCode::UserError` (1) |
| TC-UT-003 | `CliError::InvalidId(_)` | `ExitCode::UserError` (1) |
| TC-UT-004 | `CliError::RecordNotFound(_)` | `ExitCode::UserError` (1) |
| TC-UT-005 | `CliError::VaultNotInitialized(_)` | `ExitCode::UserError` (1) |
| TC-UT-006 | `CliError::NonInteractiveRemove` | `ExitCode::UserError` (1) |
| TC-UT-007 | `CliError::Persistence(_)` | `ExitCode::SystemError` (2) |
| TC-UT-008 | `CliError::Domain(_)` | `ExitCode::SystemError` (2) |
| TC-UT-009 | `CliError::EncryptionUnsupported` | `ExitCode::EncryptionUnsupported` (3) |

配置: `src/error.rs` の `#[cfg(test)] mod tests`。

### 2.2 `view::RecordView::from_record` + `shikomi_core::Record::text_preview`

**ペテルギウス／服部平次 review 対応**: `shikomi-cli` では `SecretString::expose_secret()` を**一切呼ばない**。Text kind の preview は `shikomi_core::Record::text_preview(max_chars) -> Option<String>` に委譲する（詳細設計 §infra-changes.md）。

| TC-ID | 対象 | 種別 | 入力 | 期待結果 |
|-------|------|------|------|---------|
| TC-UT-010 | `RecordView::from_record` | 正常 | Text kind, value 短い | `ValueView::Plain(value)`（`record.text_preview(40)` が `Some(value)` を返す経路） |
| TC-UT-011 | `RecordView::from_record` | 境界 | Text kind, value 41 文字 | `ValueView::Plain("<40 文字>…")`（`text_preview(40)` が 40 文字で切って `…` 付与） |
| TC-UT-012 | `RecordView::from_record` | 正常（セキュリティ） | Secret kind | `ValueView::Masked`（`text_preview` が `None` を返すため `Plain` 経路に入らない） |

### 2.3 `shikomi_core::Record::text_preview`（本 feature で `shikomi-core` に追加された新規メソッド）

配置: `crates/shikomi-core/src/vault/record.rs` の `#[cfg(test)] mod tests`。

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-100 | 正常 | Text kind, value="hello", `max_chars=40` | `Some("hello")` |
| TC-UT-101 | 境界 | Text kind, value="a".repeat(100), `max_chars=40` | `Some("a".repeat(40))`（truncate、**`…` 付与は `RecordView` 側の責務**。本メソッドは純粋に `take(max_chars).collect()`） |
| TC-UT-102 | 境界 | Text kind, value="hello", `max_chars=0` | `Some("")`（空文字） |
| TC-UT-103 | セキュリティ | Secret kind | `None`（Secret kind は決して平文を返さない） |
| TC-UT-104 | 境界（Unicode） | Text kind, value="あいうえお", `max_chars=3` | `Some("あいう")`（char 単位で切る、`detailed-design/public-api.md §引き継ぎ注記` 準拠） |

**監査契約**: TC-UT-103 は `shikomi-core` 内での `expose_secret()` 呼び出しが Secret kind では**到達不能**であることを型で保証する。CI grep（`ci.md §TC-CI-013`）は `shikomi-cli/src/` に対する 0 件契約だが、`shikomi-core` 内部の `expose_secret` 呼び出しはこの TC-UT-103 で「Secret kind の経路では到達しない」と**テストレベルで保証**する。

### 2.4 `presenter::warning::render_shell_history_warning`

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-013 | 正常 | `Locale::English` | 英語の警告文（`shell history` を含む）のみ |
| TC-UT-014 | 正常 | `Locale::JapaneseEn` | 英語 + 日本語 2 段の警告文 |

### 2.5 `io::paths::resolve_vault_dir`

**ペテルギウス指摘 ④ 対応**: env の真実源は clap `#[arg(env = "SHIKOMI_VAULT_DIR")]` に**単一化**され、`resolve_vault_dir` 内部では `std::env::var` を**呼ばない**。`args.vault_dir: Option<PathBuf>` を受けて:
- `Some(p)` → `Ok(p)`（flag or env どちらも clap 経由で吸われた値）
- `None` → `Ok(dirs::data_dir()?.join("shikomi"))`（OS デフォルト）

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-020 | 正常（Some） | `args.vault_dir = Some("/custom/path")` | `Ok(PathBuf::from("/custom/path"))`（値をそのまま返す） |
| TC-UT-021 | **削除** | （旧「env var 優先」検証） | clap 単一化により本ケースは `resolve_vault_dir` の責務外。E2E TC-E2E-060, 061 が clap の env 解決を検証するため、ここでは不要 |
| TC-UT-022 | 正常（None → OS デフォルト） | `args.vault_dir = None`, `HOME` を `tempdir` に設定 | `Ok(<HOME>/.local/share/shikomi)`（OS 依存、`dirs::data_dir()` の実動作） |
| TC-UT-023 | 異常 | `args.vault_dir = None`, `HOME` / `APPDATA` を unset して `dirs::data_dir()` が None | `Err(CliError::Persistence(_))` or `Err(CliError::UsageError(_))`（詳細設計に準拠） |

TC-UT-022, 023 は env 操作が必要なため `#[serial]`（`serial_test` クレート）で逐次化。または pure 切り出し（`resolve_vault_dir_from(flag_or_env: Option<&Path>, os_default: Option<PathBuf>) -> Result<PathBuf, CliError>`）があれば env 操作不要（`§3 引き継ぎ §10.5` 推奨）。

### 2.6 `presenter::error::render_error`

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-030 | パラメタライズ（9 バリアント × English） | `CliError::*`, `Locale::English` | `error: ...\nhint: ...` の 2 行、英語のみ |
| TC-UT-031〜038 | TC-UT-030 のパラメタライズ 9 ケース実体化 | — | 各バリアントで MSG-CLI-xxx の英語文を含む |
| TC-UT-040 | パラメタライズ（9 バリアント × JapaneseEn） | `CliError::*`, `Locale::JapaneseEn` | 4 行（英語 error / 日本語 error / 英語 hint / 日本語 hint）の整合 |
| TC-UT-041 | TC-UT-040 のパラメタライズ 9 ケース実体化 | — | — |

### 2.7 `presenter::list::render_list`

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-050 | 空 | `&[]`, English | `render_empty(English)` の出力と一致 |
| TC-UT-051 | 1 件 Text | `Plain("V")` | ヘッダ + 1 行、`V` を含む |
| TC-UT-052 | 1 件 Secret | `Masked` | ヘッダ + 1 行、`****` を含み投入値を含まない |
| TC-UT-053 | 複数件 + ラベル長すぎ | label 100 文字 | ラベルが `LIST_LABEL_MAX_WIDTH` (40) で truncate + `…` |

### 2.8 `presenter::success::*`

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-060 | `render_added` | `RecordId`, English | `added: <uuid>` |
| TC-UT-061 | `render_added` | `RecordId`, JapaneseEn | 英 + 日 2 段 |
| TC-UT-062 | `render_updated` / `render_removed` / `render_cancelled` / `render_initialized_vault` パラメタライズ | 各 MSG-CLI-001〜005 の英 / 日 確認 |

### 2.9 `error::CliError` の `Display`

| TC-ID | 種別 | 対象 | 期待結果 |
|-------|------|------|---------|
| TC-UT-070 | Display 実装 | 全バリアント | 英語固定文字列のみ（日本語を含まない）、secret 投入値を含まない |

### 2.10 `Locale::detect_from_lang_env_value`（pure 関数、`§3 引き継ぎ §10.1` で要求）

**詳細設計で pure 関数として提供済みであれば env 操作不要**。`detect_from_env()` は `detect_from_lang_env_value(std::env::var("LANG").ok().as_deref())` の wrapper。

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-080 | LANG 未設定 | `None` | `Locale::English` |
| TC-UT-081 | LANG=C | `Some("C")` | `Locale::English` |
| TC-UT-082 | LANG=en_US.UTF-8 | `Some("en_US.UTF-8")` | `Locale::English` |
| TC-UT-083 | LANG=ja_JP.UTF-8 | `Some("ja_JP.UTF-8")` | `Locale::JapaneseEn` |
| TC-UT-084 | LANG=ja | `Some("ja")` | `Locale::JapaneseEn` |
| TC-UT-085 | LANG=JA_JP（大文字） | `Some("JA_JP")` | `Locale::JapaneseEn`（大文字小文字無視） |

### 2.11 `KindArg → RecordKind` 変換

| TC-ID | 種別 | 入力 | 期待結果 |
|-------|------|------|---------|
| TC-UT-090 | `From<KindArg> for RecordKind` | `KindArg::Text` | `RecordKind::Text` |
| TC-UT-091 | 同上 | `KindArg::Secret` | `RecordKind::Secret` |

### 2.12 `ConfirmedRemoveInput`（型の構築契約、ペテルギウス指摘 ⑤）

| TC-ID | 種別 | 入力 / 操作 | 期待結果 |
|-------|------|-----------|---------|
| TC-UT-110 | 正常 | `ConfirmedRemoveInput::new(id)` | `Ok(...)`（または `.id()` accessor 経由で入力 id と一致） |
| TC-UT-111 | doc-test（コンパイル保証） | `src/input.rs` の `ConfirmedRemoveInput` の doc-test に `/// ```compile_fail` ブロックを置き、`ConfirmedRemoveInput { id, confirmed: false }` 相当の bool 引数渡しが compile error になることを示す | `cargo test --doc` が pass |

**TC-UT-111 の例**:

```rust
/// ```compile_fail
/// use shikomi_cli::input::ConfirmedRemoveInput;
/// use shikomi_core::vault::RecordId;
/// let id = RecordId::new_v7();
/// let _ = ConfirmedRemoveInput { id, confirmed: false };  // フィールド不在で compile error
/// ```
```

これにより **`bool` フィールドを渡す使い方ができない**ことを型で保証する（Parse, don't validate）。

### 2.13 `io::terminal::is_stdin_tty` / `read_password`

I/O 抽象化の境界であり、ユニットテストでは対象外（E2E で間接カバー、TC-E2E-030/031）。実装担当が `#[cfg(test)]` で最小 smoke テストを書く余地はあるが、カバレッジ目標への寄与は低い。

---

## 3. 実装担当（坂田銀時）への引き継ぎ事項

テスト設計を完成させるために、実装段階で以下の調整を希望する。

### 3.1 `Locale::detect_from_lang_env_value` の pure 関数化（強く推奨）

詳細設計で `Locale::detect_from_env() -> Locale` が提供される想定だが、**内部で `std::env::var("LANG")` を呼ぶとユニットテストで env 操作が必要**となり並列実行と相性が悪い（`std::env::set_var` は thread-unsafe）。

**推奨**:
- `pub fn detect_from_lang_env_value(lang: Option<&str>) -> Locale` を pure として実装
- `pub fn detect_from_env() -> Locale` は `detect_from_lang_env_value(std::env::var("LANG").ok().as_deref())` の薄い wrapper

これで TC-UT-080〜085 が pure 関数テストで完結する。**詳細設計 §data-structures.md の `Locale` 章に本分解を明記してくれ**（設計側の合意必要）。

### 3.2 `shikomi-cli` の `[lib]` 化（決着済、詳細設計 §public-api.md 採用案 A）

ペテルギウス指摘 ③ は詳細設計で解決済み。`shikomi-cli` は `[lib] + [[bin]]` 構成で、`src/lib.rs` に `#[doc(hidden)] pub mod usecase; #[doc(hidden)] pub mod presenter; ...` を配置。結合テストは `use shikomi_cli::usecase::*;` で import。実装担当は:

- `Cargo.toml` に `[lib]` セクションを追加（`path = "src/lib.rs"`, `name = "shikomi_cli"`）
- `src/lib.rs` の冒頭に `//! Internal API. Not stable; subject to change without notice.` を doc コメント
- 全 `pub mod` に `#[doc(hidden)]` を付与（外部クレートからの依存を抑止する契約化）
- `src/main.rs` は `use shikomi_cli::run;` で `run()` を呼ぶだけの薄いエントリ

### 3.3 暗号化 vault フィクスチャヘルパー

TC-E2E-040, TC-E2E-041, TC-IT-012, TC-IT-023, TC-IT-033, TC-IT-040 で必要。

**作成方法（推奨）**:
- `tests/common/fixtures.rs` に `pub fn create_encrypted_vault(dir: &Path) -> Result<(), anyhow::Error>` を実装
- 内部で `shikomi-infra` の test-only API（`VaultHeader::new_encrypted_for_test(...)` 等。**未実装なら `shikomi-infra` 側に `#[cfg(feature = "test-fixtures")]` 付きで追加**）を呼んで生成
- これにより `tests/fixtures/vault_encrypted.db` をコミットせず、テスト実行時に毎回生成

**未対応時のフォールバック**: `shikomi-infra` の暗号化モード書き出し API がそもそも未実装なら、該当 TC を `#[ignore]` フォールバック（Phase 2 実装時に ignore 解除）。リーダーに起票を要請。

### 3.4 擬似 TTY が CI で動かない場合のフォールバック

TC-E2E-030（TTY 確認プロンプトで `y` 入力）は CI 環境では擬似 TTY が動かない可能性がある。フォールバック:

- TC-E2E-030 を `#[ignore]` 属性付きでローカル専用に置き、PR の test plan に「ローカル `cargo test --test e2e_remove -- --ignored` で手動検証」と明記
- 主受入基準（6 の Fail Fast 部分）は TC-E2E-031（非 TTY） で完全自動カバー

### 3.5 `resolve_vault_dir` の pure 切り出し（オプション）

TC-UT-022, 023 は env 操作が必要。`resolve_vault_dir_from(flag_or_env: Option<&Path>, os_default: Option<PathBuf>) -> Result<PathBuf, CliError>` の pure 関数を内部に切り出せば env 操作不要で unit test 可能。`dirs::data_dir()` の呼び出しは wrapper 側に残す。本提案は `§3.1` と同じ原則（pure / impure 境界の明示）。詳細設計で合意が得られれば採用、得られなければ `#[serial]` で逐次化。

---

## 4. カバレッジ対象

本ユニットテストレイヤでカバーする対応受入基準と REQ:

| 受入基準 | カバー TC |
|---------|----------|
| 1（Secret マスク、Text preview） | TC-UT-010〜012, TC-UT-050〜053, TC-UT-100〜104 |
| 3（secret 露出禁止） | TC-UT-012, TC-UT-070, TC-UT-103 |
| 4（shell history 警告） | TC-UT-013, 014 |
| 6（remove 確認型化） | TC-UT-110, 111 |
| 10（vault パス解決） | TC-UT-020, 022, 023 |
| 11（i18n） | TC-UT-030〜041, 080〜085 |
| REQ-CLI-006（終了コード契約） | TC-UT-001〜009 |
| REQ-CLI-012（Clean Arch 縦串、expose_secret 経路） | TC-UT-103（Secret kind から `expose_secret` 到達不能を型で保証） |

---

*この文書は `index.md` の分割成果。結合テストは `integration.md`、E2E は `e2e.md`、CI は `ci.md` を参照*
