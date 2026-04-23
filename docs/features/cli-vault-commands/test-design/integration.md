# テスト設計書 — cli-vault-commands / 結合テスト

> `index.md` の §2 索引からの分割ファイル。UseCase 単位の結合テスト（実 SQLite + `tempfile`）を扱う。

## 1. 設計方針

- **テスト対象**: `usecase::list::list_records`、`usecase::add::add_record`、`usecase::edit::edit_record`、`usecase::remove::remove_record` の 4 関数
- **エントリポイント**: 各 UseCase 関数を直接呼ぶ（CLI バイナリは経由しない、clap パースもしない）
- **DB は実接続**: テスト戦略ガイド準拠で `SqliteVaultRepository::from_directory(tempdir.path())` を実物として渡す。**モック `VaultRepository` は使わない**
- **検証スタイル**: 契約検証。戻り値の型・`CliError` のバリアント・`save()` 後の状態を別エンドポイント（`load()` か `list_records`）で**ラウンドトリップ**確認
- **UseCase 入力型の変更（ペテルギウス review 対応）**:
  - `list_records(repo: &dyn VaultRepository) -> Result<Vec<RecordView>, CliError>`（`ListInput` 削除）
  - `add_record(repo: &dyn VaultRepository, input: AddInput, now: OffsetDateTime) -> Result<RecordId, CliError>`
  - `edit_record(repo: &dyn VaultRepository, input: EditInput, now: OffsetDateTime) -> Result<RecordId, CliError>`（`EditInput` から `kind` 削除、Phase 1 スコープ外）
  - `remove_record(repo: &dyn VaultRepository, input: ConfirmedRemoveInput) -> Result<RecordId, CliError>`（`bool` フィールド撤廃、**型の存在自体が確認経由を意味**する）

## 2. 呼び出し経路（`[lib] + [[bin]]` 採用）

`shikomi-cli` に `[lib]` を追加、`src/lib.rs` に `#[doc(hidden)] pub mod usecase; #[doc(hidden)] pub mod presenter; ...` を配置（詳細設計 §public-api.md 採用案 A、ペテルギウス指摘 ③ 解決）。結合テストは `use shikomi_cli::usecase::*;` で UseCase を import して直接呼ぶ。

`lib.rs` の冒頭に `//! Internal API. Not stable; subject to change without notice. `#[doc(hidden)]` forbids downstream use.` を明示。

---

## 3. I/O 物理化（共通セットアップ）

```rust
// tests/common/mod.rs の想定
fn fresh_repo() -> (TempDir, SqliteVaultRepository) {
    let dir = TempDir::new().unwrap();
    let repo = SqliteVaultRepository::from_directory(dir.path()).unwrap();
    (dir, repo)  // dir は caller が Drop まで保持
}
```

- 各テストで `TempDir::new()` により独立した vault ディレクトリを生成
- `SqliteVaultRepository::from_directory(&Path)` を**直接呼ぶ**（ペテルギウス指摘 ⑦ 対応、`VaultPaths` を介さない）
- `new()` は呼ばない — env var の影響を排除するため
- テスト終了時に `TempDir` の `Drop` で自動クリーンアップ
- 並列実行は cargo のデフォルトに任せる（各テストが独立 `TempDir` のため衝突なし）
- 時刻: `OffsetDateTime::UNIX_EPOCH + Duration::hours(N)` の固定値を注入（ユニット相当の決定性を保つ）

---

## 4. テストケース一覧

### 4.1 `list_records`

| TC-ID | 種別 | 入力 / 操作 | 期待結果 |
|-------|------|-----------|---------|
| TC-IT-001 | 正常系 | 空 vault（`exists()=true` だが record 0 件）→ 呼び出し | `Ok(Vec::new())` |
| TC-IT-002 | 正常系 | 3 件 mixed kind（Text × 2, Secret × 1）の vault | `Ok(Vec<RecordView>)` 長さ 3、Secret は `ValueView::Masked`、Text は `ValueView::Plain(..)` |
| TC-IT-003 | 異常系 | `exists()=false` の vault（`vault.db` 不在の tempdir） | `Err(CliError::VaultNotInitialized(_))` |

### 4.2 `add_record`

| TC-ID | 種別 | 入力 / 操作 | 期待結果 |
|-------|------|-----------|---------|
| TC-IT-010 | 正常系 | vault 未作成 + Text 入力 `AddInput { kind: Text, label: "L", value: "V" }` | `Ok(RecordId)`、続く `list_records(&repo)` で 1 件存在、取得した `RecordView::value` が `Plain("V")`（Text kind のラウンドトリップ） |
| TC-IT-011 | 正常系（セキュリティ） | Secret 入力 `AddInput { kind: Secret, label: "S", value: SecretString::from_string("SECRET_TEST_VALUE") }` | `Ok(RecordId)`、`list_records` で 1 件、`RecordView::value == ValueView::Masked`。加えて `format!("{:?}", record.payload())` が `"[REDACTED]"` を含み `SECRET_TEST_VALUE` を含まない |
| TC-IT-012 | 異常系 | 暗号化 vault フィクスチャ（`create_encrypted_vault` ヘルパー経由） | `Err(CliError::EncryptionUnsupported)` |
| TC-IT-013 | 異常系 | 不正ラベル（空文字） | `Err(CliError::InvalidLabel(_))` |

### 4.3 `edit_record`

| TC-ID | 種別 | 入力 / 操作 | 期待結果 |
|-------|------|-----------|---------|
| TC-IT-020 | 正常系 | 既存 1 件 + `EditInput { id, label: Some(_), value: None }` | `Ok(RecordId)`、`list_records` で当該レコードの label のみ更新、value 不変、`updated_at` が注入した `now` と一致 |
| TC-IT-021 | 正常系 | 既存 1 件 + `EditInput { id, label: Some(_), value: Some(_) }` | `Ok(RecordId)`、両フィールド更新 |
| TC-IT-022 | 異常系 | 存在しない id | `Err(CliError::RecordNotFound(_))` |
| TC-IT-023 | 異常系 | 暗号化 vault フィクスチャ | `Err(CliError::EncryptionUnsupported)` |
| TC-IT-024 | 異常系 | `EditInput { id, label: None, value: None }`（全 None） | `Err(CliError::UsageError(_))`（「少なくとも 1 つ必要」） |

**注記**: `EditInput` に `kind` フィールドは存在しない（Phase 1 スコープ外）。requirements.md REQ-CLI-003 / 詳細設計 §data-structures.md で削除済み。E2E TC-E2E-025 で clap レベルの拒否を別途検証（本結合テスト対象外）。

### 4.4 `remove_record`

| TC-ID | 種別 | 入力 / 操作 | 期待結果 |
|-------|------|-----------|---------|
| TC-IT-030 | 正常系 | 既存 1 件 + `ConfirmedRemoveInput::new(id)` | `Ok(RecordId)`、`list_records` で record 消失 |
| TC-IT-031 | 異常系 | 存在しない id + `ConfirmedRemoveInput::new(nonexistent_id)` | `Err(CliError::RecordNotFound(_))` |
| TC-IT-032 | 設計契約 | **コンパイル時検証**: `ConfirmedRemoveInput` に `bool` フィールドを渡そうとするコードが compile error になる doc-test（`unit.md` TC-UT-110 で実装） | — |
| TC-IT-033 | 異常系 | 暗号化 vault + 既存 id + `ConfirmedRemoveInput::new(id)` | `Err(CliError::EncryptionUnsupported)` |

**ペテルギウス指摘 ⑤ の反映**: 旧 `RemoveInput { id, confirmed: bool }` → 新 `ConfirmedRemoveInput { id }`。`bool` フィールド撤廃により、**型の存在自体が「確認経由」を意味**する（Parse, don't validate）。したがって旧 TC-IT-032（`confirmed=false` で debug panic）は**削除**。型で表現可能な事前条件を `bool` で持たせる設計は廃止された。

### 4.5 UseCase 横断（パラメタライズ）

| TC-ID | 種別 | 入力 / 操作 | 期待結果 |
|-------|------|-----------|---------|
| TC-IT-040 | 異常系（パラメタライズ） | 暗号化 vault フィクスチャに対して `list_records` / `add_record` / `edit_record` / `remove_record` の 4 UseCase 全てを実行 | 全て `Err(CliError::EncryptionUnsupported)` を返す。vault 内容は変更されない（`vault.db` のファイルハッシュが変わっていないことを assert） |
| TC-IT-050 | 異常系（パラメタライズ） | `exists()=false` の空 tempdir に対して `list_records` / `edit_record` / `remove_record` を実行（`add_record` は自動初期化するため除外） | 全て `Err(CliError::VaultNotInitialized(_))` |

---

## 5. 暗号化 vault フィクスチャヘルパー

`tests/common/fixtures.rs` に配置:

```rust
// 想定シグネチャ
pub fn create_encrypted_vault(dir: &Path) -> Result<(), anyhow::Error>;
```

**実装方針**（`unit.md §引き継ぎ §10.3` と対応）:
- `shikomi-infra` 側に **test-only API**（`#[cfg(any(test, feature = "test-fixtures"))]`）として `VaultHeader::new_encrypted_for_test(...)` を追加し、`vault.db` の SQLite を生成するヘルパーを作る
- 本 feature のテストで `dev-dependencies.shikomi-infra = { path = "...", features = ["test-fixtures"] }` として有効化
- これにより `tests/fixtures/vault_encrypted.db` をバイナリコミットせず、テスト実行時に毎回生成

**未対応時のフォールバック**: `shikomi-infra` の暗号化書き出し API がそもそも未実装の場合、TC-E2E-040 / 041 / TC-IT-012 / 023 / 033 / 040 を `#[ignore]` フォールバック（Phase 2 で実装）。本テスト設計はヘルパー有り前提で書いているが、無ければリーダーに起票を要請する（`unit.md §10.3`）。

---

## 6. 結合テストでの時刻注入と決定性

- UseCase は `now: OffsetDateTime` を引数で受ける（詳細設計 §public-api.md）ため、テスト側で固定時刻を注入可能
- 例: `let now = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);` を `add_record(repo, input, now)` に渡し、`list_records` 後の `RecordView` で `updated_at == now` を assert
- **`SystemTime::now()` に依存するテストは書かない**（flaky の温床）

---

## 7. カバレッジ対象

本結合テストレイヤでカバーする対応受入基準と REQ:

| 受入基準 | カバー TC |
|---------|----------|
| 1（`list`） | TC-IT-001, TC-IT-002, TC-IT-003 |
| 2, 3（`add`） | TC-IT-010, TC-IT-011, TC-IT-013 |
| 5（`edit`） | TC-IT-020, TC-IT-021, TC-IT-022, TC-IT-024 |
| 6, 7（`remove`） | TC-IT-030, TC-IT-031 |
| 8（暗号化 Fail Fast） | TC-IT-012, TC-IT-023, TC-IT-033, TC-IT-040 |
| 9（vault 未初期化） | TC-IT-003, TC-IT-050 |
| 12（Clean Arch 縦串検証） | TC-IT-030（`ConfirmedRemoveInput` 経由で UseCase → Repository の型契約確認） |

---

## 8. 結合テストファイル構成

```
crates/shikomi-cli/tests/
├── common/
│   ├── mod.rs               # fresh_repo(), fixed_time() ヘルパー
│   └── fixtures.rs          # create_encrypted_vault()
├── it_usecase_list.rs       # TC-IT-001〜003
├── it_usecase_add.rs        # TC-IT-010〜013
├── it_usecase_edit.rs       # TC-IT-020〜024
├── it_usecase_remove.rs     # TC-IT-030, 031, 033
└── it_usecase_cross.rs      # TC-IT-040, 050（横断パラメタライズ）
```

各ファイルの docstring に対応 REQ-ID と Issue 番号を書く（テスト戦略ガイド準拠）。

---

## 9. 想定外の挙動の取り扱い

バグ発見時は `index.md §6 モック方針` の方針ではなく、**バグレポートを作成**する:

- ファイル名・該当箇所（行番号）
- 期待される動作（本 TC 設計書の期待結果欄）と実際の動作
- 再現手順（`cargo test --test it_usecase_xxx -- TC_NAME`）

バグレポートは `/app/shared/attachments/マユリ/cli-vault-commands-bugs.md` に保存し、Discord で共有する（`ci.md §証跡提出方針`）。

---

*この文書は `index.md` の分割成果。ユニットテストは `unit.md`、E2E は `e2e.md`、CI は `ci.md` を参照*
