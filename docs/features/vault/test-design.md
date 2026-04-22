# テスト設計書 — vault（shikomi-core ドメイン型定義）

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | vault（`shikomi-core` crate vault ドメイン型定義） |
| 対象 Issue | [#7](https://github.com/shikomi-dev/shikomi/issues/7) |
| 対象ブランチ | `feature/issue-7-vault-domain` |
| 設計根拠 | `docs/features/vault/requirements-analysis.md`（REQ-001〜REQ-010）、`basic-design.md`、`detailed-design.md` |
| テスト実行タイミング | `feature/issue-7-vault-domain` → `develop` へのマージ前 |

## 2. テスト対象と受入基準

| 受入基準ID | 受入基準 | 検証レベル |
|-----------|---------|-----------|
| AC-01 | 全機能 REQ-001〜REQ-010 の型が `shikomi-core` の公開 API に存在する | 結合 |
| AC-02 | 排他条件を型で保証（`Plaintext` ヘッダに wrapped_VEK フィールドが存在しない等） | ユニット |
| AC-03 | `RecordLabel` が空文字列・制御文字（ただし `\t`/`\n`/`\r` 除く）・255 grapheme 超過を構築時に拒否する | ユニット |
| AC-04 | `SecretString` / `SecretBytes` が `Debug` / `format!` で中身を露出しない | ユニット・結合 |
| AC-05 | `NonceCounter::next()` が $2^{32}$ 到達で `DomainError::NonceOverflow` を返す | ユニット |
| AC-06 | `cargo test -p shikomi-core` が pass する | 結合 |
| AC-07 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` が pass する | 結合 |
| AC-08 | 公開 API 上に生 `String` / `Vec<u8>` がシークレット経路に露出しない（`SecretString` / `SecretBytes` / `CipherText` 等 newtype 経由） | ユニット |
| AC-09 | `deny.toml` の `[advisories]` コメントに `secrecy` / `zeroize` が暗号クリティカル crate として明記され、かつ `ignore = []` リストに含まれていない | 静的確認 |

## 3. テストマトリクス（トレーサビリティ）

### ユニットテスト

| テストID | 機能ID | 受入基準ID | 検証内容 | 種別 |
|---------|--------|-----------|---------|------|
| TC-U01-01 | REQ-001 | AC-01 | `ProtectionMode::as_persisted_str` が `Plaintext` に `"plaintext"` を返す | 正常系 |
| TC-U01-02 | REQ-001 | AC-01 | `ProtectionMode::as_persisted_str` が `Encrypted` に `"encrypted"` を返す | 正常系 |
| TC-U01-03 | REQ-001 | AC-01 | `ProtectionMode::try_from_persisted_str("plaintext")` が `Plaintext` を返す | 正常系 |
| TC-U01-04 | REQ-001 | AC-01 | `ProtectionMode::try_from_persisted_str("encrypted")` が `Encrypted` を返す | 正常系 |
| TC-U01-05 | REQ-001 | AC-01 | `ProtectionMode::try_from_persisted_str` が未知文字列で `InvalidProtectionMode` を返す | 異常系 |
| TC-U02-01 | REQ-002 | AC-01 | `VaultVersion::try_new(1)` が成功する（CURRENT = 1） | 正常系 |
| TC-U02-02 | REQ-002 | AC-01 | `VaultVersion::try_new(0)` が `UnsupportedVaultVersion(0)` を返す（MIN_SUPPORTED - 1） | 境界値 |
| TC-U02-03 | REQ-002 | AC-01 | `VaultVersion::try_new(2)` が `UnsupportedVaultVersion(2)` を返す（CURRENT + 1） | 境界値 |
| TC-U02-04 | REQ-002 | AC-01 | `VaultVersion::value()` が設定した `u16` 値を返す | 正常系 |
| TC-U03-01 | REQ-003 | AC-01, AC-02 | `VaultHeader::new_plaintext` が有効なバージョンで `VaultHeader::Plaintext` を返す | 正常系 |
| TC-U03-02 | REQ-003 | AC-01 | `VaultHeader::new_plaintext` が非対応バージョンで `UnsupportedVaultVersion` を返す | 異常系 |
| TC-U03-03 | REQ-003 | AC-01 | `VaultHeader::new_encrypted` が有効な引数で `VaultHeader::Encrypted` を返す | 正常系 |
| TC-U03-04 | REQ-003 | AC-01 | `VaultHeader::new_encrypted` が kdf_salt 長 ≠ 16 で `InvalidVaultHeader(KdfSaltLength)` を返す | 異常系 |
| TC-U03-05 | REQ-003 | AC-01 | `VaultHeader::new_encrypted` が `wrapped_vek_by_pw` 空で `InvalidVaultHeader(WrappedVekEmpty)` を返す | 異常系 |
| TC-U03-06 | REQ-003 | AC-01 | `VaultHeader::new_encrypted` が `wrapped_vek_by_recovery` 空で `InvalidVaultHeader(WrappedVekEmpty)` を返す | 異常系 |
| TC-U03-07 | REQ-003 | AC-02 | `VaultHeader::Plaintext.protection_mode()` が `ProtectionMode::Plaintext` を返す | 正常系 |
| TC-U03-08 | REQ-003 | AC-02 | `VaultHeader::Encrypted.protection_mode()` が `ProtectionMode::Encrypted` を返す | 正常系 |
| TC-U04-01 | REQ-004 | AC-01 | `RecordId::new` が有効な UUIDv7 で成功する | 正常系 |
| TC-U04-02 | REQ-004 | AC-01 | `RecordId::new` が UUIDv4 で `InvalidRecordId(WrongVersion)` を返す | 異常系 |
| TC-U04-03 | REQ-004 | AC-01 | `RecordId::new` が nil UUID で `InvalidRecordId(NilUuid)` を返す | 境界値 |
| TC-U04-04 | REQ-004 | AC-01 | `RecordId::try_from_str` が有効な UUIDv7 文字列で成功する | 正常系 |
| TC-U04-05 | REQ-004 | AC-01 | `RecordId::try_from_str` が不正な文字列で `InvalidRecordId(ParseError)` を返す | 異常系 |
| TC-U04-06 | REQ-004 | AC-01 | `RecordId::as_uuid()` が格納された Uuid を返す | 正常系 |
| TC-U05-01 | REQ-005 | AC-03 | `RecordLabel::try_new` が 1 文字ラベルで成功する（境界値: 最小） | 境界値 |
| TC-U05-02 | REQ-005 | AC-03 | `RecordLabel::try_new` が 255 grapheme のラベルで成功する（境界値: 最大） | 境界値 |
| TC-U05-03 | REQ-005 | AC-03 | `RecordLabel::try_new` が 256 grapheme のラベルで `InvalidRecordLabel(TooLong)` を返す（境界値: 最大 + 1） | 境界値 |
| TC-U05-04 | REQ-005 | AC-03 | `RecordLabel::try_new` が空文字列で `InvalidRecordLabel(Empty)` を返す | 境界値 |
| TC-U05-05 | REQ-005 | AC-03 | `RecordLabel::try_new` が U+0000（NUL）を含む文字列で `InvalidRecordLabel(ControlChar)` を返す | 異常系 |
| TC-U05-06 | REQ-005 | AC-03 | `RecordLabel::try_new` が U+001F を含む文字列で `InvalidRecordLabel(ControlChar)` を返す（制御文字範囲境界） | 境界値 |
| TC-U05-07 | REQ-005 | AC-03 | `RecordLabel::try_new` が U+007F（DEL）を含む文字列で `InvalidRecordLabel(ControlChar)` を返す | 異常系 |
| TC-U05-08 | REQ-005 | AC-03 | `RecordLabel::try_new` が `\t`（U+0009）を含む文字列で成功する（許可制御文字） | 正常系 |
| TC-U05-09 | REQ-005 | AC-03 | `RecordLabel::try_new` が `\n`（U+000A）を含む文字列で成功する（許可制御文字） | 正常系 |
| TC-U05-10 | REQ-005 | AC-03 | `RecordLabel::try_new` が `\r`（U+000D）を含む文字列で成功する（許可制御文字） | 正常系 |
| TC-U05-11 | REQ-005 | AC-01 | `RecordLabel::as_str()` が格納された文字列を返す | 正常系 |
| TC-U06-01 | REQ-006 | AC-01, AC-02 | `RecordPayload::Plaintext` バリアントが `SecretString` を保持する | 正常系 |
| TC-U06-02 | REQ-006 | AC-01 | `RecordPayloadEncrypted::new` が有効な nonce(12B)/ciphertext/aad で成功する | 正常系 |
| TC-U06-03 | REQ-006 | AC-01 | `RecordPayloadEncrypted::new` が nonce 長 ≠ 12 で `InvalidRecordPayload(NonceLength)` を返す | 異常系 |
| TC-U06-04 | REQ-006 | AC-01 | `RecordPayloadEncrypted::new` が空 ciphertext で `InvalidRecordPayload(CipherTextEmpty)` を返す | 境界値 |
| TC-U06-05 | REQ-006 | AC-02 | `RecordPayload::Plaintext.variant_mode()` が `ProtectionMode::Plaintext` を返す | 正常系 |
| TC-U06-06 | REQ-006 | AC-02 | `RecordPayload::Encrypted.variant_mode()` が `ProtectionMode::Encrypted` を返す | 正常系 |
| TC-U07-01 | REQ-007 | AC-01 | `Vault::new(Plaintext header)` で records が空の Vault が構築される（失敗しない） | 正常系 |
| TC-U07-02 | REQ-007 | AC-01 | `Vault::new(Encrypted header)` で records が空の Vault が構築される（失敗しない） | 正常系 |
| TC-U07-03 | REQ-007 | AC-01 | `Vault::add_record` がモード整合レコードを追加できる（Plaintext vault + Plaintext payload） | 正常系 |
| TC-U07-04 | REQ-007 | AC-02 | `Vault::add_record` が Plaintext vault に Encrypted payload を渡すと `VaultConsistencyError(ModeMismatch)` を返す | 異常系 |
| TC-U07-05 | REQ-007 | AC-02 | `Vault::add_record` が Encrypted vault に Plaintext payload を渡すと `VaultConsistencyError(ModeMismatch)` を返す | 異常系 |
| TC-U07-06 | REQ-007 | AC-01 | `Vault::add_record` が重複 RecordId で `VaultConsistencyError(DuplicateId)` を返す | 異常系 |
| TC-U07-07 | REQ-007 | AC-01 | `Vault::remove_record` が存在するレコードを削除して返す | 正常系 |
| TC-U07-08 | REQ-007 | AC-01 | `Vault::remove_record` が存在しない id で `VaultConsistencyError(RecordNotFound)` を返す | 異常系 |
| TC-U07-09 | REQ-007 | AC-01 | `Vault::find_record` が存在するレコードを `Some` で返す | 正常系 |
| TC-U07-10 | REQ-007 | AC-01 | `Vault::find_record` が存在しない id で `None` を返す | 正常系 |
| TC-U07-11 | REQ-007 | AC-01 | `Vault::protection_mode()` がヘッダのモードを返す | 正常系 |
| TC-U07-12 | REQ-007 | AC-01 | `Vault::records()` が全レコードのスライスを返す | 正常系 |
| TC-U07-13 | REQ-007 | AC-01 | `Vault::update_record` が updater クロージャ経由でレコードを更新できる | 正常系 |
| TC-U07-14 | REQ-007 | AC-01 | `Vault::rekey_with` が Plaintext vault で `VaultConsistencyError(RekeyInPlaintextMode)` を返す | 異常系 |
| TC-U07-15 | REQ-007 | AC-01 | `Vault::rekey_with` が Encrypted vault + 成功ダミー `VekProvider` で `Ok(())` を返す | 正常系 |
| TC-U07-16 | REQ-007 | AC-01 | `Vault::rekey_with` が Encrypted vault + 失敗ダミー `VekProvider`（`reencrypt_all` が `Err`）で `VaultConsistencyError(RekeyPartialFailure)` を返す | 異常系 |
| TC-U09-01 | REQ-009 | AC-01 | `DomainError::InvalidProtectionMode("x")` の `Display` が `"unknown protection mode: x"` を含む（MSG-DEV-001） | 正常系 |
| TC-U09-02 | REQ-009 | AC-01 | `DomainError::UnsupportedVaultVersion(99)` の `Display` が `"unsupported vault version: 99"` を含む（MSG-DEV-002） | 正常系 |
| TC-U09-03 | REQ-009 | AC-01 | `DomainError::InvalidRecordLabel(Empty)` の `Display` が `"invalid record label"` を含む（MSG-DEV-003） | 正常系 |
| TC-U09-04 | REQ-009 | AC-01 | `DomainError::VaultConsistencyError(ModeMismatch { .. })` の `Display` が `"vault and record payload mode mismatch"` を含む（MSG-DEV-004） | 正常系 |
| TC-U09-05 | REQ-009 | AC-01 | `DomainError::NonceOverflow` の `Display` が `"nonce counter exhausted"` を含む（MSG-DEV-005） | 正常系 |
| TC-U09-06 | REQ-009 | AC-01 | `DomainError::InvalidRecordId(NilUuid)` の `Display` が `"invalid record id"` を含む（MSG-DEV-006） | 正常系 |
| TC-U09-07 | REQ-009 | AC-01 | `DomainError::InvalidRecordPayload(CipherTextEmpty)` の `Display` が `"invalid record payload"` を含む（MSG-DEV-007） | 正常系 |
| TC-U09-08 | REQ-009 | AC-01 | `DomainError::InvalidVaultHeader(KdfSaltLength { expected:16, got:15 })` の `Display` が `"invalid vault header"` を含む（MSG-DEV-008） | 正常系 |
| TC-U11-01 | REQ-006 | AC-01 | `Aad::new(record_id, vault_version, valid_created_at)` が `Ok(Aad)` を返す | 正常系 |
| TC-U11-02 | REQ-006 | AC-01 | `Aad::new(record_id, vault_version, out_of_range_created_at)` が `InvalidRecordPayload(AadTimestampOutOfRange)` を返す（Fail Fast） | 異常系 |
| TC-U11-03 | REQ-006 | AC-01 | `Aad::to_canonical_bytes()` が 26 byte 固定長の配列を返す | 正常系 |
| TC-U08-01 | REQ-008 | AC-01 | `SecretString::from_string` でインスタンスを構築できる（失敗しない） | 正常系 |
| TC-U08-02 | REQ-008 | AC-04 | `format!("{:?}", secret_string)` の出力が `"[REDACTED]"` である | 正常系 |
| TC-U08-03 | REQ-008 | AC-04 | `SecretString::expose_secret()` が元の文字列を返す | 正常系 |
| TC-U08-04 | REQ-008 | AC-01 | `SecretBytes::from_boxed_slice` でインスタンスを構築できる（失敗しない） | 正常系 |
| TC-U08-05 | REQ-008 | AC-04 | `format!("{:?}", secret_bytes)` の出力が `"[REDACTED]"` である | 正常系 |
| TC-U08-06 | REQ-008 | AC-04 | `SecretBytes::expose_secret()` が元のバイト列を返す | 正常系 |
| TC-U10-01 | REQ-010 | AC-01 | `NonceCounter::new` で counter が 0 から開始する | 正常系 |
| TC-U10-02 | REQ-010 | AC-01 | `NonceCounter::next()` が 12 バイトの `NonceBytes` を返す | 正常系 |
| TC-U10-03 | REQ-010 | AC-01 | `NonceCounter::current_counter()` が `next()` 呼び出し後にインクリメントされる | 正常系 |
| TC-U10-04 | REQ-010 | AC-01 | `NonceCounter::resume(random_prefix, n)` で counter が `n` から再開する | 正常系 |
| TC-U10-05 | REQ-010 | AC-05 | `NonceCounter` の counter が `u32::MAX - 1` の状態で `next()` が成功する（境界値: 上限直前） | 境界値 |
| TC-U10-06 | REQ-010 | AC-05 | `NonceCounter` の counter が `u32::MAX` の状態で `next()` が `DomainError::NonceOverflow` を返す（境界値: 上限） | 境界値 |
| TC-U10-07 | REQ-010 | AC-01 | `NonceCounter::next()` で生成された `NonceBytes` は上位 8 バイトが `random_prefix` と一致する | 正常系 |

### 結合テスト（tests/ 配下の integration test）

| テストID | 機能ID | 受入基準ID | 検証内容 | 種別 |
|---------|--------|-----------|---------|------|
| TC-I01 | REQ-007 | AC-01, AC-06 | Plaintext vault ライフサイクル: `new` → `add_record` × 2 → `find_record` → `remove_record` → `records()` で件数確認 | 正常系 |
| TC-I02 | REQ-007 | AC-02, AC-06 | Vault とレコードのモード不整合を外部 API から検出: Plaintext vault に Encrypted payload レコードを投入 → `ModeMismatch` | 異常系 |
| TC-I03 | REQ-005 | AC-03, AC-06 | `RecordLabel` の境界値を外部 API から検証: 空 / 1文字 / 255 grapheme / 256 grapheme / 制御文字 / 許可制御文字 | 境界値 |
| TC-I04 | REQ-008 | AC-04, AC-06 | `SecretString` / `SecretBytes` の Debug フォーマットでリーク不存在を確認（`format!("{:?}")` に秘密値が含まれないこと） | 正常系 |
| TC-I05 | REQ-010 | AC-05, AC-06 | `NonceCounter` のオーバーフロー検知を外部 API から確認: `resume(prefix, u32::MAX)` 後に `next()` → `NonceOverflow` | 境界値 |
| TC-I06 | REQ-001〜REQ-010 | AC-07 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` が CI 環境で pass する | 正常系 |
| TC-I07 | REQ-009 | AC-09 | `deny.toml` に `secrecy` / `zeroize` が `ignore` 禁止コメントに明記され、`ignore = []` リストに含まれていないことを grep で確認 | 静的確認 |

## 4. E2Eテスト設計

**省略理由**: `shikomi-core` はエンドユーザーが直接操作する UI / CLI / 公開 HTTP API を持たない純粋ドメイン型ライブラリ。テスト戦略ガイドの「エンドユーザー操作がない場合は結合テストで代替」方針に従い、E2E は設計対象外とする。受入基準の検証は §5 の結合テスト（integration test）と §6 のユニットテストで網羅する。

## 5. 結合テスト設計（tests/ 配下の integration test）

> **ツール選択根拠**: 対象は Rust library crate の公開 API。テスト戦略ガイドの Rust 慣習に従い、integration test は `tests/` 配下（`#[cfg(test)]` 外）に配置し、`shikomi-core` の公開 API のみ通じてテストする。モックは不要（no-I/O pure Rust のため）。

### TC-I01: Plaintext vault ライフサイクル

| 項目 | 内容 |
|------|------|
| テストID | TC-I01 |
| 対応する機能ID | REQ-007 |
| 対応する受入基準ID | AC-01, AC-06 |
| 対応する工程 | 基本設計（Vault 集約ルート処理フロー） |
| 種別 | 正常系 |
| 前提条件 | `Vault::new` / `add_record` / `find_record` / `remove_record` / `records` が実装済み |
| 操作 | (1) Plaintext header で Vault 生成 → (2) Plaintext payload のレコード 2 件を add → (3) 1件を find して内容確認 → (4) 1件を remove → (5) `records().len() == 1` を確認 |
| 期待結果 | 全ステップで `Ok` / 期待値一致。最終レコード件数 1 件 |

### TC-I02: Vault モード不整合検知（外部 API 視点）

| 項目 | 内容 |
|------|------|
| テストID | TC-I02 |
| 対応する機能ID | REQ-007 |
| 対応する受入基準ID | AC-02, AC-06 |
| 対応する工程 | 基本設計（Vault::add_record 処理フロー） |
| 種別 | 異常系 |
| 前提条件 | `Vault::new` / `add_record` が実装済み |
| 操作 | (1) Plaintext header で Vault 生成 → (2) Encrypted payload のレコードを `add_record` に渡す |
| 期待結果 | `Err(DomainError::VaultConsistencyError(ModeMismatch { .. }))` が返る |

### TC-I03: RecordLabel 境界値（外部 API 視点）

| 項目 | 内容 |
|------|------|
| テストID | TC-I03 |
| 対応する機能ID | REQ-005 |
| 対応する受入基準ID | AC-03, AC-06 |
| 対応する工程 | 詳細設計（RecordLabel::try_new 検証ロジック） |
| 種別 | 境界値 |
| 前提条件 | `RecordLabel::try_new` が実装済み |
| 操作 | 以下を順に試す: (a) `""` → (b) `"A"` → (c) 255 grapheme 文字列 → (d) 256 grapheme 文字列 → (e) `"\x00"` → (f) `"\x1F"` → (g) `"\x7F"` → (h) `"\t"` → (i) `"\n"` |
| 期待結果 | (a) `Err(Empty)` / (b) `Ok` / (c) `Ok` / (d) `Err(TooLong { grapheme_count: 256 })` / (e) `Err(ControlChar { .. })` / (f) `Err(ControlChar { .. })` / (g) `Err(ControlChar { .. })` / (h) `Ok` / (i) `Ok` |

### TC-I04: SecretString / SecretBytes の非リーク確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I04 |
| 対応する機能ID | REQ-008 |
| 対応する受入基準ID | AC-04, AC-06 |
| 対応する工程 | 詳細設計（SecretString / SecretBytes の Debug 実装） |
| 種別 | 正常系 |
| 前提条件 | `SecretString::from_string` / `SecretBytes::from_boxed_slice` が実装済み |
| 操作 | (1) `SecretString::from_string("my-password".to_string())` を生成し `format!("{:?}", s)` を取得 → (2) `SecretBytes::from_boxed_slice(b"secret".to_vec().into_boxed_slice())` を生成し `format!("{:?}", b)` を取得 |
| 期待結果 | (1) 結果文字列に `"my-password"` が含まれず、`"[REDACTED]"` を含む / (2) 結果文字列に生バイトが含まれず `"[REDACTED]"` を含む |

### TC-I05: NonceCounter オーバーフロー（外部 API 視点）

| 項目 | 内容 |
|------|------|
| テストID | TC-I05 |
| 対応する機能ID | REQ-010 |
| 対応する受入基準ID | AC-05, AC-06 |
| 対応する工程 | 詳細設計（NonceCounter::next オーバーフロー処理） |
| 種別 | 境界値 |
| 前提条件 | `NonceCounter::resume` / `NonceCounter::next` が実装済み |
| 操作 | (1) `NonceCounter::resume([0u8; 8], u32::MAX)` で counter を上限に設定 → (2) `next()` を呼ぶ |
| 期待結果 | `Err(DomainError::NonceOverflow)` が返る |

### TC-I06: CI cargo コマンド通過確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I06 |
| 対応する機能ID | REQ-001〜REQ-010 |
| 対応する受入基準ID | AC-07 |
| 対応する工程 | 基本設計（CI/CD） |
| 種別 | 正常系 |
| 前提条件 | `shikomi-core` のソースコードが実装済みかつコミット済み |
| 操作 | (1) `cargo build --workspace` → (2) `cargo test --workspace` → (3) `cargo fmt --check --all` → (4) `cargo clippy --workspace -- -D warnings` → (5) `cargo deny check` |
| 期待結果 | 全コマンド exit code == 0 |

### TC-I07: deny.toml 暗号クリティカル crate 登録確認

| 項目 | 内容 |
|------|------|
| テストID | TC-I07 |
| 対応する機能ID | REQ-009 |
| 対応する受入基準ID | AC-09 |
| 対応する工程 | 要件定義（requirements-analysis.md §受入基準 #9） |
| 種別 | 静的確認 |
| 前提条件 | `deny.toml` がリポジトリルートに存在する |
| 操作 | (1) `grep -n "secrecy\|zeroize" deny.toml` でコメント行への記載を確認 → (2) `deny.toml` の `ignore = [` 以下に `secrecy` / `zeroize` の advisory ID が**含まれない**ことを確認 |
| 期待結果 | (1) `secrecy` / `zeroize` がコメント（`#` 行）に出現する / (2) `ignore = []` が空のまま、または `secrecy` / `zeroize` 関連 advisory ID を含まない |

## 6. ユニットテスト設計（詳細）

> **ツール**: Rust 慣習に従い、各ソースモジュール内に `#[cfg(test)] mod tests { ... }` として配置する。`shikomi-core` は no-I/O pure Rust のためモック不要。

### TC-U01〜TC-U02: ProtectionMode / VaultVersion

**配置先**: `crates/shikomi-core/src/vault/protection_mode.rs` / `version.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U01-01 | — | `ProtectionMode::Plaintext.as_persisted_str()` | `"plaintext"` |
| TC-U01-02 | — | `ProtectionMode::Encrypted.as_persisted_str()` | `"encrypted"` |
| TC-U01-03 | — | `ProtectionMode::try_from_persisted_str("plaintext")` | `Ok(ProtectionMode::Plaintext)` |
| TC-U01-04 | — | `ProtectionMode::try_from_persisted_str("encrypted")` | `Ok(ProtectionMode::Encrypted)` |
| TC-U01-05 | — | `ProtectionMode::try_from_persisted_str("PLAINTEXT")` | `Err(DomainError::InvalidProtectionMode(_))` |
| TC-U02-01 | — | `VaultVersion::try_new(1)` | `Ok`（CURRENT = 1） |
| TC-U02-02 | — | `VaultVersion::try_new(0)` | `Err(DomainError::UnsupportedVaultVersion(0))` |
| TC-U02-03 | — | `VaultVersion::try_new(2)` | `Err(DomainError::UnsupportedVaultVersion(2))` |
| TC-U02-04 | `try_new(1)` | `.value()` | `1u16` |

### TC-U03: VaultHeader

**配置先**: `crates/shikomi-core/src/vault/header.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U03-01 | — | `VaultHeader::new_plaintext(VaultVersion::CURRENT, now)` | `Ok(VaultHeader::Plaintext(_))` |
| TC-U03-02 | — | `VaultHeader::new_plaintext(VaultVersion(2), now)` | `Err(UnsupportedVaultVersion)` |
| TC-U03-03 | — | `VaultHeader::new_encrypted(current, now, [0u8;16], valid_wrapped, valid_wrapped)` | `Ok(VaultHeader::Encrypted(_))` |
| TC-U03-04 | — | `new_encrypted` に `kdf_salt = [0u8; 15]` を渡す | `Err(InvalidVaultHeader(KdfSaltLength { expected:16, got:15 }))` |
| TC-U03-05 | — | `new_encrypted` に `wrapped_vek_by_pw = []` を渡す | `Err(InvalidVaultHeader(WrappedVekEmpty))` |
| TC-U03-06 | — | `new_encrypted` に `wrapped_vek_by_recovery = []` を渡す | `Err(InvalidVaultHeader(WrappedVekEmpty))` |
| TC-U03-07 | `new_plaintext` 成功後 | `.protection_mode()` | `ProtectionMode::Plaintext` |
| TC-U03-08 | `new_encrypted` 成功後 | `.protection_mode()` | `ProtectionMode::Encrypted` |

### TC-U04: RecordId

**配置先**: `crates/shikomi-core/src/vault/id.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U04-01 | — | `RecordId::new(uuid_v7)` | `Ok(RecordId(_))` |
| TC-U04-02 | — | `RecordId::new(uuid_v4)` | `Err(InvalidRecordId(WrongVersion { actual: 4 }))` |
| TC-U04-03 | — | `RecordId::new(Uuid::nil())` | `Err(InvalidRecordId(NilUuid))` |
| TC-U04-04 | — | `RecordId::try_from_str("01234567-0123-7000-8000-0123456789ab")` | `Ok` |
| TC-U04-05 | — | `RecordId::try_from_str("not-a-uuid")` | `Err(InvalidRecordId(ParseError(_)))` |
| TC-U04-06 | `new(uuid_v7)` 成功後 | `.as_uuid()` | 元の `Uuid` 値 |

### TC-U05: RecordLabel

**配置先**: `crates/shikomi-core/src/vault/record.rs`（または独立ファイル）

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U05-01 | — | `RecordLabel::try_new("A".to_string())` | `Ok` |
| TC-U05-02 | — | `RecordLabel::try_new("あ".repeat(255))` | `Ok`（255 grapheme） |
| TC-U05-03 | — | `RecordLabel::try_new("あ".repeat(256))` | `Err(InvalidRecordLabel(TooLong { grapheme_count: 256 }))` |
| TC-U05-04 | — | `RecordLabel::try_new("".to_string())` | `Err(InvalidRecordLabel(Empty))` |
| TC-U05-05 | — | `RecordLabel::try_new("\x00A".to_string())` | `Err(InvalidRecordLabel(ControlChar { position: 0 }))` |
| TC-U05-06 | — | `RecordLabel::try_new("\x1FA".to_string())` | `Err(InvalidRecordLabel(ControlChar { .. }))` |
| TC-U05-07 | — | `RecordLabel::try_new("\x7FA".to_string())` | `Err(InvalidRecordLabel(ControlChar { .. }))` |
| TC-U05-08 | — | `RecordLabel::try_new("A\tB".to_string())` | `Ok`（\t 許可） |
| TC-U05-09 | — | `RecordLabel::try_new("A\nB".to_string())` | `Ok`（\n 許可） |
| TC-U05-10 | — | `RecordLabel::try_new("A\rB".to_string())` | `Ok`（\r 許可） |
| TC-U05-11 | `try_new("Test".to_string())` 成功後 | `.as_str()` | `"Test"` |

### TC-U06: RecordPayload

**配置先**: `crates/shikomi-core/src/vault/record.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U06-01 | — | `RecordPayload::Plaintext(SecretString::from_string("s".to_string()))` | バリアントが Plaintext で `SecretString` を保持 |
| TC-U06-02 | — | `RecordPayloadEncrypted::new(nonce_12b, cipher, aad)` | `Ok` |
| TC-U06-03 | — | `RecordPayloadEncrypted::new(nonce_11b, cipher, aad)` | `Err(InvalidRecordPayload(NonceLength { expected:12, got:11 }))` |
| TC-U06-04 | — | `RecordPayloadEncrypted::new(nonce_12b, empty_cipher, aad)` | `Err(InvalidRecordPayload(CipherTextEmpty))` |
| TC-U06-05 | `RecordPayload::Plaintext(_)` | `.variant_mode()` | `ProtectionMode::Plaintext` |
| TC-U06-06 | `RecordPayload::Encrypted(_)` | `.variant_mode()` | `ProtectionMode::Encrypted` |

### TC-U07: Vault 集約ルート

**配置先**: `crates/shikomi-core/src/vault/mod.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U07-01 | — | `Vault::new(plaintext_header)` | `records().is_empty() == true` |
| TC-U07-02 | — | `Vault::new(encrypted_header)` | `records().is_empty() == true` |
| TC-U07-03 | Plaintext vault | `add_record(plaintext_record)` | `Ok(())`、`records().len() == 1` |
| TC-U07-04 | Plaintext vault | `add_record(encrypted_payload_record)` | `Err(VaultConsistencyError(ModeMismatch { .. }))` |
| TC-U07-05 | Encrypted vault | `add_record(plaintext_payload_record)` | `Err(VaultConsistencyError(ModeMismatch { .. }))` |
| TC-U07-06 | Plaintext vault + 1 record 追加済み | 同 id で `add_record` を再度呼ぶ | `Err(VaultConsistencyError(DuplicateId(_)))` |
| TC-U07-07 | 1 record 追加済み | `remove_record(&id)` | `Ok(record)` が返り `records().is_empty() == true` |
| TC-U07-08 | 空の Vault | `remove_record(&unknown_id)` | `Err(VaultConsistencyError(RecordNotFound(_)))` |
| TC-U07-09 | 1 record 追加済み | `find_record(&id)` | `Some(&record)` |
| TC-U07-10 | 空の Vault | `find_record(&unknown_id)` | `None` |
| TC-U07-11 | Plaintext vault | `protection_mode()` | `ProtectionMode::Plaintext` |
| TC-U07-12 | 2 record 追加済み | `records()` | 長さ 2 のスライス |
| TC-U07-13 | 1 record 追加済み | `update_record(&id, \|r\| Ok(r.with_updated_label(new_label, now)?))` | `Ok(())` かつ `find_record` で更新後ラベルを確認できる |
| TC-U07-14 | Plaintext vault | `rekey_with(&mut dummy_provider)` | `Err(VaultConsistencyError(RekeyInPlaintextMode))` |
| TC-U07-15 | Encrypted vault + 1 record 追加済み | `rekey_with(&mut succeeding_dummy_provider)` | `Ok(())` |
| TC-U07-16 | Encrypted vault + 1 record 追加済み | `rekey_with(&mut failing_dummy_provider)` （`reencrypt_all` が `Err` を返す） | `Err(VaultConsistencyError(RekeyPartialFailure))` |

> **ダミー `VekProvider` について**: `VekProvider` は `shikomi-core` が定義する trait。ユニットテストでは `#[cfg(test)]` モジュール内に `struct DummyVekProvider { should_fail: bool }` を定義し trait を実装する。これは外部 I/O を代替するモックではなく、**トレイト境界を満たすテスト用実装**（test double）であり、テスト戦略の「自分が書いたコードはモックするな」に抵触しない（実際の `shikomi-infra` 実装が存在しないスケルトン段階での唯一の検証手段）。

### TC-U08: SecretString / SecretBytes

**配置先**: `crates/shikomi-core/src/secret/mod.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U08-01 | — | `SecretString::from_string("password".to_string())` | パニックなし（失敗しない） |
| TC-U08-02 | `from_string("password")` 成功後 | `format!("{:?}", s)` | 文字列に `"password"` が含まれず `"[REDACTED]"` を含む |
| TC-U08-03 | `from_string("password")` 成功後 | `s.expose_secret()` | `"password"` |
| TC-U08-04 | — | `SecretBytes::from_boxed_slice(vec![1,2,3].into_boxed_slice())` | パニックなし（失敗しない） |
| TC-U08-05 | `from_boxed_slice([1,2,3])` 成功後 | `format!("{:?}", b)` | 文字列に生バイト値が含まれず `"[REDACTED]"` を含む |
| TC-U08-06 | `from_boxed_slice([1,2,3])` 成功後 | `b.expose_secret()` | `&[1u8, 2, 3]` |

### TC-U09: DomainError Display 実装

**配置先**: `crates/shikomi-core/src/error.rs`

> **検証方針**: `thiserror::Error` 由来の `Display` 実装が MSG-DEV-001〜008 のメッセージ文面を正しく返すことを確認する。完全一致ではなく、メッセージの主要キーワードを `contains()` で検証する（メッセージ文面の細部修正で CI が落ちないよう意図的に柔軟化）。
> **注記**: 旧 MSG-DEV-006（InvalidSecretLength）は `detailed-design.md` セル commit `db52923` にて YAGNI 違反として削除済み。以降の番号は詰め直されている。

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U09-01 | — | `format!("{}", DomainError::InvalidProtectionMode("x".into()))` | `"unknown protection mode"` を含む（MSG-DEV-001） |
| TC-U09-02 | — | `format!("{}", DomainError::UnsupportedVaultVersion(99))` | `"unsupported vault version"` かつ `"99"` を含む（MSG-DEV-002） |
| TC-U09-03 | — | `format!("{}", DomainError::InvalidRecordLabel(Empty))` | `"invalid record label"` を含む（MSG-DEV-003） |
| TC-U09-04 | — | `format!("{}", DomainError::VaultConsistencyError(ModeMismatch { .. }))` | `"vault"` かつ `"mode mismatch"` を含む（MSG-DEV-004） |
| TC-U09-05 | — | `format!("{}", DomainError::NonceOverflow)` | `"nonce counter exhausted"` を含む（MSG-DEV-005） |
| TC-U09-06 | — | `format!("{}", DomainError::InvalidRecordId(NilUuid))` | `"invalid record id"` を含む（MSG-DEV-006） |
| TC-U09-07 | — | `format!("{}", DomainError::InvalidRecordPayload(CipherTextEmpty))` | `"invalid record payload"` を含む（MSG-DEV-007） |
| TC-U09-08 | — | `format!("{}", DomainError::InvalidVaultHeader(KdfSaltLength { expected: 16, got: 15 }))` | `"invalid vault header"` を含む（MSG-DEV-008） |

### TC-U10: NonceCounter

**配置先**: `crates/shikomi-core/src/vault/nonce.rs`

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U10-01 | — | `NonceCounter::new([0u8; 8]).current_counter()` | `0u32` |
| TC-U10-02 | `new([0u8; 8])` 後 | `counter.next()` | `Ok(NonceBytes)` かつ `as_array().len() == 12` |
| TC-U10-03 | `new([0u8; 8])` 後に `next()` × 1 | `current_counter()` | `1u32` |
| TC-U10-04 | — | `NonceCounter::resume([0u8; 8], 42).current_counter()` | `42u32` |
| TC-U10-05 | `resume([0u8;8], u32::MAX - 1)` | `next()` | `Ok(NonceBytes)` |
| TC-U10-06 | `resume([0u8;8], u32::MAX)` | `next()` | `Err(DomainError::NonceOverflow)` |
| TC-U10-07 | `new([0xAB; 8])` 後 | `next()` → `as_array()[0..8]` | `[0xAB; 8]`（上位 8B が random_prefix） |

### TC-U11: Aad（追加認証データ）

**配置先**: `crates/shikomi-core/src/vault/crypto_data.rs`

> **根拠**: セル commit `db52923` にて `Aad::new` が `Result<Aad, DomainError>` に変更された（`record_created_at` を i64 マイクロ秒に変換する際の範囲外を Fail Fast 検知）。また `Aad::to_canonical_bytes()` の戻り値が 26 byte 固定長配列（`[u8; 26]`）と確定したため、バイナリ正規形の契約検証テストを追加する。

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U11-01 | — | `Aad::new(valid_record_id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH)` | `Ok(Aad)` |
| TC-U11-02 | — | `Aad::new(valid_record_id, VaultVersion::CURRENT, out_of_range_created_at)` | `Err(DomainError::InvalidRecordPayload(AadTimestampOutOfRange))` |
| TC-U11-03 | `Aad::new` 成功後 | `aad.to_canonical_bytes().len()` | `26`（26 byte 固定長） |

> **out_of_range_created_at の設定**: i64 マイクロ秒が `i64::MAX` を超える未来日付、または `i64::MIN` を下回る過去日付を `OffsetDateTime` として構成する。詳細設計 §バイナリ正規形仕様に定める変換範囲外が対象。

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
      mod.rs         # TC-U07 (#[cfg(test)] mod tests)
      protection_mode.rs  # TC-U01
      version.rs     # TC-U02
      header.rs      # TC-U03
      id.rs          # TC-U04
      record.rs      # TC-U05, TC-U06
      nonce.rs       # TC-U10
      crypto_data.rs # TC-U11（Aad::new Fail Fast / to_canonical_bytes）
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
| 正常系 | 全ケース必須（ユニット 68 件 + 結合 7 件 = 合計 75 件） |
| 異常系 | エラーバリアントの種別まで検証（`assert!(matches!(err, DomainError::Xxx))` レベル） |
| 境界値 | `RecordLabel`（0/1/254/255/256 grapheme）、`NonceCounter`（u32::MAX-1 / u32::MAX）、`VaultVersion`（0/1/2）を必須とする |

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — AC-09（deny.toml 暗号クリティカル crate 確認）追加・TC-I07 追加。TC-U09（DomainError Display MSG-DEV-001〜009）追加。TC-U07-14〜16（Vault::rekey_with テスト）追加。合計 60件→73件*
*改訂: 涅マユリ（テスト担当）/ 2026-04-22 — セル commit db52923 対応: TC-U09 の MSG-DEV 番号を 001〜008 に詰め直し（旧 MSG-DEV-006 InvalidSecretLength 削除）。TC-U11（Aad::new Fail Fast / to_canonical_bytes バイナリ正規形）追加。合計 73件→75件*
*対応 Issue: #7 feat(shikomi-core): vault ドメイン型定義*
