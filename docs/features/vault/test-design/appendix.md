# テスト設計書 — vault: モック方針・ディレクトリ構造・実行手順・カバレッジ

> **ファイル構成**
> | ファイル | 内容 |
> |---------|------|
> | [`overview.md`](overview.md) | §1概要・§2受入基準・§3テストマトリクス・§4E2Eテスト |
> | [`integration.md`](integration.md) | §5 結合テスト設計（TC-I01〜TC-I07） |
> | [`unit.md`](unit.md) | §6 ユニットテスト設計（TC-U01〜TC-U12） |
> | `appendix.md`（本ファイル） | §7 モック方針・§8 ディレクトリ構造・§9 実行手順・§10 カバレッジ |

---

## 7. モック方針

| 検証対象 | モック要否 | 理由 |
|---------|---------|------|
| `shikomi-core` 内の型と関数 | **不要** | pure Rust / no-I/O。外部 API・DB・ファイルシステム・OS API を一切呼ばない |
| 時刻（`OffsetDateTime`） | **不要**（テスト用固定値を直接渡す） | `OffsetDateTime::now_utc()` を呼ぶのはテストではなく呼び出し側責務。型が値を受け取るだけ |
| 乱数源（`NonceCounter::new` の random_prefix） | **不要**（`[0u8; 8]` 等の固定値を使う） | `shikomi-core` は乱数源を持たない設計（詳細設計 §注記6） |
| `cargo` コマンド（TC-I06） | **不要**（本物を使用） | 実際のツールチェーンを使う |
| `VekProvider` trait（TC-U07-14〜16） | **test double を使用**（モックではない） | `shikomi-infra` 実装がスケルトン段階で存在しないため、`#[cfg(test)]` 内に `DummyVekProvider { should_fail: bool }` を定義し trait を実装する。外部 I/O の代替でなく trait 境界を満たす最小実装。「自分が書いたコードはモックするな」の対象外 |

外部 I/O を持たないため assumed mock は発生しない。Characterization test も不要。

## 8. テストディレクトリ構造

Rust 慣習に従う（テスト戦略ガイド §テストディレクトリ構造 参照）。

```
crates/shikomi-core/
  src/
    vault/
      mod.rs         # TC-U07, TC-U12（Vault / Record サブ秒丸め）
      protection_mode.rs  # TC-U01
      version.rs     # TC-U02
      header.rs      # TC-U03
      id.rs          # TC-U04
      record.rs      # TC-U05, TC-U06
      nonce.rs       # TC-U10（TC-U10-08 big-endian 検証を含む）
      crypto_data.rs # TC-U11（Aad::new Fail Fast / to_canonical_bytes / 黄金値 TC-U11-04）
    secret/
      mod.rs         # TC-U08
    error.rs         # TC-U09（DomainError Display）
  tests/
    vault_lifecycle.rs      # TC-I01, TC-I02
    record_label_boundary.rs # TC-I03
    secret_no_leak.rs       # TC-I04
    nonce_overflow.rs       # TC-I05
    ci_commands.rs          # TC-I06（cargo コマンド確認は CI 環境で実施）
    deny_toml_check.rs      # TC-I07（deny.toml grep 確認。fs::read_to_string で読み込み assert）
```

**テスト命名規則**: `test_何をした時_どうなるべきか`（例: `test_add_record_with_mode_mismatch_returns_consistency_error`）

## 9. 実行手順と証跡

### 実行コマンド（ユニット + 結合テスト）

```bash
# shikomi-core のみ
cargo test -p shikomi-core --verbose 2>&1

# ワークスペース全体
cargo test --workspace 2>&1
```

### CI コマンド確認（TC-I06）

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --check --all
cargo clippy --workspace -- -D warnings
cargo deny check
```

### 証跡

- テスト実行結果（stdout/stderr/exit code）を Markdown で記録する
- 結果ファイルを `/app/shared/attachments/マユリ/` に保存して Discord に添付する

## 10. カバレッジ基準

| 観点 | 基準 |
|------|------|
| 受入基準の網羅 | AC-01〜AC-09 が全テストケースで網羅されていること |
| 行カバレッジ | `cargo test -p shikomi-core` で 80% 以上（受入基準 AC-06）。計測は `cargo llvm-cov` 等で行う |
| 正常系 | 全ケース必須（ユニット 84 件 + 結合 7 件 = 合計 91 件） |
| 異常系 | エラーバリアントの種別まで検証（`assert!(matches!(err, DomainError::Xxx))` レベル） |
| 境界値 | `RecordLabel`（0/1/254/255/256 grapheme）、`NonceCounter`（u32::MAX-1 / u32::MAX）、`VaultVersion`（0/1/2）を必須とする |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — AC-09（deny.toml 暗号クリティカル crate 確認）追加・TC-I07 追加。TC-U09（DomainError Display MSG-DEV-001〜009）追加。TC-U07-14〜16（Vault::rekey_with テスト）追加。合計 60件→73件*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — セル commit db52923 対応: TC-U09 の MSG-DEV 番号を 001〜008 に詰め直し（旧 MSG-DEV-006 InvalidSecretLength 削除）。TC-U11（Aad::new Fail Fast / to_canonical_bytes バイナリ正規形）追加。合計 73件→75件*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — ペテルギウス第3ラウンド指摘対応: TC-U11-04（Aad黄金値テスト・26byte完全一致）追加。TC-U10-08（NonceCounter下位4B big-endianバイトオーダー検証）追加。TC-U12（Record::new サブ秒丸めround-trip 2件）追加。合計 75件→91件（ユニット84件+結合7件。旧「79件」はサブケース集計漏れによる誤記）*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — ペガサス指摘対応: test-design.md（507行）を test-design/ ディレクトリに分割。overview.md / integration.md / unit.md / appendix.md の4ファイル構成に再編。*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — ペテルギウス指摘対応: §10 カバレッジ基準の件数を「72+7=79件」から「84+7=91件」に訂正（マトリクス実数との乖離解消）。*
*対応 Issue: #7 feat(shikomi-core): vault ドメイン型定義*
