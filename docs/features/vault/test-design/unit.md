# テスト設計書 — vault: ユニットテスト設計（TC-U01〜TC-U12）

> **ファイル構成**
> | ファイル | 内容 |
> |---------|------|
> | [`overview.md`](overview.md) | §1概要・§2受入基準・§3テストマトリクス・§4E2Eテスト |
> | [`integration.md`](integration.md) | §5 結合テスト設計（TC-I01〜TC-I07） |
> | `unit.md`（本ファイル） | §6 ユニットテスト設計（TC-U01〜TC-U12） |
> | [`appendix.md`](appendix.md) | §7 モック方針・§8 ディレクトリ構造・§9 実行手順・§10 カバレッジ |

---

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
| TC-U09-04 | — | `format!("{}", DomainError::VaultConsistencyError(ModeMismatch { vault_mode: Plaintext, record_mode: Encrypted }))` | `"vault"` かつ `"mode"` を含む（MSG-DEV-004 改訂: `#[error("{0}")]` により `VaultConsistencyReason::ModeMismatch` の Display `"vault is in ... mode but record payload is ..."` が素通しされる） |
| TC-U09-04b | — | `format!("{}", DomainError::VaultConsistencyError(DuplicateId(..)))` | `"duplicate"` を含み `"mode mismatch"` を含まない（旧 `#[error("vault and record payload mode mismatch: {0}")]` で混入していた文言が除去されたことの確認） |
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
| TC-U10-08 | `new([0u8; 8])` 後 | `next()` × 3 → 各 `as_array()[8..12]` | 1回目: `[0x00, 0x00, 0x00, 0x00]` / 2回目: `[0x00, 0x00, 0x00, 0x01]` / 3回目: `[0x00, 0x00, 0x00, 0x02]`（big-endian カウンタ） |

### TC-U11: Aad（追加認証データ）

**配置先**: `crates/shikomi-core/src/vault/crypto_data.rs`

> **根拠**: セル commit `db52923` にて `Aad::new` が `Result<Aad, DomainError>` に変更された（`record_created_at` を i64 マイクロ秒に変換する際の範囲外を Fail Fast 検知）。また `Aad::to_canonical_bytes()` の戻り値が 26 byte 固定長配列（`[u8; 26]`）と確定したため、バイナリ正規形の契約検証テストを追加する。

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U11-01 | — | `Aad::new(valid_record_id, VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH)` | `Ok(Aad)` |
| TC-U11-02 | — | `Aad::new(valid_record_id, VaultVersion::CURRENT, out_of_range_created_at)` | `Err(DomainError::InvalidRecordPayload(AadTimestampOutOfRange))` |
| TC-U11-03 | `Aad::new` 成功後 | `aad.to_canonical_bytes().len()` | `26`（26 byte 固定長） |
| TC-U11-04 | — | 下記「黄金値テストベクタ」を使い `to_canonical_bytes()` の出力を `assert_eq!` | 期待 26 byte 列と完全一致（バイト単位） |

> **黄金値テストベクタ**（TC-U11-04 専用）
>
> | 入力 | 値 |
> |------|---|
> | `RecordId` | `Uuid::from_bytes([0x01, 0x8f, 0x12, 0x34, 0x56, 0x78, 0x7a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x90, 0x12])` — UUIDv7 形式（`uuid::Uuid::as_bytes()` 順） |
> | `VaultVersion` | `VaultVersion::CURRENT`（= 1） |
> | `record_created_at` | `OffsetDateTime::UNIX_EPOCH`（Unix エポック, マイクロ秒 = 0） |
>
> | オフセット | 期待バイト列 | 由来 |
> |-----------|------------|------|
> | `0..16`   | `[0x01, 0x8f, 0x12, 0x34, 0x56, 0x78, 0x7a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x90, 0x12]` | `uuid::Uuid::as_bytes()` — MSB first |
> | `16..18`  | `[0x00, 0x01]` | u16 = 1 の big-endian |
> | `18..26`  | `[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]` | i64 = 0 の big-endian（エポック） |
>
> 期待全体: `[0x01,0x8f,0x12,0x34,0x56,0x78,0x7a,0xbc,0xde,0xf0,0x12,0x34,0x56,0x78,0x90,0x12,0x00,0x01,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00]`

> **out_of_range_created_at の設定**: i64 マイクロ秒が `i64::MAX` を超える未来日付、または `i64::MIN` を下回る過去日付を `OffsetDateTime` として構成する。詳細設計 §バイナリ正規形仕様に定める変換範囲外が対象。

### TC-U12: Record タイムスタンプ精度（サブ秒丸め round-trip）

**配置先**: `crates/shikomi-core/src/vault/mod.rs` または `record.rs`

> **根拠**: `detailed-design.md` §バイナリ正規形仕様 L385「`Record::new(..)` は内部で `created_at` / `updated_at` をマイクロ秒精度に**切り捨て**る（nanoseconds 以下は捨てる）」。この契約は AAD 計算と SQLite 永続化が同じ値を参照する保証の基盤。切り捨て実装が欠落すると `Aad::to_canonical_bytes()` と永続化値が diverge しサイレントデコード失敗を招く。

| テストID | 前提条件 | 操作 | 期待結果 |
|---------|---------|------|---------|
| TC-U12-01 | — | `OffsetDateTime::from_unix_timestamp_nanos(123_456_789)` (= 0.123456789s, 789ns sub-µs成分あり) を `now` として `Record::new` を呼ぶ | `record.created_at().nanosecond() % 1_000 == 0`（789ns が切り捨てられて 000ns になる） |
| TC-U12-02 | — | 同じ `now` で `record.updated_at()` も確認 | `record.updated_at().nanosecond() % 1_000 == 0` かつ `record.updated_at() == record.created_at()` |

> **テスト上の留意点**: `time::OffsetDateTime` から nanoseconds 成分を取り出すには `.nanosecond()` を使う。切り捨て後の値は `OffsetDateTime::from_unix_timestamp_nanos(123_456_000)` と等しい（789 → 000 ns）。

---

*作成: 涅マユリ（テスト担当）/ 2026-04-22*
*対応 Issue: #7 feat(shikomi-core): vault ドメイン型定義*
