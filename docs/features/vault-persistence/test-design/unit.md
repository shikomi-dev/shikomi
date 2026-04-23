# ユニットテスト設計 — vault-persistence

> このファイルは `test-design/index.md` の §6 に相当する。テストマトリクス・モック方針・実行手順は `index.md` を参照。

> **Rust のテスト配置**: ユニットテストは `#[cfg(test)]` モジュールをソースファイル末尾に配置する（Rust 言語慣習）。OS 依存ケースは `#[cfg(unix)]` / `#[cfg(windows)]` でガード。
> **モックなし**: `Mapping` は純関数。`PermissionGuard::verify_*` はファイルシステムに対して `stat` するため、tempdir 内の実ファイルを使用する（mock 不要）。assumed mock は禁止。

---

## TC-U01: VaultPaths::new — 正常パスのパス導出

| 項目 | 内容 |
|------|------|
| テストID | TC-U01 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P08, P15、VaultPaths、7段階バリデーション） |
| 種別 | 正常系 |
| 前提条件 | tempdir が実際に存在する（`tempfile::TempDir` で作成）。絶対パスで `..` を含まず、シンボリックリンクでなく、保護領域でもないこと |
| 操作 | `VaultPaths::new(tempdir.path().to_path_buf())` を呼び（`Result<VaultPaths, PersistenceError>` を返す）、`dir()` / `vault_db()` / `vault_db_new()` / `vault_db_lock()` を確認 |
| 期待結果 | `Ok(paths)` が返る。`paths.dir()` が tempdir の絶対パス / `paths.vault_db()` が `<tempdir>/vault.db` / `paths.vault_db_new()` が `<tempdir>/vault.db.new` / `paths.vault_db_lock()` が `<tempdir>/vault.db.lock` |

---

## TC-U02: Mapping::vault_header_to_params — 平文モード

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

## TC-U03: Mapping::row_to_vault_header — 正常系

| 項目 | 内容 |
|------|------|
| テストID | TC-U03 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P09、Mapping::row_to_vault_header） |
| 種別 | 正常系 |
| 前提条件 | in-memory SQLite（`Connection::open_in_memory()`）に正常な `vault_header` 行（plaintext mode）を INSERT 済み |
| 操作 | `SELECT_VAULT_HEADER` → `Mapping::row_to_vault_header(&row)` |
| 期待結果 | `Ok(VaultHeader)` が返り、`protection_mode == ProtectionMode::Plaintext` |

---

## TC-U04: Mapping::row_to_vault_header — UnknownProtectionMode

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

## TC-U05: Mapping::row_to_vault_header — InvalidRfc3339

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

## TC-U06: Mapping::row_to_record — 正常系（plaintext variant）

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

## TC-U07: Mapping::row_to_record — InvalidUuidString

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

## TC-U08: Mapping::row_to_record — NullViolation

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

## TC-U09: PermissionGuard::verify_dir — 0700 → Ok（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U09 |
| 対応する受入基準ID | — |
| 対応する工程 | 詳細設計（REQ-P06、PermissionGuard::verify_dir） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(unix)]`。tempdir を `std::os::unix::fs::PermissionsExt::set_mode` で `0o700` に設定済み |
| 操作 | `PermissionGuard::verify_dir(tempdir_path)` |
| 期待結果 | `Ok(())` |

---

## TC-U10: PermissionGuard::verify_dir — 0755 → InvalidPermission（Unix）

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

## TC-U11: PermissionGuard::verify_file — 0600 → Ok（Unix）

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

## TC-U12: PermissionGuard::verify_file — 0644 → InvalidPermission（Unix）

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

## TC-U13: VaultPaths::new — 相対パス → NotAbsolute

| 項目 | 内容 |
|------|------|
| テストID | TC-U13 |
| 対応する受入基準ID | AC-16 |
| 対応する工程 | 詳細設計（REQ-P15、VaultDirReason::NotAbsolute、バリデーション step ①） |
| 種別 | 異常系 |
| 前提条件 | — |
| 操作 | `VaultPaths::new(PathBuf::from("relative/path"))` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidVaultDir { path: PathBuf::from("relative/path"), reason: VaultDirReason::NotAbsolute })` |

---

## TC-U14: VaultPaths::new — `..` 含むパス → PathTraversal

| 項目 | 内容 |
|------|------|
| テストID | TC-U14 |
| 対応する受入基準ID | AC-16 |
| 対応する工程 | 詳細設計（REQ-P15、VaultDirReason::PathTraversal、バリデーション step ②） |
| 種別 | 異常系 |
| 前提条件 | — |
| 操作 | `VaultPaths::new(PathBuf::from("/tmp/shikomi/../../etc/passwd"))` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::PathTraversal, .. })` が返る。`canonicalize` を呼ぶ前の早期拒否であること（`/etc/passwd` が実際に存在していても拒否される） |

---

## TC-U15: VaultPaths::new — シンボリックリンク → SymlinkNotAllowed（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U15 |
| 対応する受入基準ID | AC-16 |
| 対応する工程 | 詳細設計（REQ-P15、VaultDirReason::SymlinkNotAllowed、バリデーション step ③） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(unix)]`。`std::os::unix::fs::symlink(real_dir, symlink_path)` でシンボリックリンクを作成済み。実際のディレクトリ（`real_dir`）が存在する |
| 操作 | `VaultPaths::new(symlink_path)` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::SymlinkNotAllowed, .. })` |

---

## TC-U16: VaultPaths::new — 保護領域 → ProtectedSystemArea（Unix）

| 項目 | 内容 |
|------|------|
| テストID | TC-U16 |
| 対応する受入基準ID | AC-16 |
| 対応する工程 | 詳細設計（REQ-P15、VaultDirReason::ProtectedSystemArea、バリデーション step ⑤） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(unix)]`。`/etc/shikomi_test_dir` は実際に作成しない（`canonicalize` 失敗の前にバリデーションで拒否されるため） |
| 操作 | `VaultPaths::new(PathBuf::from("/etc/shikomi_test_dir"))` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidVaultDir { reason: VaultDirReason::ProtectedSystemArea { prefix: "/etc" }, .. })` |

---

*対応 Issue: #10 / 親ドキュメント: `test-design/index.md`*
