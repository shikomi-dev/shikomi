# 結合テスト設計 — vault-persistence（概要・前提条件）

> 本ディレクトリは `test-design/index.md` の §5 に相当する。テストマトリクス・モック方針・実行手順は `../../index.md` を参照。

> **ツール選択根拠**: このシステムは Rust ライブラリ crate であり、エントリポイントは Rust 公開 API（`SqliteVaultRepository::save` / `load` / `exists`）。Rust の統合テスト（`crates/shikomi-infra/tests/` 配下）で `tempfile::TempDir` を使い、実際の SQLite ファイルに対して結合テストを行う。外部 API / 外部サービスへの依存はなく、モックは不要（全て本物の `rusqlite` + ファイルシステムを使用）。OS パーミッション検証ケースは `#[cfg(unix)]` でガードし Windows CI では自動スキップ、Windows 固有 ACL / file-handle semantics 検証ケースは `#[cfg(windows)]` でガードし Linux/macOS CI では自動スキップ。

> **`#[cfg(windows)] #[ignore]` での回避禁止**（Issue #65 由来の防衛線）: Win 固有の TC（TC-I24〜I29 等）を `#[ignore]` で囲んで CI green を装う PR は問答無用で**却下対象**（CI スコープ錯覚 = Bug-F-003 の再演温床、`../basic-design/error.md` §禁止事項 §Windows rename retry の盲目採用は禁止 と整合）。Win ランナーが落ちる場合は根本原因の articulate を伴った修正を必須とする。テスト担当（涅マユリ）が `#[ignore]` を解剖時に発見した場合は実験不全として即時差戻し。

---

## 0. Issue #65 由来の外部 I/O 依存マップ

Issue #65（Windows AtomicWrite rename 失敗）の修正対象が触る外部 I/O 境界を全て列挙し、characterization 状態を明示する（assumed mock 禁止、テスト戦略ガイドの「外部I/O依存マップ」要件に対応）。

| 外部 I/O 依存 | 経由クレート / API | テスト方式 | raw fixture | factory | characterization 状態 |
|------------|-----------------|----------|-----------|---------|---------------------|
| SQLite ファイル（`vault.db.new`、`-wal` / `-shm` / `-journal` サイドカー含む） | `rusqlite::Connection`（バンドル SQLite） | **本物** を `tempfile::TempDir` 配下で使用（モック不要、結合テスト方針に従う） | 不要（実 SQLite を直接利用） | 不要 | **済** — 実 SQLite で結合テスト可能 |
| ファイルシステム rename | `std::fs::rename`（Unix: `rename(2)` / Windows: 内部で `MoveFileExW`） | **本物** を tempdir で使用 | 不要 | 不要 | **済** — `std::fs` 直接利用 |
| Windows rename 一過性エラー（`ERROR_ACCESS_DENIED 5` / `ERROR_SHARING_VIOLATION 32` / `ERROR_LOCK_VIOLATION 33`） | OS 直返（`std::io::Error::raw_os_error()` で識別） | TC-I29 で並行 read open による race を**実環境で再現**（モック不要） | **要保存**: PR #64 失敗 CI ログ 5 件のスタックトレース全文 https://github.com/shikomi-dev/shikomi/actions/runs/24950291068/job/73058649443 を `tests/fixtures/characterization/raw/issue65/pr64_failure_log.txt` に保存（マスク不要、公開 CI ログ） | 不要（一過性エラーは OS 直返、合成不要） | **要起票** — 実装者は本ファイルを修正前のベースラインとして固定し、修正後の CI ログ（5 件 PASS）と diff 比較する責務を負う |
| `MoveFileExW` Win32 API（`ReplaceFileW` 経由） | `windows` crate `Win32::Storage::FileSystem::ReplaceFileW`（cfg(windows)） | **本物** を実 Windows CI ランナーで実行（仮想環境 Wine では `MoveFileExW` 挙動が再現できないため）| 不要 | 不要 | **済** — `test-infra-windows` ジョブで raw 検証 |

**reviewer 却下基準**:
- raw fixture（PR #64 失敗ログ）が `tests/fixtures/characterization/raw/issue65/` に保存されないまま実装 PR 提出 → **[却下]**
- TC-I29 が `mockall` 等で `MoveFileExW` をモックする → **[却下]**（実環境の race 検出にならない、assumed mock 違反）
- `test-infra-windows` ジョブを CI 必須 check から外す PR → **[却下]**（CI スコープ錯覚再演）

---

## 0.1 Issue #86: Windows `TempDir` DACL 正規化要件（`normalize_tempdir_dacl` ヘルパ設計）

> **問題軸**: Issue #65（VM レベル rename 遅延 = Bug-G-002〜G-008 articulate 済）とは**独立した別問題**。Issue #86 の問題軸は「fixture 生成時の DACL 継承状態」であり、Issue #65 の「AtomicWriter rename retry タイミング」とは直交する。

### 問題の根拠

`tempfile::TempDir::new()` が `windows-latest` CI ランナー上で作成するディレクトリは、**親ディレクトリ（`%TEMP%`）から DACL を継承した状態**（`SE_DACL_PROTECTED` ビット未設定）を持つ。

`repo.load()` は内部で `load_inner` → `PermissionGuard::verify_dir(dir)` を呼ぶ（`repository.rs:178`）。この 4 不変条件チェックは

① `SE_DACL_PROTECTED` セット確認 → **継承 DACL = 不変条件① 違反 = `InvalidPermission` 返却**

であるため、vault ディレクトリが DACL 正規化されていない状態で `repo.load()` を呼ぶと、**テストが意図したアサーション（例: `EncryptionUnsupported`、`Sqlite` エラー）に到達する前に `InvalidPermission` で先行失敗する**。

なお `repo.from_directory(path)` は `VaultPaths::new()` を呼ぶのみで DACL チェックは一切行わない。また `repo.save()` は内部で `ensure_dir`（DACL 設定）を呼ぶため、**`save()` を先に実行した場合は DACL が正しく設定され、後続の `load()` での `verify_dir` は通過する**。DACL 継承問題が顕在化するのは「`save()` を経由せずに vault.db を事前生成し、`load()` を直接呼ぶ」パターンのみ。

### 影響テスト一覧

| テストファイル | 関数 / TC | 症状 | 対策状況 |
|-------------|---------|-----|---------|
| `crates/shikomi-infra/tests/integration_error.rs` | TC-I13（ゼロバイト vault.db）/ TC-I14（不正バイト） | `verify_dir` 先行 `InvalidPermission`、想定 `Sqlite` / `SchemaMismatch` エラーに未到達 | `normalize_tempdir_dacl` 追加が必要 |
| `crates/shikomi-cli/tests/it_usecase_add.rs` | `tc_it_012_add_record_on_encrypted_vault_returns_encryption_unsupported` | `load()` → `verify_dir` 先行失敗 → `EncryptionUnsupported` 未到達 | `common::fresh_repo()` → `tighten_perms_unix`（Windows では `ensure_vault_dir`）経由で対応済み |
| `crates/shikomi-cli/tests/it_usecase_edit.rs` | `tc_it_023_edit_record_on_encrypted_vault_returns_encryption_unsupported` | 同上 | 同左 |
| `crates/shikomi-cli/tests/it_usecase_remove.rs` | `tc_it_033_remove_record_on_encrypted_vault_returns_encryption_unsupported` | 同上 | 同左 |
| `crates/shikomi-cli/tests/it_usecase_cross.rs` | `tc_it_040_all_usecases_on_encrypted_vault_return_encryption_unsupported_without_side_effects` | 同上 | 同左 |
| `crates/shikomi-cli/tests/e2e_edit.rs` | 全テスト（`setup_vault_with_record` 経由） | 同上 | `tighten_perms_unix` 呼出済み |
| `crates/shikomi-cli/tests/e2e_encrypted.rs` | TC-E2E-040 / TC-E2E-041（`setup_encrypted_vault` 経由） | 同上 | `tighten_perms_unix` + `create_encrypted_vault` 双方で対応済み |

### `normalize_tempdir_dacl` ヘルパ設計

**呼出タイミング**: `TempDir::new()` 直後、`repo.load()` を呼ぶ前（`save()` を先に呼ぶ場合は `save` 内部の `ensure_dir` が DACL を設定するため不要だが、fixture として vault.db を事前生成するケースでは必須）

**戻り値 / エラー時振る舞い**: 戻り値なし（`panic!`）。テスト環境セットアップ失敗はテスト環境が壊れていることを意味するため、`Result` で呼び出し元に判断を委ねず即断する（Fail Fast 原則、`expect()` を使用）

**DRY 設計 — オプション (b) 既採用**: `shikomi-infra` は `persistence::ensure_vault_dir` および `ensure_vault_file` を `#[cfg(any(test, feature = "test-fixtures"))]` でゲートして公開している（`src/persistence/mod.rs:54-72`）。これがクロスクレート共有の SSoT であり、コード複製は不要。

| 配置場所 | 実装方針 |
|---------|---------|
| `crates/shikomi-infra/tests/helpers/mod.rs`（`#[cfg(windows)]` ガード） | `shikomi_infra::persistence::ensure_vault_dir(path).expect(...)` を薄くラップした `normalize_tempdir_dacl` 関数を追加 |
| `crates/shikomi-cli/tests/common/mod.rs` | `tighten_perms_unix`（Windows 実装）が `shikomi_infra::persistence::ensure_vault_dir` を直接呼出（既実装、`common/mod.rs:55`） |
| `crates/shikomi-cli/tests/common/fixtures.rs` | `create_encrypted_vault` が dir に `ensure_vault_dir`、vault.db に `ensure_vault_file` を呼出（既実装、`fixtures.rs:76, 126`） |

cli 側はコード複製なし。infra 側も `ensure_vault_dir` への委譲のみ。

**アルゴリズム（`PermissionGuard::ensure_dir` / `ensure.rs` の内部実装）**:

1. `GetNamedSecurityInfoW(path, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION, ...)` で現在の所有者 SID を取得
2. `SetEntriesInAclW` で所有者 SID のみの `ACCESS_ALLOWED_ACE`（`AccessMask = EXPECTED_DIR_MASK`（`FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_TRAVERSE` = `0x0012_01BF`））から新規 DACL を構築
3. `SetNamedSecurityInfoW(path, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, ...)` で `SE_DACL_PROTECTED` ビットを立てた状態で新規 DACL を適用（継承 ACE を破棄）

   > **権限注記（OWASP A01 — 権限昇格リスクなし）**: `SetNamedSecurityInfoW` に `DACL_SECURITY_INFORMATION` フラグを渡す場合、ファイル / ディレクトリの**所有者**であれば `SE_SECURITY_PRIVILEGE` は不要。`SE_SECURITY_PRIVILEGE` が必要なのは `SACL_SECURITY_INFORMATION` 変更時のみ（MSDN 明記）。CI ランナーでは `RUNNER_USER` が対象 TempDir の所有者であるため、追加権限昇格なしで DACL 変更が完了する。

**副作用**: `PermissionGuard::verify_dir` の 4 不変条件（①`SE_DACL_PROTECTED` / ②`AceCount==1` / ③トラスティ SID = 所有者 SID / ④`AccessMask == EXPECTED_DIR_MASK`）を全て満たす状態に正規化する。

### reviewer 却下基準

- Windows 上で vault.db を事前生成し `repo.load()` を呼ぶテスト（`#[cfg(windows)]` 付き TC）で `normalize_tempdir_dacl`（または `ensure_vault_dir`）の呼出が欠落している PR → **[却下]**（`verify_dir` が DACL 継承により先行失敗し、TC の意図したアサーションが検証されない）
- `SqliteVaultRepository` に `verify_dir` スキップ付きのテスト専用コンストラクタ / フラグを追加して回避する実装 → **[却下]**（本番 API の意味論を変えずに fixture を正規化するのが正しい設計。`PermissionGuard::verify_dir` のスキップは Fail Fast 原則違反）
- cli 側テストで `ensure_vault_dir` / `ensure_vault_file` の呼出を省き、独自の Win32 API コードをコピーして使う実装 → **[却下]**（DRY 違反、`shikomi-infra` の `test-fixtures` feature 経由の共通実装を使え）

---

*TC一覧: [TC-I01〜I11](./tc-i01-i11.md) / [TC-I12〜I23](./tc-i12-i23.md) / [TC-I24〜I29-D](./tc-i24-i29d.md) / 改訂履歴: [../changelog.md](../changelog.md)*
