# テスト設計書 — vault-persistence（vault 永続化層 平文モード）

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | vault-persistence（`shikomi-infra` への `VaultRepository` trait + SQLite 実装 + atomic write） |
| 対象 Issue | [#10](https://github.com/shikomi-dev/shikomi/issues/10) |
| 対象ブランチ | `feat/issue-10-vault-persistence` |
| 設計根拠 | `docs/features/vault-persistence/requirements-analysis.md`（REQ-P01〜P12・受入基準12項目）、`docs/features/vault-persistence/basic-design.md`（モジュール構成）、`docs/features/vault-persistence/detailed-design.md`（エラー型・SQL・atomic write アルゴリズム） |
| テスト実行タイミング | `feat/issue-10-vault-persistence` → `develop` へのマージ前 |

---

## 2. テスト対象と受入基準

| 受入基準ID | 受入基準 | 検証レベル |
|-----------|---------|-----------|
| AC-01 | 全機能 REQ-P01〜REQ-P12 の型とメソッドが `shikomi-infra` の公開 API に存在する | 結合 |
| AC-02 | 平文 vault の `save` → `load` で同一 `Vault` が復元される（レコード順含む） | 結合 |
| AC-03 | 暗号化モード vault を `save` すると `PersistenceError::UnsupportedYet` が返る | 結合 |
| AC-04 | 暗号化モード vault を `load` すると `PersistenceError::UnsupportedYet` が返る | 結合 |
| AC-05 | `.new` ファイルを手動で残した状態で `load` を呼ぶと `PersistenceError::OrphanNewFile` が返る | 結合 |
| AC-06 | save 中に SIGKILL 相当のクラッシュを再現 → 再起動後 vault.db 本体が破損しないこと | 結合 |
| AC-07 | vault ディレクトリが `0777` で作られている状態で `load` すると `PersistenceError::InvalidPermission` が返る | 結合（Unix） |
| AC-08 | vault.db に対し任意の UTF-8 文字列（絵文字含む）の label を保存し復元できる | 結合 |
| AC-09 | 生 SQL 連結を使っていない（`rusqlite::params!` マクロ経由でのみバインドしている） | 結合（静的 grep） |
| AC-10 | `cargo test -p shikomi-infra` が pass、行カバレッジ 80% 以上 | 結合（CI） |
| AC-11 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` pass | 結合（CI） |
| AC-12 | `SqliteVaultRepository::save` 直後に `stat` でファイルパーミッションを確認すると `0600` である | 結合（Unix） |
| AC-13 | 破損した SQLite ファイル（ゼロバイト / 不正バイト列）を渡すと `PersistenceError::Corrupted` または `PersistenceError::Sqlite` が返り panic しない | 結合 |

---

## 3. テストマトリクス（トレーサビリティ）

| テストID | 受入基準ID | REQ-ID | 検証内容 | テストレベル | 種別 |
|---------|-----------|-------|---------|------------|------|
| TC-I01 | AC-01 | REQ-P01〜P12 | `cargo doc -p shikomi-infra --no-deps` が成功し、`VaultRepository` / `SqliteVaultRepository` / `PersistenceError` 等の公開型が出力される | 結合 | 正常系 |
| TC-I02 | AC-02 | REQ-P01, P02, P03, P04, P09 | 平文 vault（レコード 1 件）の save → load round-trip で `Vault` が等価復元される | 結合 | 正常系 |
| TC-I03 | AC-03 | REQ-P11 | `VaultHeader::Encrypted` を持つ `Vault` を save → `PersistenceError::UnsupportedYet` が返る | 結合 | 異常系 |
| TC-I04 | AC-04 | REQ-P11 | `protection_mode='encrypted'` の vault.db を直接作成して load → `PersistenceError::UnsupportedYet` が返る | 結合 | 異常系 |
| TC-I05 | AC-05 | REQ-P05 | `vault.db.new` を手動作成した状態で `load()` を呼ぶ → `PersistenceError::OrphanNewFile` が返る | 結合 | 異常系 |
| TC-I06 | AC-06 | REQ-P04 | save 中に子プロセスを SIGKILL → 親で `vault.db` が未変更であることを確認。`vault.db.new` が残存する場合は `OrphanNewFile` を返すことも確認 | 結合 | 異常系 |
| TC-I07 | AC-07 | REQ-P06 | vault ディレクトリを `chmod 0777` した状態で `load()` → `PersistenceError::InvalidPermission` が返る（Unix のみ） | 結合 | 異常系 |
| TC-I08 | AC-08 | REQ-P09, P12 | 絵文字・CJK・制御文字を含む UTF-8 label を save → load して同一文字列が復元される | 結合 | 境界値 |
| TC-I09 | AC-09 | REQ-P12 | `format!` / 文字列 `+` で SQL を組み立てている箇所がないことを静的 grep で確認 | 結合（静的） | 正常系 |
| TC-I10 | AC-10 | — | `cargo test -p shikomi-infra` が pass かつ `cargo llvm-cov -p shikomi-infra` で行カバレッジ ≥ 80% | 結合（CI） | 正常系 |
| TC-I11 | AC-11 | — | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check --all` / `cargo deny check` が全て pass | 結合（CI） | 正常系 |
| TC-I12 | AC-12 | REQ-P06 | `SqliteVaultRepository::save` 完了後に `stat(vault.db).permissions().mode() & 0o777` が `0o600` であることを確認（Unix のみ） | 結合 | 正常系 |
| TC-I13 | AC-13 | REQ-P09, P10 | ゼロバイトの `vault.db` を配置して `load()` → `PersistenceError::Sqlite` または `PersistenceError::SchemaMismatch` が返り panic しない | 結合 | 異常系 |
| TC-I14 | AC-13 | REQ-P09, P10 | 不正バイト列（ランダムバイト）の `vault.db` を配置して `load()` → `PersistenceError::Sqlite` または `PersistenceError::SchemaMismatch` が返り panic しない | 結合 | 異常系 |
| TC-I15 | — | REQ-P05 | `vault.db.new` を手動作成した状態で `save()` を呼ぶ → `PersistenceError::OrphanNewFile` が返る（save 側でも `.new` 残存を検出する） | 結合 | 異常系 |
| TC-I16 | — | REQ-P01 | `exists()` が vault.db 非存在時に `Ok(false)` を返す | 結合 | 正常系 |
| TC-I17 | — | REQ-P01 | `exists()` が vault.db 存在時に `Ok(true)` を返す（save 後に確認） | 結合 | 正常系 |
| TC-I18 | — | REQ-P08 | `SHIKOMI_VAULT_DIR` 環境変数を tempdir に設定した状態で `SqliteVaultRepository::new()` を呼ぶ → 指定ディレクトリに vault.db が作成される | 結合 | 正常系 |
| TC-I19 | AC-02 | REQ-P02, P03 | ゼロレコードの平文 vault の save → load round-trip で `Vault` が等価復元される（ヘッダのみ） | 結合 | 境界値 |
| TC-I20 | — | REQ-P03, P12 | `rusqlite::Connection` で CHECK 制約に違反する生 SQL を直接実行 → `SQLITE_CONSTRAINT_CHECK` エラーが返る（防衛線の確認） | 結合 | 異常系 |
| TC-U01 | — | REQ-P08 | `VaultPaths::new(dir)` で `vault_db()` / `vault_db_new()` が期待パス（`dir/vault.db` / `dir/vault.db.new`）を返す | ユニット | 正常系 |
| TC-U02 | — | REQ-P09 | `Mapping::vault_header_to_params` が平文モード `VaultHeader` を正しいパラメータ束に変換する（`kdf_salt` = `None` 等） | ユニット | 正常系 |
| TC-U03 | — | REQ-P09 | `Mapping::row_to_vault_header` が正常な `'plaintext'` 行から `VaultHeader` を復元する | ユニット | 正常系 |
| TC-U04 | — | REQ-P09, P10 | `Mapping::row_to_vault_header` に `protection_mode = 'unknown'` を渡す → `Corrupted { reason: UnknownProtectionMode }` が返る | ユニット | 異常系 |
| TC-U05 | — | REQ-P09, P10 | `Mapping::row_to_vault_header` に不正な RFC3339 文字列を渡す → `Corrupted { reason: InvalidRfc3339 }` が返る | ユニット | 異常系 |
| TC-U06 | — | REQ-P09 | `Mapping::row_to_record` が正常な `'plaintext'` レコード行から `Record` を復元する | ユニット | 正常系 |
| TC-U07 | — | REQ-P09, P10 | `Mapping::row_to_record` に不正な UUID 文字列を渡す → `Corrupted { reason: InvalidUuidString }` が返る | ユニット | 異常系 |
| TC-U08 | — | REQ-P09, P10 | `Mapping::row_to_record` に `payload_variant = 'plaintext'` かつ `plaintext_value = NULL` を渡す → `Corrupted { reason: NullViolation }` が返る | ユニット | 異常系 |
| TC-U09 | — | REQ-P06 | `PermissionGuard::verify_dir` に `mode = 0o700` のディレクトリを渡す → `Ok(())` が返る（Unix のみ） | ユニット | 正常系 |
| TC-U10 | — | REQ-P06 | `PermissionGuard::verify_dir` に `mode = 0o755` のディレクトリを渡す → `Err(InvalidPermission)` が返る（Unix のみ） | ユニット | 異常系 |
| TC-U11 | — | REQ-P06 | `PermissionGuard::verify_file` に `mode = 0o600` のファイルを渡す → `Ok(())` が返る（Unix のみ） | ユニット | 正常系 |
| TC-U12 | — | REQ-P06 | `PermissionGuard::verify_file` に `mode = 0o644` のファイルを渡す → `Err(InvalidPermission)` が返る（Unix のみ） | ユニット | 異常系 |

---

## 4. E2Eテスト設計

**省略理由**: `shikomi-infra` は `VaultRepository` を提供する内部ライブラリ crate であり、エンドユーザーが直接操作する CLI / GUI / 公開 HTTP API を持たない。

テスト戦略ガイドの方針「エンドユーザー操作（UI/CLI/公開API）がない場合は結合テストで代替可」に従い、E2E は本 feature では設計対象外とする。エンドユーザーが `shikomi-infra` を直接触れるのは、後続 Issue（`shikomi-cli` / `shikomi-daemon`）が公開される段階。受入基準の全 12 項目は §5 の結合テストで網羅する。

---

## 5. 結合テスト設計

> **ツール選択根拠**: このシステムは Rust ライブラリ crate であり、エントリポイントは Rust 公開 API（`SqliteVaultRepository::save` / `load` / `exists`）。Rust の統合テスト（`crates/shikomi-infra/tests/` 配下）で `tempfile::TempDir` を使い、実際の SQLite ファイルに対して結合テストを行う。外部 API / 外部サービスへの依存はなく、モックは不要（全て本物の `rusqlite` + ファイルシステムを使用）。OS パーミッション検証ケース（TC-I07, TC-I12）は `#[cfg(unix)]` でガードし、Windows CI では自動スキップ。

### TC-I01: 公開 API ドキュメント確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I01 |
| 対応する受入基準ID | AC-01 |
| 対応する工程 | 基本設計（モジュール構成、REQ-P01〜P12） |
| 種別 | 正常系 |
| 前提条件 | `feat/issue-10-vault-persistence` ブランチで実装コミット済み |
| 操作 | `cargo doc -p shikomi-infra --no-deps` を実行し、出力 HTML に `VaultRepository` / `SqliteVaultRepository` / `PersistenceError` / `CorruptedReason` / `AtomicWriteStage` / `VaultPaths` の各型が記載されていることを確認 |
| 期待結果 | exit code == 0。`target/doc/shikomi_infra/persistence/` 配下に上記型のドキュメントが生成される |

---

### TC-I02: 平文 vault round-trip（レコード複数件）

| 項目 | 内容 |
|------|------|
| テストID | TC-I02 |
| 対応する受入基準ID | AC-02 |
| 対応する工程 | 基本設計（REQ-P01, P02, P03, P04, P09） |
| 種別 | 正常系 |
| 前提条件 | `tempfile::TempDir` を作成済み。`shikomi-core` の `Vault::new_plaintext` / `Record` 組立が可能 |
| 操作 | 1. `SqliteVaultRepository::with_dir(tempdir)` でリポジトリ構築 2. 平文モードの `Vault` を 5 件のレコード（種別・ラベル・値を多様に）で組立 3. `repo.save(&vault)` 4. `repo.load()` |
| 期待結果 | `load()` の戻り値が `Ok(vault2)` であり、`vault.header()` == `vault2.header()`（`protection_mode`, `vault_version`, `created_at` 等）、`vault.records()` と `vault2.records()` がレコード順（`created_at ASC, id ASC`）で一致する |

---

### TC-I03: 暗号化モード vault を save → UnsupportedYet

| 項目 | 内容 |
|------|------|
| テストID | TC-I03 |
| 対応する受入基準ID | AC-03 |
| 対応する工程 | 詳細設計（REQ-P11、save アルゴリズム step 1） |
| 種別 | 異常系 |
| 前提条件 | `VaultHeader::new_encrypted` で暗号化モードのヘッダを組立可能であること（`shikomi-core` の API による） |
| 操作 | 1. `SqliteVaultRepository::with_dir(tempdir)` 2. 暗号化モードの `Vault` を構築 3. `repo.save(&vault)` |
| 期待結果 | `Err(PersistenceError::UnsupportedYet { feature: "encrypted vault persistence", .. })` が返る。`vault.db.new` が作成されていない（step 2 以降のファイル操作が一切実行されていない） |

---

### TC-I04: 暗号化モード vault.db を load → UnsupportedYet

| 項目 | 内容 |
|------|------|
| テストID | TC-I04 |
| 対応する受入基準ID | AC-04 |
| 対応する工程 | 詳細設計（REQ-P11、load アルゴリズム step 10） |
| 種別 | 異常系 |
| 前提条件 | tempdir 配下に `protection_mode='encrypted'` を持つ vault.db を `rusqlite` で直接作成済み（スキーマは本 Issue の DDL に準拠、暗号化カラムに適当な BLOB を挿入） |
| 操作 | `repo.load()` |
| 期待結果 | `Err(PersistenceError::UnsupportedYet { .. })` が返る |

---

### TC-I05: .new 残存 + load → OrphanNewFile

| 項目 | 内容 |
|------|------|
| テストID | TC-I05 |
| 対応する受入基準ID | AC-05 |
| 対応する工程 | 詳細設計（REQ-P05、load アルゴリズム step 2） |
| 種別 | 異常系 |
| 前提条件 | tempdir に `vault.db` が存在し、かつ `vault.db.new` を `std::fs::write` で空ファイルとして作成済み |
| 操作 | `repo.load()` |
| 期待結果 | `Err(PersistenceError::OrphanNewFile { path })` が返る。`path` が `vault.db.new` の絶対パスと一致する。`vault.db` は変更されていない |

---

### TC-I06: atomic write クラッシュ耐性（SIGKILL 再現）

| 項目 | 内容 |
|------|------|
| テストID | TC-I06 |
| 対応する受入基準ID | AC-06 |
| 対応する工程 | 詳細設計（REQ-P04、atomic write アルゴリズム） |
| 種別 | 異常系 |
| 前提条件 | Unix 環境。tempdir に既存の `vault.db`（平文 vault）が save 済み。初期 vault の内容（レコード数・ラベル）を記録済み |
| 操作 | 1. `std::process::Command` で子プロセスを起動し、子プロセスが `save()` の `write_new` 段階（fsync 前）まで進んだところで `SIGKILL` を送信（子プロセス内で `write_new` 完了後 `fsync_and_rename` 直前に `SIGKILL` を受け取るよう細工、または子プロセス起動直後に kill） 2. 親プロセスで `vault.db` の内容を `repo.load()` で確認 |
| 期待結果 | 選択肢 A（rename 前に kill）: `vault.db` が初期状態のまま。`repo.load()` が成功し、初期 vault と同一 `Vault` を返す。または `Err(OrphanNewFile)` が返り panic なし（`.new` が残存した場合）。選択肢 B（vault.db 未存在で kill）: `repo.exists()` が `Ok(false)`、`vault.db.new` が存在すれば `OrphanNewFile`。いずれの場合も `vault.db` が中途半端な状態にはならない |

---

### TC-I07: 0777 ディレクトリ + load → InvalidPermission（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-I07 |
| 対応する受入基準ID | AC-07 |
| 対応する工程 | 詳細設計（REQ-P06、load アルゴリズム step 1） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(unix)]` ガード付き。tempdir を `chmod 0o777` で変更済み（`std::os::unix::fs::PermissionsExt::set_mode` を使用） |
| 操作 | `repo.load()` |
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected, actual })` が返る。`path` が vault ディレクトリ、`actual` に `"0777"` 相当の情報が含まれる |

---

### TC-I08: UTF-8 特殊文字ラベルの round-trip

| 項目 | 内容 |
|------|------|
| テストID | TC-I08 |
| 対応する受入基準ID | AC-08 |
| 対応する工程 | 詳細設計（REQ-P09, P12、Mapping::record_to_params） |
| 種別 | 境界値 |
| 前提条件 | 以下の文字列が `RecordLabel::try_new` で受理されること（`shikomi-core` の制約内）: 絵文字（🗝️🔒💀）、CJK 文字（漢字・ひらがな）、アラビア文字、制御文字を除く全 Unicode コードポイント |
| 操作 | 1. label に `"🗝️秘密のキー💀شيكومي"` を設定したレコードを持つ `Vault` を save 2. load で復元 |
| 期待結果 | 復元された `Record` の `label()` が save 時と**バイト単位で同一の文字列**を返す |

---

### TC-I09: SQL インジェクション禁止設計の静的 grep 確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I09 |
| 対応する受入基準ID | AC-09 |
| 対応する工程 | 詳細設計（REQ-P12、SchemaSql 設計判断） |
| 種別 | 正常系 |
| 前提条件 | `crates/shikomi-infra/src/` 配下にソースが存在する |
| 操作 | `grep -rEn 'format!\s*\(\s*".*\{.*\}.*sql\|"[^"]*\{[^"]*\}[^"]*"\s*\+' --include="*.rs" crates/shikomi-infra/src/` および `grep -rEn '".*SELECT\|.*INSERT\|.*UPDATE\|.*DELETE.*".*\+' --include="*.rs" crates/shikomi-infra/src/` を実行（SQL 文字列の連結パターン検索） |
| 期待結果 | マッチ行ゼロ。全 SQL は `const` リテラルで定義され、`params!` マクロ経由のバインドのみ使用している |

---

### TC-I10: cargo test + カバレッジ

| 項目 | 内容 |
|------|------|
| テストID | TC-I10 |
| 対応する受入基準ID | AC-10 |
| 対応する工程 | — |
| 種別 | 正常系 |
| 前提条件 | `cargo-llvm-cov` インストール済み |
| 操作 | `cargo test -p shikomi-infra` および `cargo llvm-cov -p shikomi-infra --summary-only` を実行 |
| 期待結果 | `cargo test` が exit code == 0。`llvm-cov` の行カバレッジ（`Line` coverage）が 80% 以上 |

---

### TC-I11: cargo clippy / fmt / deny

| 項目 | 内容 |
|------|------|
| テストID | TC-I11 |
| 対応する受入基準ID | AC-11 |
| 対応する工程 | — |
| 種別 | 正常系 |
| 前提条件 | `deny.toml` がリポジトリルートに存在する |
| 操作 | `cargo clippy --workspace -- -D warnings`、`cargo fmt --check --all`、`cargo deny check` を順に実行 |
| 期待結果 | 全コマンドが exit code == 0 |

---

### TC-I12: save 後のファイルパーミッション確認（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-I12 |
| 対応する受入基準ID | AC-12 |
| 対応する工程 | 詳細設計（REQ-P06、AtomicWriter::write_new step 4） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(unix)]` ガード付き。tempdir を使用 |
| 操作 | 1. `repo.save(&vault)` 2. `std::fs::metadata(vault_db_path).permissions().mode() & 0o777` を確認 |
| 期待結果 | `0o600` と等しい |

---

### TC-I13: ゼロバイト vault.db → panic せずエラー返却

| 項目 | 内容 |
|------|------|
| テストID | TC-I13 |
| 対応する受入基準ID | AC-13 |
| 対応する工程 | 詳細設計（REQ-P09, P10） |
| 種別 | 異常系 |
| 前提条件 | tempdir に 0 バイトの `vault.db` を配置済み（`std::fs::write(path, b"")`） |
| 操作 | `repo.load()` |
| 期待結果 | `Err(PersistenceError::Sqlite { .. })` または `Err(PersistenceError::SchemaMismatch { .. })` が返る。`panic!` しない |

---

### TC-I14: 不正バイト列 vault.db → panic せずエラー返却

| 項目 | 内容 |
|------|------|
| テストID | TC-I14 |
| 対応する受入基準ID | AC-13 |
| 対応する工程 | 詳細設計（REQ-P09, P10） |
| 種別 | 異常系 |
| 前提条件 | tempdir に非 SQLite バイト列（`b"this is not a sqlite file\x00\xFF"`）の `vault.db` を配置済み |
| 操作 | `repo.load()` |
| 期待結果 | `Err(PersistenceError::Sqlite { .. })` または `Err(PersistenceError::SchemaMismatch { .. })` が返る。`panic!` しない |

---

### TC-I15: .new 残存 + save → OrphanNewFile

| 項目 | 内容 |
|------|------|
| テストID | TC-I15 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P05、save アルゴリズム step 3） |
| 種別 | 異常系 |
| 前提条件 | tempdir に `vault.db.new` を空ファイルとして作成済み |
| 操作 | `repo.save(&vault)` |
| 期待結果 | `Err(PersistenceError::OrphanNewFile { path })` が返る。既存の `vault.db` は変更されていない |

---

### TC-I16: exists() — vault 非存在

| 項目 | 内容 |
|------|------|
| テストID | TC-I16 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P01、`exists` アルゴリズム） |
| 種別 | 正常系 |
| 前提条件 | tempdir が空（vault.db なし） |
| 操作 | `repo.exists()` |
| 期待結果 | `Ok(false)` |

---

### TC-I17: exists() — vault 存在

| 項目 | 内容 |
|------|------|
| テストID | TC-I17 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P01、`exists` アルゴリズム） |
| 種別 | 正常系 |
| 前提条件 | `repo.save(&vault)` 完了済み |
| 操作 | `repo.exists()` |
| 期待結果 | `Ok(true)` |

---

### TC-I18: SHIKOMI_VAULT_DIR 環境変数 override

| 項目 | 内容 |
|------|------|
| テストID | TC-I18 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P08、ENV_VAR_VAULT_DIR） |
| 種別 | 正常系 |
| 前提条件 | `SHIKOMI_VAULT_DIR` を tempdir のパスに設定（`std::env::set_var` 使用、テスト分離に注意） |
| 操作 | `SqliteVaultRepository::new()` で構築し `repo.save(&vault)` |
| 期待結果 | `Ok(())` が返り、指定 tempdir 配下に `vault.db` が作成されている |

---

### TC-I19: ゼロレコード vault round-trip

| 項目 | 内容 |
|------|------|
| テストID | TC-I19 |
| 対応する受入基準ID | AC-02 |
| 対応する工程 | 詳細設計（REQ-P02, P03、境界値） |
| 種別 | 境界値 |
| 前提条件 | `SqliteVaultRepository::with_dir(tempdir)` 構築済み |
| 操作 | 1. レコードゼロの平文 `Vault` を save 2. load |
| 期待結果 | `load()` が `Ok(vault2)` を返し、`vault2.records().is_empty() == true`、ヘッダが等価 |

---

### TC-I20: CHECK 制約の防衛線確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I20 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P03、DDL CHECK 制約） |
| 種別 | 異常系 |
| 前提条件 | `Connection::open` で tempdir の vault.db を作成し、`CREATE TABLE IF NOT EXISTS vault_header ...`（本 Issue のスキーマ）を実行済み |
| 操作 | `conn.execute("INSERT INTO vault_header(id, protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, wrapped_vek_by_recovery) VALUES (1, 'plaintext', 1, '2026-01-01T00:00:00Z', X'DEADBEEF', NULL, NULL)", [])` — CHECK 制約（`plaintext` かつ `kdf_salt IS NOT NULL`）に違反する生 SQL を実行 |
| 期待結果 | `rusqlite::Error::SqliteFailure` が返る（`SQLITE_CONSTRAINT_CHECK` 相当）。アプリケーション層でも同種の不整合が防がれることを確認 |

---

## 6. ユニットテスト設計

> **Rust のテスト配置**: ユニットテストは `#[cfg(test)]` モジュールをソースファイル末尾に配置する（Rust 言語慣習）。OS 依存ケースは `#[cfg(unix)]` / `#[cfg(windows)]` でガード。
> **モックなし**: `Mapping` は純関数。`PermissionGuard::verify_*` はファイルシステムに対して `stat` するため、tempdir 内の実ファイルを使用する（mock 不要）。assumed mock は禁止。

### TC-U01: VaultPaths::new — パス導出

| 項目 | 内容 |
|------|------|
| テストID | TC-U01 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P08、VaultPaths） |
| 種別 | 正常系 |
| 前提条件 | — |
| 操作 | `VaultPaths::new(PathBuf::from("/tmp/shikomi_test"))` を呼び、`dir()` / `vault_db()` / `vault_db_new()` を確認 |
| 期待結果 | `dir()` == `"/tmp/shikomi_test"` / `vault_db()` == `"/tmp/shikomi_test/vault.db"` / `vault_db_new()` == `"/tmp/shikomi_test/vault.db.new"` |

---

### TC-U02: Mapping::vault_header_to_params — 平文モード

| 項目 | 内容 |
|------|------|
| テストID | TC-U02 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、Mapping、型写像表） |
| 種別 | 正常系 |
| 前提条件 | 平文モードの `VaultHeader` を `shikomi-core` API で組立済み |
| 操作 | `Mapping::vault_header_to_params(&header)` を呼ぶ |
| 期待結果 | `protection_mode` が `"plaintext"`、`kdf_salt` / `wrapped_vek_by_pw` / `wrapped_vek_by_recovery` パラメータが全て `None`（SQL では `NULL`）である |

---

### TC-U03: Mapping::row_to_vault_header — 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-U03 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、Mapping::row_to_vault_header） |
| 種別 | 正常系 |
| 前提条件 | in-memory SQLite に正常な `vault_header` 行（plaintext mode）を INSERT 済み |
| 操作 | `SELECT_VAULT_HEADER` → `Mapping::row_to_vault_header(&row)` |
| 期待結果 | `Ok(VaultHeader)` が返り、`protection_mode == ProtectionMode::Plaintext` |

---

### TC-U04: Mapping::row_to_vault_header — UnknownProtectionMode

| 項目 | 内容 |
|------|------|
| テストID | TC-U04 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、CorruptedReason::UnknownProtectionMode） |
| 種別 | 異常系 |
| 前提条件 | in-memory SQLite に `protection_mode = 'unknown_future_mode'` の行を直接 INSERT（CHECK 制約を外した DDL を使用） |
| 操作 | `SELECT_VAULT_HEADER` → `Mapping::row_to_vault_header(&row)` |
| 期待結果 | `Err(PersistenceError::Corrupted { reason: CorruptedReason::UnknownProtectionMode { raw: "unknown_future_mode" }, .. })` |

---

### TC-U05: Mapping::row_to_vault_header — InvalidRfc3339

| 項目 | 内容 |
|------|------|
| テストID | TC-U05 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、CorruptedReason::InvalidRfc3339） |
| 種別 | 異常系 |
| 前提条件 | in-memory SQLite に `created_at = 'not-a-date'` の行を INSERT（`created_at` 列の型チェックは SQLite が行わないため直接 INSERT 可能） |
| 操作 | `SELECT_VAULT_HEADER` → `Mapping::row_to_vault_header(&row)` |
| 期待結果 | `Err(PersistenceError::Corrupted { reason: CorruptedReason::InvalidRfc3339 { column: "created_at", raw: "not-a-date" }, .. })` |

---

### TC-U06: Mapping::row_to_record — 正常系（plaintext variant）

| 項目 | 内容 |
|------|------|
| テストID | TC-U06 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、Mapping::row_to_record） |
| 種別 | 正常系 |
| 前提条件 | in-memory SQLite に正常な `records` 行（`payload_variant='plaintext'`、`plaintext_value='test value'`）を INSERT 済み |
| 操作 | `SELECT_RECORDS_ORDERED` → `Mapping::row_to_record(&row)` |
| 期待結果 | `Ok(Record)` が返り、`record.label().as_str()` が期待ラベルと一致する |

---

### TC-U07: Mapping::row_to_record — InvalidUuidString

| 項目 | 内容 |
|------|------|
| テストID | TC-U07 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、CorruptedReason::InvalidUuidString） |
| 種別 | 異常系 |
| 前提条件 | in-memory SQLite に `id = 'not-a-uuid'` の行を INSERT（PK 制約のみで UUID 形式チェックは SQLite にない） |
| 操作 | `SELECT_RECORDS_ORDERED` → `Mapping::row_to_record(&row)` |
| 期待結果 | `Err(PersistenceError::Corrupted { reason: CorruptedReason::InvalidUuidString { raw: "not-a-uuid" }, .. })` |

---

### TC-U08: Mapping::row_to_record — NullViolation

| 項目 | 内容 |
|------|------|
| テストID | TC-U08 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、CorruptedReason::NullViolation） |
| 種別 | 異常系 |
| 前提条件 | in-memory SQLite に `payload_variant='plaintext'` かつ `plaintext_value=NULL` の行を INSERT（CHECK 制約を外した DDL を使用） |
| 操作 | `SELECT_RECORDS_ORDERED` → `Mapping::row_to_record(&row)` |
| 期待結果 | `Err(PersistenceError::Corrupted { reason: CorruptedReason::NullViolation { column: "plaintext_value" }, .. })` |

---

### TC-U09: PermissionGuard::verify_dir — 0700 → Ok（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U09 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P06、PermissionGuard::verify_dir） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(unix)]`。tempdir を `chmod 0o700` 済み |
| 操作 | `PermissionGuard::verify_dir(tempdir_path)` |
| 期待結果 | `Ok(())` |

---

### TC-U10: PermissionGuard::verify_dir — 0755 → InvalidPermission（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U10 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P06、PermissionGuard::verify_dir） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(unix)]`。tempdir を `chmod 0o755` 済み |
| 操作 | `PermissionGuard::verify_dir(tempdir_path)` |
| 期待結果 | `Err(PersistenceError::InvalidPermission { expected: "0700", .. })` |

---

### TC-U11: PermissionGuard::verify_file — 0600 → Ok（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U11 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P06、PermissionGuard::verify_file） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(unix)]`。tempdir 内に空ファイルを `chmod 0o600` で作成済み |
| 操作 | `PermissionGuard::verify_file(file_path)` |
| 期待結果 | `Ok(())` |

---

### TC-U12: PermissionGuard::verify_file — 0644 → InvalidPermission（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U12 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P06、PermissionGuard::verify_file） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(unix)]`。tempdir 内に空ファイルを `chmod 0o644` で作成済み |
| 操作 | `PermissionGuard::verify_file(file_path)` |
| 期待結果 | `Err(PersistenceError::InvalidPermission { expected: "0600", .. })` |

---

## 7. モック方針

| 検証対象 | モック要否 | 理由 |
|---------|---------|------|
| `rusqlite::Connection` | **不要**（本物を使用） | in-memory または tempdir の実 SQLite で結合テスト。モックでは CHECK 制約・PRAGMA 動作が再現できない |
| ファイルシステム（`std::fs`） | **不要**（tempdir を使用） | `tempfile::TempDir` で OS 標準ファイルシステムを使う。in-memory ファイルシステムでは atomic rename / fsync の挙動が再現できない |
| `dirs::data_dir()` | **不要**（環境変数 `SHIKOMI_VAULT_DIR` で override） | TC-I18 で環境変数による上書きを使う。`dirs` crate 自体はモックしない |
| `shikomi-core` ドメイン型 | **不要** | ドメイン型は実装の一部（`shikomi-core` crate）を直接利用する。assumed mock 禁止 |
| OS パーミッション API | **不要** | 実際の `stat` / `chmod` を使う。OS 依存ケースは `#[cfg(unix)]` でガード |
| 外部 API / ネットワーク | **対象外** | 本 crate は外部 API を持たない |

---

## 8. 実行手順と証跡

### 実行環境

| 項目 | 内容 |
|------|------|
| Rust toolchain | `rustup` でインストール済み（`stable` チャネル） |
| cargo-llvm-cov | `cargo install cargo-llvm-cov` |
| cargo-deny | `cargo install cargo-deny` |
| OS | Linux / macOS（Unix ケース）。Windows CI でも Windows 非依存ケースは通過する |

### 実行コマンド例（結合テスト・ユニットテスト）

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
- TC-I06（クラッシュ耐性）は子プロセスの PID・SIGKILL 送信タイミング・`vault.db` の MD5 ビフォー/アフターを記録する

---

## 9. カバレッジ基準

| 観点 | 基準 |
|------|------|
| 受入基準の網羅 | AC-01〜AC-13 全項目が TC-I01〜TC-I20 のいずれかで網羅されていること（トレーサビリティ §3 を参照） |
| 正常系 | 全 正常系テストケース必須（TC-I02, I16, I17, I18, I19, TC-U01〜U03, U06, U09, U11） |
| 異常系 | 全 10 種 `PersistenceError` バリアントが少なくとも 1 ケースで発生・検証されること。`panic!` が一切発生しないことを確認 |
| 境界値 | ゼロレコード（TC-I19）、絵文字ラベル（TC-I08）、ゼロバイトファイル（TC-I13）、不正バイト列（TC-I14） |
| カバレッジ数値 | `cargo llvm-cov` 行カバレッジ 80% 以上（AC-10） |
| Fail Secure ケース | `.new` 残存（TC-I05, TC-I15）・OS パーミッション異常（TC-I07, TC-I12, TC-U10, TC-U12）・暗号化モード拒否（TC-I03, TC-I04）が**必須**（省略不可） |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-23*
*対応 Issue: #10 feat(shikomi-infra): vault 永続化層（平文モード）*
