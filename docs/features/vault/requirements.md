# 要件定義書

<!-- feature: vault / Issue #7 -->
<!-- 配置先: docs/features/vault/requirements.md -->

## 機能要件

### REQ-001: ProtectionMode（保護モード enum）

| 項目 | 内容 |
|------|------|
| 入力 | なし（ドメイン列挙型） |
| 処理 | `Plaintext` / `Encrypted` の排他 2 値を提供。永続化時は `"plaintext"` / `"encrypted"` の文字列に写像 |
| 出力 | enum バリアント |
| エラー時 | 未知文字列（永続化から復元時）→ `DomainError::InvalidProtectionMode` |

### REQ-002: VaultVersion（フォーマットバージョン）

| 項目 | 内容 |
|------|------|
| 入力 | `u16` リテラル |
| 処理 | 対応範囲チェック（CURRENT 定数 1、MIN_SUPPORTED 定数 1）。範囲外は拒否 |
| 出力 | `VaultVersion` 値 |
| エラー時 | 範囲外 → `DomainError::UnsupportedVaultVersion` |

### REQ-003: VaultHeader（ヘッダ集約）

| 項目 | 内容 |
|------|------|
| 入力 | `ProtectionMode`, `VaultVersion`, `OffsetDateTime`（作成時刻）。暗号化モード時は追加で `KdfSalt`（16B）、`WrappedVek`（pw 経路）、`WrappedVek`（recovery 経路） |
| 処理 | モードごとに **enum バリアントで有効フィールドを排他**し、平文モード時は暗号フィールドを持たない型としてコンストラクタ分岐 |
| 出力 | `VaultHeader` 値（モードごとに型レベルで異なる中身を保持する enum） |
| エラー時 | 非対応バージョン / KDF salt 長不一致 → `DomainError::InvalidVaultHeader` サブバリアント |

### REQ-004: RecordId（UUIDv7 レコード ID）

| 項目 | 内容 |
|------|------|
| 入力 | 新規生成: 外部から `uuid::Uuid`（v7）を受け取る（純粋ドメインなので乱数源は持たない）／復元: `&str` |
| 処理 | UUIDv7 以外のバージョンを拒否、ゼロ UUID を拒否 |
| 出力 | `RecordId` |
| エラー時 | 非 UUIDv7 / ゼロ UUID / 不正書式 → `DomainError::InvalidRecordId` |

### REQ-005: Record / RecordKind / RecordLabel

| 項目 | 内容 |
|------|------|
| 入力 | `RecordId`, `RecordKind`（`Text` / `Secret`）, `RecordLabel`（String 経由の検証付き）, `RecordPayload`, `OffsetDateTime`（created_at / updated_at） |
| 処理 | `RecordLabel` は非空・Unicode スカラ値制御文字禁止（U+0000〜U+001F / U+007F を除外、ただし U+0009 / U+000A / U+000D は許可）・正規化前後で 255 文字（grapheme ベース）以下を検証 |
| 出力 | `Record` 値 |
| エラー時 | ラベル違反 → `DomainError::InvalidRecordLabel(Reason)`（`Empty` / `ControlChar` / `TooLong`） |

### REQ-006: RecordPayload（平文／暗号化 enum）

| 項目 | 内容 |
|------|------|
| 入力 | 平文: `SecretString`／暗号化: `NonceBytes`（12B）+ `CipherText` (Vec<u8>) + `Aad`（構造化 AAD） |
| 処理 | enum バリアント 2 値で排他。平文バリアントに nonce/ciphertext が混在しないことを型で保証 |
| 出力 | `RecordPayload` 値 |
| エラー時 | nonce 長不一致 / ciphertext 長 = 0 → `DomainError::InvalidRecordPayload(Reason)` |

### REQ-007: Vault（集約ルート）

| 項目 | 内容 |
|------|------|
| 入力 | `VaultHeader`, `Vec<Record>` |
| 処理 | ヘッダの `ProtectionMode` と全レコードの `RecordPayload` バリアントが**整合すること**を保証（平文モード時に暗号 payload が混ざっていたら拒否、およびその逆）。重複 `RecordId` を拒否。更新系メソッドは `add_record` / `remove_record` / `update_record` / `rekey_with` 等を集約自身に生やす（Tell, Don't Ask） |
| 出力 | `Vault` 値 / 各操作メソッドの結果 |
| エラー時 | モード不整合 / ID 重複 / rekey 時の nonce オーバーフロー → `DomainError` の該当バリアント |

### REQ-008: SecretString / SecretBytes

| 項目 | 内容 |
|------|------|
| 入力 | `String` / `Box<[u8]>`（呼び出し側は生値を即座に wrapper に渡す規約） |
| 処理 | `secrecy::SecretBox` に格納。`Debug` / `Display` 実装は必ず `"[REDACTED]"` 固定文字列を出力。`serde::Serialize` は実装しない（コンパイル時に誤シリアライズを検出） |
| 出力 | `SecretString` / `SecretBytes`（可変長のみ。本 Issue では固定長バリアントを提供しない — 呼び出し側で長さ検証が必要な型は将来の shikomi-infra Issue で追加） |
| エラー時 | 入力検証は呼び出し側責務（本型は中身を判定しない） |

### REQ-009: DomainError

| 項目 | 内容 |
|------|------|
| 入力 | 各型の構築／状態遷移呼び出し |
| 処理 | `thiserror::Error` で列挙型として実装。バリアントは機能ごとに分離（`InvalidProtectionMode` / `UnsupportedVaultVersion` / `InvalidVaultHeader(Reason)` / `InvalidRecordId(Reason)` / `InvalidRecordLabel(Reason)` / `InvalidRecordPayload(Reason)` / `VaultConsistencyError(Reason)` / `NonceOverflow`） |
| 出力 | `DomainError` 値 |
| エラー時 | エラー型自体は Fail しない（エラーを表現する型） |

### REQ-010: NonceCounter

| 項目 | 内容 |
|------|------|
| 入力 | `NonceCounter::new()` / `NonceCounter::resume(u32)`（永続化からの再開） |
| 処理 | 内部 `u32` を保持。`next()` は現在値を `NonceBytes`（12B、下位 4B = カウンタ、上位 8B = CSPRNG 乱数）として返しインクリメント。`u32::MAX` 到達で次回 `next()` が `NonceOverflow` を返す |
| 出力 | `NonceBytes`（`[u8; 12]` newtype） |
| エラー時 | 上限到達 → `DomainError::NonceOverflow`（rekey トリガ） |

## 画面・CLI仕様

該当なし — 理由: 本 Issue は `shikomi-core` 内部ドメイン型のみ。CLI/GUI は `shikomi-cli` / `shikomi-gui` の後続 Issue で定義する。

## API仕様

本 crate の公開 API は Rust 型 API であり、HTTP エンドポイントではない。主要な公開型を以下に列挙する（詳細シグネチャは basic-design / detailed-design）:

| モジュール | 公開型 | 用途 |
|----------|-------|------|
| `shikomi_core::vault` | `Vault` | 集約ルート |
| 〃 | `VaultHeader`, `VaultVersion`, `ProtectionMode` | ヘッダ／保護モード |
| 〃 | `Record`, `RecordId`, `RecordKind`, `RecordLabel`, `RecordPayload` | レコード |
| 〃 | `NonceBytes`, `NonceCounter`, `KdfSalt`, `WrappedVek`, `CipherText`, `Aad` | 暗号化関連データ型 |
| `shikomi_core::secret` | `SecretString`, `SecretBytes` | 秘密値ラッパ |
| `shikomi_core::error` | `DomainError`, `InvalidRecordLabelReason`, `InvalidRecordPayloadReason`, `VaultConsistencyReason` | ドメインエラー |

**公開 API のシグネチャ方針**:

- 集約操作は `Vault::add_record(&mut self, record: Record) -> Result<(), DomainError>` のように集約側に生やす
- 問合せは `Vault::protection_mode(&self) -> ProtectionMode` 等（Ask）最小限に留め、処理は集約メソッド（Tell）を優先する
- 生 `String` / `Vec<u8>` は公開 API に出さない。暗号文（鍵なしで安全）は `CipherText` newtype 経由で公開

## データモデル

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| Vault | header | VaultHeader | 必須、1 vault に 1 個 | 集約ルート |
| Vault | records | Vec<Record> | レコード 0 件以上、RecordId 一意 | Vault → Record (1..N) |
| VaultHeader | protection_mode | ProtectionMode | 必須 | — |
| VaultHeader | version | VaultVersion | `CURRENT` または互換範囲 | — |
| VaultHeader | created_at | OffsetDateTime | UTC、RFC3339 | — |
| VaultHeader (Encrypted) | kdf_salt | KdfSalt | 16 byte 固定 | — |
| VaultHeader (Encrypted) | wrapped_vek_by_pw | WrappedVek | pw 経路 | — |
| VaultHeader (Encrypted) | wrapped_vek_by_recovery | WrappedVek | recovery 経路 | — |
| Record | id | RecordId | UUIDv7、Vault 内一意 | — |
| Record | kind | RecordKind | `Text` / `Secret` | — |
| Record | label | RecordLabel | 非空、制御文字除外、255 grapheme 以下 | — |
| Record | payload | RecordPayload | ヘッダ mode と整合必須 | — |
| Record | created_at / updated_at | OffsetDateTime | updated_at ≥ created_at | — |
| RecordPayload::Plaintext | value | SecretString | — | — |
| RecordPayload::Encrypted | nonce | NonceBytes | 12 byte 固定 | — |
| 〃 | ciphertext | CipherText | 非空 | — |
| 〃 | aad | Aad | record_id + version + created_at を含む | — |

## ユーザー向けメッセージ一覧

本 Issue は内部ドメイン型のため**エンドユーザー向けメッセージは生成しない**。エラー型 `DomainError` の `Display` は「開発者向けエラー文面」であり、CLI/GUI でユーザーに表示する際は各 crate で i18n ラベルに写像する。

| ID | 種別 | メッセージ | 表示条件 |
|----|------|----------|---------|
| MSG-DEV-001 | 開発者エラー | `unknown protection mode: {0}` | `InvalidProtectionMode` |
| MSG-DEV-002 | 開発者エラー | `unsupported vault version: {0}` | `UnsupportedVaultVersion` |
| MSG-DEV-003 | 開発者エラー | `invalid record label: {0}` | `InvalidRecordLabel` |
| MSG-DEV-004 | 開発者エラー | 内包 `VaultConsistencyReason` の `Display` を素通し（`#[error("{0}")]`、各バリアント文言は `error.rs` の `VaultConsistencyReason` 定義を参照。`DuplicateId` 等は mode mismatch 文言を含まない） | `VaultConsistencyError` |
| MSG-DEV-005 | 開発者エラー | `nonce counter exhausted; rekey required` | `NonceOverflow` |
| MSG-DEV-006 | 開発者エラー | `invalid record id: {0}` | `InvalidRecordId` |
| MSG-DEV-007 | 開発者エラー | `invalid record payload: {0}` | `InvalidRecordPayload` |
| MSG-DEV-008 | 開発者エラー | `invalid vault header: {0}` | `InvalidVaultHeader` |

**エンドユーザー向け文言**（例: SmartScreen 警告対応、リカバリコード入力画面）は GUI/CLI Issue で `MSG-UI-xxx` として別途定義する。

## 依存関係

| crate | バージョン | feature | 用途 |
|-------|----------|--------|------|
| `uuid` | 1.x（最新安定系） | `v7`, `serde` | `RecordId` の UUIDv7 |
| `serde` | 1.x | `derive` | 型のシリアライズ属性（`SecretString`/`SecretBytes` は非実装） |
| `secrecy` | 0.10 以降 | `serde` 非使用 | 秘密値ラッパ |
| `zeroize` | 1.x | `derive` | drop 時 `zeroize` |
| `thiserror` | 2.x | — | `DomainError` 実装 |
| `time` | 0.3.x | `serde`, `macros` | `OffsetDateTime` |

全て `Cargo.toml` ルートの `[workspace.dependencies]` 経由で指定し、`crates/shikomi-core/Cargo.toml` では `{ workspace = true }` で参照する（`docs/architecture/tech-stack.md` §4.1 / §4.4）。

## 関連 feature

| feature | 関係 | 参照先 |
|---------|------|--------|
| `vault-persistence`（Issue #10 以降） | 本 feature で定義したドメイン型（`Vault` / `VaultHeader` / `Record` / `SecretString` / `WrappedVek` 等）を SQLite に永続化する層。`VaultRepository` trait と `SqliteVaultRepository` 実装を提供。本 feature は I/O を一切持たず、永続化は全て `vault-persistence` に委譲する | `docs/features/vault-persistence/` |
