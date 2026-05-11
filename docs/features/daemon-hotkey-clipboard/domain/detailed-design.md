# 詳細設計書 — domain（daemon-hotkey-clipboard）

<!-- feature: daemon-hotkey-clipboard / sub-feature: domain / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/domain/detailed-design.md -->
<!-- 疑似コード・実装コードブロック禁止 -->

## 1. `Hotkey` 型の公開 API 仕様

### 1.1 `Hotkey` struct

`Hotkey` は**正規化文字列のみを内部状態として持つ**。`modifiers: ModifierSet` / `key: Key` の個別フィールド保持を廃止する。

| 要素 | 型 | 可視性 | 説明 |
|------|----|--------|------|
| `normalized` | `Box<str>` | `private` | `"alt+ctrl+1"` 形式の正規化文字列（唯一の内部状態）|

**廃止判断根拠**: `modifiers` / `key` を個別保持すると `normalized` との同期バグが潜在する（DRY 違反 / Tell Don't Ask 崩壊）。`PartialEq` / `Hash` / `Display` は `normalized` のみで完結する。個別フィールドへの外部アクセスを `pub(crate)` で許容することは Tell Don't Ask 原則に違反する。**操作はすべて `Hotkey` のメソッドで閉じること**。

公開メソッド:
- `parse(s: &str) -> Result<Hotkey, HotkeyParseError>` — 文字列をパースし正規化して構築
- `as_str(&self) -> &str` — 正規化文字列を返す
- `Display` — `as_str()` と同一
- `PartialEq` / `Eq` / `Hash` — `normalized` 文字列で比較

### 1.2 `Hotkey::parse(s: &str) -> Result<Hotkey, HotkeyParseError>`

| ステップ | 処理 |
|---------|------|
| 1 | `s` を `'+'` で split（最大 5 パーツ） |
| 2 | 各パーツを lowercase に変換 |
| 3 | 修飾キー候補（`ctrl` / `alt` / `shift` / `meta`）を分類し `ModifierSet` を構築 |
| 4 | 残り 1 パーツが主キー候補。英数字 1 文字 → `Key::Char(c)`、`f1`〜`f12` → `Key::Function(n)` |
| 5 | 修飾キーが 0 個 → `HotkeyParseError::NoModifier` |
| 6 | 主キーが 0 個または 2 個以上 → `HotkeyParseError::InvalidKey` |
| 7 | 正規化文字列をアルファベット順修飾キー（alt → ctrl → meta → shift）+ `+` + 主キーで構築し、`Hotkey { normalized }` を返す（`normalized` が唯一のフィールド） |

**正規化ルール**: `"Ctrl+Alt+1"` / `"alt+ctrl+1"` は同一 `Hotkey` になる。`PartialEq` / `Hash` は `normalized` で比較。

### 1.3 パース内部アルゴリズム（中間表現）

`parse` の内部でのみ使用する中間表現として `alt: bool, ctrl: bool, meta: bool, shift: bool, key_char: Option<char>, key_fn: Option<u8>` のローカル変数を用いる。パース完了後に正規化文字列を構築し `Hotkey { normalized }` を返す。**中間表現は struct フィールドに昇格させない**。

### 1.4 `HotkeyParseError` enum（`thiserror` 使用）

| バリアント | メッセージ例 |
|-----------|------------|
| `NoModifier` | `"hotkey must include at least one modifier (ctrl, alt, shift, meta)"` |
| `InvalidKey { raw: String }` | `"invalid key: '{raw}'. expected a-z, 0-9, or f1-f12"` |
| `Empty` | `"hotkey string is empty"` |
| `TooManyParts` | `"too many '+'-separated parts (max 5)"` |

## 2. `Vault` 追加メソッド詳細

### 2.1 `assign_hotkey`

処理順序:
1. `records` から `id` のレコードを `find_mut` → `RecordNotFound` で Fail Fast
2. `records.iter().filter(|r| r.id != id)` で競合チェック → `r.hotkey == Some(hotkey)` ならば `HotkeyConflict { assigned_to: r.id }` を返す
3. 対象レコードの `hotkey` を `Some(hotkey)` に更新
4. `updated_at` を現在時刻で更新（既存 `edit` メソッドと同じパターン）
5. `Ok(())`

**前提の明示**: `hotkey` パラメータは `Hotkey` 型（parse 済み）。`assign_hotkey` 内で parse はしない（single responsibility）。

### 2.2 `find_by_hotkey`

- `hotkey_entries()` の線形探索で `r.hotkey.as_ref() == Some(hotkey)` を検索
- `Hotkey::PartialEq` は `normalized` 文字列比較なので衝突なし
- 結果は `Option<&Record>`（借用）

## 3. IPC スキーマ詳細

### 3.1 `IpcRequest::AddRecord` 拡張後フィールド一覧

| フィールド | 型 | 変更 | 説明 |
|-----------|----|------|------|
| `label` | `String` | 既存 | レコードラベル |
| `value` | `SerializableSecretBytes` | 既存 | ペイロード値 |
| `secret` | `bool` | 既存 | 機密フラグ |
| `hotkey` | `Option<String>` | **追加** | ホットキー文字列。`None` = ホットキーなし |

### 3.2 `IpcRequest::EditRecord` 拡張後フィールド一覧

| フィールド | 型 | 変更 | 説明 |
|-----------|----|------|------|
| `id` | `RecordId` | 既存 | 対象レコード ID |
| `label` | `Option<String>` | 既存 | 変更後ラベル |
| `value` | `Option<SerializableSecretBytes>` | 既存 | 変更後値 |
| `hotkey` | `Option<String>` | **追加** | 変更後ホットキー文字列 |
| `clear_hotkey` | `bool` | **追加** | `true` でホットキー解除（`hotkey` フィールドより優先） |

`hotkey` と `clear_hotkey` の優先順位: `clear_hotkey == true` の場合は `hotkey` 値を無視してクリア。両方指定は `HotkeyParseError` 相当のエラーで弾く。

### 3.3 `RecordSummary` 拡張後フィールド一覧

| フィールド | 型 | 変更 | 説明 |
|-----------|----|------|------|
| `id` | `RecordId` | 既存 | |
| `label` | `String` | 既存 | |
| `kind` | `RecordKind` | 既存 | |
| `hotkey` | `Option<String>` | **追加** | 正規化済みホットキー文字列（`None` = なし） |

## 4. `VaultVersion` マイグレーション定義

| バージョン | 変更内容 |
|-----------|---------|
| `V1` | 初期スキーマ（vault-persistence feature） |
| `V2` | 暗号化ヘッダ追加（vault-encryption feature） |
| `V3` | **本 feature**: `records` テーブルに `hotkey_combo TEXT DEFAULT NULL UNIQUE` カラム追加 |

`VaultVersion::V3` を `shikomi-core::vault::version` に追加し、`shikomi-infra::persistence::vault_migration` に `V2→V3` マイグレーション処理を実装する。

マイグレーション SQL:
```sql
ALTER TABLE records ADD COLUMN hotkey_combo TEXT DEFAULT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_records_hotkey_combo
  ON records (hotkey_combo)
  WHERE hotkey_combo IS NOT NULL;
```

（SQL は `schema.rs` 内定数として管理。`detailed-design.md` での記述は仕様確認用。実装は `shikomi-infra` 側に委ねる）

## 5. V3 マイグレーション失敗時のロールバック戦略

`V2 → V3` マイグレーション（`hotkey_combo` カラム追加）は `BEGIN TRANSACTION` / `COMMIT` / `ROLLBACK` で原子性を保証する。

| ステップ | 処理 | 失敗時 |
|---------|------|--------|
| 1 | `BEGIN TRANSACTION` | — |
| 2 | `ALTER TABLE records ADD COLUMN hotkey_combo TEXT DEFAULT NULL` | `ROLLBACK` → `VaultMigrationError::SchemaChange` で Fail Fast |
| 3 | `CREATE UNIQUE INDEX ...` | `ROLLBACK` → `VaultMigrationError::SchemaChange` |
| 4 | vault header の `VaultVersion` を `V3` に更新 | `ROLLBACK` → `VaultMigrationError::HeaderUpdate` |
| 5 | `COMMIT` | 失敗なら SQLite が自動 rollback |

**SQLite の `ALTER TABLE` はトランザクション内でも ROLLBACK 可能**（DDL implicit commit はない）。マイグレーション失敗後の vault.db は `V2` 状態のまま保持される。daemon は起動を中止し `tracing::error!` でユーザに通知する（Fail Fast）。

既存データの `UNIQUE INDEX` 競合: `hotkey_combo` はデフォルト `NULL` で追加されるため、既存全レコードは `NULL` → 競合しない（`UNIQUE INDEX WHERE hotkey_combo IS NOT NULL` で NULL を除外）。

## 6. `serde` 互換性

- `Hotkey` は `serde::Serialize` / `Deserialize` を実装する
- シリアライズ表現は **正規化文字列** (`"alt+ctrl+1"` 形式)
- `Deserialize` 実装内で `Hotkey::parse` を呼び出し、パースエラーを `serde::de::Error::custom` に写像する

## 7. 依存関係

本 sub-feature で `shikomi-core` に追加する外部依存: **なし**。

既存の `thiserror` / `serde` / `time` で全要件を満たす。
