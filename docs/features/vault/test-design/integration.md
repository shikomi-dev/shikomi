# テスト設計書 — vault: 結合テスト設計（TC-I01〜TC-I07）

> **ファイル構成**
> | ファイル | 内容 |
> |---------|------|
> | [`overview.md`](overview.md) | §1概要・§2受入基準・§3テストマトリクス・§4E2Eテスト |
> | `integration.md`（本ファイル） | §5 結合テスト設計（TC-I01〜TC-I07） |
> | [`unit.md`](unit.md) | §6 ユニットテスト設計（TC-U01〜TC-U12） |
> | [`appendix.md`](appendix.md) | §7 モック方針・§8 ディレクトリ構造・§9 実行手順・§10 カバレッジ |

---

## 5. 結合テスト設計（tests/ 配下の integration test）

> **ツール選択根拠**: 対象は Rust library crate の公開 API。テスト戦略ガイドの Rust 慣習に従い、integration test は `tests/` 配下（`#[cfg(test)]` 外）に配置し、`shikomi-core` の公開 API のみ通じてテストする。モックは不要（no-I/O pure Rust のため）。

### TC-I01: Plaintext vault ライフサイクル

| 項目 | 内容 |
|------|------|
| テストID | TC-I01 |
| 対応する機能ID | REQ-007 |
| 対応する受入基準ID | AC-01, AC-06 |
| 対応する工程 | 基本設計（Vault 集約ルート処理フロー） |
| 種別 | 正常系 |
| 前提条件 | `Vault::new` / `add_record` / `find_record` / `remove_record` / `records` が実装済み |
| 操作 | (1) Plaintext header で Vault 生成 → (2) Plaintext payload のレコード 2 件を add → (3) 1件を find して内容確認 → (4) 1件を remove → (5) `records().len() == 1` を確認 |
| 期待結果 | 全ステップで `Ok` / 期待値一致。最終レコード件数 1 件 |

### TC-I02: Vault モード不整合検知（外部 API 視点）

| 項目 | 内容 |
|------|------|
| テストID | TC-I02 |
| 対応する機能ID | REQ-007 |
| 対応する受入基準ID | AC-02, AC-06 |
| 対応する工程 | 基本設計（Vault::add_record 処理フロー） |
| 種別 | 異常系 |
| 前提条件 | `Vault::new` / `add_record` が実装済み |
| 操作 | (1) Plaintext header で Vault 生成 → (2) Encrypted payload のレコードを `add_record` に渡す |
| 期待結果 | `Err(DomainError::VaultConsistencyError(ModeMismatch { .. }))` が返る |

### TC-I03: RecordLabel 境界値（外部 API 視点）

| 項目 | 内容 |
|------|------|
| テストID | TC-I03 |
| 対応する機能ID | REQ-005 |
| 対応する受入基準ID | AC-03, AC-06 |
| 対応する工程 | 詳細設計（RecordLabel::try_new 検証ロジック） |
| 種別 | 境界値 |
| 前提条件 | `RecordLabel::try_new` が実装済み |
| 操作 | 以下を順に試す: (a) `""` → (b) `"A"` → (c) 255 grapheme 文字列 → (d) 256 grapheme 文字列 → (e) `"\x00"` → (f) `"\x1F"` → (g) `"\x7F"` → (h) `"\t"` → (i) `"\n"` |
| 期待結果 | (a) `Err(Empty)` / (b) `Ok` / (c) `Ok` / (d) `Err(TooLong { grapheme_count: 256 })` / (e) `Err(ControlChar { .. })` / (f) `Err(ControlChar { .. })` / (g) `Err(ControlChar { .. })` / (h) `Ok` / (i) `Ok` |

### TC-I04: SecretString / SecretBytes の非リーク確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I04 |
| 対応する機能ID | REQ-008 |
| 対応する受入基準ID | AC-04, AC-06 |
| 対応する工程 | 詳細設計（SecretString / SecretBytes の Debug 実装） |
| 種別 | 正常系 |
| 前提条件 | `SecretString::from_string` / `SecretBytes::from_boxed_slice` が実装済み |
| 操作 | (1) `SecretString::from_string("my-password".to_string())` を生成し `format!("{:?}", s)` を取得 → (2) `SecretBytes::from_boxed_slice(b"secret".to_vec().into_boxed_slice())` を生成し `format!("{:?}", b)` を取得 |
| 期待結果 | (1) 結果文字列に `"my-password"` が含まれず、`"[REDACTED]"` を含む / (2) 結果文字列に生バイトが含まれず `"[REDACTED]"` を含む |

### TC-I05: NonceCounter オーバーフロー（外部 API 視点）

| 項目 | 内容 |
|------|------|
| テストID | TC-I05 |
| 対応する機能ID | REQ-010 |
| 対応する受入基準ID | AC-05, AC-06 |
| 対応する工程 | 詳細設計（NonceCounter::next オーバーフロー処理） |
| 種別 | 境界値 |
| 前提条件 | `NonceCounter::resume` / `NonceCounter::next` が実装済み |
| 操作 | (1) `NonceCounter::resume([0u8; 8], u32::MAX)` で counter を上限に設定 → (2) `next()` を呼ぶ |
| 期待結果 | `Err(DomainError::NonceOverflow)` が返る |

### TC-I06: CI cargo コマンド通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I06 |
| 対応する機能ID | REQ-001〜REQ-010 |
| 対応する受入基準ID | AC-07 |
| 対応する工程 | 基本設計（CI/CD） |
| 種別 | 正常系 |
| 前提条件 | `shikomi-core` のソースコードが実装済みかつコミット済み |
| 操作 | (1) `cargo build --workspace` → (2) `cargo test --workspace` → (3) `cargo fmt --check --all` → (4) `cargo clippy --workspace -- -D warnings` → (5) `cargo deny check` |
| 期待結果 | 全コマンド exit code == 0 |

### TC-I07: deny.toml 暗号クリティカル crate 登録確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I07 |
| 対応する機能ID | REQ-009 |
| 対応する受入基準ID | AC-09 |
| 対応する工程 | 要件定義（requirements-analysis.md §受入基準 #9） |
| 種別 | 静的確認 |
| 前提条件 | `deny.toml` がリポジトリルートに存在する |
| 操作 | (1) `grep -n "secrecy\|zeroize" deny.toml` でコメント行への記載を確認 → (2) `deny.toml` の `ignore = [` 以下に `secrecy` / `zeroize` の advisory ID が**含まれない**ことを確認 |
| 期待結果 | (1) `secrecy` / `zeroize` がコメント（`#` 行）に出現する / (2) `ignore = []` が空のまま、または `secrecy` / `zeroize` 関連 advisory ID を含まない |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*対応 Issue: #7 feat(shikomi-core): vault ドメイン型定義*
