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
│   ├── mod.rs                  # fresh_repo(), fixed_time(), build_cli() ヘルパー
│   ├── fixtures.rs             # create_encrypted_vault()
│   └── ipc_harness.rs          # DaemonHarness（Sub-F 新規追加 — §10.2 参照）
├── it_usecase_list.rs          # TC-IT-001〜003
├── it_usecase_add.rs           # TC-IT-010〜013
├── it_usecase_edit.rs          # TC-IT-020〜024
├── it_usecase_remove.rs        # TC-IT-030, 031, 033
├── it_usecase_cross.rs         # TC-IT-040, 050（横断パラメタライズ）
├── vault_subcommands.rs        # TC-F-I01〜I09, I11, I12（Sub-F 新規）
└── mode_banner_integration.rs  # TC-F-I10（Sub-F 新規）
```

各ファイルの docstring に対応 REQ-ID / Issue 番号を書く（テスト戦略ガイド準拠）。

---

## 9. 想定外の挙動の取り扱い

バグ発見時は `index.md §6 モック方針` の方針ではなく、**バグレポートを作成**する:

- ファイル名・該当箇所（行番号）
- 期待される動作（本 TC 設計書の期待結果欄）と実際の動作
- 再現手順（`cargo test --test it_usecase_xxx -- TC_NAME`）

バグレポートは `/app/shared/attachments/マユリ/cli-vault-commands-bugs.md` に保存し、Discord で共有する（`ci.md §証跡提出方針`）。

---

## 10. Sub-F vault サブコマンド 結合テスト（TC-F-I01〜I12）

> Issue #77 / #74-C。  
> SSoT: `vault-encryption/test-design/sub-e-vek-cache-ipc.md §14.12 後続 Sub-F への引継ぎ`（`sub-f-cli-subcommands.md` は未作成のため本 §10 が代替 SSoT として機能する。`sub-f-cli-subcommands.md` 作成後はそちらを正本に切り替えること）。

### 10.1 設計方針

- **テスト対象**: `shikomi vault {unlock,lock,encrypt,decrypt,change-password,rotate-recovery,rekey,recovery-show}` CLI サブコマンドの CLI → IPC V2 結合経路
- **エントリポイント**: `assert_cmd::Command::cargo_bin("shikomi")` で実バイナリ呼び出し（§1〜§4 の UseCase 直接呼び出しとは異なり、clap パースを含む）
- **daemon 依存の取り扱い**: `tests/common/ipc_harness.rs` の `DaemonHarness`（in-process daemon + temp Unix ソケット）を使用。実プロセス spawn は TC-F-E01（E2E）に委譲
- **vault 状態**: `TempDir` + `SqliteVaultRepository::from_directory` 実接続（§3 と同一）。暗号化 vault は §5 `create_encrypted_vault()` ヘルパー経由
- **IPC ソケット**: `SHIKOMI_IPC_SOCKET` env var にテンポラリパスを注入し `DaemonHarness` ソケットへ向ける
- **検証スタイル**: stdout / stderr / exit code を `assert_cmd` の `predicate` で assert（半ブラックボックス、契約検証）。vault 内部状態の確認は別 CLI コマンド（`shikomi list` 等）経由ラウンドトリップで行い、DB 直接 assert は禁止

### 10.2 daemon 子プロセス spawn / IPC V2 handshake 戦略

**方式**: in-process `DaemonHarness`（`tests/common/ipc_harness.rs`）

```rust
// tests/common/ipc_harness.rs 想定シグネチャ
pub struct DaemonHarness {
    socket_path: PathBuf,
    _vault_dir: TempDir,
    daemon_handle: tokio::task::JoinHandle<()>,
}

impl DaemonHarness {
    /// vault_dir に対して in-process daemon サーバーを起動し、
    /// _vault_dir/shikomi-test-<uuid>.sock に Unix ソケットをバインドする
    pub async fn new(vault_dir: &Path) -> anyhow::Result<Self> { ... }

    /// `SHIKOMI_IPC_SOCKET` に設定すべきパス文字列を返す
    pub fn socket_path_str(&self) -> &str { ... }
}

impl Drop for DaemonHarness {
    fn drop(&mut self) { self.daemon_handle.abort(); }
}
```

**テスト共通セットアップパターン**:

```rust
// vault_subcommands.rs 内の想定
fn cli() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("shikomi").unwrap()
}

async fn setup_encrypted_harness() -> (TempDir, DaemonHarness) {
    let dir = TempDir::new().unwrap();
    create_encrypted_vault(dir.path()).unwrap();
    let harness = DaemonHarness::new(dir.path()).await.unwrap();
    (dir, harness)
}
```

**IPC V2 handshake 戦略**:

1. CLI 起動時に `IpcRequest::Handshake { client_version: V2 }` を送信
2. daemon が `IpcResponse::Handshake { server_version: V2 }` で応答
3. 以降 V2 専用 variant（`Unlock` / `Lock` / `ChangePassword` / `RotateRecovery` / `Rekey`）を使用
4. V1 クライアントが V2 専用 variant を送信した場合: `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` — TC-F-I11f で検証

**非 TTY passphrase 注入**:

```bash
SHIKOMI_MASTER_PASSWORD=<passphrase>  # env var 注入（非 TTY テスト用）
# または
echo "<passphrase>" | shikomi vault unlock --password-stdin
```

### 10.3 外部 I/O 依存マップ

| 外部I/O | 方針 | characterization 状態 |
|---|---|---|
| **Unix ソケット（IPC）** | `DaemonHarness` が TempDir 内に temp socket を生成。テスト後 `Drop` で自動削除 | 不要（テスト専用ソケット）|
| **shikomi-daemon プロセス** | in-process `DaemonHarness` で代替。実プロセス spawn は TC-F-E01（E2E）に委譲 | 不要（in-process）|
| **vault.db（SQLite）** | §3 と同一: `TempDir` + `SqliteVaultRepository::from_directory` 実接続 | 不要（既存パターン）|
| **暗号化 vault フィクスチャ** | §5 `create_encrypted_vault()` ヘルパー経由（`shikomi-infra` test-only API）| 既存（§5 参照）|
| **passphrase 入力（TTY）** | `SHIKOMI_MASTER_PASSWORD` env var または `--password-stdin` で stdin mock | 不要（env var 注入）|
| **時刻** | §6 と同一: `OffsetDateTime::UNIX_EPOCH + Duration::hours(N)` 注入 | 不要（既存パターン）|

**未対応時のフォールバック**: `DaemonHarness` 実装（shikomi-daemon の in-process 起動 API）が未整備の場合、TC-F-I01〜I12 全件を `#[ignore]` フォールバックし、リーダーに起票を要請する。

### 10.4 テストケース一覧（TC-F-I01〜I12）

#### 10.4.1 vault unlock（TC-F-I01 / I03a / I03b）

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I01 | 正常系 | 暗号化 vault + `DaemonHarness` 起動済み | `shikomi vault unlock`（`SHIKOMI_MASTER_PASSWORD` 経由）| exit 0、stdout に MSG-S03「vault をアンロックしました」含有、後続 `shikomi list` が exit 0 かつ `[encrypted]` バナー含有（Unlocked 状態のラウンドトリップ確認）|
| TC-F-I03a | 正常系 + 異常系 | 暗号化 vault + `DaemonHarness` | (a) 正しい passphrase、(b) 誤り passphrase 5 回連続、(c) `MigrationError::RecoveryRequired` を返す daemon mock | (a) exit 0 + MSG-S03、(b) exit 1 + stderr に「N 秒後に再試行可能」含有（`BackoffActive`）、(c) exit 1 + stderr に「`vault unlock --recovery` も可能」含有（MSG-S09 (a)）|
| TC-F-I03b | 正常系 | 暗号化 vault（リカバリニーモニック有り）+ `DaemonHarness` | `shikomi vault unlock --recovery`（24 語を stdin 経由）| exit 0、stdout に MSG-S03 含有、後続 `shikomi list` が exit 0（Unlocked 状態のラウンドトリップ確認）|

#### 10.4.2 vault lock（TC-F-I02）

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I02 | 正常系 | 暗号化 vault（Unlocked 状態）+ `DaemonHarness` | `shikomi vault lock` | exit 0、stdout に MSG-S04「vault をロックしました」+「VEK はメモリから消去」含有、後続 `shikomi list` が exit 3（Locked → `EncryptionUnsupported` 系エラー）でラウンドトリップ確認|

#### 10.4.3 vault encrypt / decrypt（TC-F-I04 / I05）

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I04 | 正常系 | plaintext vault（レコード 1 件以上）+ `DaemonHarness` | `shikomi vault encrypt`（`SHIKOMI_MASTER_PASSWORD` 経由）| exit 0、後続 `shikomi list` の stdout 1 行目に `[encrypted]` バナー含有（vault が暗号化状態に遷移したことをラウンドトリップ確認）|
| TC-F-I05 | 正常系 | 暗号化 vault（Unlocked）+ `DaemonHarness` | `shikomi vault decrypt`（DECRYPT 確認入力を `--password-stdin` 経由）| exit 0、後続 `shikomi list` の stdout 1 行目に `[plaintext]` バナー含有、MSG-S14 の二段確認（大文字 `DECRYPT` 入力要求）が機能することを assert_cmd stdin で検証 |

#### 10.4.4 vault change-password / rekey / rotate-recovery（TC-F-I06 / I07 / I08）

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I06 | 正常系 | 暗号化 vault（Unlocked）+ `DaemonHarness` | `shikomi vault change-password`（旧・新 passphrase stdin 経由）| exit 0、stdout に MSG-S05「VEK は不変のため再 unlock は不要」+「daemon キャッシュも維持」含有、後続 `shikomi list` が再 unlock なしに exit 0（Unlocked cache 維持のラウンドトリップ確認）|
| TC-F-I07 | 正常系 | 暗号化 vault（Unlocked、レコード N 件）+ `DaemonHarness` | `shikomi vault rekey` | exit 0、stdout に MSG-S07「N 件のレコードを新 VEK で再暗号化しました」含有（`Rekeyed { records_count: N }`）、後続 `shikomi list` で N 件が正常取得できる（旧 VEK → 新 VEK 再暗号化後のラウンドトリップ確認）|
| TC-F-I08 | 正常系 | 暗号化 vault（Unlocked）+ リカバリニーモニック有り + `DaemonHarness` | `shikomi vault rotate-recovery`（passphrase stdin 経由）| exit 0、stdout に新 24 語ニーモニック含有 + MSG-S18 含有（アクセシビリティ案内）、旧ニーモニックで `vault unlock --recovery` が exit 1、新ニーモニックで exit 0（ラウンドトリップ確認）|

#### 10.4.5 vault recovery-show 不在確認（TC-F-I09）

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I09 | 異常系 | `vault rotate-recovery` 完了後（初回表示済み）の vault + `DaemonHarness` | `shikomi vault recovery-show` 2 回目実行 | exit 1、stderr に §C-37「リカバリニーモニックは初回表示のみ利用可能です」含有（所有権消費後の再表示禁止）|

#### 10.4.6 mode banner 表示（TC-F-I10）

> `mode_banner_integration.rs` に実装。`unit.md §5 TC-UT-050〜053`（`render_list` pure 関数テスト）との棲み分けは §10.5 参照。

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I10a | 正常系 | plaintext vault（`DaemonHarness` 不要）| `shikomi list` | stdout 1 行目に `[plaintext]` バナー含有 |
| TC-F-I10b | 正常系 | 暗号化 vault（Unlocked）+ `DaemonHarness` | `shikomi list` | stdout 1 行目に `[encrypted]` バナー含有 |
| TC-F-I10c | 異常系 | 暗号化 vault（Locked）+ `DaemonHarness` | `shikomi list` | exit 3、stderr に MSG-S16「vault はロック中」+「`shikomi vault unlock` でアンロック」含有 |
| TC-F-I10d | 正常系（NO_COLOR）| plaintext vault + `NO_COLOR=1` env var | `shikomi list` | `[plaintext]` バナー含有かつ ANSI エスケープシーケンス（`\x1b[` 等）不含 |

#### 10.4.7 exit code 整合（TC-F-I11）

| TC-ID | 種別 | 操作 | 期待 exit code |
|-------|------|------|--------------|
| TC-F-I11a | 正常系 | `shikomi vault unlock`（正しい passphrase）| 0 |
| TC-F-I11b | 異常系 | `shikomi vault unlock`（誤り passphrase、BackoffActive 未発動）| 1（ユーザー入力エラー）|
| TC-F-I11c | 異常系 | `shikomi vault unlock`（BackoffActive 発動後 N 秒待ち中）| 1 |
| TC-F-I11d | 異常系 | plaintext vault 状態で `shikomi vault encrypt` 実施済みの後に再度 `shikomi vault encrypt` | 1（`AlreadyEncrypted`）|
| TC-F-I11e | 異常系 | `shikomi vault decrypt`（確認入力 `DECRYPT` を誤って入力）| 1 |
| TC-F-I11f | 異常系 | daemon 未起動状態（`SHIKOMI_IPC_SOCKET` が存在しないパス）で `shikomi vault unlock` | 2（システムエラー）|

#### 10.4.8 env allowlist 結合経路（TC-F-I12）

| TC-ID | 種別 | 前提条件 | 操作 | 期待結果 |
|-------|------|---------|------|---------|
| TC-F-I12a | 正常系 | `SHIKOMI_VAULT_DIR` + `SHIKOMI_IPC_SOCKET` を `DaemonHarness` のパスに設定 | `shikomi vault unlock` | env var が優先され、指定 vault dir / socket path を使用。exit 0 |
| TC-F-I12b | 異常系 | `SHIKOMI_VAULT_DIR` が存在しないパス | `shikomi vault unlock` | exit 2 + stderr にパス不在エラー含有（`VaultNotInitialized` or `Io` 系）|
| TC-F-I12c | 正常系 | `SHIKOMI_MASTER_PASSWORD` 設定（非 TTY テスト用）| `shikomi vault unlock` | passphrase が env var から読み込まれ stdin prompt スキップ。exit 0 |

### 10.5 unit.md §5 との棲み分け表

`unit.md §5（TC-UT-050〜053）` は `presenter::list::render_list` の **pure function ユニットテスト**（副作用なし、入力 DTO → 文字列変換）。本 §10 は CLI バイナリ全体の結合経路テスト。

| 検証観点 | unit.md §5（TC-UT-050〜053）| integration.md §10（TC-F-I10）|
|---------|---------------------------|-------------------------------|
| テスト対象 | `presenter::list::render_list(records, mode: VaultMode)` | `shikomi list` CLI バイナリ（assert_cmd）|
| バナー文字列生成ロジック | ✅ `[plaintext]` / `[encrypted]` / `[locked]` 文字列の構築を詳細検証 | ❌ 生成ロジックには立ち入らない（出力文字列の含有のみ確認）|
| 実際の CLI 出力 | ❌ CLI を経由しない（pure 関数直呼び出し）| ✅ stdout / stderr / exit code を assert |
| vault 状態の実物 | ❌ 入力 DTO をテスト側で直接構築 | ✅ `TempDir` + 実 vault ファイル |
| IPC 経路 | ❌ IPC なし | ✅ `DaemonHarness` 経由で実 IPC V2（encrypted / locked 状態）|
| NO_COLOR 対応 | ✅ 入力フラグで生成ロジック検証 | ✅ env var `NO_COLOR=1` で CLI 出力検証（重複は意図的）|

**棲み分け原則**: unit.md §5 は「バナー文字列が正しく構築されるか」を検証し、integration.md §10 は「CLI から IPC までの結合経路でバナーが正しく表示されるか」を検証する。両層とも必要で、どちらか一方では担保できない。

### 10.6 `vault_subcommands.rs` / `mode_banner_integration.rs` 責務分割

| テストファイル | TC | 責務 |
|-------------|-----|------|
| `tests/vault_subcommands.rs` | TC-F-I01, I02, I03a, I03b, I04〜I09, I11, I12 | vault 管理サブコマンド（unlock / lock / encrypt / decrypt / change-password / rotate-recovery / rekey / recovery-show）の CLI → IPC V2 結合経路。`DaemonHarness` を使用 |
| `tests/mode_banner_integration.rs` | TC-F-I10a〜d | `shikomi list` 等の既存コマンド出力に mode banner（`[plaintext]` / `[encrypted]` / `[locked]`）が正しく含まれることを検証。plaintext vault は `DaemonHarness` 不要、encrypted / locked 状態は `DaemonHarness` 使用 |

**共通インフラ**（`tests/common/` に集約）:

| ファイル | 提供するもの |
|---------|------------|
| `mod.rs` | `fresh_repo()`, `fixed_time()`, `build_cli()` |
| `fixtures.rs` | `create_encrypted_vault()` — §5 既存 |
| `ipc_harness.rs` | `DaemonHarness` — Sub-F 新規追加 |

### 10.7 カバレッジ対象（Sub-F）

| 受入基準 / 契約 | カバー TC |
|--------------|----------|
| Sub-E C-22〜C-28 の CLI 側 IPC V2 経路検証 | TC-F-I01, I02, I03a, I03b |
| REQ-S16 mode banner（`[plaintext]` / `[encrypted]` / `[locked]`）| TC-F-I10a〜d |
| Sub-E EC-3 ChangePassword VEK 不変 + daemon cache 維持 | TC-F-I06 |
| Sub-E EC-4 RotateRecovery 初回 1 度のみ + §C-37 所有権消費 | TC-F-I08, I09 |
| Sub-E EC-5 Rekey 全レコード再暗号化 + records_count 検証 | TC-F-I07 |
| `vault encrypt` plaintext → encrypted 遷移 | TC-F-I04 |
| `vault decrypt` encrypted → plaintext 遷移 + 二段確認 | TC-F-I05 |
| REQ-CLI-006 exit code 契約（vault サブコマンド範囲）| TC-F-I11a〜f |
| REQ-CLI-005 env var 単一化（IPC ソケット + vault dir）| TC-F-I12a〜c |

---

*この文書は `index.md` の分割成果。ユニットテストは `unit.md`、E2E は `e2e.md`、CI は `ci.md` を参照*
