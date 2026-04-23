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

---

## TC-U17: PermissionGuard::ensure_dir — owner-only DACL 作成（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U17 |
| 対応する受入基準ID | REQ-P07 受入観点① |
| 対応する工程 | 詳細設計（REQ-P07、PermissionGuardWindows::ensure_dir、classes.md §13） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir` で親ディレクトリを作成済み。`{tempdir}/vault_dir/` はまだ存在しない |
| 操作 | `PermissionGuard::ensure_dir(new_subdir_path)` を呼ぶ（ディレクトリ作成 + DACL 適用） |
| 期待結果 | `Ok(())` が返る。作成されたディレクトリの DACL を `GetNamedSecurityInfoW` で取得すると ①`SE_DACL_PROTECTED` bit が立っている ②`AceCount == 1` かつ `ACCESS_ALLOWED_ACE_TYPE` ③ ACE のトラスティ SID がディレクトリ所有者 SID と `EqualSid` で一致 ④ `AccessMask` が `EXPECTED_DIR_MASK`（`FILE_GENERIC_READ \| FILE_GENERIC_WRITE \| FILE_TRAVERSE`）と完全一致 |

---

## TC-U18: PermissionGuard::verify_dir — ensure_dir 適用後は 4 不変条件を満たす（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U18 |
| 対応する受入基準ID | REQ-P07 受入観点① |
| 対応する工程 | 詳細設計（REQ-P07、PermissionGuardWindows::verify_dir） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。TC-U17 と同一セットアップ（`ensure_dir` 適用済みのサブディレクトリ） |
| 操作 | `PermissionGuard::verify_dir(subdir_path)` を呼ぶ |
| 期待結果 | `Ok(())` |

---

## TC-U19: PermissionGuard::verify_dir — `SE_DACL_PROTECTED` なし（継承 ACE 残存）→ InvalidPermission（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U19 |
| 対応する受入基準ID | REQ-P07 受入観点③ |
| 対応する工程 | 詳細設計（REQ-P07、verify_dacl_owner_only 不変条件①） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir` が返すディレクトリそのもの（親 `%TEMP%` から ACE を継承しており `SE_DACL_PROTECTED` bit が立っていない状態）。`ensure_dir` は呼ばない |
| 操作 | `PermissionGuard::verify_dir(tempdir.path())` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE\|FILE_TRAVERSE)", actual, .. })` が返る。`actual` フィールドが `"inherited DACL (SE_DACL_PROTECTED not set)"` と等しい（不変条件①違反の確定ラベル、`flows.md §OS 別パーミッション実装詳細 §Windows` 参照） |

---

## TC-U20: PermissionGuard::ensure_file — owner-only DACL 作成（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U20 |
| 対応する受入基準ID | REQ-P07 受入観点① |
| 対応する工程 | 詳細設計（REQ-P07、PermissionGuardWindows::ensure_file） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`ensure_dir` 適用済みの vault ディレクトリ内に空ファイルを `std::fs::File::create` で作成済み |
| 操作 | `PermissionGuard::ensure_file(file_path)` を呼ぶ（DACL 書換のみ、所有者 touch なし） |
| 期待結果 | `Ok(())` が返る。ファイルの DACL を `GetNamedSecurityInfoW` で取得すると ①`SE_DACL_PROTECTED` bit が立っている ②`AceCount == 1` かつ `ACCESS_ALLOWED_ACE_TYPE` ③ ACE のトラスティ SID がファイル所有者 SID と `EqualSid` で一致 ④ `AccessMask` が `EXPECTED_FILE_MASK`（`FILE_GENERIC_READ \| FILE_GENERIC_WRITE`）と完全一致 |

---

## TC-U21: PermissionGuard::verify_file — ensure_file 適用後は 4 不変条件を満たす（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U21 |
| 対応する受入基準ID | REQ-P07 受入観点① |
| 対応する工程 | 詳細設計（REQ-P07、PermissionGuardWindows::verify_file） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。TC-U20 と同一セットアップ（`ensure_file` 適用済みのファイル） |
| 操作 | `PermissionGuard::verify_file(file_path)` を呼ぶ |
| 期待結果 | `Ok(())` |

---

## TC-U22: PermissionGuard::verify_file — ACE 追加後（AceCount = 2、SE_DACL_PROTECTED 維持）→ 不変条件② 単独違反（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U22 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 詳細設計（REQ-P07、verify_dacl_owner_only 不変条件②） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`ensure_file` 適用済みファイルに対し、`EXPLICIT_ACCESS_W` で `BUILTIN\Users` への `GENERIC_READ` Allow ACE を追加する。`SetNamedSecurityInfoW` 呼び出し時は `DACL_SECURITY_INFORMATION \| PROTECTED_DACL_SECURITY_INFORMATION` の両フラグを指定し、`SE_DACL_PROTECTED` を**維持したまま** ACE 数のみ 2 にする（不変条件①は意図的に満たしたまま②のみを壊す） |
| 操作 | `PermissionGuard::verify_file(file_path)` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { actual, .. })` が返る。`actual` フィールドに全 ACE の列挙文字列（`trustee_sid=<SID>, ace_type=..., access_mask=0x<hex>` の形式 2 行分）が含まれる（不変条件②単独違反——`SE_DACL_PROTECTED` は立っているため①はパス、`AceCount == 2` で②が失敗） |

---

## TC-U24: PermissionGuard::verify_file — AccessMask に余剰ビット（WRITE_DAC）→ 不変条件④ 単独違反（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U24 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 詳細設計（REQ-P07、verify_dacl_owner_only 不変条件④） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`ensure_file` 適用済みファイルに対し、`EXPLICIT_ACCESS_W` を 1 個（ACE 数 = 1 を維持）、トラスティはファイル所有者 SID（`fetch_owner_sid_from_path` で取得した SID と同一）、`grfAccessPermissions = FILE_GENERIC_READ \| FILE_GENERIC_WRITE \| WRITE_DAC`（`EXPECTED_FILE_MASK` に `WRITE_DAC` を追加した過剰マスク）に設定し `SetNamedSecurityInfoW(DACL_SECURITY_INFORMATION \| PROTECTED_DACL_SECURITY_INFORMATION, ...)` で DACL を書き換える（不変条件①②③は全て満たしたまま④のみを壊す） |
| 操作 | `PermissionGuard::verify_file(file_path)` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { actual, .. })` が返る。`actual` フィールドが `"ace_mask=0x<実際のマスク値>, expected=0x<EXPECTED_FILE_MASK 値>"` の形式と一致する（不変条件④単独違反——①`SE_DACL_PROTECTED` 立ち、②`AceCount=1`、③所有者 SID 一致、を全てパスした後に④ `AccessMask` 不一致で失敗。`WRITE_DAC` が 1 ビットでも余分にあれば攻撃者が DACL を後から書き換えられるため、ビット包含チェックではなく完全一致で防ぐ） |

---

## TC-U23: PermissionGuard::ensure_dir + verify_dir — UAC 昇格ランナーで所有者が BUILTIN\Administrators でも成立（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U23 |
| 対応する受入基準ID | REQ-P07 受入観点④ |
| 対応する工程 | 詳細設計（REQ-P07、fetch_owner_sid_from_path — ファイル側 OWNER SID 取得ポリシー） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`windows-latest` CI ランナーはデフォルトで管理者実行のため、作成ディレクトリの所有者が `BUILTIN\Administrators` SID になりうる。プロセストークン所有者（`GetCurrentProcess` + `TokenOwner`）と一致しない可能性がある |
| 操作 | 1. `ensure_dir(subdir_path)` を呼ぶ 2. `verify_dir(subdir_path)` を呼ぶ |
| 期待結果 | `ensure_dir` と `verify_dir` がともに `Ok(())` を返す。`ensure_dir` が**ファイル側の `OWNER_SECURITY_INFORMATION`**（`GetNamedSecurityInfoW` で取得した SID）を ACE トラスティとして使っているため、所有者が `BUILTIN\Administrators` であっても ACE 1 個 + `EqualSid` 一致の契約が成立する |

---

## TC-U25: PermissionGuard::verify_file — ACE トラスティが所有者 SID 以外（SE_DACL_PROTECTED 維持・ACE=1・AccessMask 維持）→ 不変条件③ 単独違反（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-U25 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 詳細設計（REQ-P07、verify_dacl_owner_only 不変条件③） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`ensure_file` 適用済みファイルに対し、`EXPLICIT_ACCESS_W` を 1 個（ACE 数 = 1 を維持）、`AccessMask = EXPECTED_FILE_MASK`（`FILE_GENERIC_READ \| FILE_GENERIC_WRITE`）を維持、`grfAccessMode = SET_ACCESS`、`AceFlags = 0`、`Trustee.TrusteeForm = TRUSTEE_IS_SID` で、**トラスティ SID だけを `BUILTIN\Users` の well-known SID（`S-1-5-32-545`）に書き換える**。`SetNamedSecurityInfoW(DACL_SECURITY_INFORMATION \| PROTECTED_DACL_SECURITY_INFORMATION, ...)` で DACL を適用し、`SE_DACL_PROTECTED` は維持したまま ACE 数・AccessMask は変更しない（不変条件①②④は意図的に満たしたまま③のみを壊す） |
| 操作 | `PermissionGuard::verify_file(file_path)` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { actual, .. })` が返る。`actual` フィールドが `trustee_mismatch` ラベル形式——所有者 SID（例: `S-1-5-21-...`）と ACE トラスティ SID（`S-1-5-32-545`）の両方を `ConvertSidToStringSidW` で文字列化した診断文字列を含む（`flows.md §OS 別パーミッション実装詳細 §Windows` の `EqualSid` 失敗時フォーマット参照）。不変条件①`SE_DACL_PROTECTED` 立ち・②`AceCount=1`・④`AccessMask` 一致 を全てパスした後に③`EqualSid` 失敗で Fail Fast する順序を確認 |

---

*対応 Issue: #14 / 親ドキュメント: `test-design/index.md`*
