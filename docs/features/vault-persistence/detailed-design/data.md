# 詳細設計 — データ構造

<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/detailed-design/data.md -->
<!-- 上位文書: ./index.md / ./classes.md -->

> **記述ルール**: 疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書かない。Rust のシグネチャはインライン `code` で示す。

## 定数・境界値

| 名前 | 型 | 用途 | 値 |
|------|---|------|------|
| `SchemaSql::APPLICATION_ID` | `u32` | `PRAGMA application_id`。"shkm" のバイト列 | `0x73_68_6B_6D` |
| `SchemaSql::USER_VERSION_INITIAL` | `u32` | `PRAGMA user_version`。本 Issue のスキーマ世代 | `1` |
| `SchemaSql::USER_VERSION_SUPPORTED_MIN` | `u32` | 読込み時の下限 | `1` |
| `SchemaSql::USER_VERSION_SUPPORTED_MAX` | `u32` | 読込み時の上限 | `1` |
| `VAULT_DB_FILENAME` | `&'static str` | vault ファイル名 | `"vault.db"` |
| `VAULT_DB_NEW_FILENAME` | `&'static str` | atomic write 中間ファイル名 | `"vault.db.new"` |
| `VAULT_DB_LOCK_FILENAME` | `&'static str` | advisory lock ファイル名 | `"vault.db.lock"` |
| `PROTECTED_PATH_PREFIXES_UNIX` | `&'static [&'static str]` | `SHIKOMI_VAULT_DIR` で拒否する保護領域プレフィックス（Unix） | `["/proc", "/sys", "/dev", "/etc", "/boot", "/root"]` |
| `PROTECTED_PATH_PREFIXES_WIN` | `&'static [&'static str]` | 同上（Windows、case-insensitive 比較） | `["C:\\Windows", "C:\\Program Files", "C:\\Program Files (x86)"]` |
| `DIR_MODE_UNIX` | `u32` | ディレクトリの期待 mode（Unix） | `0o700` |
| `FILE_MODE_UNIX` | `u32` | ファイルの期待 mode（Unix） | `0o600` |
| `MODE_MASK_UNIX` | `u32` | mode 比較時に有効な下位 bit | `0o777` |
| `ENV_VAR_VAULT_DIR` | `&'static str` | 環境変数名 | `"SHIKOMI_VAULT_DIR"` |
| `APP_SUBDIR_NAME` | `&'static str` | `dirs::data_dir()` 下のサブディレクトリ | `"shikomi"` |
| `TRACKING_ISSUE_ENCRYPTED_VAULT` | `Option<u32>` | `UnsupportedYet` で参照する tracking issue 番号（本 Issue 次の暗号化 Issue）。**`Option` 型で保持**し、暗号化 Issue 発行前は `None`、発行後に `Some(n)` へ定数差替え。`0` プレースホルダ運用を禁止（commit 忘れで `#0` を永遠に出し続けるバグ温床を型で防止、Fail Safe by type） | 本 Issue マージ時点では `None`。暗号化 Issue 発行 PR 内で `Some(<number>)` に更新し、`PersistenceError::UnsupportedYet::Display` が `tracking issue: (未発行)` / `tracking issue: #<番号>` を出し分ける |

**日時表現**:

- SQLite `TEXT` カラムに RFC3339 UTC 文字列で保存。形式: `YYYY-MM-DDTHH:MM:SS.ssssssZ`（マイクロ秒 6 桁、`Z` 固定）
- 書込時は `time::OffsetDateTime` を UTC に変換し、マイクロ秒に切り捨てたうえで `format_description::well_known::Rfc3339` で出力
- 読込時は `OffsetDateTime::parse` で復元。パース失敗は `Corrupted { reason: InvalidRfc3339, ... }` でラップ
- **マイクロ秒丸め**は `docs/features/vault/detailed-design.md` §バイナリ正規形仕様と整合（AAD の 26 byte レイアウトが前提とするため、暗号化モード時の round-trip で壊れないように本 Issue の段階からマイクロ秒粒度を強制）

## モジュール別公開メソッドのシグネチャ（要点）

Rust のシグネチャをインラインで示す。`Result` は `Result<_, PersistenceError>` の略記。

### `VaultRepository` trait

- `fn load(&self) -> Result<Vault>`
- `fn save(&self, vault: &Vault) -> Result<()>`
- `fn exists(&self) -> Result<bool>`

（trait オブジェクト利用のため `&self` 固定。`Send + Sync` 境界は trait 定義側では課さず、実装側と呼出側が必要に応じて要求する）

### `SqliteVaultRepository`

`impl SqliteVaultRepository`:
- `pub fn new() -> Result<Self, PersistenceError>` — OS 標準 vault ディレクトリで構築（`SHIKOMI_VAULT_DIR` または `dirs::data_dir()`）。得られたパスは `VaultPaths::new` の 7 段階バリデーションを通過する
- `#[doc(hidden)] pub fn with_dir(dir: PathBuf) -> Self` — 明示ディレクトリで構築。**`#[doc(hidden)]` で公開 doc から隠蔽する内部・テスト専用 API**。`rustdoc` には現れず、`use shikomi_infra::persistence::SqliteVaultRepository` 経由でのみ参照可能（`cargo doc` 標準利用者には見えない）。内部で `VaultPaths::new_unchecked(dir)` を呼んでバリデーションをスキップする（tempdir に対する無用な失敗を回避）。本関数の正当用途は以下に限定する: (a) `shikomi-infra` 内部の統合テスト（`#[cfg(test)]` 配下）、(b) `SHIKOMI_VAULT_DIR` バリデーション済みパスでの内部呼出、(c) 将来の CLI から `--vault-dir` オプション経由で渡す場合（この場合 CLI 側が再度バリデーションを通す責務）。`new()` が正規 API
- `pub fn paths(&self) -> &VaultPaths`

`impl VaultRepository for SqliteVaultRepository`: `load`, `save`, `exists`（trait シグネチャに従う）

### `VaultPaths`

- `pub fn new(dir: PathBuf) -> Result<Self, PersistenceError>` — **公開 API**、7 段階バリデーションを通す（`../basic-design/security.md` §vault ディレクトリ検証）。失敗は `InvalidVaultDir { path, reason: VaultDirReason }`
- `pub(crate) fn new_unchecked(dir: PathBuf) -> Self` — **crate 内部・infallible**。バリデーションをスキップしパスだけを派生させる（`with_dir` 専用）。`pub(crate)` で crate 外からは呼べない。明示的な `_unchecked` サフィックスで「検証を意図的にスキップしている」ことを型名で可視化（Fail Safe by naming）
- `pub fn dir(&self) -> &Path` — canonicalize 済みの絶対パス（`new` 経由の場合）
- `pub fn vault_db(&self) -> &Path`
- `pub fn vault_db_new(&self) -> &Path`
- `pub fn vault_db_lock(&self) -> &Path` — advisory lock ファイル（`vault.db.lock`）

### `VaultLock`（`pub(crate)`、RAII ハンドル、REQ-P13）

- `pub(crate) fn acquire_exclusive(paths: &VaultPaths) -> Result<VaultLock, PersistenceError>` — `save` 用。`fs4::FileExt::try_lock_exclusive`（Unix: `flock(LOCK_EX | LOCK_NB)`）／ `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY)`（Windows）。非ブロッキング、失敗時は `Locked { path, holder_hint }`
- `pub(crate) fn acquire_shared(paths: &VaultPaths) -> Result<VaultLock, PersistenceError>` — `load` 用。`try_lock_shared` / `LockFileEx(LOCKFILE_FAIL_IMMEDIATELY)`（`LOCKFILE_EXCLUSIVE_LOCK` なし）
- `Drop` 実装でロック解放とファイルハンドル close。`vault.db.lock` ファイル自体は残す（再利用、0 bytes）

### `Audit`（`pub(crate)`、REQ-P14、秘密を受けない 6 関数）

- `pub(crate) fn entry_load(paths: &VaultPaths)` — `load` 冒頭の info イベント発行
- `pub(crate) fn entry_save(paths: &VaultPaths, record_count: usize)` — `save` 冒頭の info イベント発行
- `pub(crate) fn exit_ok_load(record_count: usize, protection_mode: ProtectionMode, elapsed_ms: u64)` — load 成功時
- `pub(crate) fn exit_ok_save(record_count: usize, bytes_written: u64, elapsed_ms: u64)` — save 成功時
- `pub(crate) fn exit_err(err: &PersistenceError, elapsed_ms: u64)` — 全エラー経路の終了イベント（内部でバリアント→レベル写像、`../basic-design/security.md` §監査ログ規約参照）
- `pub(crate) fn retry_event(stage: &'static str, attempt: u32, raw_os_error: i32, elapsed_ms: u64, outcome: RetryOutcome)` — Issue #65、`cfg(windows)` rename 段の retry 発火・完了・全敗を発行（`Pending` / `Succeeded` は `warn`、`Exhausted` は `error`、`../basic-design/security.md` §retry 監査ログ参照）。`#[cfg_attr(not(windows), allow(dead_code))]` で非 Windows ビルドの dead_code 警告を抑止しつつ API 公開は全プラットフォームで維持（テスト・将来の他経路再利用想定）

### `RetryOutcome`（`pub(crate)`、Issue #65、`Audit::retry_event` 引数の型安全 enum）

```text
RetryOutcome
├── Pending      — 各 retry 試行直前（sleep + 再 rename の前）に発行、warn レベル
├── Succeeded    — retry の rename 成功直後に発行、warn レベル
└── Exhausted    — 5 回全敗で AtomicWriteFailed { stage: Rename } 返却直前に発行、error レベル（DoS 兆候、OWASP A09 上位通報候補）
```

- **採用根拠（DRY / Tell, Don't Ask / Fail Safe by type）**: 旧シグネチャ `outcome: &'static str` は `if outcome == "exhausted"` の文字列 switch を内部で必要とし、タイポ即バグ（`"exhuasted"` 等を実装側が誤記しても型検査で検出されない）。enum 化で **発行レベル分岐の網羅性をコンパイラに検査させる**（新バリアント追加時に `match` の分岐漏れを CI で検出）。`PersistenceError::AtomicWriteStage` enum と同方針（`./error.md` §`AtomicWriteStage`）
- **`Display` 実装**: `Pending => "pending"` / `Succeeded => "succeeded"` / `Exhausted => "exhausted"` の文字列を返す。`tracing::warn!(outcome = %outcome, ...)` の `%` 表記でフィールド出力され、subscriber 側 grep / 集計ロジックは旧文字列語彙のまま機能（後方互換）。subscriber が enum の構造（discriminant 等）に依存することは設計外
- **`Debug` 実装**: derive で十分（`Pending` / `Succeeded` / `Exhausted` のバリアント名がそのまま出る、秘密値を含まない型なので `?outcome` も安全）
- **`#[cfg_attr(not(windows), allow(dead_code))]`**: `retry_event` と同様に非 Windows ビルドでの dead_code 警告を抑止。本 enum 自体は `cfg(any(test, windows))` ガードを掛けず常時定義（テスト経路 / 将来の他経路再利用想定、API 一貫性）

### `PersistenceError`（`thiserror::Error` derive）

- **全 11 バリアント**（`Io` / `Sqlite` / `Corrupted` / `InvalidPermission` / `InvalidVaultDir` / `OrphanNewFile` / `AtomicWriteFailed` / `SchemaMismatch` / `UnsupportedYet` / `CannotResolveVaultDir` / `Locked`）
- `#[source]` で下位 error を保持
- `From<std::io::Error>` / `From<rusqlite::Error>` / `From<shikomi_core::DomainError>` を手動実装し、`?` 演算子で透過的に伝播

### `PermissionGuard`（`pub(crate)`、関連関数の集合）

- `fn ensure_dir(path: &Path) -> Result<()>` — 作成時に `0o700` / 所有者 ACL を**強制設定**
- `fn ensure_file(path: &Path) -> Result<()>` — 作成済みファイルに `0o600` / 所有者 ACL を設定
- `fn verify_dir(path: &Path) -> Result<()>` — 既存ディレクトリの mode / ACL を検証
- `fn verify_file(path: &Path) -> Result<()>` — 既存ファイルの mode / ACL を検証

### `AtomicWriter`（`pub(crate)`、関連関数の集合、状態なし）

- `fn detect_orphan(new_path: &Path) -> Result<()>` — `.new` が存在したら `Err(OrphanNewFile)`
- `fn write_new(paths: &VaultPaths, vault: &Vault) -> Result<()>` — `.new` 作成 → PRAGMA → DDL → Tx 内 insert → COMMIT → close。OS パーミッション設定を作成直後に行う
- `#[cfg(test)] fn write_new_only(paths: &VaultPaths, vault: &Vault) -> Result<()>` — **テスト専用フック**（AC-06 対応）。`write_new` と同一だが、`fsync_and_rename` を呼ばずに `.new` を残したまま return する。atomic write の中断状態を決定的に再現するため
- `fn fsync_and_rename(paths: &VaultPaths) -> Result<()>` — `.new` を open し `sync_all`、親ディレクトリを open し `sync_all`、rename / ReplaceFileW
- `fn cleanup_new(new_path: &Path) -> Result<()>` — best-effort 削除。失敗時は `tracing::warn!` でログ、上位には伝播しない（呼出側のエラーを上書きしない）

### `Mapping`（`pub(crate)`、関連関数の集合、状態なし）

- `fn vault_header_to_params(header: &VaultHeader) -> HeaderParams` — insert 用パラメータ束を作る
- `fn row_to_vault_header(row: &rusqlite::Row) -> Result<VaultHeader, PersistenceError>`
- `fn record_to_params<'a>(record: &'a Record) -> RecordParams<'a>` — insert 用パラメータ束
- `fn row_to_record(row: &rusqlite::Row) -> Result<Record, PersistenceError>`

（`HeaderParams` / `RecordParams` は `rusqlite::params!` を組み立てるための内部型。`pub(crate)` で外部に漏らさない）

### `SchemaSql`（`pub(crate)`、定数のみ）

- `pub(crate) const CREATE_VAULT_HEADER: &str` — `CREATE TABLE IF NOT EXISTS vault_header ...`
- `pub(crate) const CREATE_RECORDS: &str`
- `pub(crate) const PRAGMA_JOURNAL_MODE: &str = "PRAGMA journal_mode = DELETE;"`
- `pub(crate) const PRAGMA_APPLICATION_ID_SET: &str`
- `pub(crate) const PRAGMA_USER_VERSION_SET: &str`
- `pub(crate) const PRAGMA_APPLICATION_ID_GET: &str = "PRAGMA application_id;"`
- `pub(crate) const PRAGMA_USER_VERSION_GET: &str = "PRAGMA user_version;"`
- `pub(crate) const SELECT_VAULT_HEADER: &str`
- `pub(crate) const SELECT_RECORDS_ORDERED: &str`（`ORDER BY created_at ASC, id ASC` で決定的順序）
- `pub(crate) const INSERT_VAULT_HEADER: &str`
- `pub(crate) const INSERT_RECORD: &str`

全 SQL リテラルは **const**。コンパイル時に解決され、実行時に文字列連結される箇所を作らない（REQ-P12）。

## SQLite スキーマ詳細（DDL）

**`vault_header` テーブル**（1 行固定、`id = 1` で強制）:

| カラム | 型 | NULL | 制約 |
|-------|---|------|------|
| `id` | `INTEGER` | NOT NULL | `PRIMARY KEY CHECK(id = 1)` |
| `protection_mode` | `TEXT` | NOT NULL | `CHECK(protection_mode IN ('plaintext', 'encrypted'))` |
| `vault_version` | `INTEGER` | NOT NULL | `CHECK(vault_version >= 1)` |
| `created_at` | `TEXT` | NOT NULL | — |
| `kdf_salt` | `BLOB` | NULL 可 | 以下の 1 つの CHECK で強制 |
| `wrapped_vek_by_pw` | `BLOB` | NULL 可 | 同上 |
| `wrapped_vek_by_recovery` | `BLOB` | NULL 可 | 同上 |

**テーブル CHECK 制約**（`CREATE TABLE ... CHECK(...)` に集約）:

- `(protection_mode = 'plaintext' AND kdf_salt IS NULL AND wrapped_vek_by_pw IS NULL AND wrapped_vek_by_recovery IS NULL) OR (protection_mode = 'encrypted' AND kdf_salt IS NOT NULL AND length(kdf_salt) = 16 AND wrapped_vek_by_pw IS NOT NULL AND length(wrapped_vek_by_pw) >= 32 AND wrapped_vek_by_recovery IS NOT NULL AND length(wrapped_vek_by_recovery) >= 32)`
- wrapped VEK の最小長 32 byte は「AES-256-GCM の wrap 出力 = `VEK 32B + tag 16B = 48B` 最低」を緩めに下限チェック。暗号化モード対応 Issue で正確な長さ検証に差し替え可能

**`records` テーブル**:

| カラム | 型 | NULL | 制約 |
|-------|---|------|------|
| `id` | `TEXT` | NOT NULL | `PRIMARY KEY`、UUIDv7 文字列 |
| `kind` | `TEXT` | NOT NULL | `CHECK(kind IN ('text', 'secret'))` |
| `label` | `TEXT` | NOT NULL | `CHECK(length(label) > 0)` |
| `payload_variant` | `TEXT` | NOT NULL | `CHECK(payload_variant IN ('plaintext', 'encrypted'))` |
| `plaintext_value` | `TEXT` | NULL 可 | テーブル CHECK で variant と相関 |
| `nonce` | `BLOB` | NULL 可 | 同上 |
| `ciphertext` | `BLOB` | NULL 可 | 同上 |
| `aad` | `BLOB` | NULL 可 | 同上 |
| `created_at` | `TEXT` | NOT NULL | — |
| `updated_at` | `TEXT` | NOT NULL | — |

**テーブル CHECK 制約**:

- `(payload_variant = 'plaintext' AND plaintext_value IS NOT NULL AND nonce IS NULL AND ciphertext IS NULL AND aad IS NULL) OR (payload_variant = 'encrypted' AND plaintext_value IS NULL AND nonce IS NOT NULL AND length(nonce) = 12 AND ciphertext IS NOT NULL AND length(ciphertext) > 0 AND aad IS NOT NULL AND length(aad) = 26)`

**インデックス**: 初期スキーマでは不要（`id` の PK インデックスのみ）。レコード件数は個人利用で数百〜数千、`SELECT_RECORDS_ORDERED` は full scan で p95 50 ms を満たす（非機能要求）。将来レコード数が増えた場合は `(updated_at)` インデックス等を別 Issue で追加。
