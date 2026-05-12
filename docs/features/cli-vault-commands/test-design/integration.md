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
    socket_path: PathBuf,        // socket 親ディレクトリは必ず 0700 で作成
    process: std::process::Child,
}

impl DaemonSpawn {
    /// vault_dir を TempDir に作成し、shikomi-daemon を実子プロセスとして起動
    /// env: SHIKOMI_VAULT_DIR=<vault_dir> + SHIKOMI_DAEMON_* C-40 allowlist
    ///
    /// **セキュリティ契約（daemon-ipc/security.md §シングルインスタンス準拠）**:
    /// 1. socket 親ディレクトリを `std::fs::set_permissions(dir, PermissionsExt::from_mode(0o700))`
    ///    で **0700** に強制設定してから daemon を起動する
    /// 2. daemon 起動後、`std::fs::metadata(socket_parent).mode() & 0o777 == 0o700` を
    ///    `stat` で検証。不一致なら `anyhow::bail!("socket parent dir is not 0700")` で fail fast
    /// 3. 上記 2 ステップが失敗した場合はテスト全体を error（panic ではなく `?` 伝播）にする
    pub fn new(vault_dir: &Path) -> anyhow::Result<Self> { ... }

    /// C-40 allowlist 経由で idle 短縮を有効化（debug build 限定）
    pub fn with_idle_threshold(mut self, secs: u64) -> Self { ... }

    /// C-40 allowlist 経由で cache_relocked:false fault injection を有効化
    pub fn with_force_relock_fail(mut self) -> Self { ... }

    /// assert_cmd に渡す env vars を返す
    pub fn env_args(&self) -> Vec<(OsString, OsString)> { ... }
}

impl Drop for DaemonSpawn {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait(); // ゾンビ化防止: kill 後に必ず wait する（CI 並列実行時の pid リソース枯渇防止）
    }
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
| **`shikomi-daemon` プロセス** | `DaemonSpawn`（`tests/helpers/daemon_spawn.rs`）経由で実子プロセス起動。`SHIKOMI_VAULT_DIR` env + **tempdir socket 親ディレクトリを `0700` 強制（起動前 chmod + 起動後 stat 検証 fail fast）** + `Drop` で `kill()` | **既存資産拡張**（Sub-F 工程3 で銀時実装）|
| **TTY（password / mnemonic / DECRYPT 確認）** | `expectrl`（**Unix 限定** dev-dep、Sub-D `e2e_daemon_phase15_pty.rs` で既導入）で PTY 擬似制御。stdin パイプ拒否確認（TC-F-I12）は `assert_cmd::Command::write_stdin` で非 TTY 経路。**Windows CI 扱い**: `expectrl` は Unix 専用のため、PTY 経由入力を必要とする TC（TC-F-I01 / I02 / I02b / I03 / I03b / I05 / I06 / I07 / I07c / I08）に `#[cfg_attr(target_os = "windows", ignore = "expectrl PTY not available on Windows, covered by Unix CI (3-OS matrix design intent: TC-F-I* PTY path is Unix+macOS only)")]` を付与する。TC-F-I12（stdin パイプ拒否）は `write_stdin` 非 TTY 経路のため Windows でも実行可能 | **既存資産再利用** |
| **vault.db（SQLite）** | §3 と同一: `TempDir` + `create_encrypted_vault()` ヘルパー経由 | 不要（既存パターン）|
| **env seam（C-40 allowlist）** | `DaemonSpawn::with_idle_threshold` / `with_force_relock_fail` 経由で `#[cfg(debug_assertions)]` 限定 env 注入 | 不要（local env）|

**`#[ignore]` ゲート管理（reason 文字列規約）**: TC-F-I07c は `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` が `#[cfg(debug_assertions)]` 限定のため release ビルドでは実行不可。**無声 skip 禁止** —— CI ログに reason を明示して監査経路に含める。

reason 文字列の必須要素（vault-persistence/test-design/integration/changelog.md v8.4 確立規約 + Bug-F-003 再演防止 Boy Scout）:

| 要素 | 内容 |
|------|------|
| ① skip 理由 | なぜ skip されるか（例: `requires debug build`）|
| ② 関連ゲート | 制約の根拠（例: `C-40 allowlist gate`）|
| ③ 設計書クロス参照 | TC が記述されている設計書パス + セクション（例: `test-design integration.md §10.3`）|
| ④ 解除条件 | 将来 skip を外せる条件（例: `unlock condition: SHIKOMI_DAEMON_FORCE_RELOCK_FAIL extended to release builds by explicit flag`）|

TC-F-I07c の完全 reason 文字列:
```
"requires debug build (C-40 allowlist gate, test-design integration.md §10.3,
 unlock condition: SHIKOMI_DAEMON_FORCE_RELOCK_FAIL extended to release builds by explicit flag)"
```

TC-F-I10a〜d のうち Windows で skip するものおよび TC-F-I11b も同形式で reason 文字列を付与すること（実装担当の責務）。

### 10.4 テストケース一覧（TC-F-I01〜I12 / SSoT §15.6 1:1 対応）

> **TC-F-I 全件の共通前提条件（セキュリティ契約）**: `DaemonSpawn::new()` は socket 親ディレクトリを `0700` で作成し、起動後 `stat` で mode を検証して fail fast する（`daemon-ipc/security.md §シングルインスタンス準拠`）。この検証が通過しないとテスト自体が error になる設計であり、各テストケースは「socket 親 `0700` 強制が有効」な状態でのみ実行される。

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

#### 10.4.8 インジェクション境界値（TC-F-I11）

> `basic-design/security.md` は「rusqlite パラメータバインディング・`RecordLabel::try_new`・`VaultPaths::new` 検証に委譲」と規定する。本 §10.4.8 はその委譲が CLI → IPC V2 → daemon 経路で正しく機能するかを結合テストレイヤで証明する（OWASP A03 インジェクション防御）。ユニットテストで個別委譲先を検証するだけでは CLI 経由の連結経路での防御が保証されないため、結合テストレイヤでの確認が必要。

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-I11a | basic-design/security.md §OWASP A03 | plaintext vault + `DaemonSpawn` | `shikomi add Text "; DROP TABLE records;--" "value"` | exit 1 + `CliError::InvalidLabel` 由来エラー文言。後続 `shikomi list` で records テーブルが**消えていない**（SQL インジェクション防御の結合経路委譲確認、`RecordLabel::try_new` が CLI → IPC V2 → daemon 経路で機能することを証明）|
| TC-F-I11b | basic-design/security.md §OWASP A03 | `SHIKOMI_VAULT_DIR` env を使用（`DaemonSpawn` 不要、`#[serial]` 直列化）| `SHIKOMI_VAULT_DIR=../../../../etc/passwd` を設定して `shikomi list` 実行 | exit 1 + `PersistenceError::InvalidVaultDir` 由来エラー文言（`VaultPaths::new` パストラバーサル防衛の委譲確認）。`/etc/` 配下に vault.db が**生成されない**。シェルメタ文字（`` ` `` / `$(...)` 等）を含む VAULT_DIR 値でも同様に拒否 |

**TC-F-I11b Windows CI 扱い**: `#[cfg_attr(target_os = "windows", ignore = "path traversal boundary is /etc (Unix only), Windows equivalent covered by VaultPaths::new unit test (test-design integration.md §10.4.8, unlock condition: add Windows-specific traversal boundary TC)")]`

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
| `crates/shikomi-cli/tests/vault_subcommands.rs` | TC-F-I01, I02, I02b, I03, I03b, I04, I05, I06, I07, I07c, I08, I09, I09b, I11a, I11b, I12 | vault 管理サブコマンドの CLI→IPC V2 結合経路 + インジェクション境界値。`DaemonSpawn` + `expectrl` PTY 使用 |
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
| OWASP A03 インジェクション防御（basic-design/security.md 委譲確認、結合経路証明）| TC-F-I11a, TC-F-I11b |

---

## 11. Sub-F vault アクセシビリティ出力 結合テスト（TC-F-A01〜A05）

> Issue #78 / #74-D。  
> SSoT: `vault-encryption/test-design/sub-f-cli-subcommands/index.md §15.7`（Rev1）

### 11.1 設計方針

- **テスト対象**: `shikomi vault encrypt --output {print,braille,audio}` のアクセシビリティ出力経路 + `SHIKOMI_ACCESSIBILITY=1` 自動切替 + umask 077 ファイル権限
- **エントリポイント**: §10 同様、`assert_cmd::Command::cargo_bin("shikomi")` で実バイナリ呼び出し（clap パース込み）
- **daemon 依存**: `vault encrypt` は IPC V2 経由で daemon が暗号化処理を担うため、全 TC で `DaemonSpawn` を使用（§10.2 と同一セットアップ）
- **TTY 入力**: C-38 stdin パイプ拒否により、パスフレーズは `expectrl` PTY 経由で入力する
- **ファイルガード**: `crates/shikomi-cli/tests/accessibility_paths.rs` の先頭に `#![cfg(unix)]` を付与。`expectrl` PTY は Unix 専用かつ `umask` が Unix 固有であるため、Windows CI はファイル全体がコンパイル対象外となる（3-OS matrix のうち ubuntu + macOS でのみ実行）
- **stdout バイナリキャプチャ**: `--output print`（PDF）/ `--output braille`（BRF）はバイナリ出力のため、`assert_cmd::Output.stdout: Vec<u8>` を直接参照してバイト列でアサートする（`predicates::str::contains` ではない）
- **liblouis FFI 不採用**: braille 変換は `shikomi-daemon` 内の自前 wordlist テーブルで実装（SSoT §15.2 確定。FFI 依存・外部共有ライブラリ不要）
- **出力キャプチャ方式の使い分け**: TC-F-A01 / A02 は `assert_cmd::Output.stdout: Vec<u8>` でバイト列キャプチャ（ファイル未生成）。TC-F-A05 は `> out.pdf` シェルリダイレクト + `std::fs::metadata` で mode 検証（stdout キャプチャ不使用）

### 11.2 外部 I/O 依存マップ

| 外部 I/O | 方針 | characterization 状態 |
|---|---|---|
| **`shikomi-daemon` プロセス** | §10.2 と同一: `DaemonSpawn` で実子プロセス起動 + socket 親 `0700` + `Drop` で `kill()`→`wait()` | 既存資産再利用 |
| **TTY（passphrase）** | `expectrl` PTY 経由で passphrase を入力（C-38 前提、§10.3 TTY 行と同じ dev-dep 再利用）| 既存資産再利用 |
| **PDF 出力（--output print）** | `assert_cmd::Output.stdout` の `Vec<u8>` でキャプチャ。magic byte / EOF marker をバイト列 assert | 不要（stdout キャプチャ）|
| **BRF 出力（--output braille）** | 同上。Unicode braille 範囲（U+2800..U+28FF）または ASCII `.brf` 行末でアサート。liblouis FFI なし（自前 wordlist）| 不要（stdout キャプチャ）|
| **Audio 出力（--output audio）** | CI では fake `say` / `espeak` バイナリを `PATH` 先頭に配置して spawn を観測。実スピーカー出力は要求しない | fake バイナリ用フィクスチャ要整備 |
| **umask（TC-F-A05）** | `unix::process::CommandExt::pre_exec(|| { unsafe { libc::umask(0o077); Ok(()) } })` で子プロセス前に umask 設定し、`shikomi vault encrypt --output print > out.pdf` をシェルリダイレクト付きで実行。`std::fs::metadata("out.pdf").permissions().mode() & 0o777` で `0o600` を assert（stdout キャプチャ不使用）| Unix 専用（ファイルガードで保護）|

### 11.3 CI スキップ条件と `#[ignore]` reason 文字列規約

**ファイルレベルガード（全 TC）**: `#![cfg(unix)]` により Windows CI では全 TC が自動的にコンパイル対象外となる（`#[ignore]` 個別付与不要）。

**TC-F-A03 個別スキップ**: CI 環境に `say`（macOS）/ `espeak`（Linux）が PATH 未登録の場合、fake バイナリフィクスチャが未整備として skip。reason 文字列（v8.4 規約 §10.3 準拠）:

```
"requires fake TTS binary in PATH (audio spawn gate,
 test-design integration.md §11.3,
 unlock condition: add fake_say fixture to tests/helpers/ and register in CI workflow)"
```

**TC-F-A03 実装注意**: fake TTS バイナリへのニーモニックテキスト渡しを tempfile に平文記録することは**禁止**（vault secret 漏洩リスク、OWASP A02）。spawn 確認は CLI の `stdout pid: N` 形式出力で行う（§11.4 TC-F-A03 参照）。

**TC-F-A04 個別スキップ**: `SHIKOMI_ACCESSIBILITY=1` の自動切替先が OS オーディオパスに依存し、fake TTS が PATH 未登録の場合に skip。reason 文字列（v8.4 規約 §10.3 準拠）:

```
"SHIKOMI_ACCESSIBILITY=1 auto-select may trigger audio path requiring TTS binary (audio auto-select gate,
 test-design integration.md §11.3,
 unlock condition: implementation guarantees print/braille fallback when TTS unavailable, or fake TTS registered in CI)"
```

### 11.4 テストケース一覧（TC-F-A01〜A05 / SSoT §15.7 1:1 対応）

> **共通前提条件**: `DaemonSpawn::new()` によるセキュリティ契約（socket 親 `0700` + stat fail fast）が全 TC に適用される（§10.4 冒頭と同一）。

| TC-ID | SSoT 受入基準 | 前提条件 | 操作 | 期待結果 |
|-------|-------------|---------|------|---------|
| TC-F-A01 | EC-F1 / SSoT §15.7 A01 | plaintext vault + `DaemonSpawn` | `shikomi vault encrypt --output print`（`expectrl` PTY 経由 passphrase）| exit 0 + `stdout` バイト列が `%PDF-1.7`（magic byte）および `%%EOF`（終端 marker）を含む。24 語ニーモニックが 36pt 相当のコンテンツとして PDF 本文に埋め込まれている（テキスト抽出 / 構造解析で確認）。vault secret 値（`SECRET_TEST_VALUE` 相当のバイト列）が `stdout` に含まれない（OWASP A02 情報漏洩防衛）|
| TC-F-A02 | EC-F1 / SSoT §15.7 A02 | plaintext vault + `DaemonSpawn` | `shikomi vault encrypt --output braille`（`expectrl` PTY 経由）| exit 0 + `stdout` バイト列が U+2800..U+28FF Unicode braille 範囲のコードポイントを含む（または ASCII BRF 行末 `\r\n` 形式）。Grade 2 短縮形エンコード（例: "the" → `⠮`）が自前 wordlist テーブルで正しく生成されている。vault secret 値のバイト列が BRF `stdout` に含まれない（OWASP A02 情報漏洩防衛）|
| TC-F-A03 | SSoT §15.7 A03 | plaintext vault + `DaemonSpawn` + fake `say`/`espeak` を `PATH` 先頭配置（`#[ignore]` 付き）| `shikomi vault encrypt --output audio`（`expectrl` PTY 経由）| exit 0 + `stdout` に `pid: N`（整数）形式で TTS サブプロセス ID が出力される（CLI が spawn した証跡）。env allowlist 通過確認（許可 env のみ TTS に渡されることを stderr / stdout で確認）。fake TTS は受け取った引数を tempfile に平文記録しない（OWASP A02、dictation 学習 prefs 汚染なし）|
| TC-F-A04 | SSoT §15.7 A04 | plaintext vault + `DaemonSpawn` + `SHIKOMI_ACCESSIBILITY=1` env（`#[ignore]` 候補: §11.3 reason 文字列参照）| `shikomi vault encrypt`（`--output` フラグなし、`expectrl` PTY 経由）| exit 0。stdout / stderr に print / braille / audio いずれかの出力形式が現れること。レコード内容が平文で stdout / stderr に露出しないこと（grep 0 件）|
| TC-F-A05 | SSoT §15.7 A05 | plaintext vault + `DaemonSpawn` + `CommandExt::pre_exec` unsafe ブロックで umask `0o077` 設定 | `shikomi vault encrypt --output print > out.pdf`（`expectrl` PTY 経由、`>` リダイレクトでファイル生成）| exit 0。`std::fs::metadata("out.pdf").permissions().mode() & 0o777 == 0o600`（owner read/write のみ、umask `0o077` の反映確認）。`/tmp` 以下に vault.db 関連の中間ファイルが生成されない（`/tmp` mtime 変化なし）|
| TC-F-A06 | OWASP A01 / SSoT §15.7 A01-guard | 暗号化 vault（**Locked**）+ `DaemonSpawn` | `shikomi vault encrypt --output print`（`expectrl` PTY 経由）| exit 1 以上 + `CliError::AlreadyEncrypted` または `VaultLocked` 由来エラー文言。`stdout` に `%PDF-1.7` バイト列が含まれない（Locked vault でアクセシビリティ出力が生成されない、OWASP A01 認可バイパス防衛確認）|

### 11.5 `accessibility_paths.rs` 責務分割

| テストファイル | TC | 責務 |
|-------------|-----|------|
| `crates/shikomi-cli/tests/accessibility_paths.rs` | TC-F-A01〜A06 | vault encrypt アクセシビリティ出力（PDF / BRF / Audio）+ `SHIKOMI_ACCESSIBILITY=1` 自動切替 + umask 077 権限検証 + Locked vault 認可バイパス防衛（OWASP A01）。`#![cfg(unix)]` ファイルガード |

**共通インフラ**: §10.6 と同一（`DaemonSpawn` / `common/mod.rs` / `common/fixtures.rs`）。TC-F-A03 用の fake TTS バイナリフィクスチャは `tests/helpers/fake_tts.rs` として工程3（銀時実装）で追加する（`#[ignore]` 解除条件）。

### 11.6 カバレッジ対象（Sub-F アクセシビリティ、SSoT §15.3 対応）

| 受入基準 / 契約 | カバー TC |
|--------------|----------|
| EC-F1（encrypt）アクセシビリティ出力 3 形式（PDF / BRF / Audio）| TC-F-A01, TC-F-A02, TC-F-A03 |
| `SHIKOMI_ACCESSIBILITY=1` 自動切替 + exit 0 + 情報漏洩なし | TC-F-A04 |
| umask 077 出力権限 `0o600` + `/tmp` 中間ファイル生成禁止 | TC-F-A05 |
| OWASP A02 PDF / BRF バイナリへの vault secret 混入防衛（情報漏洩陰性確認）| TC-F-A01, TC-F-A02 |
| OWASP A01 Locked vault + `--output` 認可バイパス防衛 | TC-F-A06 |

---

*この文書は `index.md` の分割成果。ユニットテストは `unit.md`、E2E は `e2e.md`、CI は `ci.md` を参照*
