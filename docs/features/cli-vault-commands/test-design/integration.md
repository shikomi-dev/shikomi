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

## 5. Windows DACL fixture helper（Issue #86 対応）

**経緯**: GitHub Actions `windows-latest` runner 上で `tempfile::TempDir` が生成するディレクトリの DACL は親フォルダ（`C:\Users\runneradmin\AppData\Local\Temp`）から**継承**された状態（`SE_DACL_PROTECTED` 未設定）で渡される。Issue #65 で導入した owner-only DACL 検証（`PersistenceError::InvalidPermission`）はこの継承 DACL を Fail Fast で弾くため、暗号化 vault フィクスチャを使う TC-IT-012 / 023 / 033 / 040 や同経路の E2E テスト群が Windows runner 限定で `Persistence(InvalidPermission { actual: "inherited DACL (SE_DACL_PROTECTED not set)" })` を返し、`EncryptionUnsupported` 期待と不一致になって FAIL する。本契約は **Issue #65 owner-only DACL 検証は本番要件として維持**したまま、テスト fixture 側で **TempDir 生成直後に owner-only DACL を強制適用**することで Windows CI を緑化する設計を確定する。

**責務範囲**:

| 項目 | 仕様 |
|-----|------|
| 配置 | `crates/shikomi-cli/tests/common/windows_dacl_fixture.rs`（新規モジュール、`tests/common/mod.rs` から `pub mod windows_dacl_fixture;` で公開） |
| コンパイル条件 | `#[cfg(target_os = "windows")]` 配下のみ。Linux / macOS では本モジュール自体がコンパイル対象外 |
| 公開 API | 本モジュールが提供する `enforce_owner_only_dacl(path)` 関数（責務：渡されたパスの DACL を `SE_DACL_PROTECTED` 立てて owner-only に正規化）。詳細シグネチャ・エラー型は `../detailed-design/data-structures.md §テスト fixture モジュール / windows_dacl_fixture` を参照 |
| 内部実装 | Win32 Security API `SetSecurityInfo` を `PROTECTED_DACL_SECURITY_INFORMATION \| DACL_SECURITY_INFORMATION` 指定で呼び、所有者 SID（カレントプロセストークンから取得）に `FILE_GENERIC_READ \| FILE_GENERIC_WRITE \| FILE_TRAVERSE` の ACE のみを持つ DACL を構築・適用する。Issue #65 の本番側検証ロジックと**完全同一の DACL 形を fixture 側で先に作る** |
| 失敗時挙動 | `Result<(), io::Error>` を返し、呼び出し側（テストコード）は `expect("...")` で即 panic させて Fail Fast。fixture が失敗した時点でテストの前提が崩れているため、握り潰しは禁止 |

**呼び出し契約**（共通セットアップ §3 への追記）:

- `fresh_repo()` ヘルパーは `TempDir::new()` 直後・`SqliteVaultRepository::from_directory(dir.path())` 呼び出し**前**に Windows のみ `windows_dacl_fixture::enforce_owner_only_dacl(dir.path())` を呼ぶ
- `#[cfg(target_os = "windows")]` 経路でのみ呼び出し、他 OS では呼び出しコード自体がコンパイル対象外（条件コンパイルで分岐）
- 暗号化 vault fixture（`fixtures::create_encrypted_vault`）も同様に、SQLite DB を生成する直前に DACL を強制する。`create_encrypted_vault(dir)` の内部で TempDir パスを受け取った直後に `enforce_owner_only_dacl(dir)` を呼ぶ

**影響を受けるテスト**（Windows runner で fixture 適用が必要な TC）:

| TC-ID | 適用箇所 | 期待結果 |
|-------|---------|---------|
| TC-IT-012 | `add_record` × 暗号化 vault | DACL 正規化後、`Err(EncryptionUnsupported)` を返す（`InvalidPermission` で早期失敗しない） |
| TC-IT-023 | `edit_record` × 暗号化 vault | 同上 |
| TC-IT-033 | `remove_record` × 暗号化 vault | 同上 |
| TC-IT-040 | 4 UseCase 横断 × 暗号化 vault | 全 4 経路で `Err(EncryptionUnsupported)` |
| TC-IT-001〜003 / 010〜011 / 013 / 020〜022 / 024 / 030〜031 | 平文 vault 系 | DACL 正規化後、本来の期待結果通り（`InvalidPermission` で早期失敗しない） |
| `e2e_edit` / `e2e_encrypted` 系（`e2e.md` 側で詳細） | E2E バイナリ呼び出し | 同上、E2E の `--vault-dir <tempdir>` 指定 fixture 内でも同じ DACL 強制を行う |

**設計判断**:

| 案 | 概要 | 採否 | 根拠 |
|----|------|------|------|
| A | テスト fixture 側で `SetSecurityInfo` + `PROTECTED_DACL_SECURITY_INFORMATION` を呼び owner-only DACL を強制 | **採用** | Issue #65 の owner-only DACL 検証（**本番要件**）を緩めずに、Windows runner 環境差異のみを吸収する最小変更。本番コード（`src/`）には一切触らない |
| B | 本番側の DACL 検証から Windows tempdir パスを除外する | 不採用 | 本番側のセキュリティ検証ロジックに「テスト環境特有の例外」を持ち込むのは脅威モデル退行。Issue #65 が解決した owner-only Fail Fast 契約を侵食する |
| C | テストを Windows でのみ `#[ignore]` する | 不採用 | Windows 経路の DACL 検証 / 暗号化 vault Fail Fast 経路がカバレッジ穴になる。CI で 3 OS matrix を維持している以上、Windows 緑化は要件 |
| D | `tempfile::Builder::new().permissions(...)` 等の上流ライブラリ機能で DACL 設定 | 不採用 | `tempfile` クレートは Windows DACL の owner-only 強制 API を提供していない（Unix mode bits は対応するが Windows DACL は範囲外、公式 README 確認済）。自前 fixture が必要 |

**Boy Scout 観点**:

- 本 fixture モジュール追加に伴い、`tests/common/mod.rs` を**初めて作成**する（既存の `crates/shikomi-cli/tests/` 配下に共通モジュールがなかった、各テストファイルがバラバラに setup していた）。DRY 原則に従い、`fresh_repo()` / `fixed_time()` / `windows_dacl_fixture` を共通モジュールに集約する
- 既存テストが各々で `TempDir::new()` していた箇所も、本機会に `common::fresh_repo()` 経由に置換する（複数 PR に分割せず、Issue #86 の修正 PR 内で一括）

---

## 6. 暗号化 vault フィクスチャヘルパー

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

## 7. 結合テストでの時刻注入と決定性

- UseCase は `now: OffsetDateTime` を引数で受ける（詳細設計 §public-api.md）ため、テスト側で固定時刻を注入可能
- 例: `let now = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);` を `add_record(repo, input, now)` に渡し、`list_records` 後の `RecordView` で `updated_at == now` を assert
- **`SystemTime::now()` に依存するテストは書かない**（flaky の温床）

---

## 8. カバレッジ対象

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

## 9. 結合テストファイル構成

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

## 10. 想定外の挙動の取り扱い

バグ発見時は `index.md §6 モック方針` の方針ではなく、**バグレポートを作成**する:

- ファイル名・該当箇所（行番号）
- 期待される動作（本 TC 設計書の期待結果欄）と実際の動作
- 再現手順（`cargo test --test it_usecase_xxx -- TC_NAME`）

バグレポートは `/app/shared/attachments/マユリ/cli-vault-commands-bugs.md` に保存し、Discord で共有する（`ci.md §証跡提出方針`）。

---

## 11. 出典・参考

- Issue #65 owner-only DACL 強化（PR #71/#72）: 本 fixture が満たすべき DACL 形（`FILE_GENERIC_READ \| FILE_GENERIC_WRITE \| FILE_TRAVERSE` の owner-only ACE、`SE_DACL_PROTECTED` 立て）の根拠
- Issue #86 (本 PR): Windows runner tempdir DACL 継承による fixture 欠落の再現と解決策
- Microsoft Learn `SetSecurityInfo`: https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setsecurityinfo
- Microsoft Learn `SECURITY_INFORMATION` (`PROTECTED_DACL_SECURITY_INFORMATION`): https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-security_information
- `tempfile` crate README（Windows DACL に対する責務外を明示）: https://github.com/Stebalien/tempfile

---

*この文書は `index.md` の分割成果。ユニットテストは `unit.md`、E2E は `e2e.md`、CI は `ci.md` を参照*
