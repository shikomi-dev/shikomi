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
│   └── fixtures.rs             # create_encrypted_vault()
├── it_usecase_list.rs          # TC-IT-001〜003
├── it_usecase_add.rs           # TC-IT-010〜013
├── it_usecase_edit.rs          # TC-IT-020〜024
├── it_usecase_remove.rs        # TC-IT-030, 031, 033
├── it_usecase_cross.rs         # TC-IT-040, 050（横断パラメタライズ）
├── vault_subcommands.rs        # TC-F-I01〜I09, I12（Sub-F 新規）
└── mode_banner_integration.rs  # TC-F-I10（Sub-F 新規）
tests/helpers/
└── daemon_spawn.rs             # DaemonSpawn（Sub-F 新規、workspace 共有 — §10.2 参照）
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
> SSoT: `vault-encryption/test-design/sub-f-cli-subcommands/index.md §15.6`（Rev1、372行）

### 10.1 設計方針

- **テスト対象**: `shikomi vault {encrypt,decrypt,unlock,lock,change-password,rekey,rotate-recovery}` CLI サブコマンドの CLI → IPC V2 結合経路（`recovery-show` は廃止済、SSoT §15.1 / EC-F1 参照）
- **エントリポイント**: `assert_cmd::Command::cargo_bin("shikomi")` で実バイナリ呼び出し（§1〜§4 の UseCase 直接呼び出しとは異なり、clap パースを含む）
- **daemon 依存**: 実 `shikomi-daemon` 子プロセスを `tests/helpers/daemon_spawn.rs` の `DaemonSpawn` ヘルパーで起動（SSoT §15.1 結合テスト欄 明示: 実 `shikomi-daemon` 子プロセス + `expectrl` PTY）
- **TTY 入力**: `expectrl` PTY ライブラリで passphrase / mnemonic / DECRYPT 確認文字列を制御（C-38 stdin パイプ拒否前提、既存 `e2e_daemon_phase15_pty.rs` の dev-dep 再利用）
- **env seam（C-40 allowlist）**: `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS` / `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL` を `DaemonSpawn` 経由で注入（`#[cfg(debug_assertions)]` 限定）
- **検証スタイル**: stdout / stderr / exit code を `assert_cmd` の `predicate` で assert（半ブラックボックス、契約検証）。vault 状態確認は後続 CLI 呼び出しによるラウンドトリップ。**DB 直接 assert は禁止**

### 10.2 daemon 子プロセス spawn / IPC V2 handshake 戦略

**方式**: 実 daemon 子プロセス（`tests/helpers/daemon_spawn.rs`）—— SSoT §15.2 `crates/shikomi-daemon/tests/e2e_daemon.rs` の `daemon_spawn` ヘルパー拡張版

```rust
// tests/helpers/daemon_spawn.rs 想定シグネチャ（Sub-F 工程3 で銀時実装）
pub struct DaemonSpawn {
    vault_dir: TempDir,          // Drop で tempdir 自動削除
    socket_path: PathBuf,
    process: std::process::Child,
}

impl DaemonSpawn {
    /// vault_dir を TempDir に作成し、shikomi-daemon を実子プロセスとして起動
    /// env: SHIKOMI_VAULT_DIR=<vault_dir> + SHIKOMI_DAEMON_* C-40 allowlist
    pub fn new(vault_dir: &Path) -> anyhow::Result<Self> { ... }

    /// C-40 allowlist 経由で idle 短縮を有効化（debug build 限定）
    pub fn with_idle_threshold(mut self, secs: u64) -> Self { ... }

    /// C-40 allowlist 経由で cache_relocked:false fault injection を有効化
    pub fn with_force_relock_fail(mut self) -> Self { ... }

    /// assert_cmd に渡す env vars を返す
    pub fn env_args(&self) -> Vec<(OsString, OsString)> { ... }
}

impl Drop for DaemonSpawn {
    fn drop(&mut self) { let _ = self.process.kill(); }
}
```

**テスト共通セットアップパターン**（vault_subcommands.rs）:

```rust
fn cli() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("shikomi").unwrap()
}

fn setup_encrypted_daemon(vault_dir: &Path) -> DaemonSpawn {
    create_encrypted_vault(vault_dir).unwrap();
    DaemonSpawn::new(vault_dir).unwrap()
}
```

**IPC V2 handshake**（CLI が自動実行、テスト側は意識不要）:
1. CLI 起動 → `IpcRequest::Handshake { client_version: V2 }` 送信
2. daemon → `IpcResponse::Handshake { server_version: V2 }` 応答
3. 以降 V2 variant（`Unlock` / `Lock` / `ChangePassword` / `Rekey` / `RotateRecovery`）使用可能

### 10.3 外部 I/O 依存マップ（SSoT §15.2）

| 外部I/O | 方針 | characterization 状態 |
|---|---|---|
| **`shikomi-daemon` プロセス** | `DaemonSpawn`（`tests/helpers/daemon_spawn.rs`）経由で実子プロセス起動。`SHIKOMI_VAULT_DIR` env + tempdir socket 自動設定。`Drop` で `kill()` | **既存資産拡張**（Sub-F 工程3 で銀時実装）|
| **TTY（password / mnemonic / DECRYPT 確認）** | `expectrl`（Unix 限定 dev-dep、Sub-D `e2e_daemon_phase15_pty.rs` で既導入）で PTY 擬似制御。stdin パイプ拒否確認（TC-F-I12）は `assert_cmd::Command::write_stdin` で非 TTY 経路 | **既存資産再利用** |
| **vault.db（SQLite）** | §3 と同一: `TempDir` + `create_encrypted_vault()` ヘルパー経由 | 不要（既存パターン）|
| **env seam（C-40 allowlist）** | `DaemonSpawn::with_idle_threshold` / `with_force_relock_fail` 経由で `#[cfg(debug_assertions)]` 限定 env 注入 | 不要（local env）|

**`#[ignore]` ゲート管理**: TC-F-I07c は `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` が `#[cfg(debug_assertions)]` 限定のため、`#[cfg_attr(not(debug_assertions), ignore = "requires debug build")]` でゲート。**無声 skip 禁止** —— CI ログに `IGNORED: requires debug build` を明示して監査経路に含める。

### 10.4 テストケース一覧（TC-F-I01〜I12 / SSoT §15.6 1:1 対応）

#### 10.4.1 vault encrypt / decrypt（TC-F-I01 / I02 / I02b）

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I01 | EC-F1 / REQ-S15 | plaintext vault + `DaemonSpawn` | `shikomi vault encrypt --output screen`（`expectrl` PTY 経由パスワード入力）| exit 0 + stdout に MSG-S01 + 24 語表示 + vault.db が `ProtectionMode::Encrypted`（後続 `shikomi list` で `[encrypted, unlocked]` バナー確認）|
| TC-F-I02 | EC-F2 / C-20 | 暗号化 vault（Unlocked）+ `DaemonSpawn` | `shikomi vault decrypt`（正規 pass + DECRYPT 大文字確認文字列を `expectrl` 経由）| exit 0 + vault.db 平文化（後続 `shikomi list` で `[plaintext]` バナー確認）。不正 DECRYPT 入力（例: `decrypt`）では exit 1 + DECRYPT 中止メッセージ |
| TC-F-I02b | C-34 | 暗号化 vault（Unlocked）+ `DaemonSpawn` | `expectrl` で paste 模擬: (a) `< 30ms` 2 回入力、(b) `>= 30ms` 跨ぎ入力 | (a) exit 1 + MSG-S14（paste 疑い）、(b) 通常入力 OK（exit 0）|

#### 10.4.2 vault unlock / lock（TC-F-I03 / I03b / I04）

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I03 | EC-F3 / C-26 | 暗号化 vault + `DaemonSpawn` | (a) 正パスワード `expectrl` 経由 → unlock、(b) 誤りパスワード × 5 回 → 6 回目 | (a) exit 0 + MSG-S03、(b) **exit 2**（BackoffActive、SSoT cli-subcommands.md §終了コード参照）+ 待機秒数表示 |
| TC-F-I03b | EC-F3 | 暗号化 vault + `DaemonSpawn` | (a) `vault unlock --recovery`（24 語 `expectrl` 経由）、(b) 不正 mnemonic、(c) password 経路で RecoveryRequired 発火 | (a) exit 0 + MSG-S03、(b) exit 1 + MSG-S12、(c) **exit 5**（RecoveryRequired、SSoT 整合）|
| TC-F-I04 | EC-F4 | 暗号化 vault（Unlocked）+ `DaemonSpawn` | `shikomi vault lock` | exit 0 + MSG-S04「VEK はメモリから消去」、後続 `shikomi list` で **exit 3** + `[encrypted, locked]` バナー + MSG-S09(c) |

#### 10.4.3 vault change-password / disclose 防衛（TC-F-I05 / I06）

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I05 | EC-F5 / REQ-S10 | 暗号化 vault（Unlocked）+ `DaemonSpawn` | `shikomi vault change-password`（旧・新 pass `expectrl` 経由）| exit 0 + MSG-S05「VEK は不変のため再 unlock は不要」、後続 `shikomi list` で `[encrypted, unlocked]` バナー（cache 維持のラウンドトリップ確認）|
| TC-F-I06 | EC-F1 / C-35 | 暗号化済み vault + `DaemonSpawn` | 暗号化後 2 度目の `shikomi vault encrypt` 実行 | exit 1 + `MigrationError::AlreadyEncrypted` 由来 MSG-S09 系（C-35 構造防衛、`recovery-show` 廃止後も daemon 側 disclose 1 度限り意味を維持）|

#### 10.4.4 vault rekey（TC-F-I07 / I07c）

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I07 | EC-F6 | 暗号化 vault（Unlocked）+ `DaemonSpawn` | `shikomi vault rekey --output screen`（`expectrl` 経由）| exit 0 + MSG-S07「再暗号化完了 N 件」+ 24 語表示 + cache 維持（`cache_relocked: true`、後続 `shikomi list` で `[encrypted, unlocked]` バナー）|
| TC-F-I07c | C-32 / C-36 / EC-F6 | 暗号化 vault（Unlocked）+ **`DaemonSpawn::with_force_relock_fail()`**（C-40 allowlist、`#[cfg_attr(not(debug_assertions), ignore)]`）| `shikomi vault rekey --output screen` | **exit 0**（C-31/C-36 整合、operation 成功）+ stdout に MSG-S07 + S20 連結「次の操作前に `shikomi vault unlock` を再度実行」+ 後続 `shikomi list` で `[encrypted, locked]` バナー（Lie-Then-Surprise 防衛確認）|

#### 10.4.5 vault rotate-recovery / Locked CRUD（TC-F-I08 / I09 / I09b）

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I08 | EC-F7 | 暗号化 vault（Unlocked）+ `DaemonSpawn` | `shikomi vault rotate-recovery --output screen`（`expectrl` 経由）| exit 0 + MSG-S19 + 新 24 語表示 + cache 維持（`cache_relocked: true`）|
| TC-F-I09 | EC-F8 / REQ-S16 | 暗号化 vault（**Locked**）+ `DaemonSpawn` | `shikomi list` | exit **3** + MSG-S09(c)「`shikomi vault unlock` で解除してください」、stdout/stderr にレコード内容・ID・ラベル**含まない**（grep 0 件確認）|
| TC-F-I09b | EC-F8 / REQ-S16 | 暗号化 vault（**Locked**）+ `DaemonSpawn` | `shikomi add Text "label" "value"` / `shikomi edit <id> ...` / `shikomi remove <id>` 各実行 | 全て exit **3** + MSG-S09(c)、value/label が stdout/stderr に**漏洩しない**（grep 0 件、情報漏洩防衛）|

#### 10.4.6 mode banner 3 状態（TC-F-I10）

> `mode_banner_integration.rs` に実装。`unit.md §5 TC-UT-050〜053`（`render_list` pure 関数テスト）との棲み分けは §10.5 参照。

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I10a | EC-F9 / REQ-S16 | plaintext vault（`DaemonSpawn` 不要）| `shikomi list` | exit 0 + stdout に `[plaintext]` 灰色バナー含有 |
| TC-F-I10b | EC-F9 / REQ-S16 | 暗号化 vault（**Locked**）+ `DaemonSpawn` | `shikomi list` | exit 3 + `[encrypted, locked]` 橙色バナー含有 |
| TC-F-I10c | EC-F9 / REQ-S16 | 暗号化 vault（**Unlocked**）+ `DaemonSpawn` | `shikomi list` | exit 0 + `[encrypted, unlocked]` 緑色バナー含有 |
| TC-F-I10d | EC-F9 | plaintext vault + `NO_COLOR=1` env var | `shikomi list` | `[plaintext]` バナー含有かつ ANSI エスケープシーケンス（`\x1b[` 等）不含 |

#### 10.4.7 stdin パイプ拒否（TC-F-I12）

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I12 | C-38 | 暗号化 vault + `DaemonSpawn` | `assert_cmd::Command::write_stdin("strong-password")` で非 TTY パイプ経路（`echo "strong-password" \| shikomi vault unlock` 相当）| exit 1 + `CliError::NonInteractivePassword` 文言 + 「パスワードはプロンプト入力のみ。`echo \| shikomi` の経路は提供していません」案内（C-38、Rev1 服部指摘5）|

### 10.5 unit.md §5 との棲み分け表

`unit.md §5（TC-UT-050〜053）` は `presenter::list::render_list` の **pure function ユニットテスト**（副作用なし、入力 DTO → 文字列変換）。本 §10 は CLI バイナリ全体の結合経路テスト。

| 検証観点 | unit.md §5（TC-UT-050〜053）| integration.md §10（TC-F-I10）|
|---------|---------------------------|-------------------------------|
| テスト対象 | `presenter::list::render_list(records, mode: ProtectionModeBanner)` | `shikomi list` CLI バイナリ（assert_cmd）|
| バナー文字列生成ロジック | ✅ SSoT TC-F-U05 通り 4 状態（`Plaintext`/`EncryptedLocked`/`EncryptedUnlocked`/`Unknown`）を詳細検証 | ❌ 生成ロジックには立ち入らない（出力文字列の含有のみ確認）|
| 実際の CLI 出力 | ❌ CLI を経由しない（pure 関数直呼び出し）| ✅ stdout / stderr / exit code を assert |
| vault 状態の実物 | ❌ 入力 DTO をテスト側で直接構築 | ✅ `TempDir` + 実 vault ファイル |
| daemon / IPC 経路 | ❌ IPC なし | ✅ `DaemonSpawn` 経由で実 IPC V2（Locked / Unlocked 状態）|
| NO_COLOR 対応 | ✅ 入力フラグで生成ロジック検証 | ✅ env var `NO_COLOR=1` で CLI 出力検証（重複は意図的）|

**バナー状態数の差異**: unit.md §5 は 4 状態（`Unknown` を含む）を検証。§10 は正常 CLI 経路で観測できる 3 状態（`[plaintext]` / `[encrypted, locked]` / `[encrypted, unlocked]`）を結合検証する（`Unknown` は CLI 正常経路では表示されない）。

### 10.6 `vault_subcommands.rs` / `mode_banner_integration.rs` 責務分割

| テストファイル | TC | 責務 |
|-------------|-----|------|
| `crates/shikomi-cli/tests/vault_subcommands.rs` | TC-F-I01, I02, I02b, I03, I03b, I04, I05, I06, I07, I07c, I08, I09, I09b, I12 | vault 管理サブコマンドの CLI→IPC V2 結合経路。`DaemonSpawn` + `expectrl` PTY 使用 |
| `crates/shikomi-cli/tests/mode_banner_integration.rs` | TC-F-I10a〜d | `shikomi list` mode banner 3 状態 + NO_COLOR。plaintext は `DaemonSpawn` 不要、Locked / Unlocked 状態は `DaemonSpawn` 使用 |

**共通インフラ**:

| ファイル | 提供するもの |
|---------|------------|
| `tests/helpers/daemon_spawn.rs` | `DaemonSpawn`（実子プロセス起動 + C-40 env seam）— Sub-F 工程3 で銀時実装（SSoT §15.2）|
| `crates/shikomi-cli/tests/common/mod.rs` | `fresh_repo()`, `fixed_time()`, `build_cli()` — §3 既存 |
| `crates/shikomi-cli/tests/common/fixtures.rs` | `create_encrypted_vault()` — §5 既存 |

### 10.7 カバレッジ対象（Sub-F 結合テスト、SSoT §15.3 対応）

| 受入基準 / 契約 | カバー TC |
|--------------|----------|
| EC-F1 vault encrypt + disclose 1 度のみ（C-35） | TC-F-I01, TC-F-I06 |
| EC-F2 vault decrypt + C-34 paste 抑制（`< 30ms = Err`）| TC-F-I02, TC-F-I02b |
| EC-F3 unlock 2 経路 + exit 0 / **2(BackoffActive)** / **5(RecoveryRequired)** | TC-F-I03, TC-F-I03b |
| EC-F4 vault lock + `[encrypted, locked]` バナー + exit 3 CRUD | TC-F-I04 |
| EC-F5 change-password + cache 維持（`[encrypted, unlocked]` 継続）| TC-F-I05 |
| EC-F6 rekey + cache_relocked 2 経路（C-32/C-36、fault injection）| TC-F-I07, TC-F-I07c |
| EC-F7 rotate-recovery + 24 語 + cache 維持 | TC-F-I08 |
| EC-F8 Locked 時 CRUD fail fast + 情報漏洩防衛（grep 0 件）| TC-F-I09, TC-F-I09b |
| EC-F9 / REQ-S16 mode banner 3 状態 + NO_COLOR | TC-F-I10a〜d |
| C-38 stdin パイプ拒否（Rev1 服部指摘5）| TC-F-I12 |

---

*この文書は `index.md` の分割成果。ユニットテストは `unit.md`、E2E は `e2e.md`、CI は `ci.md` を参照*
