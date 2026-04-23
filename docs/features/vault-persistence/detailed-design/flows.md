# 詳細設計 — 制御フロー・エラー型・OS別実装

<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/detailed-design/flows.md -->
<!-- 上位文書: ./index.md / ./classes.md / ./data.md -->

> **記述ルール**: 疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書かない。

## エラー型詳細

### `PersistenceError` の各バリアントとフィールド（全 11 バリアント）

| バリアント | フィールド | 発生箇所 |
|-----------|-----------|---------|
| `Io { path: PathBuf, #[source] source: std::io::Error }` | 対象パスと原因 | ファイルシステム操作 |
| `Sqlite { #[source] source: rusqlite::Error }` | — | SQLite API |
| `Corrupted { table: &'static str, row_key: Option<String>, reason: CorruptedReason, #[source] source: Option<shikomi_core::DomainError> }` | 対象テーブル名、特定できる場合は row PK（UUID 文字列）、原因分類、下位ドメインエラー（旧 `DomainError` バリアントを本バリアントに統合、`classes.md` §設計判断 §12 参照） | `Mapping::row_to_*` / `Vault::add_record` 失敗 |
| `InvalidPermission { path: PathBuf, expected: &'static str, actual: String }` | 対象パス、期待値（例: `"0600"` or `"owner-only DACL"`）、実測（mode 数値文字列または DACL 要約） | `PermissionGuard::verify_*` |
| `InvalidVaultDir { path: PathBuf, reason: VaultDirReason }` | `SHIKOMI_VAULT_DIR` が危険パス（パストラバーサル・シンボリックリンク・root 保護領域等）を指している場合 | `VaultPaths::new` のバリデーション |
| `OrphanNewFile { path: PathBuf }` | `.new` 絶対パス | `AtomicWriter::detect_orphan` |
| `AtomicWriteFailed { stage: AtomicWriteStage, #[source] source: std::io::Error }` | stage 列挙、下位 I/O error | `AtomicWriter::*` |
| `SchemaMismatch { expected_application_id: u32, found_application_id: u32, expected_version_range: (u32, u32), found_user_version: u32 }` | 期待値と実測値 | `SqliteVaultRepository::load` 冒頭 |
| `UnsupportedYet { feature: &'static str, tracking_issue: Option<u32> }` | 未対応機能名、tracking issue 番号（発行前は `None`） | 暗号化モード検出時 |
| `CannotResolveVaultDir` | — | `SqliteVaultRepository::new` |
| `Locked { path: PathBuf, holder_hint: Option<u32> }` | 別プロセスが vault ディレクトリを排他ロック中（`holder_hint` は Unix の `F_GETLK` PID、Windows では `None`） | `VaultLock::acquire_*` 失敗時 |

### `VaultDirReason` の各バリアント（`InvalidVaultDir.reason`）

| バリアント | 意味 |
|-----------|------|
| `NotAbsolute` | 相対パスが指定された（`PathBuf::is_absolute()` が false） |
| `PathTraversal` | パス要素に `..` を含む（`canonicalize` 前の生値チェック、早期拒否） |
| `SymlinkNotAllowed` | `fs::symlink_metadata` → `FileType::is_symlink()` が true（シンボリックリンクを含む経路） |
| `Canonicalize { #[source] source: std::io::Error }` | `fs::canonicalize` 自体が失敗（存在しない・読取不可等、但し「存在しない」は正常ケースとして扱い `NotADirectory` と区別） |
| `ProtectedSystemArea { prefix: &'static str }` | canonicalize 後のパスが `/proc` / `/sys` / `/dev` / `/etc`（Unix）、または `C:\Windows` / `C:\Program Files`（Windows）配下を指す |
| `NotADirectory` | 既存パスだがディレクトリではない |

### `CorruptedReason` の各バリアント

| バリアント | 意味 |
|-----------|------|
| `MissingVaultHeader` | `vault_header` テーブルに 0 行 |
| `UnknownProtectionMode { raw: String }` | CHECK 制約を抜けた不明値（通常は起こらないが破損検出用） |
| `InvalidRowCombination { detail: String }` | CHECK 制約を満たしているはずなのに組合せ不整合（SQLite 破損等の想定外） |
| `InvalidRfc3339 { column: &'static str, raw: String }` | RFC3339 パース失敗 |
| `InvalidUuidString { raw: String }` | UUIDv7 文字列パース失敗 |
| `PayloadVariantMismatch { expected: &'static str, got: &'static str }` | variant と NULL 組合せの不整合 |
| `NullViolation { column: &'static str }` | CHECK を抜けた NULL 検出（想定外） |

### `AtomicWriteStage` の各バリアント

| バリアント | 意味 |
|-----------|------|
| `PrepareNew` | `.new` 作成前の準備（親ディレクトリ作成等） |
| `WriteTemp` | `.new` への SQLite 書込中（open / PRAGMA / DDL / insert / COMMIT） |
| `FsyncTemp` | `.new` の `sync_all` |
| `FsyncDir` | 親ディレクトリの `sync_all` |
| `Rename` | `rename` / `ReplaceFileW` |
| `CleanupOrphan` | `.new` の削除失敗（best-effort） |

## load / save のアルゴリズム詳細（制御フロー）

### `SqliteVaultRepository::load(&self)`

1. `audit::entry_load(&self.paths)` — 監査ログに load 開始を記録
2. `PermissionGuard::verify_dir(self.paths.dir())` — 失敗なら `InvalidPermission` を即 return
3. `VaultLock::acquire_shared(&self.paths)?` — 共有ロック取得（複数プロセス read は許可）。別プロセスが排他ロック保持中なら `Locked { path, holder_hint }` を即 return（Fail Fast、待機しない）。以降 step 15 まで `VaultLock` がスコープに生存し、drop 時に自動解放（RAII）
4. `AtomicWriter::detect_orphan(self.paths.vault_db_new())` — 残存なら `OrphanNewFile` を即 return
5. `self.paths.vault_db().try_exists()?` が `false` なら `Io { path, source: NotFound-like }` を即 return（「vault が無い」判定は `exists()` の責務）
6. `PermissionGuard::verify_file(self.paths.vault_db())` — 失敗なら `InvalidPermission` を即 return
7. `Connection::open_with_flags(self.paths.vault_db(), OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX)` — 失敗は `Sqlite` で return
8. `PRAGMA application_id` を `query_row`、取得値が `SchemaSql::APPLICATION_ID` でなければ `SchemaMismatch`
9. `PRAGMA user_version` を `query_row`、取得値が `[USER_VERSION_SUPPORTED_MIN, USER_VERSION_SUPPORTED_MAX]` 範囲外なら `SchemaMismatch`
10. `SELECT_VAULT_HEADER` を実行、0 行なら `Corrupted { reason: MissingVaultHeader }`、2 行以上は `CHECK(id=1)` により物理的に起きないが防御で `Corrupted { reason: InvalidRowCombination }`
11. `Mapping::row_to_vault_header(&row)` で `VaultHeader` を再構築（失敗は `Corrupted { reason: ..., source: Some(domain_error) }` に統合、`classes.md` §設計判断 §12 参照）
12. `VaultHeader::protection_mode() == ProtectionMode::Encrypted` なら **`UnsupportedYet { feature: "encrypted vault persistence", tracking_issue: TRACKING_ISSUE_ENCRYPTED_VAULT }` を即 return**（step 11 で得た `header` を使わず、records を読まない）
13. `Vault::new(header)` で集約を構築
14. `SELECT_RECORDS_ORDERED` を実行、各行を `Mapping::row_to_record` で `Record` に変換
15. 各 `Record` を `Vault::add_record` で集約に追加（**ドメイン側でモード整合とID重複が検証される**）。失敗は `Corrupted { table: "records", row_key: Some(uuid_str), reason: InvalidRowCombination, source: Some(domain_error) }` にラップして return
16. `audit::exit_ok_load(record_count, protection_mode, elapsed_ms)` を発行、`VaultLock` が drop され共有ロックが解放される
17. `Ok(vault)` を返却

（任意の step で `Err` を return する際は、return 直前に `audit::exit_err(&err, elapsed_ms)` を発行する）

### `SqliteVaultRepository::save(&self, vault: &Vault)`

1. `audit::entry_save(&self.paths, vault.record_count())` — 監査ログに save 開始を記録
2. `vault.protection_mode() == ProtectionMode::Encrypted` なら **`UnsupportedYet { ... }` を即 return**（Fail Fast、step 3 以降のファイル操作を一切しない）
3. `PermissionGuard::ensure_dir(self.paths.dir())` — 作成 or 既存強制
4. `VaultLock::acquire_exclusive(&self.paths)?` — 排他ロック取得。別プロセスが排他/共有ロックを保持中なら `Locked { path, holder_hint }` を即 return（Fail Fast、待機・再試行しない）。以降 step 7 完了まで `VaultLock` がスコープに生存し drop 時に自動解放（RAII）
5. `AtomicWriter::detect_orphan(self.paths.vault_db_new())` — 残存なら `OrphanNewFile` を return（ユーザ操作待ち、AC-14 の save 側検証）
6. `AtomicWriter::write_new(self.paths, vault)`:
   1. `File::create(new_path)` 相当の mode 指定 open（Unix: `OpenOptions::mode(0o600)`、Windows: 作成後に ACL 設定）
   2. file handle を drop（SQLite が同じパスを再 open できるようにする）
   3. `Connection::open_with_flags(new_path, SQLITE_OPEN_CREATE | SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_NO_MUTEX)`
   4. `PermissionGuard::ensure_file(new_path)` — SQLite が open 時に mode を変えた場合の再強制
   5. `execute(PRAGMA_APPLICATION_ID_SET)`、`execute(PRAGMA_USER_VERSION_SET)`、`execute(PRAGMA_JOURNAL_MODE)`
   6. `execute(CREATE_VAULT_HEADER)`、`execute(CREATE_RECORDS)`
   7. `let tx = conn.transaction()?`
   8. `Mapping::vault_header_to_params(vault.header())` で params 取得、`tx.execute(INSERT_VAULT_HEADER, params)` 実行
   9. `for record in vault.records()`: `Mapping::record_to_params(record)` → `tx.execute(INSERT_RECORD, params)`
   10. `tx.commit()?`
   11. `drop(conn)`
7. `AtomicWriter::fsync_and_rename(self.paths)`:
   1. `File::open(new_path)?.sync_all()?`（`FsyncTemp`）
   2. `File::open(dir)?.sync_all()?`（`FsyncDir`、Unix のみ。Windows では no-op）
   3. `fs::rename(new_path, final_path)?` または `ReplaceFileW(..., REPLACEFILE_WRITE_THROUGH)`（`Rename`）
   4. 各段階で失敗したら `AtomicWriter::cleanup_new(new_path)` を呼び、best-effort で `.new` を削除。元のエラーを `AtomicWriteFailed { stage, source }` にラップして return
8. `audit::exit_ok_save(record_count, bytes_written, elapsed_ms)` を発行、`VaultLock` が drop され排他ロックが解放される
9. `Ok(())` を返却

### `SqliteVaultRepository::exists(&self)`

1. `self.paths.vault_db().try_exists().map_err(|e| PersistenceError::Io { path: self.paths.vault_db().to_path_buf(), source: e })`

（`exists` は軽量クエリのため `audit::entry_*` / `exit_*` は呼ばず、`tracing::debug!` で「呼出と結果」を直接記録する例外扱いとする — `basic-design.md` §監査ログ規約）

### `AtomicWriter::write_new_only`（テスト専用フック、AC-06 対応）

1. `write_new` と同一ロジックで `.new` を書き込む
2. **`fsync_and_rename` を呼ばず**に `.new` を残したまま `Ok(())` を return
3. `#[cfg(test)]` 限定で公開。本番ビルドには含まれない
4. テスト側は `write_new_only` 呼出後に `load()` を呼び、`.new` 残存による `OrphanNewFile` 返却と `vault.db` 本体が未変更であることを検証する（SIGKILL 非決定的テストの論理等価版）

## OS 別パーミッション実装詳細

### Unix（`cfg(unix)`）

- `ensure_dir`: ディレクトリが存在しない場合は `fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?`。存在する場合は `fs::set_permissions(path, Permissions::from_mode(0o700))?` で強制上書き
- `ensure_file`: `fs::set_permissions(path, Permissions::from_mode(0o600))?`
- `verify_dir`: `fs::metadata(path)?.permissions().mode() & 0o777 == 0o700` を検証
- `verify_file`: `fs::metadata(path)?.permissions().mode() & 0o777 == 0o600` を検証
- macOS / Linux で挙動は同一（`std::os::unix::fs` が共通 trait を提供）

### Windows（`cfg(windows)`）

- `ensure_dir` / `ensure_file`:
  1. `GetSidSubAuthorityCount` 等で現在のプロセスの所有者 SID を取得
  2. `BuildExplicitAccessWithNameW`（または `EXPLICIT_ACCESS_W` を直接構築）で所有者 SID に `GENERIC_READ | GENERIC_WRITE` を Allow
  3. `SetEntriesInAclW` で新 DACL を構築
  4. `SetNamedSecurityInfoW(path, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, ...)` で適用（継承を破棄）
- `verify_dir` / `verify_file`:
  1. `GetNamedSecurityInfoW` で DACL を取得
  2. `GetAclInformation` で ACE 数を取得
  3. 各 ACE を `GetAce` で取得、所有者 SID 以外のトラスティが **Allow ACE を持っていれば拒否**、所有者 SID の ACE に含まれる AccessMask が `GENERIC_READ | GENERIC_WRITE` 以外の権限（`GENERIC_EXECUTE`, `DELETE` 等）を含めば異常
- Windows の `ReplaceFileW`（`windows::Win32::Storage::FileSystem`）: `lpReplacementFileName = .new`, `lpReplacedFileName = vault.db`, `dwReplaceFlags = REPLACEFILE_WRITE_THROUGH`（内部で fsync 相当が走る）, `lpBackupFileName = null_ptr`（バックアップ不要）

## 具体的な SQL の要点（定数値の抜粋）

**`CREATE_VAULT_HEADER`**（1 行フォーマットで `const &str`、CHECK 制約を含む。以下は構造の要約、完全な SQL は実装ファイルで 1 箇所定義）:

- `CREATE TABLE IF NOT EXISTS vault_header(id INTEGER PRIMARY KEY CHECK(id = 1), protection_mode TEXT NOT NULL CHECK(protection_mode IN ('plaintext', 'encrypted')), vault_version INTEGER NOT NULL CHECK(vault_version >= 1), created_at TEXT NOT NULL, kdf_salt BLOB, wrapped_vek_by_pw BLOB, wrapped_vek_by_recovery BLOB, CHECK(...mode-col coherence...))`

**`CREATE_RECORDS`** 同様:

- `CREATE TABLE IF NOT EXISTS records(id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('text', 'secret')), label TEXT NOT NULL CHECK(length(label) > 0), payload_variant TEXT NOT NULL CHECK(payload_variant IN ('plaintext', 'encrypted')), plaintext_value TEXT, nonce BLOB, ciphertext BLOB, aad BLOB, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, CHECK(...variant-col coherence...))`

**`SELECT_RECORDS_ORDERED`**:

- `SELECT id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, created_at, updated_at FROM records ORDER BY created_at ASC, id ASC`

**`INSERT_VAULT_HEADER`**:

- `INSERT INTO vault_header(id, protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, wrapped_vek_by_recovery) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)`

**`INSERT_RECORD`**:

- `INSERT INTO records(id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)`

全 `?n` は `rusqlite::params!` マクロで**型付きバインド**する。`to_sql()` の Rust 型は以下の写像:

| ドメイン値 | Rust 型 | SQLite 型 | 備考 |
|----------|--------|----------|------|
| `ProtectionMode` | `&'static str`（`as_persisted_str()` の戻り） | `TEXT` | `"plaintext"` or `"encrypted"` |
| `VaultVersion` | `u16`（`value()`） | `INTEGER` | |
| `OffsetDateTime` | `String`（RFC3339 UTC） | `TEXT` | マイクロ秒丸め |
| `KdfSalt` | `&[u8]`（`as_array()`） | `BLOB` | NULL 可 |
| `WrappedVek` | `&[u8]`（`as_bytes()`） | `BLOB` | NULL 可 |
| `RecordId` | `String`（`Display` 経由） | `TEXT` | UUIDv7 ハイフン区切り |
| `RecordKind` | `&'static str` | `TEXT` | `"text"` or `"secret"` |
| `RecordLabel` | `&str`（`as_str()`） | `TEXT` | |
| `RecordPayload::Plaintext(SecretString)` | `&str`（`expose_secret()`） | `TEXT` | **`expose_secret` はここ 1 箇所のみ** |
| `RecordPayload::Encrypted(enc)` | `&[u8]`（nonce / ciphertext / aad） | `BLOB` | `Aad::to_canonical_bytes()` で 26B |

## テスト設計担当向けメモ（参考）

テスト設計書（`test-design/`）作成時の参考観点:

- **Round-trip プロパティテスト**: ランダム生成の `Vault`（1〜100 件レコード、平文モード）を save → load し、`header` / `records` が等価
- **CHECK 制約違反テスト**: `rusqlite::Connection::execute` で CHECK を故意に破る生 SQL を実行し、SQLite が `SQLITE_CONSTRAINT_CHECK` エラーを返すことを確認（防衛線）
- **atomic write 論理等価テスト（AC-06）**: `AtomicWriter::write_new_only` フックで `.new` のみ作成 → `vault.db` 未変更 / 後続 `load()` が `OrphanNewFile` を返すことを検証。SIGKILL 非決定的テストの代替
- **OS パーミッション テスト**: Unix で `chmod 644` → `load` が `InvalidPermission` を返す
- **暗号化モード vault**: `VaultHeader::new_encrypted` で作った `Vault` を save → `UnsupportedYet` が返る。暗号化モード vault.db（別 Issue 完成前に手動作成した fixture）を load → 同じく `UnsupportedYet`
- **破損 DB**: ゼロバイトファイル / 不正マジックバイト / 一部 SELECT 行の UUID 不正 → `Corrupted` / `SchemaMismatch` / `Sqlite` のいずれかが返り panic しない
- **REQ-P13 advisory lock**: 子プロセス 2 本並列 save → 一方が `Locked` で即 return
- **REQ-P14 監査ログ**: `tracing-test` で全ログを収集 → 秘密値（`plaintext_value` / `ciphertext` / `SecretString::expose_secret` の結果）が 1 文字も現れない
- **REQ-P15 vault ディレクトリ検証**: `SHIKOMI_VAULT_DIR=/etc`, `=../../`, 既存シンボリックリンクパス → 全て `InvalidVaultDir` で拒否（Unix で `tempfile::symlink` を使いリンク作成）
- **ユニットテストと結合テストの境界**:
  - ユニット: `Mapping` の純関数（`row_to_*`）、`PermissionGuard::verify_*` の mode 比較ロジック、`VaultPaths::new` 7 段階バリデーション各分岐
  - 結合: `SqliteVaultRepository::save` → `load` の round-trip（tempdir）、atomic write、OS パーミッション、`.new` 残存検出、`VaultLock` 競合、`tracing-test` 秘密漏洩検証
