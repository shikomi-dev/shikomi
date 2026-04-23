# 要件定義書

<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/requirements.md -->

## 機能要件

### REQ-P01: `VaultRepository` trait

| 項目 | 内容 |
|------|------|
| 入力 | `Vault` 値（save）、vault パス（load の構築時引数） |
| 処理 | 永続化の抽象インターフェース。`load` / `save` / `exists` の 3 メソッドを定義する。`shikomi-infra` が所有し、将来の別実装（テスト用 in-memory、別フォーマット）もこの trait に従う |
| 出力 | `load`: `Result<Vault, PersistenceError>` ／ `save`: `Result<(), PersistenceError>` ／ `exists`: `Result<bool, PersistenceError>`（パス解決時に I/O エラーが起きうるため `bool` に潰さず `Result` で返す、Fail Fast） |
| エラー時 | `PersistenceError`（REQ-P10）の各バリアントで区別 |

### REQ-P02: `SqliteVaultRepository`（`VaultRepository` 実装）

| 項目 | 内容 |
|------|------|
| 入力 | vault ディレクトリパス（`PathBuf`、テストでは tempdir） |
| 処理 | `rusqlite` バンドル版で SQLite ファイル（`vault.db`）を読み書きする `VaultRepository` の具象実装。1 インスタンスが 1 vault ファイルを管理 |
| 出力 | trait シグネチャに従う |
| エラー時 | 同上 |

### REQ-P03: SQLite スキーマ定義

| 項目 | 内容 |
|------|------|
| 入力 | — |
| 処理 | 2 テーブル構成: `vault_header`（1 行固定、`id INTEGER PRIMARY KEY CHECK(id=1)`）、`records`（PK=UUIDv7 TEXT）。両モード対応カラムを定義（暗号化カラムは暗号化モード時のみ NOT NULL、平文モード時は NULL、これを CHECK 制約で強制）。`PRAGMA application_id` と `PRAGMA user_version` に shikomi マジック値を記録 |
| 出力 | 初回 `save` 時にスキーマを自動作成（`CREATE TABLE IF NOT EXISTS`）。マイグレーションは `PRAGMA user_version` で管理、本 Issue では初期バージョン `1` のみ |
| エラー時 | スキーマ不一致（将来の後方互換チェック） → `PersistenceError::SchemaMismatch { file_version, supported_range }` |

### REQ-P04: Atomic Write

| 項目 | 内容 |
|------|------|
| 入力 | `Vault` 値 |
| 処理 | 書き込み先 `vault.db.new` に新規 SQLite ファイルを作成（既存 `.new` があれば事前に削除）→ 全データ書き込み → SQLite コネクション close（これにより `.new` へ内部バッファを flush）→ `.new` を `File::open` で開き直し `sync_all()` → 親ディレクトリを開き `sync_all()`（`rename` メタデータの永続化のため Unix で必要）→ `fs::rename`（Unix）/ `ReplaceFileW`（Windows）で `vault.db` を atomic 差替え |
| 出力 | `Ok(())` |
| エラー時 | 途中失敗時は `.new` を削除。`vault.db` は未変更のまま保たれる。`PersistenceError::AtomicWriteFailed { stage, source }` を返す（stage は `WriteTemp` / `FsyncTemp` / `FsyncDir` / `Rename` / `CleanupOrphan`） |

### REQ-P05: `.new` 残存検出

| 項目 | 内容 |
|------|------|
| 入力 | `load()` 呼び出し時 |
| 処理 | vault ディレクトリ内に `vault.db.new` が残存していれば、前回 save が途中で失敗した痕跡とみなす。`load` は自動削除せず **エラーで返す**（ユーザがリカバリ操作を選択するまで破壊的操作を行わない、Fail Secure） |
| 出力 | `Err(PersistenceError::OrphanNewFile(PathBuf))` |
| エラー時 | 呼び出し側が明示的に削除または復旧する。将来の GUI リカバリ画面（別 Issue）で誘導 |

### REQ-P06: OS パーミッション強制・検証（Unix）

| 項目 | 内容 |
|------|------|
| 入力 | vault ディレクトリパス / vault ファイルパス |
| 処理 | **作成時**: ディレクトリは `fs::DirBuilder::mode(0o700)`、ファイルは `OpenOptions::mode(0o600)` で作成。**検証時**（`load`/`save` の冒頭）: `fs::metadata` → `Permissions::mode()` で下位 9 bit を取り出し、`0o700`（ディレクトリ） / `0o600`（ファイル）に完全一致するかチェック。他ユーザに権限が開いているパターン（`0o640`, `0o644`, `0o755` 等）はすべて拒否 |
| 出力 | 正常: 続行 |
| エラー時 | `Err(PersistenceError::InvalidPermission { path, expected_mode, actual_mode })` |

### REQ-P07: Windows ACL 強制・検証

| 項目 | 内容 |
|------|------|
| 入力 | vault ディレクトリパス / vault ファイルパス |
| 処理 | **作成時**: `windows` crate の `SetNamedSecurityInfoW(SE_FILE_OBJECT, DACL_SECURITY_INFORMATION \| PROTECTED_DACL_SECURITY_INFORMATION, ...)` で DACL を設定する。**所有者 SID は「ファイル側の `OWNER_SECURITY_INFORMATION`」から取得**する（UAC 昇格状態で作られたファイルの所有者は `BUILTIN\Administrators` になりうるため、プロセストークン所有者と一致しない前提、§セキュリティ設計）。ACE はちょうど 1 個で、所有者 SID に対して **ファイル: `FILE_GENERIC_READ \| FILE_GENERIC_WRITE`**、**ディレクトリ: `FILE_GENERIC_READ \| FILE_GENERIC_WRITE \| FILE_TRAVERSE`** の Allow ACE を付与。`PROTECTED_DACL_SECURITY_INFORMATION` で親からの継承 ACE を破棄（`SE_DACL_PROTECTED` bit が立つ）、Everyone / Users / Authenticated Users / CREATOR_OWNER 等の組込みトラスティを DACL に含めない（明示 ACE 無しは暗黙拒否）。**検証時**: `GetNamedSecurityInfoW` で DACL と所有者 SID を 1 回で取得し、次の 4 条件を全て満たすことを検証する（1 つでも満たさなければ異常）：①`SE_DACL_PROTECTED` bit が立っている（継承破棄済み）、②`AceCount == 1`、③唯一の ACE が `ACCESS_ALLOWED_ACE_TYPE` かつトラスティ SID が所有者 SID と `EqualSid` で一致、④ACE の AccessMask が**期待値マスクと完全一致**（ファイル/ディレクトリ別、追加ビット `DELETE` / `WRITE_DAC` / `WRITE_OWNER` / `FILE_EXECUTE`（ファイル時）等があれば拒否） |
| 出力 | 正常: 続行 |
| エラー時 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE[\|FILE_TRAVERSE])", actual: <DACL 要約文字列> })`。**`actual` フィールドには ACE ごとの `trustee_sid: <S-1-5-21-... 形式>, access_mask: 0x<hex>, ace_type: <type>` を列挙した診断文字列を入れ**、SID は `ConvertSidToStringSidW` で文字列化する。秘密値は含まれない（§セキュリティ設計 §監査ログ規約） |
| 受入観点 | ①CI `windows-latest` runner で作成 → 検証 → 再検証（DACL 未変更）が pass、②手動で `icacls <path> /grant Everyone:F` で DACL を壊した後の `load` が `InvalidPermission` で失敗、③`tempfile::tempdir()` 直下の vault ディレクトリ（親 TEMP の継承 ACE を持つ）を `ensure_dir` 適用後に `verify_dir` すると継承 ACE が除去されている（`SE_DACL_PROTECTED` 確認）、④UAC 昇格シナリオ（runner デフォルト）でファイル所有者が `BUILTIN\Administrators` でも ACE 1 個 + SID 一致の契約が成立する |

### REQ-P08: Vault ディレクトリ解決

| 項目 | 内容 |
|------|------|
| 入力 | なし（環境変数・OS デフォルト）／テスト用に明示パス |
| 処理 | デフォルト: `dirs::data_dir()` の戻り値に `shikomi/` を付加。環境変数 `SHIKOMI_VAULT_DIR` が設定されていれば優先（CI / 明示指定向け、**`VaultPaths::new` でパストラバーサル・シンボリックリンク検証を必ず通す**、`basic-design/security.md` §vault ディレクトリ検証）。内部・テスト専用の構築口として `#[doc(hidden)] SqliteVaultRepository::with_dir(PathBuf)` を提供し、tempdir を直接受け取れる（正規 API は `new()`） |
| 出力 | `PathBuf`（vault ディレクトリ） |
| エラー時 | `dirs` が解決失敗（例: HOME 未設定の極端環境）→ `PersistenceError::CannotResolveVaultDir` |

### REQ-P09: ドメイン再構築時の検証

| 項目 | 内容 |
|------|------|
| 入力 | SQLite から取り出した生値（`String`, `i64`, `Vec<u8>` 等） |
| 処理 | 各 newtype の `try_new` / `try_from_str` 経由で `shikomi-core` ドメイン型に組み立てる。`RecordId::try_from_str` / `RecordLabel::try_new` / `VaultVersion::try_new` / `VaultHeader::new_*` / `RecordPayloadEncrypted::new` / `NonceBytes::try_new` / `KdfSalt::try_new` / `WrappedVek::try_new` / `CipherText::try_new` の順に通す。`Vault::add_record` でモード整合も検証される |
| 出力 | 正常: `Vault` |
| エラー時 | 検証失敗を `PersistenceError::Corrupted { table: &'static str, row_key: Option<String>, reason: CorruptedReason, source: Option<shikomi_core::DomainError> }` にラップ。旧 `domain_error` フィールドは `reason` + `#[source] source` に分離（設計判断メモ §12 で `DomainError` バリアントを `Corrupted` に統合した結果、`detailed-design/classes.md` 参照）。ドメインエラーの詳細は `#[source]` チェーンで辿れる |

### REQ-P10: `PersistenceError` 型

| 項目 | 内容 |
|------|------|
| 入力 | 各 I/O / SQLite / パーミッション異常の発生箇所 |
| 処理 | `thiserror::Error` で列挙型として実装。**11 バリアント**を排他区別: `Io` / `Sqlite` / `Corrupted`（ドメイン整合性違反も本バリアントに統合、旧 `DomainError` は廃止） / `InvalidPermission` / `InvalidVaultDir`（`SHIKOMI_VAULT_DIR` バリデーション違反、REQ-P15） / `OrphanNewFile` / `AtomicWriteFailed` / `SchemaMismatch` / `UnsupportedYet` / `CannotResolveVaultDir` / `Locked`（プロセス間 advisory lock 競合、REQ-P13）。全バリアントに `#[source]` または診断フィールド（`path` / `stage` / `reason` 等）を保持 |
| 出力 | `PersistenceError` 値 |
| エラー時 | エラー型自体は Fail しない（エラーを表現する型） |

### REQ-P11: 暗号化モード vault の明示拒否

| 項目 | 内容 |
|------|------|
| 入力 | `VaultHeader::Encrypted` variant を持つ `Vault`（save 側）／ vault.db の `vault_header.protection_mode='encrypted'` 行（load 側） |
| 処理 | 本 Issue のスコープは平文モードのみ。暗号化モードを受けたら入口で即時拒否（Fail Fast）。レコード BLOB を読み書きせず、スキーマが壊れない状態で早期 return |
| 出力 | `Err(PersistenceError::UnsupportedYet { feature: "encrypted vault persistence", tracking_issue: <番号> })` |
| エラー時 | 同上。別 Issue で `VekProvider` 実装と合わせて解除する |

### REQ-P12: SQL インジェクション禁止設計

| 項目 | 内容 |
|------|------|
| 入力 | ドメイン値（`RecordLabel`, `RecordId`, バイト列等） |
| 処理 | 全 SQL は `const` 文字列リテラルで定義し `rusqlite::params!` マクロで値をバインド。文字列連結（`format!` / `+`）で SQL を組み立てない。`PRAGMA` 値は静的リテラルのみで可変部分を持たない |
| 出力 | 正常: 続行 |
| エラー時 | 設計上発生しない（型とマクロで排除）。コードレビュー / grep / clippy で機械的検証 |

### REQ-P13: プロセス間 advisory lock

| 項目 | 内容 |
|------|------|
| 入力 | `VaultPaths`（`vault.db.lock` ファイルを派生パスに持つ） |
| 処理 | `VaultLock::acquire_exclusive(&paths)` を `save` 冒頭で、`VaultLock::acquire_shared(&paths)` を `load` 冒頭で呼ぶ。`fs4` crate（fs2 の積極メンテ中フォーク、本体 fs2 は 2018 以降停止のため A06 回避）の `try_lock_exclusive` / `try_lock_shared`（Unix: `flock(LOCK_EX/LOCK_SH, LOCK_NB)`、Windows: `LockFileEx(LOCKFILE_FAIL_IMMEDIATELY [+LOCKFILE_EXCLUSIVE_LOCK])`）。`Drop` 実装でロック解放（RAII）。非ブロッキング、失敗時は即時エラー |
| 出力 | `Ok(VaultLock)`。呼出側のスコープで生存 |
| エラー時 | `PersistenceError::Locked { path, holder_hint }`（`holder_hint` は Unix `F_GETLK` 由来の競合プロセス PID、取得不能時 `None`）。呼出側（CLI）はエラー表示して別プロセス終了をユーザに促す責務 |

### REQ-P14: 監査ログ（秘密漏洩防止）

| 項目 | 内容 |
|------|------|
| 入力 | `load` / `save` / `exists` の各メソッド呼出、および `PersistenceError` 発生イベント |
| 処理 | 本 crate は `audit.rs` モジュールを介してのみ `tracing` を呼ぶ。`audit::entry_load(paths)` / `entry_save(paths, record_count)` / `exit_ok_load(record_count, protection_mode, elapsed_ms)` / `exit_ok_save(record_count, elapsed_ms, bytes_written)` / `exit_err(err, elapsed_ms)` の 5 関数のみが公開。直接 `tracing::info!` 等を payload に対して呼ぶことは clippy の `disallowed-methods` lint で禁止。全関数のシグネチャは**秘密値を受けない型**のみ |
| 出力 | `tracing` subscriber（daemon 側の別 Issue で設定）へイベント発行 |
| エラー時 | 発生しない（`tracing` 自体は panic しない）。監査ログの欠落は上位エラーを上書きしない |

### REQ-P15: `SHIKOMI_VAULT_DIR` バリデーション

| 項目 | 内容 |
|------|------|
| 入力 | `PathBuf`（環境変数または `dirs::data_dir()` の戻り値） |
| 処理 | `VaultPaths::new(dir)` 内で 7 段階検証（`basic-design/security.md` §vault ディレクトリ検証）: ①絶対パス必須、②`..` 要素早期拒否、③シンボリックリンク全面禁止、④`canonicalize`、⑤保護領域 prefix 拒否、⑥ディレクトリ判定、⑦パス派生（`vault.db` / `vault.db.new` / `vault.db.lock`） |
| 出力 | `Ok(VaultPaths)` |
| エラー時 | `PersistenceError::InvalidVaultDir { path, reason: VaultDirReason }`。`VaultDirReason` の各バリアントで拒否理由を区別 |

## 画面・CLI仕様

該当なし — 理由: 本 Issue は `shikomi-infra` の内部ライブラリ実装。CLI/GUI は後続 Issue で本 crate を呼び出す側となる。本 Issue 自体は直接 CLI を提供しない。

## API仕様

本 crate の公開 API は Rust 型 API であり、HTTP エンドポイントではない。主要な公開型を以下に列挙する（詳細シグネチャは basic-design / detailed-design）。

| モジュール | 公開型 | 用途 |
|----------|-------|------|
| `shikomi_infra::persistence` | `VaultRepository` trait | 永続化抽象 |
| 〃 | `SqliteVaultRepository` | `VaultRepository` の SQLite 実装 |
| 〃 | `PersistenceError`, `CorruptedReason`, `AtomicWriteStage`, `VaultDirReason` | エラー型と付随 Reason 列挙 |
| 〃 | `VaultPaths` | vault ディレクトリ / ファイルパスを束ねる値オブジェクト（`vault.db` / `vault.db.new` / `vault.db.lock`） |

**公開 API のシグネチャ方針**:

- 全 I/O メソッドは同期 API（`async` ではない）。呼び出し側 daemon は `tokio::task::spawn_blocking` でラップする
- `SqliteVaultRepository::new()` は OS デフォルトディレクトリ（または検証済み `SHIKOMI_VAULT_DIR`）で構築する正規 API。`#[doc(hidden)] SqliteVaultRepository::with_dir(PathBuf)` は内部・テスト専用で公開 doc から隠蔽される（パス検証は呼出側責務）
- 秘密値は `shikomi_core::SecretString` / `SecretBytes` の型のまま API に出し入れする。生 `String` / `Vec<u8>` で平文レコード値を渡さない

## データモデル

論理データモデル（SQLite 物理スキーマは `basic-design/index.md` §ER 図 / `detailed-design/data.md` §SQLite スキーマ詳細を参照）:

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| VaultFile | path | `PathBuf` | OS 標準ディレクトリ配下の `vault.db` | 1 VaultFile に 1 VaultHeader + N Records |
| VaultFile | schema_version | `u32` | `PRAGMA user_version`、初期 `1` | — |
| VaultFile | application_id | `u32` | `PRAGMA application_id`、shikomi マジック値 `0x73_68_6B_6D`（"shkm"） | — |
| VaultHeaderRow | protection_mode | `TEXT` | `CHECK('plaintext','encrypted')` | 1 vault に 1 行（`id=1`） |
| VaultHeaderRow | vault_version | `INTEGER` | NOT NULL, `VaultVersion::MIN_SUPPORTED..=CURRENT` | — |
| VaultHeaderRow | created_at | `TEXT` | RFC3339 UTC | — |
| VaultHeaderRow | kdf_salt | `BLOB` | 平文モード時 NULL、暗号化モード時 16B NOT NULL（CHECK 制約で強制） | — |
| VaultHeaderRow | wrapped_vek_by_pw | `BLOB` | 平文モード時 NULL、暗号化モード時 NOT NULL | — |
| VaultHeaderRow | wrapped_vek_by_recovery | `BLOB` | 平文モード時 NULL、暗号化モード時 NOT NULL | — |
| RecordRow | id | `TEXT PRIMARY KEY` | UUIDv7 文字列、Vault 内一意 | — |
| RecordRow | kind | `TEXT` | `CHECK('text','secret')` | — |
| RecordRow | label | `TEXT` | 非空、255 grapheme 以下（ドメイン層で保証） | — |
| RecordRow | payload_variant | `TEXT` | `CHECK('plaintext','encrypted')` | — |
| RecordRow | plaintext_value | `TEXT` | 平文バリアント時 NOT NULL、暗号文バリアント時 NULL | SecretString の中身 |
| RecordRow | nonce | `BLOB` | 暗号文バリアント時 12B NOT NULL、平文バリアント時 NULL | — |
| RecordRow | ciphertext | `BLOB` | 暗号文バリアント時 NOT NULL、平文バリアント時 NULL | — |
| RecordRow | aad | `BLOB` | 暗号文バリアント時 26B NOT NULL（`Aad::to_canonical_bytes()` と完全一致）、平文バリアント時 NULL | — |
| RecordRow | created_at / updated_at | `TEXT` | RFC3339 UTC、サブ秒はマイクロ秒で丸める（`docs/features/vault/detailed-design.md` §バイナリ正規形仕様と整合） | — |

**整合性ルール**（SQLite の CHECK 制約で物理レベルに強制する）:

- `vault_header.protection_mode = 'plaintext'` のとき、`kdf_salt IS NULL AND wrapped_vek_by_pw IS NULL AND wrapped_vek_by_recovery IS NULL`
- `vault_header.protection_mode = 'encrypted'` のとき、`kdf_salt IS NOT NULL AND wrapped_vek_by_pw IS NOT NULL AND wrapped_vek_by_recovery IS NOT NULL`
- `records.payload_variant = 'plaintext'` のとき、`plaintext_value IS NOT NULL AND nonce IS NULL AND ciphertext IS NULL AND aad IS NULL`
- `records.payload_variant = 'encrypted'` のとき、`plaintext_value IS NULL AND nonce IS NOT NULL AND ciphertext IS NOT NULL AND aad IS NOT NULL`
- 全 `records.payload_variant` が `vault_header.protection_mode` と一致する（論理制約、ドメイン層 `Vault::add_record` で強制。SQLite では TRIGGER を使わず、load 時の `Vault::add_record` 呼出時に `VaultConsistencyError::ModeMismatch` として検出される）

## ユーザー向けメッセージ一覧

本 Issue は内部ライブラリのため**エンドユーザー向けメッセージは生成しない**。`PersistenceError` の `Display` は「開発者向けエラー文面」であり、CLI/GUI でユーザーに表示する際は各 crate で i18n ラベルに写像する（vault feature の MSG-DEV と同じ扱い）。

| ID | 種別 | メッセージ | 表示条件 |
|----|------|----------|---------|
| MSG-DEV-P01 | 開発者エラー | `i/o error at {path}: {source}` | `PersistenceError::Io` |
| MSG-DEV-P02 | 開発者エラー | `sqlite error: {source}` | `PersistenceError::Sqlite` |
| MSG-DEV-P03 | 開発者エラー | `vault file is corrupted: {reason}` | `PersistenceError::Corrupted` |
| MSG-DEV-P04 | 開発者エラー | `invalid file permission at {path}: expected={expected}, actual={actual}`（Unix では `expected="0600"` or `"0700"`、Windows では `expected="owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE[\|FILE_TRAVERSE])"`。`actual` は Unix で `"0644"` 等の mode 文字列、Windows で ACE 列挙の診断文字列、`detailed-design/flows.md` §`InvalidPermission` 参照） | `PersistenceError::InvalidPermission` |
| MSG-DEV-P05 | 開発者エラー | `orphan .new file detected at {path}; recovery required` | `PersistenceError::OrphanNewFile` |
| MSG-DEV-P06 | 開発者エラー | `atomic write failed at stage {stage}: {source}` | `PersistenceError::AtomicWriteFailed` |
| MSG-DEV-P07 | 開発者エラー | `schema mismatch: file_version={file_version}, supported={supported_range}` | `PersistenceError::SchemaMismatch` |
| MSG-DEV-P08 | 開発者エラー | `feature {feature} is not yet supported` ／ tracking issue が `Some(n)` なら末尾に ` (tracking issue #{n})` を付加、`None` なら `(tracking issue not yet assigned)` を付加 | `PersistenceError::UnsupportedYet` |
| MSG-DEV-P09 | 開発者エラー | `cannot resolve vault directory from environment` | `PersistenceError::CannotResolveVaultDir` |
| MSG-DEV-P10 | 開発者エラー | `invalid vault directory at {path}: {reason}`（`reason` は `VaultDirReason` の variant 名を snake_case で） | `PersistenceError::InvalidVaultDir` |
| MSG-DEV-P11 | 開発者エラー | `vault is locked by another process at {path}` ／ `holder_hint = Some(pid)` の時は末尾に ` (holder pid: {pid})` を付加 | `PersistenceError::Locked` |

**エンドユーザー向け文言**（例: 「vault ファイルが破損しています。バックアップから復旧してください」）は CLI/GUI Issue で `MSG-UI-xxx` として別途定義する。

## 依存関係

| crate | バージョン | feature | 用途 |
|-------|----------|--------|------|
| `shikomi-core` | ワークスペース内 | — | ドメイン型（`Vault`, `Record`, `SecretString` 等） |
| `rusqlite` | 0.32 以上 | `bundled` | SQLite I/O（外部依存ゼロ） |
| `dirs` | 5.x | — | OS 標準ディレクトリ解決 |
| `thiserror` | 2.x | — | `PersistenceError` 実装 |
| `uuid` | 1.x | `v7` | `RecordId` 文字列化（`shikomi-core` 経由で間接利用） |
| `time` | 0.3.x | `serde`, `macros`, `formatting`, `parsing` | RFC3339 入出力 |
| `tempfile` | 3.x（dev-deps） | — | 結合テストで tempdir |
| `cfg-if` | 1.x | — | OS 別コード分岐の可読性向上 |
| `windows` | 0.58 以上（Windows のみ、major ピン） | `Win32_Security_Authorization`（`SetNamedSecurityInfoW` / `GetNamedSecurityInfoW` / `EXPLICIT_ACCESS_W` / `SetEntriesInAclW` / `TRUSTEE_W`）／ `Win32_Security`（`EqualSid` / `GetAclInformation` / `GetAce` / `GetSecurityDescriptorDacl` / `OpenProcessToken` / `GetTokenInformation` / `ConvertSidToStringSidW`）／ `Win32_Foundation`（`HLOCAL` / `LocalFree` / `HANDLE` / `CloseHandle`）／ `Win32_Storage_FileSystem`（`FILE_GENERIC_READ` / `FILE_GENERIC_WRITE` / `FILE_TRAVERSE` / `ReplaceFileW`）／ `Win32_System_Threading`（`GetCurrentProcess`） | Windows ACL 設定・検証（REQ-P07）・`ReplaceFileW`（REQ-P04）。Microsoft 公式 crate のため `[advisories].ignore` 登録禁止リスト（`tech-stack.md` §4.3.2）の対象外だが、major ピンで API 破壊を受ける版上げはレビュー必須。導入は `[target.'cfg(windows)'.dependencies]` で `shikomi-infra` に限定し、Linux / macOS ビルドに混入させない |
| `fs4` | 0.12 以上 | `sync` | プロセス間 advisory lock（Unix `flock` / Windows `LockFileEx` 抽象）。`VaultLock` 型で利用。**`fs2` ではなく `fs4` を採用**した根拠: `fs2` は 2018-01 以降メンテ停止（OWASP A06 Vulnerable Components 該当）。`fs4` は同 API 面で fork された後継で、2024 年以降も継続的に release されている（`cargo deny advisories` で検証） |
| `tracing` | 0.1.x | — | 監査ログ（`save`/`load`/`exists`/error 各レベル）。`basic-design/security.md` §監査ログ規約参照 |
| `tracing-test` | 0.2.x（dev-deps） | — | 監査ログに秘密値が漏れていないかの検証（AC-15） |
| `serial_test` | 3.x（dev-deps） | — | `std::env::set_var` を触るテストの直列化（test-design §実行環境） |

全て `Cargo.toml` ルートの `[workspace.dependencies]` 経由で指定し、`crates/shikomi-infra/Cargo.toml` では `{ workspace = true }` で参照する（`docs/architecture/tech-stack.md` §4.1 / §4.4）。

`[target.'cfg(windows)'.dependencies]` セクションで `windows` crate を限定（Linux / macOS ビルドで不要コードを排除、バイナリサイズ要件に寄与）。
