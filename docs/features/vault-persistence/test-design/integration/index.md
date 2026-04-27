# 結合テスト設計 — vault-persistence

> このファイルは `test-design/index.md` の §5 に相当する。テストマトリクス・モック方針・実行手順は `index.md` を参照。

> **ツール選択根拠**: このシステムは Rust ライブラリ crate であり、エントリポイントは Rust 公開 API（`SqliteVaultRepository::save` / `load` / `exists`）。Rust の統合テスト（`crates/shikomi-infra/tests/` 配下）で `tempfile::TempDir` を使い、実際の SQLite ファイルに対して結合テストを行う。外部 API / 外部サービスへの依存はなく、モックは不要（全て本物の `rusqlite` + ファイルシステムを使用）。OS パーミッション検証ケースは `#[cfg(unix)]` でガードし Windows CI では自動スキップ、Windows 固有 ACL / file-handle semantics 検証ケースは `#[cfg(windows)]` でガードし Linux/macOS CI では自動スキップ。

> **`#[cfg(windows)] #[ignore]` での回避禁止**（Issue #65 由来の防衛線）: Win 固有の TC（TC-I24〜I29 等）を `#[ignore]` で囲んで CI green を装う PR は問答無用で**却下対象**（CI スコープ錯覚 = Bug-F-003 の再演温床、`../basic-design/error.md` §禁止事項 §Windows rename retry の盲目採用は禁止 と整合）。Win ランナーが落ちる場合は根本原因の articulate を伴った修正を必須とする。テスト担当（涅マユリ）が `#[ignore]` を解剖時に発見した場合は実験不全として即時差戻し。

---

## 0. Issue #65 由来の外部 I/O 依存マップ

Issue #65（Windows AtomicWrite rename 失敗）の修正対象が触る外部 I/O 境界を全て列挙し、characterization 状態を明示する（assumed mock 禁止、テスト戦略ガイドの「外部I/O依存マップ」要件に対応）。

| 外部 I/O 依存 | 経由クレート / API | テスト方式 | raw fixture | factory | characterization 状態 |
|------------|-----------------|----------|-----------|---------|---------------------|
| SQLite ファイル（`vault.db.new`、`-wal` / `-shm` / `-journal` サイドカー含む） | `rusqlite::Connection`（バンドル SQLite） | **本物** を `tempfile::TempDir` 配下で使用（モック不要、結合テスト方針に従う） | 不要（実 SQLite を直接利用） | 不要 | **済** — 実 SQLite で結合テスト可能 |
| ファイルシステム rename | `std::fs::rename`（Unix: `rename(2)` / Windows: 内部で `MoveFileExW`） | **本物** を tempdir で使用 | 不要 | 不要 | **済** — `std::fs` 直接利用 |
| Windows rename 一過性エラー（`ERROR_ACCESS_DENIED 5` / `ERROR_SHARING_VIOLATION 32` / `ERROR_LOCK_VIOLATION 33`） | OS 直返（`std::io::Error::raw_os_error()` で識別） | TC-I29 で並行 read open による race を**実環境で再現**（モック不要） | **要保存**: PR #64 失敗 CI ログ 5 件のスタックトレース全文 https://github.com/shikomi-dev/shikomi/actions/runs/24950291068/job/73058649443 を `tests/fixtures/characterization/raw/issue65/pr64_failure_log.txt` に保存（マスク不要、公開 CI ログ） | 不要（一過性エラーは OS 直返、合成不要） | **要起票** — 実装者は本ファイルを修正前のベースラインとして固定し、修正後の CI ログ（5 件 PASS）と diff 比較する責務を負う |
| `MoveFileExW` Win32 API（`ReplaceFileW` 経由） | `windows` crate `Win32::Storage::FileSystem::ReplaceFileW`（cfg(windows)） | **本物** を実 Windows CI ランナーで実行（仮想環境 Wine では `MoveFileExW` 挙動が再現できないため）| 不要 | 不要 | **済** — `test-infra-windows` ジョブで raw 検証 |

**reviewer 却下基準**:
- raw fixture（PR #64 失敗ログ）が `tests/fixtures/characterization/raw/issue65/` に保存されないまま実装 PR 提出 → **[却下]**
- TC-I29 が `mockall` 等で `MoveFileExW` をモックする → **[却下]**（実環境の race 検出にならない、assumed mock 違反）
- `test-infra-windows` ジョブを CI 必須 check から外す PR → **[却下]**（CI スコープ錯覚再演）

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

## TC-I03: 暗号化モード vault を save → 成功（Sub-D Rev で TC 意味論変更）

<!-- Boy Scout Rule (Issue #42 / Sub-D): REQ-P11 改訂により旧 TC-I03 を退役。
     旧: 「暗号化モード vault を save → UnsupportedYet で拒否」（Sub-D 完了前の Fail Fast 検証）
     新: 「暗号化モード v1 vault を save → 成功」（Sub-D 解禁後の正常系検証）
     退役理由: REQ-P11 意味論が「暗号化モード全般拒否」→「未対応バージョン拒否」に変更
     置換先 TC ID: 本 TC-I03（同 ID 維持、内容差替え）
     新規追加 TC: TC-I04a（v999 拒否、本ファイル §TC-I04a 参照） -->

| 項目 | 内容 |
|------|------|
| テストID | TC-I03 |
| 対応する受入基準ID | AC-03（Sub-D Rev で意味論変更） |
| 対応する工程 | 詳細設計（REQ-P11 Sub-D Rev、save アルゴリズム改訂後） |
| 種別 | 正常系 |
| 前提条件 | `VaultHeader::new_encrypted` で v1 暗号化モードのヘッダを組立、`VaultEncryptedHeader` の全フィールド（kdf_salt / wrapped_vek_by_pw / wrapped_vek_by_recovery / nonce_counter / kdf_params / header_aead_envelope）を不透明 BLOB として用意可能（`shikomi-core` API、`vault-encryption/detailed-design/repository-and-migration.md` §`VaultEncryptedHeader`） |
| 操作 | 1. `SqliteVaultRepository::with_dir(tempdir)` 2. v1 暗号化モードの `Vault` を構築（暗号文・nonce・tag は不透明 BLOB として既に AEAD 計算済 fixture を使用、`vault-persistence` 自身は AEAD 計算しない） 3. `repo.save(&vault)` |
| 期待結果 | `Ok(())` が返る。`vault.db` に `protection_mode='encrypted'` 行が永続化される。CHECK 制約（`kdf_salt 16B` / `wrapped_vek_* 28B+` / `header_aead_*` の長さ）が全て満たされる |

---

## TC-I04: 暗号化モード vault.db を load → 成功（Sub-D Rev で TC 意味論変更）

<!-- Boy Scout Rule (Issue #42 / Sub-D): 旧 TC-I04 「暗号化モード vault.db を load → UnsupportedYet で拒否」を退役。
     退役理由: REQ-P11 意味論変更（同上）
     置換先 TC ID: 本 TC-I04（同 ID 維持、内容差替え） -->

| 項目 | 内容 |
|------|------|
| テストID | TC-I04 |
| 対応する受入基準ID | AC-04（Sub-D Rev で意味論変更） |
| 対応する工程 | 詳細設計（REQ-P11 Sub-D Rev、load アルゴリズム改訂後） |
| 種別 | 正常系 |
| 前提条件 | tempdir 配下に v1 `protection_mode='encrypted'` を持つ vault.db を `rusqlite` で直接作成済み（Sub-D で改訂した DDL に準拠、`kdf_params` / `header_aead_*` カラムに有効な値を挿入） |
| 操作 | `repo.load()` |
| 期待結果 | `Ok(Vault)` が返る。`vault.header()` が `VaultHeader::Encrypted(VaultEncryptedHeader { ... })` で、各フィールドが永続化値と bit-exact 一致。`vault.records()` の各 `Record` が `Record::Encrypted(EncryptedRecord)` variant で構築される。**AEAD 検証 / wrap_VEK 復号は `vault-persistence` の責務外、`VaultMigration` 側で別途検証**（本 TC スコープ外） |

---

## TC-I04a: 未対応バージョンの vault.db を load → UnsupportedYet（Sub-D Rev 新規追加）

<!-- Boy Scout Rule (Issue #42 / Sub-D): REQ-P11 改訂で「未対応バージョン拒否」の Fail Fast 経路を新規 TC として追加。 -->

| 項目 | 内容 |
|------|------|
| テストID | TC-I04a |
| 対応する受入基準ID | AC-04（バージョン範囲外検証、Sub-D Rev 新規） |
| 対応する工程 | 詳細設計（REQ-P11 Sub-D Rev、load step 9 改訂後） |
| 種別 | 異常系 |
| 前提条件 | tempdir 配下に `PRAGMA user_version = 999`（`USER_VERSION_SUPPORTED_MAX` を超過する未来バージョン）を持つ vault.db を `rusqlite` で直接作成済み |
| 操作 | `repo.load()` |
| 期待結果 | `Err(PersistenceError::UnsupportedYet { feature: "vault schema version", supported_range: (V_MIN, V_MAX), actual: 999 })` が返る。step 10 以降（`SELECT_VAULT_HEADER` 等）のクエリが**実行されない**（Fail Fast、攻撃面情報漏洩回避） |

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
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE)", actual, .. })` が返る。`actual` フィールドに全 ACE の列挙文字列（`trustee_sid=<SID>, ace_type=..., access_mask=0x<hex>` の形式 2 行分）が含まれる——不変条件②（`ace_count`）違反時のラベル形式（`flows.md §OS 別パーミッション実装詳細 §Windows` 参照）。秘密値を含まない |

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
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE\|FILE_TRAVERSE)", actual, .. })` が返る。`actual` フィールドが `"inherited DACL (SE_DACL_PROTECTED not set)"` と等しい——不変条件①（`inherited`）違反時の確定ラベル（`flows.md §OS 別パーミッション実装詳細 §Windows` 参照）。`vault.db` は変更されていない |

---

## TC-I28: Sub-D `vault_migration_integration` 5 件 green 化（Windows、Issue #65 受入）

> **背景**: Sub-D（Issue #42 / PR #58）由来の integration test `crates/shikomi-infra/tests/vault_migration_integration.rs` が **Windows ランナーのみ** で 5 件全失敗していた（PR #64 CI ログ参照）。Issue #65 修正のミニマム受入条件として、これら 5 件が修正後の Windows CI で PASS することを本 TC で明示的に検証対象化する（既存テスト = 受入観点の SSoT、新規テスト追加なしで AC を満たす）。

| 項目 | 内容 |
|------|------|
| テストID | TC-I28 |
| 対応する受入基準ID | AC-18（Issue #65 受入、新規） |
| 対応する工程 | 詳細設計（REQ-P04、`AtomicWriter::write_new` クローズ順序契約 / `fsync_and_rename` Win 限定 retry、`../detailed-design/flows.md` §`save` step 6.10〜6.13 / step 7.3 / `../detailed-design/classes.md` §設計判断 §3.1） |
| 種別 | 異常系の green 化（修正前は Windows で `AtomicWriteFailed { stage: Rename, source: code:5 PermissionDenied }`、修正後は PASS） |
| 前提条件 | `feature/issue-65-windows-atomic-rename` ブランチ。`AtomicWriter::write_new` に `PRAGMA wal_checkpoint(TRUNCATE)` + `PRAGMA journal_mode = DELETE` + `Connection::close()` 明示呼出が実装されている。`AtomicWriter::fsync_and_rename` に `cfg(windows)` 限定の指数バックオフ rename retry（`50ms × 2^(n-1)` ± `25ms` jitter × 5、最悪 ~1675ms、Bug-G-001 反映後）が実装されている。raw fixture `tests/fixtures/characterization/raw/issue65/pr64_failure_log.txt` がベースライン保存されている |
| 操作 | Windows CI ランナー上で `cargo test -p shikomi-infra --test vault_migration_integration` を実行（テスト関数: `tc_d_i01_encrypt_then_unlock_password_roundtrip` / `tc_d_i02_encrypt_then_decrypt_roundtrip` / `tc_d_i03_rekey_then_unlock_with_same_password_observation` / `tc_d_i04_rekey_then_decrypt_vault_all_records_succeed` / `tc_d_i05_req_p11_v1_accepted_via_vault_migration` の 5 件） |
| 期待結果 | 5 件全て PASS（exit code == 0、`test result: ok. 5 passed; 0 failed`）。Linux / macOS でも引き続き PASS。raw fixture（PR #64 失敗ログ）と CI ログ diff を比較し「`AtomicWriteFailed { stage: Rename, code: 5 }` パターンが消えた」ことを証跡として記録する。**`#[cfg(windows)] #[ignore]` で 5 件を回避する PR は問答無用で却下**（防衛線、本ファイル冒頭注記参照） |

---

## TC-I29: 並行 read open 中の rename race を retry で吸収（Windows、Issue #65 補強検証）

> **背景**: Issue #65 の根本対策（`Connection::close()` 明示 + WAL checkpoint + `journal_mode=DELETE`）に加えて、Win Indexer / Defender 等の一過性ハンドル残存に対する補強として実装される `cfg(windows)` 限定 rename retry（50ms × 5 回）の機能を**決定的に再現するテスト**。並行スレッドが `vault.db` を read open している短時間ウィンドウ中に save を発火させ、retry が成功して save が `Ok(())` を返すことを直接検証する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29 |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、新規） |
| 対応する工程 | 詳細設計（REQ-P04、`AtomicWriter::fsync_and_rename` step 7.3 Windows 分岐、`../detailed-design/flows.md`） |
| 種別 | 異常系（race 状態下での正常完了検証） |
| 前提条件 | `#[cfg(windows)]` ガード付き。`tempfile::TempDir` を使用。初期 `vault.db` を save 済（記録済レコード 1 件）。`std::thread::spawn` で補助スレッドを起動できる |
| 操作 | 1. メインスレッドで初期 vault を save 完了 2. 補助スレッドを起動し、`std::fs::OpenOptions::new().read(true).share_mode(0)` 相当（`FILE_SHARE_NONE`）で `vault.db` を open し、**短時間保持**（指数バックオフ込み最悪 ~1675ms の内側、典型 200ms で retry 3 回目（累積 ~350ms）までに吸収される設計）してから drop する 3. 補助スレッドの open 直後にメインスレッドで別内容の vault を `repo.save(&new_vault)` する 4. save の戻り値と `vault.db` 内容を確認 |
| 期待結果 | `repo.save()` が `Ok(())` を返す（補助スレッドが drop した後、retry の 1〜4 回目で rename が成功する。CI Defender 介入時は 4〜5 回目で吸収）。`repo.load()` で復元した vault が新内容と一致する（最終的に `.new` から `vault.db` への置換が完了している）。**retry が機能していなければ `Err(AtomicWriteFailed { stage: Rename, source: code:5 })` で fail する**（修正前の挙動）。タイムアウト記録: **約 1675ms 超過なら fail**（指数バックオフの retry 上限契約違反、`../basic-design/security.md` §atomic write の二次防衛線 §jitter — `50ms × 2^(n-1)` ± `25ms` jitter × 5 = 最悪 ~1675ms / 平均 ~1550ms、Bug-G-001 反映後）|

**実装上の注意（Win API 直叩き、unsafe）**:
- `std::fs::OpenOptions` は標準では `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` を立てるため race 再現にならない。`std::os::windows::fs::OpenOptionsExt::share_mode(0)` で **share_mode = 0**（排他 open）を指定する必要がある
- 補助スレッドの保持時間は **典型 200ms 程度**（指数バックオフ後の SSoT に追従、Bug-G-001 反映後）。retry 3 回目（累積中央値 ~350ms）までに吸収される設計。CI ランナー (windows-latest) で `drop(File)` の close 遅延 + Defender/Indexer の追加 lock を考慮しても retry 4 回目（累積 ~750ms）までには確実に吸収される
- 経過時間 deadline は **3000ms 程度**（指数バックオフ最悪 ~1675ms × 1.8 buffer + write_new + thread spawn / channel 同期の余裕を考慮）。これを超えるなら指数バックオフ SSoT 上限契約違反
- 並行スレッドが指数バックオフ込み最悪 ~1675ms を超えて保持し続けると `Err(AtomicWriteFailed { stage: Rename })` が返る（**意図通りの fail fast**）。これを直接検証するのが TC-I29-A
- 3 ケース（TC-I29 / TC-I29-A / TC-I29-B）は `#[serial_test::serial(windows_atomic_rename_retry)]` で直列化。並列実行時に補助スレッドの share_mode(0) ロックが他テスト (別 TempDir) の Defender scan 経路を経由して干渉する可能性を排除
- `tracing_test` は **integration テスト crate では既定で対象 crate のログを env filter で弾く**ため、workspace `Cargo.toml` で `features = ["no-env-filter"]` を有効化する。これがないと `Audit::retry_event` の emit が `logs_contain` で観測できない（公式注記）

---

## TC-I29-A: retry 5 回全敗で `outcome=exhausted` が **error レベル**で発火する（Windows、Issue #65 DoS 兆候）

> **背景**: Issue #65 retry 補強の **DoS 兆候側 emit 経路**を直接検証する。補助スレッドが `vault.db` を `share_mode(0)` で **指数バックオフ最悪 ~1675ms を確実に超える時間**保持し、retry を 5 回全敗に追い込む。`Audit::retry_event` の `outcome=exhausted` 経路 (error レベル、`%outcome` Display 経由のクォート無し wire format、`../../basic-design/security.md` §retry 監査ログ) が発火し、daemon 側 subscriber が DoS 兆候として OWASP A09 連携で上位通報できる起点を担保する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29-A |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、DoS 兆候側） |
| 対応する工程 | 基本設計（`../basic-design/security.md` §atomic write の二次防衛線 §retry 監査ログ §rename retry 全敗 / 詳細設計 `../detailed-design/flows.md` §`save` step 7.3） |
| 種別 | 異常系（fail fast の意図確認 + 監査ログ error 経路の発火確認） |
| 前提条件 | `#[cfg(windows)]` ガード。`tempfile::TempDir`。初期 `vault.db` を save 済。`tracing_test::traced_test` でログ収集 |
| 操作 | 1. 初期 vault を save 完了 2. 補助スレッドが `share_mode(0)` で `vault.db` を **2500ms 保持**（v8 で 800ms から拡張、`>1675ms` で retry を 5 回全敗させる、Bug-G-001 反映後の指数バックオフ拡張に追従） 3. 補助スレッド ready 直後に `repo.save(&new_vault)` 4. save 戻り値とトレーシングログを検証 |
| 期待結果 | `repo.save()` が `Err(AtomicWriteFailed { stage: Rename, source: code:5/32/33 })` を返す。監査ログに `"rename retry exhausted"`（error レベル）+ `outcome=exhausted`（`%outcome` Display 経由のクォート無し wire format）が emit される。`outcome=pending` も併発するが `outcome=succeeded` は emit されない（fail 経路）|

**実装上の注意**:
- `tracing_test::traced_test` は **DEBUG 以上**の events を捕捉する。`Audit::retry_event` の error 分岐は `tracing::error!` を発行するため `logs_contain("rename retry exhausted")` で観測可能
- 補助スレッドの 2500ms は指数バックオフ最悪 `~1675ms` に対して `+50%` 余裕（Bug-G-001 反映後）。CI ランナーの sleep 精度揺らぎ (±50ms) と Defender 介入による追加待機を吸収する

---

## TC-I29-B: race 不在の通常 save では retry が exhaust まで到達しない（Windows、回帰防止）

> **背景**: `windows_rename_retry` の 5 回 retry が race 無し時に exhaust 経路まで到達する**異常を検出する sanity check**。CI ランナー (windows-latest) では Defender / Indexer 介入で通常 save でも一過性 race が発生し得る (Issue #65 の根源そのもの) ため、retry 経路自体は許容する。**本 TC の責務は「exhausted まで到達しない = 正常吸収範疇」の確認**であり、retry 経路への偽 emit の厳密検証は unit test 側に委譲する（v7.1 で「retry 経路自体を NG」から緩和、CI 実測の Defender 介入を反映）。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29-B |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、回帰防止） |
| 対応する工程 | 詳細設計（`../detailed-design/flows.md` §`save` step 7、`../detailed-design/classes.md` §`AtomicWriter::rename_atomic` 制御フロー） |
| 種別 | 正常系（race 無し経路の sanity check） |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir`。`tracing_test::traced_test` |
| 操作 | 1. race 無しで `repo.save(&vault)` を呼ぶ（初回作成）2. race 無しで `repo.save(&updated)` を呼ぶ（置換）3. CI 環境の偶発失敗時は 200ms 待機 + 1 回再試行で吸収 4. トレーシングログを検証 |
| 期待結果 | 最終的に置換 save が `Ok(())`。監査ログに `"rename retry exhausted"` / `outcome=exhausted`（クォート無し wire format）が **emit されていない**。`outcome=pending` / `outcome=succeeded` 経路の emit は**許容**（CI 環境の Defender 介入で偶発 retry が起こり得るため、retry 経路自体は NG にしない）|

---

## TC-I29-D (unit): `reverify_no_reparse_point` の TOCTOU 判定単体検証（Windows、`atomic.rs` 内 `#[cfg(test)]`）

> **背景**: Issue #65 retry 補強の二次防衛線 §`Win retry 中 TOCTOU` を担保する `reverify_no_reparse_point` を**ユニットレベルで決定的に検証**する。retry sleep 窓中に junction を差し替える race は非決定的で flaky になりやすいため、判定単体を直接呼び出して 4 経路（通常ファイル / 未存在 / junction / dir symlink）を網羅する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29-D-1 〜 TC-I29-D-4 |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、TOCTOU 二次防衛線） |
| 対応する工程 | 基本設計（`../basic-design/security.md` §atomic write の二次防衛線 §Win retry 中 TOCTOU）/ 詳細設計（`AtomicWriter::reverify_no_reparse_point`） |
| 種別 | 正常系 (D-1, D-2) / 異常系 (D-3, D-4) |
| 配置 | `crates/shikomi-infra/src/persistence/sqlite/atomic.rs` の `#[cfg(test)] mod tests` 内 `#[cfg(windows)]` ガード（関数が `pub(crate)` 未満で integration 不可） |
| 操作 | D-1: 通常ファイル → `Ok` / D-2: 未存在パス → `Ok`（初回 save の `final_path` 経路）/ D-3: `mklink /J` で junction → `Err(InvalidVaultDir { reason: SymlinkNotAllowed })` / D-4: `symlink_dir` で dir symlink → 同上 |
| 期待結果 | 上記 4 経路すべて期待値通り。D-3 / D-4 は `mklink /J` / `symlink_dir` が失敗する制約付きランナー（権限不足）では skip（`stderr` に skip 理由を出力）|

**実装上の注意**:
- D-3 (junction) は **管理者権限不要** で作成可能（`FILE_ATTRIBUTE_REPARSE_POINT (0x400)` ビット検出経路）
- D-4 (dir symlink) は **Developer Mode 有効または管理者権限**が必要（`is_symlink()` 検出経路、`windows-latest` GA runner は Developer Mode 有効）
- D-3 と D-4 で **検出経路が異なる**（reparse point ビット vs symlink フラグ）ため両方検証が必要

---

*対応 Issue: #10, #14, #65 / 親ドキュメント: `../index.md` / 改訂履歴: `./changelog.md`*
