# テスト設計書 — vault（shikomi-core ドメイン型定義）: 概要・受入基準・マトリクス

> **ファイル構成**
> | ファイル | 内容 |
> |---------|------|
> | `overview.md`（本ファイル） | §1概要・§2受入基準・§3テストマトリクス・§4E2Eテスト |
> | [`integration.md`](integration.md) | §5 結合テスト設計（TC-I01〜TC-I07） |
> | [`unit.md`](unit.md) | §6 ユニットテスト設計（TC-U01〜TC-U12） |
> | [`appendix.md`](appendix.md) | §7 モック方針・§8 ディレクトリ構造・§9 実行手順・§10 カバレッジ |

---

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
| TC-U08-01 | REQ-008 | AC-01 | `SecretString::from_string` でインスタンスを構築できる（失敗しない） | 正常系 |
| TC-U08-02 | REQ-008 | AC-04 | `format!("{:?}", secret_string)` の出力が `"[REDACTED]"` である | 正常系 |
| TC-U08-03 | REQ-008 | AC-04 | `SecretString::expose_secret()` が元の文字列を返す | 正常系 |
| TC-U08-04 | REQ-008 | AC-01 | `SecretBytes::from_boxed_slice` でインスタンスを構築できる（失敗しない） | 正常系 |
| TC-U08-05 | REQ-008 | AC-04 | `format!("{:?}", secret_bytes)` の出力が `"[REDACTED]"` である | 正常系 |
| TC-U08-06 | REQ-008 | AC-04 | `SecretBytes::expose_secret()` が元のバイト列を返す | 正常系 |
| TC-U09-01 | REQ-009 | AC-01 | `DomainError::InvalidProtectionMode("x")` の `Display` が `"unknown protection mode: x"` を含む（MSG-DEV-001） | 正常系 |
| TC-U09-02 | REQ-009 | AC-01 | `DomainError::UnsupportedVaultVersion(99)` の `Display` が `"unsupported vault version: 99"` を含む（MSG-DEV-002） | 正常系 |
| TC-U09-03 | REQ-009 | AC-01 | `DomainError::InvalidRecordLabel(Empty)` の `Display` が `"invalid record label"` を含む（MSG-DEV-003） | 正常系 |
| TC-U09-04 | REQ-009 | AC-01 | `DomainError::VaultConsistencyError(ModeMismatch { .. })` の `Display` が `"vault"` かつ `"mode"` を含む（MSG-DEV-004 改訂: `#[error("{0}")]` により内包 `VaultConsistencyReason` の Display に委譲） | 正常系 |
| TC-U09-04b | REQ-009 | AC-01 | `DomainError::VaultConsistencyError(DuplicateId(..))` の `Display` が `"duplicate"` を含み `"mode mismatch"` を含まない（旧実装で混入していた文言の不混入確認） | 異常系 |
| TC-U09-05 | REQ-009 | AC-01 | `DomainError::NonceOverflow` の `Display` が `"nonce counter exhausted"` を含む（MSG-DEV-005） | 正常系 |
| TC-U09-06 | REQ-009 | AC-01 | `DomainError::InvalidRecordId(NilUuid)` の `Display` が `"invalid record id"` を含む（MSG-DEV-006） | 正常系 |
| TC-U09-07 | REQ-009 | AC-01 | `DomainError::InvalidRecordPayload(CipherTextEmpty)` の `Display` が `"invalid record payload"` を含む（MSG-DEV-007） | 正常系 |
| TC-U09-08 | REQ-009 | AC-01 | `DomainError::InvalidVaultHeader(KdfSaltLength { expected:16, got:15 })` の `Display` が `"invalid vault header"` を含む（MSG-DEV-008） | 正常系 |
| TC-U10-01 | REQ-010 | AC-01 | `NonceCounter::new` で counter が 0 から開始する | 正常系 |
| TC-U10-02 | REQ-010 | AC-01 | `NonceCounter::next()` が 12 バイトの `NonceBytes` を返す | 正常系 |
| TC-U10-03 | REQ-010 | AC-01 | `NonceCounter::current_counter()` が `next()` 呼び出し後にインクリメントされる | 正常系 |
| TC-U10-04 | REQ-010 | AC-01 | `NonceCounter::resume(random_prefix, n)` で counter が `n` から再開する | 正常系 |
| TC-U10-05 | REQ-010 | AC-05 | `NonceCounter` の counter が `u32::MAX - 1` の状態で `next()` が成功する（境界値: 上限直前） | 境界値 |
| TC-U10-06 | REQ-010 | AC-05 | `NonceCounter` の counter が `u32::MAX` の状態で `next()` が `DomainError::NonceOverflow` を返す（境界値: 上限） | 境界値 |
| TC-U10-07 | REQ-010 | AC-01 | `NonceCounter::next()` で生成された `NonceBytes` は上位 8 バイトが `random_prefix` と一致する | 正常系 |
| TC-U10-08 | REQ-010 | AC-01 | `next()` を 3 回呼んだ時、下位 4 バイト（bytes 8..12）が big-endian で `[0,0,0,0]→[0,0,0,1]→[0,0,0,2]` になる（エンディアン契約） | 正常系 |
| TC-U11-01 | REQ-006 | AC-01 | `Aad::new(record_id, vault_version, valid_created_at)` が `Ok(Aad)` を返す | 正常系 |
| TC-U11-02 | REQ-006 | AC-01 | `Aad::new(record_id, vault_version, out_of_range_created_at)` が `InvalidRecordPayload(AadTimestampOutOfRange)` を返す（Fail Fast） | 異常系 |
| TC-U11-03 | REQ-006 | AC-01 | `Aad::to_canonical_bytes()` が 26 byte 固定長の配列を返す | 正常系 |
| TC-U11-04 | REQ-006 | AC-01 | 既知の RecordId / VaultVersion / OffsetDateTime から `to_canonical_bytes()` が期待する 26 byte 列とバイト単位で完全一致する（黄金値テスト） | 正常系 |
| TC-U12-01 | REQ-005 | AC-01 | ナノ秒を含む `OffsetDateTime` を `Record::new` に渡すと `record.created_at()` がマイクロ秒精度に切り捨てられる（サブ秒丸め round-trip） | 正常系 |
| TC-U12-02 | REQ-005 | AC-01 | 同じ `OffsetDateTime` を渡した場合、`record.updated_at()` もマイクロ秒精度に切り捨てられ `created_at()` と等しい | 正常系 |

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

**省略理由**: `shikomi-core` はエンドユーザーが直接操作する UI / CLI / 公開 HTTP API を持たない純粋ドメイン型ライブラリ。テスト戦略ガイドの「エンドユーザー操作がない場合は結合テストで代替」方針に従い、E2E は設計対象外とする。受入基準の検証は [integration.md](integration.md)（結合テスト）と [unit.md](unit.md)（ユニットテスト）で網羅する。

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*対応 Issue: #7 feat(shikomi-core): vault ドメイン型定義*
