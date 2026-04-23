# 結合テスト設計 — vault-persistence

> このファイルは `test-design/index.md` の §5 に相当する。テストマトリクス・モック方針・実行手順は `index.md` を参照。

> **ツール選択根拠**: このシステムは Rust ライブラリ crate であり、エントリポイントは Rust 公開 API（`SqliteVaultRepository::save` / `load` / `exists`）。Rust の統合テスト（`crates/shikomi-infra/tests/` 配下）で `tempfile::TempDir` を使い、実際の SQLite ファイルに対して結合テストを行う。外部 API / 外部サービスへの依存はなく、モックは不要（全て本物の `rusqlite` + ファイルシステムを使用）。OS パーミッション検証ケースは `#[cfg(unix)]` でガードし Windows CI では自動スキップ。

---

## TC-I01: 公開 API ドキュメント確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I01 |
| 対応する受入基準ID | AC-01 |
| 対応する工程 | 基本設計（モジュール構成、REQ-P01〜P15） |
| 種別 | 正常系 |
| 前提条件 | `feat/issue-10-vault-persistence` ブランチで実装コミット済み |
| 操作 | `cargo doc -p shikomi-infra --no-deps` を実行し、出力 HTML に `VaultRepository` / `SqliteVaultRepository` / `PersistenceError` / `CorruptedReason` / `AtomicWriteStage` / `VaultDirReason` / `VaultPaths` の各型が記載されていることを確認 |
| 期待結果 | exit code == 0。`target/doc/shikomi_infra/persistence/` 配下に上記型のドキュメントが生成される |

---

## TC-I02: 平文 vault round-trip（レコード複数件）

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

## TC-I03: 暗号化モード vault を save → UnsupportedYet

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

## TC-I04: 暗号化モード vault.db を load → UnsupportedYet

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

## TC-I05: .new 残存 + load → OrphanNewFile

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

## TC-I06: atomic write 論理等価テスト（write_new_only フック）

> **背景**: 初版では「SIGKILL を子プロセスに送信」という非決定的テストとして定義していた。タイミング依存で AC-06 の検証にならないと指摘を受け、テスト用フックを使った決定的なテストに書き直した。

| 項目 | 内容 |
|------|------|
| テストID | TC-I06 |
| 対応する受入基準ID | AC-06 |
| 対応する工程 | 詳細設計（REQ-P04、AtomicWriter アルゴリズム） |
| 種別 | 異常系 |
| 前提条件 | **実装担当への要求**: `AtomicWriter` に `#[cfg(test)]` 限定の `write_new_only(paths: &VaultPaths, vault: &Vault) -> Result<(), PersistenceError>` を追加すること。このフックは `write_new` を完了させた後に `fsync_and_rename` を呼ばずに返す（rename しない = クラッシュ後に `.new` が残った状態を論理的に再現する）。tempdir に既存の `vault.db`（初期平文 vault）を save 済み。初期 vault のレコード数を記録済み |
| 操作 | 1. 別の vault（レコード内容が異なる）を `AtomicWriter::write_new_only(&paths, &new_vault)` で `.new` のみ作成（rename を呼ばない）2. `repo.load()` を呼ぶ |
| 期待結果 | `Err(PersistenceError::OrphanNewFile { path })` が返る。`path` が `vault.db.new` の絶対パスと一致する。`vault.db` は初期 vault の内容を保持している（`.new` の内容が `vault.db` に反映されていないことを `vault.db` のバイトハッシュ比較で確認） |

---

## TC-I07: 0777 ディレクトリ + load → InvalidPermission（Unix）

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

## TC-I08: UTF-8 特殊文字ラベルの round-trip

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

## TC-I09: SQL インジェクション禁止設計の静的 grep 確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I09 |
| 対応する受入基準ID | AC-09 |
| 対応する工程 | 詳細設計（REQ-P12、SchemaSql 設計判断） |
| 種別 | 正常系 |
| 前提条件 | `crates/shikomi-infra/src/` 配下にソースが存在する |
| 操作 | `grep -rEn 'format!\s*\(.*(?:SELECT\|INSERT\|UPDATE\|DELETE\|PRAGMA)' --include="*.rs" crates/shikomi-infra/src/` および `grep -rEn '"[^"]*(?:SELECT\|INSERT\|UPDATE\|DELETE)[^"]*"\s*\+' --include="*.rs" crates/shikomi-infra/src/` を実行（SQL 文字列の連結パターン検索） |
| 期待結果 | マッチ行ゼロ。全 SQL は `const` リテラルで定義され、`params!` マクロ経由のバインドのみ使用している |

---

## TC-I10: cargo test + カバレッジ

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

## TC-I11: cargo clippy / fmt / deny

| 項目 | 内容 |
|------|------|
| テストID | TC-I11 |
| 対応する受入基準ID | AC-11 |
| 対応する工程 | — |
| 種別 | 正常系 |
| 前提条件 | `deny.toml` がリポジトリルートに存在する |
| 操作 | `cargo clippy --workspace`、`cargo fmt --check --all`、`cargo deny check` を順に実行。**`-D warnings` は付けない**（`[workspace.lints.clippy]` が `all="deny"` / `pedantic="warn"` の 2 段構えのため、deny カテゴリ違反は cargo clippy 自体が exit 非 0 で弾く。`pedantic` の warn は意図的設計で CI を fail させない） |
| 期待結果 | 全コマンドが exit code == 0。`workspace.lints.clippy.all="deny"` カテゴリの違反がゼロであることが clippy の exit code で担保される |

---

## TC-I12: save 後のファイルパーミッション確認（Unix）

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

## TC-I13: ゼロバイト vault.db → panic せずエラー返却

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

## TC-I14: 不正バイト列 vault.db → panic せずエラー返却

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

## TC-I15: .new 残存 + save → OrphanNewFile

| 項目 | 内容 |
|------|------|
| テストID | TC-I15 |
| 対応する受入基準ID | AC-14 |
| 対応する工程 | 詳細設計（REQ-P05、save アルゴリズム step 3） |
| 種別 | 異常系 |
| 前提条件 | tempdir に `vault.db.new` を空ファイルとして作成済み |
| 操作 | `repo.save(&vault)` |
| 期待結果 | `Err(PersistenceError::OrphanNewFile { path })` が返る。既存の `vault.db` は変更されていない |

---

## TC-I16: exists() — vault 非存在

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

## TC-I17: exists() — vault 存在

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

## TC-I18: SHIKOMI_VAULT_DIR 環境変数 override

| 項目 | 内容 |
|------|------|
| テストID | TC-I18 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P08、ENV_VAR_VAULT_DIR） |
| 種別 | 正常系 |
| 前提条件 | `Cargo.toml` の `[dev-dependencies]` に `serial_test = "3"` を追加済み。テスト関数に `#[serial]` アトリビュートを付与（`std::env::set_var` はプロセス内のグローバル状態を変更するため、他テストとの並列実行で干渉するリスクがある。`serial_test` クレートで本ケースを直列化する） |
| 操作 | `std::env::set_var("SHIKOMI_VAULT_DIR", tempdir.path())` で環境変数を設定し、`SqliteVaultRepository::new()` で構築後に `repo.save(&vault)` を実行。テスト終了前に `std::env::remove_var("SHIKOMI_VAULT_DIR")` でクリーンアップ |
| 期待結果 | `Ok(())` が返り、指定 tempdir 配下に `vault.db` が作成されている |

---

## TC-I19: ゼロレコード vault round-trip

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

## TC-I20: CHECK 制約の防衛線確認

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

## TC-I21: VaultLock 競合検知（別プロセスが排他ロック保持中）

| 項目 | 内容 |
|------|------|
| テストID | TC-I21 |
| 対応する受入基準ID | AC-17 |
| 対応する工程 | 詳細設計（REQ-P13、VaultLock::acquire_exclusive、save アルゴリズム step 3） |
| 種別 | 異常系 |
| 前提条件 | tempdir に vault.db が存在する。`std::process::Command` で子プロセスを起動し、子プロセスが `VaultLock::acquire_exclusive` でロックを保持したまま `std::io::stdin().read` でブロックする状態を作れること。親プロセスと子プロセスが同一 tempdir を共有する |
| 操作 | 1. 子プロセスを起動し vault ディレクトリの排他ロックを取得させる 2. 親プロセスで `repo.save(&vault)` を呼ぶ（子プロセスがロックを保持したまま） 3. 親プロセスの save 結果を確認後、子プロセスを kill して終了させる |
| 期待結果 | `Err(PersistenceError::Locked { path, holder_hint })` が返る。`path` が `vault.db.lock` の絶対パスと一致する。`holder_hint` は Unix では子プロセスの PID（`Some(pid)`）または `None`、Windows では `None`。親プロセスの `vault.db` は変更されていない |

---

## TC-I22: VaultPaths::new — SHIKOMI_VAULT_DIR 7 段階バリデーション（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-I22 |
| 対応する受入基準ID | AC-16 |
| 対応する工程 | 詳細設計（REQ-P15、VaultPaths::new バリデーション、VaultDirReason） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(unix)]` ガード付き。`serial_test` クレートで `std::env::set_var` の競合を防ぐ |
| 操作 | 以下の各パターンを `SHIKOMI_VAULT_DIR` に設定し `SqliteVaultRepository::new()` を呼ぶ（テストを個別関数に分割する）: A. 相対パス（`"relative/path"`）B. `..` を含むパス（`"/tmp/shikomi/../../etc"`）C. シンボリックリンクを経由するパス（`tempfile::TempDir` に `std::os::unix::fs::symlink` で作成）D. `/etc/` 配下のパス（`"/etc/shikomi"`） |
| 期待結果 | A: `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::NotAbsolute, .. })` / B: `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::PathTraversal, .. })` / C: `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::SymlinkNotAllowed, .. })` / D: `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::ProtectedSystemArea { prefix: "/etc" }, .. })`。全ケースで `panic!` しない |

---

## TC-I23: tracing-test による秘密漏洩ゼロ検証

| 項目 | 内容 |
|------|------|
| テストID | TC-I23 |
| 対応する受入基準ID | AC-15 |
| 対応する工程 | 詳細設計（REQ-P14、audit.rs 監査ログ規約） |
| 種別 | 正常系 |
| 前提条件 | `Cargo.toml` の `[dev-dependencies]` に `tracing-test = "0.2"` を追加済み。平文モードの `Vault`（レコード `plaintext_value` に秘密文字列 `"TOP_SECRET_VALUE"` を設定）を tempdir に save 済み |
| 操作 | 1. `tracing_test::init()` または `#[traced_test]` アトリビュートで tracing ログを収集 2. `repo.save(&vault)` / `repo.load()` / `repo.exists()` を順に実行 3. 収集したログ文字列全体に対して、秘密値パターン（`"TOP_SECRET_VALUE"` / `"expose_secret"` / `"plaintext_value"` / `"kdf_salt"` / `"wrapped_vek"` の生値）が含まれないことを `assert!(!logs.contains(...))` で検証 |
| 期待結果 | 全操作のログに秘密値パターンがマッチしない。`audit::exit_err` で `PersistenceError` を記録する際も、`Display` が秘密値を含まないことを確認する |

---

---

## TC-I24: save 後の vault.db は owner-only DACL（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I24 |
| 対応する受入基準ID | REQ-P07 受入観点① |
| 対応する工程 | 基本設計（REQ-P07、save フロー step 6「作成直後にファイルパーミッションを所有者 ACL 設定」） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir` を使用 |
| 操作 | 1. `repo.save(&vault)` 2. `GetNamedSecurityInfoW` で `vault.db` の DACL と所有者 SID を取得 |
| 期待結果 | `save()` が `Ok(())` を返す。vault.db の DACL が 4 不変条件を満たす: ①`SE_DACL_PROTECTED` bit が立っている ②`AceCount == 1` かつ `ACCESS_ALLOWED_ACE_TYPE` ③ACE トラスティ SID が所有者 SID と `EqualSid` で一致 ④`AccessMask == FILE_GENERIC_READ \| FILE_GENERIC_WRITE`（`DELETE` / `WRITE_DAC` 等の追加ビットなし） |

---

## TC-I25: vault.db の DACL 破損後 load → InvalidPermission（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I25 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 基本設計（REQ-P07、load フロー step 4「ファイルのパーミッション確認」） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`repo.save(&vault)` 完了済み。vault.db に対し、テストコード内で `BUILTIN\Users` への `GENERIC_READ` Allow ACE を `SetNamedSecurityInfoW` で追加し DACL を壊す（ACE 数 = 2 かつ `PROTECTED_DACL_SECURITY_INFORMATION` なし） |
| 操作 | `repo.load()` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE)", .. })` が返る。`actual` フィールドに ACE 列挙の診断文字列（`trustee_sid: <SID>, access_mask: 0x<hex>, ace_type: <type>` 形式）が含まれる。秘密値を含まない |

---

## TC-I26: 継承 ACE 破棄の確認 — ensure_dir 後に SE_DACL_PROTECTED が設定される（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I26 |
| 対応する受入基準ID | REQ-P07 受入観点③ |
| 対応する工程 | 基本設計（REQ-P07、save フロー step 3「PermissionGuard::ensure_dir — DACL 適用」） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir` 直下に vault ディレクトリパスを指定（親 `%TEMP%` から ACE を継承した状態が初期値）。`repo.save` の前に vault ディレクトリが存在しないことを確認済み |
| 操作 | 1. `repo.save(&vault)`（内部で `ensure_dir` が vault ディレクトリを作成・DACL 適用） 2. `GetNamedSecurityInfoW` で vault ディレクトリの Control Flags を取得 |
| 期待結果 | `save()` が `Ok(())` を返す。取得した Control Flags に `SE_DACL_PROTECTED` bit が立っている（親 `%TEMP%` からの継承 ACE が破棄されている）。vault ディレクトリの ACE 数は 1 |

---

## TC-I27: vault dir DACL 破損後 load → InvalidPermission（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I27 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 基本設計（REQ-P07、load フロー step 1「PermissionGuard::verify_dir」） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`repo.save(&vault)` 完了済み。vault ディレクトリに対し、`SetNamedSecurityInfoW` で `DACL_SECURITY_INFORMATION`（`PROTECTED_DACL_SECURITY_INFORMATION` を除く）で書き換えることで `SE_DACL_PROTECTED` bit を意図的に落とす |
| 操作 | `repo.load()` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE\|FILE_TRAVERSE)", .. })` が返る（vault ディレクトリが verify_dir の不変条件①で失敗）。`vault.db` は変更されていない |

---

*対応 Issue: #10, #14 / 親ドキュメント: `test-design/index.md`*
