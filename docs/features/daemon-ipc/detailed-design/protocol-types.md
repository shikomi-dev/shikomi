# 詳細設計書 — protocol-types（shikomi-core::ipc 配下の型定義詳細）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-ipc / Issue #26 -->
<!-- 配置先: docs/features/daemon-ipc/detailed-design/protocol-types.md -->
<!-- 兄弟: ./index.md, ./daemon-runtime.md, ./ipc-vault-repository.md, ./composition-root.md, ./lifecycle.md, ./future-extensions.md -->

## 記述ルール

疑似コード禁止。型定義は**フィールド名・型・serde attribute**のみ示し、関数本体は書かない。

## モジュール配置

`crates/shikomi-core/src/ipc/` 配下に以下のファイルを新規作成:

```
crates/shikomi-core/src/
  ipc/
    mod.rs                    # pub mod 再エクスポート
    version.rs                # IpcProtocolVersion
    request.rs                # IpcRequest
    response.rs               # IpcResponse
    summary.rs                # RecordSummary
    error_code.rs             # IpcErrorCode
    secret_bytes.rs           # SerializableSecretBytes
```

`crates/shikomi-core/src/lib.rs` に `pub mod ipc;` を追加。

## `IpcProtocolVersion`

- 配置: `crates/shikomi-core/src/ipc/version.rs`
- 型: `pub enum IpcProtocolVersion`
- attribute:
  - `#[non_exhaustive]`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
- バリアント:
  - `V1` — 初期バージョン（`Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord` の 5 バリアント対応）

**メソッド**:
- `pub const fn current() -> Self` → `IpcProtocolVersion::V1` を返す（クライアント / サーバが「自分の対応バージョン」を取得する API、実装時のバージョンアップ追随を一箇所に集約）

**`Display` 実装**: `impl std::fmt::Display for IpcProtocolVersion` で `"v1"` 等を返す（log 可読性、CLI エラー表示で使用）。`Debug` は derive。

## `IpcRequest`

- 配置: `crates/shikomi-core/src/ipc/request.rs`
- 型: `pub enum IpcRequest`
- attribute:
  - `#[non_exhaustive]`
  - `#[derive(Debug, Clone, Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
- バリアント:

| バリアント | フィールド | 用途 |
|-----------|-----------|------|
| `Handshake { client_version: IpcProtocolVersion }` | `client_version` | 接続直後の必須 1 往復、プロトコル一致確認 |
| `ListRecords` | unit | 全レコードの `RecordSummary` 列要求 |
| `AddRecord { kind: RecordKind, label: RecordLabel, value: SerializableSecretBytes, now: OffsetDateTime }` | 4 フィールド | 新規レコード追加 |
| `EditRecord { id: RecordId, label: Option<RecordLabel>, value: Option<SerializableSecretBytes>, now: OffsetDateTime }` | 4 フィールド（label / value は任意）| 既存レコード部分更新 |
| `RemoveRecord { id: RecordId }` | `id` | レコード削除 |

**`now` フィールドの serde attribute**: `#[serde(with = "time::serde::rfc3339")]` で RFC3339 文字列として送受信。MessagePack 上は `str` 型。

**`Debug` 実装**: derive で十分（`SerializableSecretBytes` の `Debug` が `[REDACTED]` 固定なため、`AddRecord` 全体を `Debug` 経路で出しても secret は露出しない）。ただし**禁止事項**: `tracing::info!("{:?}", request)` のような全体 debug 出力は**設計規約で禁止**（`../basic-design/error.md §禁止事項` / 念のため variant 名のみ出す）

**variant 名取得用ヘルパ**: 後の log 出力のため `pub fn IpcRequest::variant_name(&self) -> &'static str` を実装。`match self { Handshake { .. } => "handshake", ListRecords => "list_records", ... }` のシンプル写像。

## `IpcResponse`

- 配置: `crates/shikomi-core/src/ipc/response.rs`
- 型: `pub enum IpcResponse`
- attribute:
  - `#[non_exhaustive]`
  - `#[derive(Debug, Clone, Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
- バリアント:

| バリアント | フィールド | 用途 |
|-----------|-----------|------|
| `Handshake { server_version: IpcProtocolVersion }` | `server_version` | ハンドシェイク成功 |
| `ProtocolVersionMismatch { server: IpcProtocolVersion, client: IpcProtocolVersion }` | 2 バージョン | 不一致、両側のバージョンを返す |
| `Records(Vec<RecordSummary>)` | `Vec<RecordSummary>` | List 応答 |
| `Added { id: RecordId }` | `id` | Add 成功 |
| `Edited { id: RecordId }` | `id` | Edit 成功 |
| `Removed { id: RecordId }` | `id` | Remove 成功 |
| `Error(IpcErrorCode)` | `IpcErrorCode` | 各種失敗 |

**variant 名取得用ヘルパ**: `pub fn IpcResponse::variant_name(&self) -> &'static str`。

## `RecordSummary`

- 配置: `crates/shikomi-core/src/ipc/summary.rs`
- 型: `pub struct RecordSummary`
- attribute:
  - `#[derive(Debug, Clone, Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
- フィールド:

| フィールド | 型 | 制約 |
|----------|---|------|
| `id` | `RecordId` | UUIDv7 |
| `kind` | `RecordKind` | `Text` / `Secret` |
| `label` | `RecordLabel` | 検証済み |
| `value_preview` | `Option<String>` | Text の場合は `Record::text_preview(40)` の結果、Secret は `None` |
| `value_masked` | `bool` | Secret は `true`、Text は `false` |

**コンストラクタ**:
- `pub fn RecordSummary::from_record(record: &Record) -> Self`
  - `record.kind() == RecordKind::Text` なら `value_preview = record.text_preview(40)` / `value_masked = false`
  - `record.kind() == RecordKind::Secret` なら `value_preview = None` / `value_masked = true`
  - 内部で `record.text_preview` を呼ぶ（既存 API、`shikomi-core::Record` に `cli-vault-commands` で追加済み）

**設計理由**:
- List 応答に Secret 値を含めない（投影型）
- `value_preview` は `Option<String>` で、Text と Secret の差を型レベルで表現
- `value_masked` は UI 側で「`****` 表示」のヒントとして使用（`presenter::list` で参照）

## `IpcErrorCode`

- 配置: `crates/shikomi-core/src/ipc/error_code.rs`
- 型: `pub enum IpcErrorCode`
- attribute:
  - `#[non_exhaustive]`
  - `#[derive(Debug, Clone, Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
- バリアント:

| バリアント | フィールド |
|-----------|-----------|
| `EncryptionUnsupported` | unit |
| `NotFound { id: RecordId }` | `id` |
| `InvalidLabel { reason: String }` | `reason`（英語短文） |
| `Persistence { reason: String }` | `reason`（英語短文） |
| `Domain { reason: String }` | `reason`（英語短文） |
| `Internal { reason: String }` | `reason`（英語短文） |

**設計規約**:
- `reason` フィールドは英語短文のみ。secret 値・絶対パス・ピア UID を含めない
- `Display` 実装: `impl std::fmt::Display for IpcErrorCode` で各バリアントを人間可読文に整形（`tracing::warn!` 等で参照）
- `Debug` は derive

## `SerializableSecretBytes`

- 配置: `crates/shikomi-core/src/ipc/secret_bytes.rs`
- 型: `pub struct SerializableSecretBytes(pub SecretBytes)`
- attribute:
  - `#[derive(Clone)]`（`SecretBytes` が `Clone` 実装ありの場合のみ。なければ手動 `Clone` で `from_vec(self.0.clone_vec())` 経由）
  - **`Debug` は derive 不可、手動実装**: `impl std::fmt::Debug` で `f.write_str("SerializableSecretBytes([REDACTED])")` を返す（`SecretBytes` の `Debug` 透過を防ぐ）
  - **`Serialize` は手動実装**:
    - `impl serde::Serialize for SerializableSecretBytes`
    - `fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>` の中で `serializer.serialize_bytes(self.0.as_serialize_slice())` を呼ぶ
    - `as_serialize_slice` は `shikomi-core::secret::SecretBytes` の `pub(crate)` メソッド（次節で定義）
  - **`Deserialize` は手動実装**:
    - `impl<'de> serde::Deserialize<'de> for SerializableSecretBytes`
    - 内部で `Vec<u8>::deserialize(deserializer)` 相当の visitor を経由 → `SecretBytes::from_vec(bytes)` で構築 → `Self(secret_bytes)` を返す

**`expose_secret` 不使用契約の遵守**:
- `as_serialize_slice` は `SecretBytes` 内部で `expose_secret` を呼ぶが、その呼出は `crates/shikomi-core/src/secret/bytes.rs` 内に閉じる
- `crates/shikomi-core/src/ipc/` 配下では `expose_secret` の文字列が出現しない（CI grep で監査）

## `shikomi-core::secret::SecretBytes` への変更

`crates/shikomi-core/src/secret/bytes.rs`（既存または新規）に以下を追加:

- `pub(crate) fn SecretBytes::as_serialize_slice(&self) -> &[u8]`
  - 内部で `self.expose_secret()` を呼んで `&[u8]` を返す
  - 公開範囲: `pub(crate)`（`shikomi-core` crate 内のみ）
  - 配置理由: `shikomi-core::ipc::secret_bytes::SerializableSecretBytes::serialize` から呼ぶため
- `pub fn SecretBytes::from_vec(bytes: Vec<u8>) -> Self`
  - 既存実装があるはずだが、ない場合は本 feature で追加
- `pub fn SecretBytes::clone_vec(&self) -> Vec<u8>`（`Clone` 経路で必要なら）

**`SecretBytes` の存在確認**: `shikomi-core::secret` モジュールに `SecretBytes` が既存かどうかを実装担当（坂田銀時）が確認。**未存在の場合は本 feature で新規追加**（`SecretString` との対称性、`Vec<u8>` ベース、`zeroize::Zeroize` 実装、`Drop` で zeroize、`Debug` で `[REDACTED]` 固定）。

## `Cargo.toml` への変更（`shikomi-core`）

`crates/shikomi-core/Cargo.toml` の `[dependencies]` に以下が**既に存在**することを確認:

- `serde = { workspace = true }`（必須）
- `time = { workspace = true }`（必須、`OffsetDateTime` のため）
- `secrecy = { workspace = true }`（必須、`SecretString` / `SecretBytes` のため）
- `zeroize = { workspace = true }`（必須）
- `uuid = { workspace = true }`（必須、`RecordId` / UUIDv7 のため）

**新規追加なし**（`tokio` / `rmp-serde` / `tokio-util` は `shikomi-core` に追加しない、§4.5 の純粋性維持）。

## `crates/shikomi-core/src/ipc/mod.rs` の再エクスポート

```
pub mod version;
pub mod request;
pub mod response;
pub mod summary;
pub mod error_code;
pub mod secret_bytes;

pub use version::IpcProtocolVersion;
pub use request::IpcRequest;
pub use response::IpcResponse;
pub use summary::RecordSummary;
pub use error_code::IpcErrorCode;
pub use secret_bytes::SerializableSecretBytes;
```

呼び出し側（daemon / cli）は `use shikomi_core::ipc::{IpcRequest, IpcResponse, IpcProtocolVersion, ...};` で参照する。

## バリアント別 wire 表現の例（参考）

具体的な MessagePack バイト列展開は **rmp-serde の `Serialize` 実装に委ねる**ため詳細設計書では扱わない。型 round-trip テスト（`crates/shikomi-daemon/tests/ipc_protocol.rs` で `rmp_serde::to_vec` → `from_slice` の同値検証）で安定性を担保する。

参考: `serde(rename_all = "snake_case")` による wire 表現の概念例（人間可読 JSON 風表記）:

| バリアント | 概念 wire |
|-----------|---------|
| `IpcRequest::Handshake { client_version: V1 }` | `{"handshake": {"client_version": "v1"}}` |
| `IpcRequest::ListRecords` | `{"list_records": null}` |
| `IpcRequest::AddRecord { ... }` | `{"add_record": {"kind": "secret", "label": "...", "value": <bytes>, "now": "2026-04-25T..."}}` |
| `IpcResponse::Records(...)` | `{"records": [{"id": "...", "kind": "text", "label": "...", "value_preview": "...", "value_masked": false}, ...]}` |
| `IpcResponse::Error(IpcErrorCode::NotFound { id })` | `{"error": {"not_found": {"id": "..."}}}` |

実際の MessagePack バイトは `rmp-serde` 最適化（`fixmap` / `map16` / `bin8` 等）で多少差が出るため、**バイト列レベルの固定はしない**。型 round-trip 検証で十分（`./index.md §設計判断 12`）。

## テスト観点（テスト設計担当向け）

以下はテスト設計担当（涅マユリ）への引き継ぎメモ。テスト設計書は `test-design/` で作成される。

**ユニットテスト（型定義由来、`crates/shikomi-core/src/ipc/` 末尾の `#[cfg(test)] mod tests`）**:

- `IpcProtocolVersion::current()` が `V1` を返すこと
- `IpcProtocolVersion::Display` が `"v1"` を返すこと
- `IpcRequest::variant_name()` の全バリアント網羅
- `IpcResponse::variant_name()` の全バリアント網羅
- `RecordSummary::from_record(&record)` が Secret kind で `value_preview = None` / `value_masked = true` を返すこと
- `RecordSummary::from_record(&record)` が Text kind で `value_preview = Some(<先頭 40 char>)` / `value_masked = false` を返すこと
- `SerializableSecretBytes` の `Debug` が `"SerializableSecretBytes([REDACTED])"` 固定であること
- `IpcErrorCode::Display` の各バリアント出力検証

**結合テスト（MessagePack round-trip、`crates/shikomi-daemon/tests/ipc_protocol.rs`）**:

- `IpcRequest` 全 5 バリアントの `rmp_serde::to_vec` → `from_slice` ラウンドトリップ同値
- `IpcResponse` 全 7 バリアントのラウンドトリップ
- `SerializableSecretBytes` のラウンドトリップ（バイト列が一致 + secret 露出なし）
- `RecordSummary` のラウンドトリップ
- `IpcErrorCode` 全 6 バリアントのラウンドトリップ
- `IpcProtocolVersion` の wire 表現確認（`"v1"` で送受信）

**CI 補助テスト**:
- `crates/shikomi-core/src/ipc/` 配下で `expose_secret` 0 件 grep（**TC-CI-016** 相当）
- `crates/shikomi-core/src/ipc/` 配下で `Raw` / `RawRef` 0 件 grep（**TC-CI-019** 相当）
- `crates/shikomi-core/src/ipc/` 配下で `tokio::` / `rmp_serde::` import 0 件 grep（純粋性監査、**TC-CI-024** 相当、テスト設計担当が確定）

テストケース番号の割当は `test-design/` が担当する。本書は設計側からの**要件の明示**に留める。

---

## Sub-E (#43) IPC V2 拡張（横断的変更、`vault-encryption` feature 双方向同期）

<!-- Boy Scout Rule (Sub-E / 工程2): vault-encryption feature の REQ-S09〜S12（VEK キャッシュ + IPC V2 拡張）に対応する protocol-types 拡張を本セクションで確定する。
     vault-encryption/detailed-design/vek-cache-and-ipc.md と双方向参照、SSoT は本ファイル（daemon-ipc）。
     全て `#[non_exhaustive]` 維持で V1 クライアント非破壊。 -->

### `IpcProtocolVersion` 拡張

- 既存 `V1` を維持、新規 `V2` を追加（`#[non_exhaustive]` enum、V1 クライアントは非破壊）
- `pub const fn current() -> Self` の戻り値を **`IpcProtocolVersion::V2`** に更新（Sub-E 完了時点の最新を返す、Sub-A の SSoT 集約原則）
- handshake で V1 クライアントが接続した場合、サーバは `V1` サブセット（`Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord`）のみ受理。V2 専用 variant 送信時は `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` で MSG-S15 に変換（`vault-encryption/requirements.md` MSG-S15 と整合）

### `IpcRequest` V2 新 variant（5 件）

| variant | フィールド | 用途 | 対応 `VaultMigration` メソッド |
|---|---|---|---|
| `Unlock { master_password: SerializableSecretBytes, recovery: Option<RecoveryMnemonicWords> }` | パスワード（必須）+ リカバリ（optional、`--recovery` 経路用 24 語） | vault unlock 要求 | `unlock_with_password` / `unlock_with_recovery` |
| `Lock` | フィールドなし | 明示 vault lock 要求 | （Sub-E `VekCache::lock` 直接呼出、Sub-D 経由しない） |
| `ChangePassword { old: SerializableSecretBytes, new: SerializableSecretBytes }` | 旧 + 新パスワード | パスワード変更 O(1) | `change_password` |
| `RotateRecovery { master_password: SerializableSecretBytes }` | パスワード再認証 | リカバリ 24 語ローテーション | （Sub-E 新規実装、Sub-D 範囲外） |
| `Rekey { master_password: SerializableSecretBytes }` | パスワード再認証 | VEK 入替 + 全レコード再暗号化 | `rekey` |

### `IpcResponse` V2 新 variant（5 件）

| variant | フィールド | 用途 |
|---|---|---|
| `Unlocked` | フィールドなし | unlock 成功（VEK 自体は IPC で返さず daemon 内キャッシュのみ） |
| `Locked` | フィールドなし | lock 完了 |
| `PasswordChanged` | フィールドなし | change_password 完了（VEK 不変、再 unlock 不要、daemon キャッシュ維持） |
| `RecoveryRotated { disclosure: RecoveryWordsDisclosure }` | 新 24 語（`RecoveryDisclosure::disclose` 経路で初回 1 度のみ）| RotateRecovery 完了。`RecoveryWordsDisclosure` は IPC 用 `Vec<String>` newtype、受信側で即 Drop（zeroize）|
| `Rekeyed { records_count: u64 }` | 再暗号化レコード数 | rekey 完了 |

### `IpcErrorCode` V2 新 variant（4 件）

| variant | 用途 | MSG マッピング |
|---|---|---|
| `VaultLocked` | `Locked` 状態で read/write IPC 受信、Sub-E `VekCache` の型レベル拒否（契約 C-22） | MSG-S09 (c) キャッシュ揮発 |
| `BackoffActive { wait_secs: u32 }` | 連続失敗 5 回後の指数バックオフ中（契約 C-26、`wait_secs` のみユーザに見せる、`failures` は隠蔽） | MSG-S09 (a) パスワード違い + 待機時間 |
| `RecoveryRequired` | Sub-D `MigrationError::RecoveryRequired` 透過、パスワード経路失敗時のリカバリ誘導（契約 C-27、Sub-D Rev5 ペガサス指摘契約の実装）| **MSG-S09 (a) リカバリ経路 (`vault unlock --recovery`) も可能 案内** |
| `ProtocolDowngrade` | V1 クライアントが V2 専用 variant を送信した時に拒否（契約 C-28）| MSG-S15 |

### `MigrationError → IpcErrorCode` マッピング（Sub-E 確定責務）

`vault-encryption/detailed-design/vek-cache-and-ipc.md` §`MigrationError → IpcError` マッピング表 と整合。9 → 4 集約で **内部詳細を秘匿**しつつ MSG マッピング 1:1 確定。

### Wire-format 例（V2 拡張）

| Rust 値 | MessagePack JSON 表現例 |
|---|---|
| `IpcRequest::Unlock { master_password: <secret>, recovery: None }` | `{"unlock": {"master_password": <bytes>, "recovery": null}}` |
| `IpcRequest::Lock` | `{"lock": null}` |
| `IpcRequest::ChangePassword { old: <secret>, new: <secret> }` | `{"change_password": {"old": <bytes>, "new": <bytes>}}` |
| `IpcResponse::Unlocked` | `{"unlocked": null}` |
| `IpcResponse::Rekeyed { records_count: 42 }` | `{"rekeyed": {"records_count": 42}}` |
| `IpcResponse::Error(IpcErrorCode::VaultLocked)` | `{"error": {"vault_locked": null}}` |
| `IpcResponse::Error(IpcErrorCode::BackoffActive { wait_secs: 30 })` | `{"error": {"backoff_active": {"wait_secs": 30}}}` |
| `IpcResponse::Error(IpcErrorCode::RecoveryRequired)` | `{"error": {"recovery_required": null}}` |
| `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` | `{"error": {"protocol_downgrade": null}}` |

### Sub-E PR レビュー必須確認

1. **`#[non_exhaustive]` 維持**: 既存 V1 クライアントが V2 ビルドの daemon と通信する経路で `serde` deserialize が `unknown variant` で正常に失敗、daemon 側で `IpcResponse::Error(ProtocolDowngrade)` 応答（V1 サブセットのみ受理）
2. **`SerializableSecretBytes` 利用**: 全 V2 variant でパスワード / リカバリ等の秘密値は `SerializableSecretBytes` 経由（`expose_secret` 0 件 grep gate 維持、TC-CI-016）
3. **`RecoveryWordsDisclosure` の所有権モデル**: IPC 応答に乗せる時点で daemon 側は所有権を放棄、CLI/GUI 側で受信後即 Drop（zeroize）。daemon ログには記録しない（C-19 同型契約の IPC 経路実装）


