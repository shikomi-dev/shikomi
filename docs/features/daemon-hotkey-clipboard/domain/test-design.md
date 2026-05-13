# テスト設計書 — domain（daemon-hotkey-clipboard）

<!-- feature: daemon-hotkey-clipboard / sub-feature: domain / Issue #89 -->
<!-- 配置先: docs/features/daemon-hotkey-clipboard/domain/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->

## 0. テスト方針参照

本テスト設計書は **`config/prompts/test_strategy.md`** に定めるテスト戦略（テストレベル定義・ダブル方針・CI ワークフロー対応方針）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは `system-test-design.md` に委ねる。

## 1. 外部 I/O 依存マップ

| テストレベル | 外部 I/O | 依存対象 | 対処 |
|------------|---------|---------|------|
| UT | なし | `Hotkey::parse` は純粋関数、I/O なし | そのまま実行可 |
| UT (`Vault` メソッド) | なし | `Vault` は in-memory ドメインオブジェクト | そのまま実行可 |
| IT (`IpcRequest` serde) | `rmp-serde` ライブラリ（ファイル/ネットワーク I/O なし）| ライブラリ呼び出しのみ | そのまま実行可 |
| IT (V3 マイグレーション) | SQLite ファイル I/O | `tempfile::TempDir` で分離 | `tempfile` を使用 |

`shikomi-core` は no-I/O 制約（`process-model.md §4.1.1` 上位設計ルール）を持つ。UT / IT ともにネットワーク・OS API・ファイルシステムへの依存を持たない。

## 2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| ユニットテスト (UT) | `crates/shikomi-core/src/vault/record/hotkey.rs` 内 `#[cfg(test)]` | `cargo test -p shikomi-core` |
| ユニットテスト (UT) | `crates/shikomi-core/src/vault/tests.rs` | `cargo test -p shikomi-core` |
| 結合テスト (IT) | `crates/shikomi-core/tests/hotkey_lifecycle.rs` | `cargo test -p shikomi-core` |

## 3. ユニットテスト一覧

### TC-HD-U01: `Hotkey::parse` 正常系

| ID | 入力 | 期待結果 |
|----|------|---------|
| TC-HD-U01-a | `"ctrl+alt+1"` | `as_str() == "alt+ctrl+1"`（正規化済み文字列で検証） |
| TC-HD-U01-b | `"Ctrl+Alt+1"` | TC-HD-U01-a と同一（大文字無視） |
| TC-HD-U01-c | `"alt+ctrl+1"` | TC-HD-U01-a と同一（順序無視） |
| TC-HD-U01-d | `"meta+shift+f12"` | `as_str() == "meta+shift+f12"`（正規化済み文字列で検証） |
| TC-HD-U01-e | `"ctrl+a"` | `as_str() == "ctrl+a"` |

### TC-HD-U02: `Hotkey::parse` 異常系

| ID | 入力 | 期待エラー |
|----|------|----------|
| TC-HD-U02-a | `""` | `HotkeyParseError::Empty` |
| TC-HD-U02-b | `"1"` | `HotkeyParseError::NoModifier` |
| TC-HD-U02-c | `"ctrl+alt+1+2"` | `HotkeyParseError::InvalidKey` |
| TC-HD-U02-d | `"ctrl+alt+f0"` | `HotkeyParseError::InvalidKey` （F0 は無効） |
| TC-HD-U02-e | `"ctrl+alt+f13"` | `HotkeyParseError::InvalidKey` （F13 は無効） |
| TC-HD-U02-f | `"ctrl+alt+!"` | `HotkeyParseError::InvalidKey` （特殊文字） |
| TC-HD-U02-g | 5 パーツ超 | `HotkeyParseError::TooManyParts` |

### TC-HD-U03: `Hotkey` 正規化と等価性

| ID | 説明 |
|----|------|
| TC-HD-U03-a | `parse("ctrl+alt+1") == parse("alt+ctrl+1")` が `true` |
| TC-HD-U03-b | `parse("ctrl+alt+1").to_string() == "alt+ctrl+1"` （アルファベット順正規化） |
| TC-HD-U03-c | `parse("ctrl+alt+1") != parse("ctrl+alt+2")` が `true` |

### TC-HD-U04: `Vault::assign_hotkey`

| ID | 説明 | 期待結果 |
|----|------|---------|
| TC-HD-U04-a | 既存エントリに新規ホットキーを割り当て | `Ok(())` / エントリの `hotkey` が `Some(Hotkey)` に更新 |
| TC-HD-U04-b | 別エントリが同一ホットキー保持中に割り当て | `Err(HotkeyConflict { assigned_to: ... })` |
| TC-HD-U04-c | 存在しない RecordId に割り当て | `Err(RecordNotFound)` |
| TC-HD-U04-d | 自エントリと同一ホットキーで上書き | `Ok(())` （競合なし） |

### TC-HD-U05: `Vault::clear_hotkey`

| ID | 説明 | 期待結果 |
|----|------|---------|
| TC-HD-U05-a | ホットキー付きエントリのクリア | `Ok(())` / `hotkey` が `None` |
| TC-HD-U05-b | ホットキーなしエントリのクリア | `Ok(())` （冪等） |
| TC-HD-U05-c | 存在しない ID | `Err(RecordNotFound)` |

### TC-HD-U06: `Vault::find_by_hotkey`

| ID | 説明 | 期待結果 |
|----|------|---------|
| TC-HD-U06-a | 登録済みホットキーで検索 | `Some(&Record)` |
| TC-HD-U06-b | 未登録ホットキーで検索 | `None` |

### TC-HD-U07: `Hotkey` の `serde` ラウンドトリップ

| ID | 説明 |
|----|------|
| TC-HD-U07-a | `serde_json::to_string(hotkey)` → `serde_json::from_str` が元の値と一致 |
| TC-HD-U07-b | 不正文字列の `from_str` が `serde` エラーを返す |

## 4. 結合テスト一覧

### TC-HD-I01: `IpcRequest::AddRecord` ホットキーフィールドの serde ラウンドトリップ

`IpcRequest::AddRecord { hotkey: Some("ctrl+alt+1"), ... }` を `rmp_serde` でシリアライズ → デシリアライズして元の値と一致することを確認。

### TC-HD-I02: `RecordSummary` ホットキーフィールド伝播

`Vault` から `RecordSummary` への変換で `hotkey` フィールドが正しく伝播することを確認。

### TC-HD-I03: `VaultVersion::V3` マイグレーション

`V2` フォーマットの vault.db を `V3` にアップグレードし、`hotkey_combo` カラムが追加され既存レコードの値が保持されることを確認。（`shikomi-infra` 結合テストとして配置）

## 5. プロパティテスト

| ID | 対象 | 不変条件 |
|----|------|---------|
| TC-HD-P01 | `Hotkey::parse` | 有効入力を parse → `to_string` → 再 parse が同一 `Hotkey` を返す |
| TC-HD-P02 | `Vault::assign_hotkey` | 重複なく登録した場合、`find_by_hotkey` が必ず `Some` を返す |

`proptest` crate で実装（既存 `aead_property.rs` と同パターン）。
