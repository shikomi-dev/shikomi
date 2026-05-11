# 詳細設計書 — domain（daemon-hotkey-clipboard）

<!-- feature: daemon-hotkey-clipboard / sub-feature: domain / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/domain/detailed-design.md -->
<!-- 疑似コード・実装コードブロック禁止 -->

## 1. `Hotkey` 型の公開 API 仕様

### 1.1 `Hotkey` struct

| 要素 | 型 | 可視性 | 説明 |
|------|----|--------|------|
| `modifiers` | `ModifierSet` | `pub(crate)` | Ctrl / Alt / Shift / Meta フラグ集合 |
| `key` | `Key` | `pub(crate)` | 主キー（英数字または Fn キー） |
| `normalized` | `Box<str>` | `private` | `Display` 用キャッシュ（一度計算して固定） |

### 1.2 `Hotkey::parse(s: &str) -> Result<Hotkey, HotkeyParseError>`

| ステップ | 処理 |
|---------|------|
| 1 | `s` を `'+'` で split（最大 5 パーツ） |
| 2 | 各パーツを lowercase に変換 |
| 3 | 修飾キー候補（`ctrl` / `alt` / `shift` / `meta`）を分類し `ModifierSet` を構築 |
| 4 | 残り 1 パーツが主キー候補。英数字 1 文字 → `Key::Char(c)`、`f1`〜`f12` → `Key::Function(n)` |
| 5 | 修飾キーが 0 個 → `HotkeyParseError::NoModifier` |
| 6 | 主キーが 0 個または 2 個以上 → `HotkeyParseError::InvalidKey` |
| 7 | 正規化文字列を `modifiers`（アルファベット順: alt → ctrl → meta → shift）+ `+` + `key` で構築し `normalized` フィールドに格納 |

**正規化ルール**: `"Ctrl+Alt+1"` / `"alt+ctrl+1"` は同一 `Hotkey` になる。`PartialEq` / `Hash` は `normalized` で比較。

### 1.3 `ModifierSet` struct

| フィールド | 型 | 説明 |
|-----------|----|------|
| `alt` | `bool` | Alt / Option キー |
| `ctrl` | `bool` | Ctrl / Control キー |
| `meta` | `bool` | Win / Command キー |
| `shift` | `bool` | Shift キー |

不変条件: `alt || ctrl || meta || shift` が `true`（コンストラクタでアサーション）。

### 1.4 `Key` enum

| バリアント | 内容 |
|-----------|------|
| `Char(char)` | ASCII 英数字 (`a`〜`z`, `0`〜`9`)。小文字に正規化 |
| `Function(u8)` | Fn キー番号 1〜12 |

### 1.5 `HotkeyParseError` enum（`thiserror` 使用）

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

## 5. `serde` 互換性

- `Hotkey` は `serde::Serialize` / `Deserialize` を実装する
- シリアライズ表現は **正規化文字列** (`"alt+ctrl+1"` 形式)
- `Deserialize` 実装内で `Hotkey::parse` を呼び出し、パースエラーを `serde::de::Error::custom` に写像する

## 6. 依存関係

本 sub-feature で `shikomi-core` に追加する外部依存: **なし**。

既存の `thiserror` / `serde` / `time` で全要件を満たす。
