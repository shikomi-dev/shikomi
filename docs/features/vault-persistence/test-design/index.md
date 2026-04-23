# テスト設計書（インデックス） — vault-persistence

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | vault-persistence（`shikomi-infra` への `VaultRepository` trait + SQLite 実装 + atomic write） |
| 対象 Issue | [#10](https://github.com/shikomi-dev/shikomi/issues/10) |
| 対象ブランチ | `feat/issue-10-vault-persistence` |
| 設計根拠 | `docs/features/vault-persistence/requirements-analysis.md`（REQ-P01〜P12・受入基準）、`docs/features/vault-persistence/basic-design.md`（モジュール構成）、`docs/features/vault-persistence/detailed-design.md`（エラー型・SQL・atomic write アルゴリズム） |
| テスト実行タイミング | `feat/issue-10-vault-persistence` → `develop` へのマージ前 |

### ファイル構成

| ファイル | 内容 |
|---------|------|
| `index.md`（本ファイル） | 概要・受入基準・テストマトリクス・E2E設計・モック方針・実行手順・カバレッジ基準 |
| `integration.md` | 結合テスト設計（TC-I01〜TC-I23）詳細 |
| `unit.md` | ユニットテスト設計（TC-U01〜TC-U16）詳細 |

---

## 2. テスト対象と受入基準

> **注**: AC-01〜AC-13 は `docs/features/vault-persistence/requirements-analysis.md` §受入基準に定義（元の受入基準#1〜#12に対応。#3が TC-03/04 の 2 ケースに分割のためAC番号に1ずれあり）。AC-14 は第1回レビュー差し戻しで追加（save 側 `.new` 残存検出、要件側受入基準#13）。AC-15〜AC-17 は第2回レビュー差し戻しで新設 REQ-P13〜P15 に対して追加。**要件側との対応**: セルのリナンバー後は要件側の受入基準番号と全AC番号が1:1対応する。

| 受入基準ID | 受入基準 | 検証レベル |
|-----------|---------|-----------|
| AC-01 | 全機能 REQ-P01〜REQ-P15 の型とメソッドが `shikomi-infra` の公開 API に存在する（`VaultLock`・`audit.rs`・`VaultDirReason` 含む） | 結合 |
| AC-02 | 平文 vault の `save` → `load` で同一 `Vault` が復元される（レコード順含む） | 結合 |
| AC-03 | 暗号化モード vault を `save` すると `PersistenceError::UnsupportedYet` が返る | 結合 |
| AC-04 | 暗号化モード vault を `load` すると `PersistenceError::UnsupportedYet` が返る | 結合 |
| AC-05 | `.new` ファイルを手動で残した状態で `load` を呼ぶと `PersistenceError::OrphanNewFile` が返る | 結合 |
| AC-06 | `AtomicWriter::write_new_only` テストフックを使って `.new` 書込完了後 rename なし状態を再現 → `vault.db` 本体が未変更で `OrphanNewFile` が返る | 結合 |
| AC-07 | vault ディレクトリが `0777` で作られている状態で `load` すると `PersistenceError::InvalidPermission` が返る | 結合（Unix） |
| AC-08 | vault.db に対し任意の UTF-8 文字列（絵文字含む）の label を保存し復元できる | 結合 |
| AC-09 | 生 SQL 連結を使っていない（`rusqlite::params!` マクロ経由でのみバインドしている） | 結合（静的 grep） |
| AC-10 | `cargo test -p shikomi-infra` が pass、行カバレッジ 80% 以上 | 結合（CI） |
| AC-11 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` pass | 結合（CI） |
| AC-12 | `SqliteVaultRepository::save` 直後に `stat` でファイルパーミッションを確認すると `0600` である | 結合（Unix） |
| AC-13 | 破損した SQLite ファイル（ゼロバイト / 不正バイト列）を渡すと `PersistenceError::Corrupted` または `PersistenceError::Sqlite` が返り panic しない | 結合 |
| AC-14 | `vault.db.new` が残存した状態で `save()` を呼ぶと `PersistenceError::OrphanNewFile` が返る（save 側の `.new` 残存検出、REQ-P05 の詳細設計 §save アルゴリズム step 3） | 結合 |
| AC-15 | `tracing` ログ（全レベル）に `SecretString` / `SecretBytes` / `plaintext_value` / `kdf_salt` / `wrapped_vek_*` の生値が一切出現しない（REQ-P14 監査ログ秘密漏洩防止） | 結合 |
| AC-16 | `SHIKOMI_VAULT_DIR` に `/etc/` / `..` 含むパス / シンボリックリンクを指定するとそれぞれ `PersistenceError::InvalidVaultDir` で拒否される（REQ-P15 `VaultPaths::new` 7段階バリデーション） | 結合（Unix） |
| AC-17 | `SqliteVaultRepository::save` 中に別プロセスが同ディレクトリで save を試みると `PersistenceError::Locked` が返る（REQ-P13 advisory lock 競合検知） | 結合 |

> **AC-06 改訂メモ**: 初版では「SIGKILL を子プロセスに送信」という非決定的テストとして定義していた。ペテルギウスレビューの指摘を受け「タイミング依存しない論理等価テスト」に差し替えた。`AtomicWriter::write_new_only`（`#[cfg(test)]` 限定フック）の実装が実装担当の必須タスクとなる。

---

## 3. テストマトリクス（トレーサビリティ）

| テストID | 受入基準ID | REQ-ID | 検証内容 | テストレベル | 種別 |
|---------|-----------|-------|---------|------------|------|
| TC-I01 | AC-01 | REQ-P01〜P15 | `cargo doc -p shikomi-infra --no-deps` が成功し公開型（`VaultLock`・`VaultDirReason` 含む）が出力される | 結合 | 正常系 |
| TC-I02 | AC-02 | REQ-P01, P02, P03, P04, P09 | 平文 vault（レコード 5 件）の save → load round-trip で `Vault` が等価復元される | 結合 | 正常系 |
| TC-I03 | AC-03 | REQ-P11 | 暗号化モード `Vault` を save → `UnsupportedYet` が返る。`.new` が作成されていない | 結合 | 異常系 |
| TC-I04 | AC-04 | REQ-P11 | `protection_mode='encrypted'` の vault.db を直接作成して load → `UnsupportedYet` が返る | 結合 | 異常系 |
| TC-I05 | AC-05 | REQ-P05 | `vault.db.new` 手動作成後 `load()` → `OrphanNewFile` が返る | 結合 | 異常系 |
| TC-I06 | AC-06 | REQ-P04 | `write_new_only` フックで `.new` のみ作成 → `vault.db` 未変更・`load()` が `OrphanNewFile` を返す | 結合 | 異常系 |
| TC-I07 | AC-07 | REQ-P06 | vault ディレクトリを `chmod 0777` → `load()` が `InvalidPermission` を返す（Unix） | 結合 | 異常系 |
| TC-I08 | AC-08 | REQ-P09, P12 | 絵文字・CJK・アラビア文字を含む label の save → load round-trip でバイト同一を確認 | 結合 | 境界値 |
| TC-I09 | AC-09 | REQ-P12 | SQL 文字列連結パターンの静的 grep → マッチ行ゼロ | 結合（静的） | 正常系 |
| TC-I10 | AC-10 | — | `cargo test -p shikomi-infra` が pass かつ行カバレッジ ≥ 80% | 結合（CI） | 正常系 |
| TC-I11 | AC-11 | — | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check --all` / `cargo deny check` が全て pass | 結合（CI） | 正常系 |
| TC-I12 | AC-12 | REQ-P06 | save 完了後 `stat(vault.db).mode() & 0o777 == 0o600` を確認（Unix） | 結合 | 正常系 |
| TC-I13 | AC-13 | REQ-P09, P10 | ゼロバイト vault.db → `Sqlite` or `SchemaMismatch` が返り panic しない | 結合 | 異常系 |
| TC-I14 | AC-13 | REQ-P09, P10 | 不正バイト列 vault.db → `Sqlite` or `SchemaMismatch` が返り panic しない | 結合 | 異常系 |
| TC-I15 | AC-14 | REQ-P05 | `vault.db.new` 手動作成後 `save()` → `OrphanNewFile` が返る。`vault.db` 未変更 | 結合 | 異常系 |
| TC-I16 | — | REQ-P01 | `exists()` が vault.db 非存在時に `Ok(false)` を返す | 結合 | 正常系 |
| TC-I17 | — | REQ-P01 | `exists()` が vault.db 存在時に `Ok(true)` を返す（save 後に確認） | 結合 | 正常系 |
| TC-I18 | — | REQ-P08 | `SHIKOMI_VAULT_DIR` 環境変数を tempdir に設定 → 指定ディレクトリに vault.db が作成される（`serial_test` クレートで直列化） | 結合 | 正常系 |
| TC-I19 | AC-02 | REQ-P02, P03 | ゼロレコード平文 vault の save → load round-trip（境界値） | 結合 | 境界値 |
| TC-I20 | — | REQ-P03, P12 | CHECK 制約違反の生 SQL を直接実行 → `SQLITE_CONSTRAINT_CHECK` エラーが返る（防衛線確認） | 結合 | 異常系 |
| TC-I21 | AC-17 | REQ-P13 | 子プロセス 2 本が同 vault ディレクトリで `save()` を同時呼出 → 後発プロセスが `PersistenceError::Locked` を返す | 結合 | 異常系 |
| TC-I22 | AC-16 | REQ-P15 | `SHIKOMI_VAULT_DIR` に `..` 含むパス / シンボリックリンク / `/etc/` 配下 をそれぞれ指定 → 全ケースで `PersistenceError::InvalidVaultDir` が返る（Unix） | 結合 | 異常系 |
| TC-I23 | AC-15 | REQ-P14 | `tracing-test` で全操作（save/load/exists/error）のログを収集し、秘密値パターンがマッチしないことを確認 | 結合 | 正常系 |
| TC-U01 | — | REQ-P08, P15 | `VaultPaths::new` が正常パスで `Ok(VaultPaths)` を返し `vault_db()` / `vault_db_new()` / `vault_db_lock()` が正しいパスを返す | ユニット | 正常系 |
| TC-U02 | — | REQ-P09 | `Mapping::vault_header_to_params` が平文モードで `kdf_salt = None` を返す | ユニット | 正常系 |
| TC-U03 | — | REQ-P09 | `Mapping::row_to_vault_header` が正常行から `VaultHeader` を復元する | ユニット | 正常系 |
| TC-U04 | — | REQ-P09, P10 | `Mapping::row_to_vault_header` に `protection_mode='unknown'` → `Corrupted { UnknownProtectionMode }` | ユニット | 異常系 |
| TC-U05 | — | REQ-P09, P10 | `Mapping::row_to_vault_header` に不正 RFC3339 → `Corrupted { InvalidRfc3339 }` | ユニット | 異常系 |
| TC-U06 | — | REQ-P09 | `Mapping::row_to_record` が正常な `'plaintext'` 行から `Record` を復元する | ユニット | 正常系 |
| TC-U07 | — | REQ-P09, P10 | `Mapping::row_to_record` に不正 UUID 文字列 → `Corrupted { InvalidUuidString }` | ユニット | 異常系 |
| TC-U08 | — | REQ-P09, P10 | `Mapping::row_to_record` に `payload_variant='plaintext'` かつ `plaintext_value=NULL` → `Corrupted { NullViolation }` | ユニット | 異常系 |
| TC-U09 | — | REQ-P06 | `PermissionGuard::verify_dir` に `mode=0o700` → `Ok(())` （Unix） | ユニット | 正常系 |
| TC-U10 | — | REQ-P06 | `PermissionGuard::verify_dir` に `mode=0o755` → `Err(InvalidPermission)` （Unix） | ユニット | 異常系 |
| TC-U11 | — | REQ-P06 | `PermissionGuard::verify_file` に `mode=0o600` → `Ok(())` （Unix） | ユニット | 正常系 |
| TC-U12 | — | REQ-P06 | `PermissionGuard::verify_file` に `mode=0o644` → `Err(InvalidPermission)` （Unix） | ユニット | 異常系 |
| TC-U13 | AC-16 | REQ-P15 | `VaultPaths::new` に相対パスを渡す → `Err(InvalidVaultDir { reason: NotAbsolute })` | ユニット | 異常系 |
| TC-U14 | AC-16 | REQ-P15 | `VaultPaths::new` に `..` を含むパスを渡す → `Err(InvalidVaultDir { reason: PathTraversal })` | ユニット | 異常系 |
| TC-U15 | AC-16 | REQ-P15 | `VaultPaths::new` にシンボリックリンクを渡す → `Err(InvalidVaultDir { reason: SymlinkNotAllowed })` （Unix） | ユニット | 異常系 |
| TC-U16 | AC-16 | REQ-P15 | `VaultPaths::new` に `/etc/` 配下のパスを渡す → `Err(InvalidVaultDir { reason: ProtectedSystemArea })` （Unix） | ユニット | 異常系 |

---

## 4. E2Eテスト設計

**省略理由**: `shikomi-infra` は `VaultRepository` を提供する内部ライブラリ crate であり、エンドユーザーが直接操作する CLI / GUI / 公開 HTTP API を持たない。

テスト戦略ガイドの方針「エンドユーザー操作（UI/CLI/公開API）がない場合は結合テストで代替可」に従い、E2E は本 feature では設計対象外とする。エンドユーザーが `shikomi-infra` を直接触れるのは後続 Issue（`shikomi-cli` / `shikomi-daemon`）が公開される段階。受入基準 AC-01〜AC-17 は `integration.md` の結合テストで網羅する。

---

## 5. モック方針

| 検証対象 | モック要否 | 理由 |
|---------|---------|------|
| `rusqlite::Connection` | **不要**（本物を使用） | in-memory または tempdir の実 SQLite で結合テスト。モックでは CHECK 制約・PRAGMA 動作が再現できない |
| ファイルシステム（`std::fs`） | **不要**（tempdir を使用） | `tempfile::TempDir` で OS 標準ファイルシステムを使う。atomic rename / fsync の挙動が再現できる |
| `dirs::data_dir()` | **不要**（環境変数 `SHIKOMI_VAULT_DIR` で override） | TC-I18 で環境変数による上書きを使う。`dirs` crate 自体はモックしない |
| `shikomi-core` ドメイン型 | **不要** | ドメイン型は実装の一部（`shikomi-core` crate）を直接利用する。assumed mock 禁止 |
| OS パーミッション API | **不要** | 実際の `stat` / `chmod` を使う。OS 依存ケースは `#[cfg(unix)]` でガード |
| 外部 API / ネットワーク | **対象外** | 本 crate は外部 API を持たない |

---

## 6. 実行手順と証跡

### 実行環境

| 項目 | 内容 |
|------|------|
| Rust toolchain | `rustup` でインストール済み（`stable` チャネル） |
| cargo-llvm-cov | `cargo install cargo-llvm-cov` |
| cargo-deny | `cargo install cargo-deny` |
| serial_test | `Cargo.toml` の `[dev-dependencies]` に `serial_test = "3"` を追加（`SHIKOMI_VAULT_DIR` 環境変数を書き換える TC-I18 はプロセス内の環境変数状態を変更するため、他テストと並列実行すると干渉する。`#[serial]` アトリビュートで直列化すること） |
| OS | Linux / macOS（Unix ケース）。Windows CI でも `#[cfg(unix)]` ガード付きケース以外は通過する |

### 実行コマンド例

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

echo "=== TC-I10, TC-U*: cargo test -p shikomi-infra ==="
cargo test -p shikomi-infra 2>&1
echo "TC-I10 (test): PASS"

echo "=== TC-I10: カバレッジ確認 ==="
cargo llvm-cov -p shikomi-infra --summary-only 2>&1 | tee /tmp/coverage.txt
echo "TC-I10 (coverage): 確認 → /tmp/coverage.txt を参照"

echo "=== TC-I01: cargo doc ==="
cargo doc -p shikomi-infra --no-deps 2>&1
echo "TC-I01: PASS"

echo "=== TC-I11: clippy / fmt / deny ==="
cargo clippy --workspace -- -D warnings 2>&1
echo "  clippy: PASS"
cargo fmt --check --all 2>&1
echo "  fmt: PASS"
cargo deny check 2>&1
echo "  deny: PASS"
echo "TC-I11: PASS"

echo "=== TC-I09: SQL インジェクション静的 grep ==="
SQL_CONCAT=$(grep -rEn 'format!\s*\(.*(?:SELECT|INSERT|UPDATE|DELETE|PRAGMA)' \
  --include="*.rs" crates/shikomi-infra/src/ || true)
if [ -z "$SQL_CONCAT" ]; then
  echo "TC-I09: PASS (SQL 文字列連結なし)"
else
  echo "TC-I09: FAIL (SQL 文字列連結を検出):"
  echo "$SQL_CONCAT"
  exit 1
fi

echo "=== 全テスト PASS ==="
```

### 証跡

- テスト実行結果（stdout/stderr/exit code）を Markdown で記録する
- 結果ファイルを `/app/shared/attachments/マユリ/` に保存して Discord に添付する
- `cargo llvm-cov` のカバレッジサマリを証跡として添付する
- TC-I06（atomic write 論理等価テスト）は `write_new_only` フック呼出前後の vault.db / vault.db.new の存在確認を記録する

---

## 7. カバレッジ基準

| 観点 | 基準 |
|------|------|
| 受入基準の網羅 | AC-01〜AC-17 全項目が TC-I01〜TC-I23 / TC-U01〜TC-U16 のいずれかで網羅されていること（§3 テストマトリクス参照） |
| 正常系 | 全 正常系テストケース必須（TC-I02, I16, I17, I18, I19, I23, TC-U01〜U03, U06, U09, U11） |
| 異常系 | 全 11 種 `PersistenceError` バリアント（`Io` / `Sqlite` / `Corrupted` / `InvalidPermission` / `InvalidVaultDir` / `OrphanNewFile` / `AtomicWriteFailed` / `SchemaMismatch` / `UnsupportedYet` / `CannotResolveVaultDir` / `Locked`）が少なくとも 1 ケースで発生・検証されること。`panic!` が一切発生しないことを確認 |
| 境界値 | ゼロレコード（TC-I19）、絵文字ラベル（TC-I08）、ゼロバイトファイル（TC-I13）、不正バイト列（TC-I14） |
| カバレッジ数値 | `cargo llvm-cov` 行カバレッジ 80% 以上（AC-10） |
| Fail Secure ケース | `.new` 残存 load 側（TC-I05）・`.new` 残存 save 側（TC-I15）・OS パーミッション異常（TC-I07, TC-I12, TC-U10, TC-U12）・暗号化モード拒否（TC-I03, TC-I04）・VaultDir バリデーション（TC-I22, TC-U13〜U16）・ロック競合（TC-I21）が**必須**（省略不可） |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-23*
*改訂 v2: 涅マユリ（テスト担当）/ 2026-04-23 — 第1回レビュー差し戻し対応: ① `test-design/` ディレクトリ分割 ② AC-14 追加（TC-I15 トレーサビリティ修正） ③ TC-I06 を決定的テストに変更（`write_new_only` フック） ④ §6 実行環境に `serial_test` 明記*
*改訂 v3: 涅マユリ（テスト担当）/ 2026-04-23 — 第2回レビュー差し戻し対応: ⑤ AC-01 範囲を REQ-P15 まで更新 ⑥ AC-15〜17 追加（REQ-P14/P15/P13 対応） ⑦ TC-I21〜I23（VaultLock競合・VaultPaths::newバリデーション・tracing秘密漏洩）追加 ⑧ TC-U01 更新（VaultPaths::new が Result 返却）⑨ TC-U13〜U16（VaultPaths::new 異常系 4 件）追加 ⑩ カバレッジ基準のバリアント数を 11 種に修正*
*対応 Issue: #10 feat(shikomi-infra): vault 永続化層（平文モード）*
