# テスト設計書 — Sub-D (#42) 暗号化 Vault リポジトリ+マイグレーション

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-d-repository-migration.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->

## 13. Sub-D (#42) テスト設計 — 暗号化 Vault リポジトリ + マイグレーション

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#42](https://github.com/shikomi-dev/shikomi/issues/42) |
| 対象 PR | #57（`b39b946`、設計フェーズ）|
| 対象成果物 | `detailed-design/repository-and-migration.md`（新規 469 行）/ `detailed-design/{crypto-types,errors-and-contracts,index}.md`（EDIT）/ `basic-design.md`（Sub-D 追記）/ `requirements.md`（REQ-S06 / S07 / S13 / MSG-S08 / S10 / S11 / S13 / S14 確定）/ **横断**: `vault-persistence/{requirements.md, basic-design/security.md, detailed-design/flows.md, test-design/integration.md}`（REQ-P11 改訂、TC-I03/I04 退役 + TC-I04a 新設）|
| 設計根拠 | `repository-and-migration.md` `VaultMigration` 6 メソッド契約 / `VaultEncryptedHeader` ヘッダ独立 AEAD タグ / `RecoveryDisclosure`（C-19）/ `DecryptConfirmation`（C-20）/ atomic write 失敗で原状復帰（C-21）、Sub-C Rev1 で凍結した MSG-S10/S11 文言指針 + Sub-D 引継ぎ表 5 系列責務 |
| 対象 crate | `shikomi-core`（`RecoveryDisclosure` / `RecoveryWords` / `DecryptConfirmation` / `MigrationError` 新規）+ `shikomi-infra`（`VaultMigration` service + `VaultEncryptedHeader` + `EncryptedRecord` 新規）|
| **Sub-D TC 総数** | **26 件**（ユニット 18 + 結合 5 + property 2 + E2E 1 = 26、Sub-A/B/C と同型）|

### 13.1 Sub-D テストレベルの読み替え（マイグレーション + 暗号化 vault 用）

Sub-D は **Sub-A/B/C で凍結した型・契約・MSG 文言指針を初めて実環境（SQLite + atomic write）で結合**する Sub。Vモデル対応：

| テストレベル | 通常の対応 | Sub-D での読み替え | 検証手段 |
|---|---|---|---|
| **ユニット** | メソッド単位、ホワイトボックス | 新規 5 型（`VaultEncryptedHeader` / `EncryptedRecord` / `RecoveryDisclosure` / `RecoveryWords` / `DecryptConfirmation` + `MigrationError`）の不変条件、`VaultMigration::change_password` の VEK 不変契約、ヘッダ AEAD タグ canonical_bytes 仕様、C-19/C-20 型レベル所有権・引数強制 | `cargo test -p shikomi-core` + `cargo test -p shikomi-infra --lib persistence::vault_migration` |
| **結合** | モジュール連携、契約検証 | (a) **SQLite 実接続**で `encrypt_vault` / `decrypt_vault` / `unlock_with_password` / `unlock_with_recovery` / `rekey` 各経路を実行、`vault.db` ファイルを実書込で確認、(b) **atomic write 失敗模擬**（`VaultRepository` への `--save 直前 SIGKILL 等価フック`）で `.new` cleanup + 原状復帰（C-21）、(c) **REQ-P11 改訂**: v1 暗号化 vault 受入 / v999 拒否 (`UnsupportedVaultVersion`) | `cargo test -p shikomi-infra --test vault_migration_integration` + tempdir SQLite |
| **property** | ランダム入力での invariant | (a) **マイグレーション往復不変**: 任意 records → encrypt → decrypt → 元 records と一致、(b) **改竄検出**: 任意ヘッダ位置 / per-record 位置の byte flip → `MigrationError::Crypto(AeadTagMismatch)` を確実に検出 | `proptest` 1000 ケース（Sub-C と同型） |
| **E2E** | 完全ブラックボックス、ペルソナ | 後続 Sub-E/F 実装者ペルソナ（野木拓海）が `cargo doc -p shikomi-infra --open` + サンプル経路から `vault encrypt → unlock → recovery-show → decrypt → rekey → change-password` の 6 メソッド連鎖を再構築できる | 人手レビュー + `cargo doc` 対話 |

### 13.2 外部 I/O 依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---|---|---|---|
| **SQLite (`rusqlite`、tempdir 経由)** | — | `tempfile::tempdir()` で test-local DB | **不要**（既存 `vault-persistence` integration test と同型、`vault-persistence/test-design/integration.md` の helper を再利用）|
| **atomic write `.new` cleanup** | `vault-persistence/tests/atomic_write/*.rs` 既存 fixture | — | **既存資産再利用**（vault-persistence TC-I06 同型、Sub-D は呼出側として委譲のみ）|
| **NIST CAVP / RFC KAT** | Sub-C の `crypto::aead::kat` 既存 | — | **永続固定**（Sub-D は KAT を直接持たず、Sub-C 経由で間接使用）|

**理由**: Sub-D は Sub-A/B/C の組合せ層、KDF / AEAD 計算の正しさは Sub-B/C で担保済。Sub-D 固有の検証対象は **(1) 6 メソッドの責務分離**、**(2) atomic write 経路の `.new` cleanup**、**(3) ヘッダ AEAD タグの canonical_bytes**、**(4) 型レベル所有権/引数強制（C-19/C-20）**。

### 13.3 Sub-D 受入基準（REQ-S06 / S07 / S13 + 契約 C-17〜C-21）

| 受入基準ID | 内容 | 検証レベル |
|---|---|---|
| DC-1 | `VaultMigration::encrypt_vault` 平文 → 暗号化往復成功（atomic write 完了 + ヘッダ + 全レコード AEAD 暗号化）| 結合 |
| DC-2 | `VaultMigration::decrypt_vault` 暗号化 → 平文往復成功（DecryptConfirmation 通過後、全レコード復号 + atomic write）| 結合 |
| DC-3 | `unlock_with_password` / `unlock_with_recovery` の 2 経路で同一 VEK を復元（鍵階層検証）| 結合 |
| DC-4 | `rekey` 後の VEK 変更 + 全レコード再暗号化、旧 VEK で復号失敗（`AeadTagMismatch`）| 結合 |
| DC-5 | `change_password` で **VEK 不変**（O(1)）+ `wrapped_VEK_by_pw` のみ更新 | ユニット + 結合 |
| **C-17** | ヘッダ独立 AEAD タグで `kdf_params` / `wrapped_VEK_*` / `nonce_counter` 改竄を検出（`AeadTagMismatch` で 1 variant 収束）| ユニット + property |
| **C-18** | ヘッダ AEAD AAD = `Aad::HeaderEnvelope(canonical_bytes)`、フィールド順序固定で永続化と検証で再現性確保 | ユニット |
| **C-19** | `RecoveryDisclosure::disclose(self)` 所有権消費で **2 度呼べない**（move 後再使用 compile_fail）| ユニット（compile_fail）|
| **C-20** | `DecryptConfirmation` 引数必須（`_private: ()` で外部 crate 直接構築禁止、`--force` でも省略不可）| ユニット（compile_fail）|
| **C-21** | マイグレーション中の atomic write 失敗で `.new` cleanup + 原状復帰（`vault-persistence` `.new` 残存検出経路に委譲）| 結合 |
| DC-6 | `RecoveryWords` `Display` 未実装、`serde::Serialize` 未実装（永続化禁止、Vek 同型）| ユニット（compile_fail）|
| DC-7 | `MigrationError` **8 variant** 網羅（Sub-D Rev2 で `ConfirmationRequired` を Sub-F 責務移譲 → 削除、最終 variant 数 = `Crypto / Persistence / Domain / AlreadyEncrypted / NotEncrypted / PlaintextNotUtf8 / RecoveryAlreadyConsumed / AtomicWriteFailed`）、`#[non_exhaustive]` で外部 crate からの破壊的変更耐性 | ユニット（match 網羅）|
| DC-8 | MSG-S10 文言: 過信防止「断定禁止」+ 過小評価回避「vault.db 信頼禁止」+ 次の一手「バックアップから復元」（Sub-C Rev1 凍結指針継承）| ユニット（文字列 assert）|
| DC-9 | MSG-S11 文言: `vault rekey` 誘導 + **残操作猶予数値非表示**（`NonceCounter::current()` を MSG に含めない）| ユニット（grep）|
| DC-10 | MSG-S13 文言: 原状復帰明示「vault.db は変更前の状態に戻っています」+ 段階情報（過信防止）| ユニット（文字列 assert）|
| DC-11 | MSG-S14 文言: 二段確認「`DECRYPT` キーワード + パスワード再入力」+ `--force` 不可明示 | ユニット（文字列 assert）|
| DC-12 | REQ-P11 改訂: v1 暗号化 vault 受入 + v999 拒否（`UnsupportedVaultVersion`）| 結合（vault-persistence 横断）|
| DC-13 | shikomi-core への AES-GCM / OsRng 不混入維持（Sub-A/B/C 累積契約の Sub-D 段階での回帰検証）| 結合（grep）|

### 13.4 Sub-D テストマトリクス

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---|---|---|---|---|
| TC-D-U01 | C-17 / C-18 | `VaultEncryptedHeader::canonical_bytes_for_aead` がフィールド順序固定（kdf_params + wrapped_VEK_by_pw + wrapped_VEK_by_recovery + nonce_counter）で出力、構築時と検証時で **bit-exact 一致** | ユニット | 仕様凍結 |
| TC-D-U02 | C-17 | `HeaderAeadEnvelope` 構築 → `expose_within_crate()` 経由で AEAD 鍵バイト借用 → 検証ラウンドトリップ成功 | ユニット | 鍵経路 |
| TC-D-U03 | DC-5 | `change_password` 模擬: 新 `wrapped_VEK_by_pw` 計算後、旧 VEK 32B と新 wrapped を unwrap した VEK が **bit-exact 一致**（O(1) 契約）| ユニット | VEK 不変 |
| TC-D-U04 | C-19 | `RecoveryDisclosure::disclose(self)` 所有権消費後、**move 後再使用が compile_fail**（doctest）| ユニット | compile_fail |
| TC-D-U05 | C-19 | `RecoveryDisclosure::drop_without_disclose(self)` で内部 `RecoveryMnemonic` の Drop 連鎖発火（Sub-A C-1 zeroize 維持）| ユニット | Drop 連鎖 |
| TC-D-U06 | DC-6 | `RecoveryWords` への `serde::Serialize` 実装は **compile_fail**（永続化禁止）| ユニット | compile_fail |
| TC-D-U07 | DC-6 | `RecoveryWords` への `Display` 実装は **compile_fail**（誤表示防止）| ユニット | compile_fail |
| TC-D-U08 | C-20 | `DecryptConfirmation::confirm()` が **引数ゼロ**で `DecryptConfirmation { _private: () }` を返す（Sub-D Rev2 で Clean Arch 観点から `subtle::ConstantTimeEq` 比較を Sub-F CLI/GUI 層に責務移譲、本関数は二段確認通過証跡を型レベルで閉じ込めるのみ） | ユニット | 通過証跡 |
| TC-D-U09 | C-20 / Sub-F 引継ぎ | **「DECRYPT」キーワード入力 + paste 抑制 + パスワード再入力 + `subtle::ConstantTimeEq` 比較**の二段確認**ロジック検証**は Sub-F CLI/GUI 層の責務（`shikomi-cli` / `shikomi-gui` で実装、Sub-F 内部のエラー型で表現）。本 TC は Sub-F 工程で詳細化、Sub-D 範囲では到達不能（Sub-D Rev2 で `MigrationError::ConfirmationRequired` も削除済、確認失敗は Sub-F 内部完結） | — | Sub-F 引継ぎ |
| TC-D-U10 | C-20 / Sub-F 引継ぎ | password 再入力不一致時のエラー UX は Sub-F CLI/GUI 層責務（shikomi-infra `confirm()` は引数ゼロのため検証経路を持たない、Clean Arch 整合）。本 TC は Sub-F 工程で詳細化 | — | Sub-F 引継ぎ |
| TC-D-U11 | C-20 | 外部 crate `tests/` から `DecryptConfirmation { _private: () }` 直接構築 → **compile_fail**（`_private` 非可視）| ユニット | compile_fail |
| TC-D-U12 | DC-7 | `MigrationError` **8 variant**（Sub-D Rev2 同期）: `Crypto(CryptoError) / Persistence(PersistenceError) / Domain(DomainError) / AlreadyEncrypted / NotEncrypted / PlaintextNotUtf8 / RecoveryAlreadyConsumed / AtomicWriteFailed { stage, source }` の `match` 網羅で `cargo check` 警告 0 件、`#[non_exhaustive]` 適用。**`ConfirmationRequired` は Sub-F 責務移譲で削除済**（前 Rev で 9 variant だった、Petelgeuse 工程5 Rev1 指摘で四層同期完了） | ユニット | enum 網羅 |
| TC-D-U13 | DC-8 | MSG-S10 i18n 翻訳辞書経由文言に **「断定」キーワード不在 + 「可能性」「いずれにせよ」「バックアップから復元」が含まれる** | ユニット | 文言 |
| TC-D-U14 | DC-9 | MSG-S11 i18n 翻訳辞書経由文言に **`NonceCounter::current()` 由来の数値（「あと N 回」等）が含まれない**（grep）| ユニット | 情報漏洩防衛 |
| TC-D-U15 | DC-10 | MSG-S13 i18n 翻訳辞書経由文言に **「変更前の状態に戻っています」が含まれる**（原状復帰明示）| ユニット | 文言 |
| TC-D-U16 | DC-11 | MSG-S14 i18n 翻訳辞書経由文言に **「DECRYPT」「パスワードを再入力」「`--force` でも省略不可」が含まれる** | ユニット | 文言 |
| TC-D-U17 | C-21 | `MigrationError::AtomicWriteFailed { stage }` で stage が `WriteTemp` / `FsyncTemp` / `FsyncDir` / `Rename` のいずれか | ユニット | enum 列挙 |
| TC-D-U18 | DC-13 / Clean Arch 回帰 | shikomi-core に `aes_gcm` / `OsRng` / `getrandom` の直接 import 不混入（Sub-A/B/C 累積、Sub-D で破壊しない）| ユニット（grep）| 回帰 |
| TC-D-I01 | DC-1 | tempdir SQLite で `encrypt_vault` 実行 → vault.db に暗号化済みヘッダ + 全レコード書込 + `unlock_with_password` で同 VEK 復元 + records 全件復号一致 | 結合 | 往復 |
| TC-D-I02 | DC-2 | 同上で `encrypt_vault → decrypt_vault(_, DecryptConfirmation::confirm(...).unwrap())` → 元平文 vault と records 全件 bit-exact 一致 | 結合 | 双方向 |
| TC-D-I03 | DC-3 / DC-4 | `encrypt_vault` 後、`unlock_with_password` と `unlock_with_recovery` 両方で **同一 VEK** 復元、その後 `rekey` 実行 → 新 VEK + 全レコード再暗号化、**旧 VEK での復号は `AeadTagMismatch`** | 結合 | 鍵階層 + rekey |
| TC-D-I04 | C-21 | atomic write `.new` 残存模擬（`VaultRepository::save` 失敗注入）→ `MigrationError::AtomicWriteFailed { stage }` 返却 + **vault.db ファイル原状（変更前バイト列）維持**を bit-exact 確認 | 結合 | atomic 保証 |
| TC-D-I05 | DC-12 / 横断 | **REQ-P11 改訂**: (a) v1 暗号化 vault を `vault-persistence::SqliteVaultRepository::load` で受入成功（旧 TC-I03/I04 退役後の置換、`vault-persistence/test-design/integration.md` TC-I03/I04 新内容と双方向参照）、(b) v999 偽造ヘッダで `Err(PersistenceError::UnsupportedVaultVersion)` 拒否（`vault-persistence` TC-I04a 新設と整合）| 結合 | 横断 |
| TC-D-P01 | DC-1 / DC-2 / property | proptest で任意 records (1..=128 件、各 label / payload 任意) → encrypt → decrypt 往復で **元 records 全件 bit-exact 一致**（1000 ケース、Sub-C TC-C-P02 同型）| property | 往復不変 |
| TC-D-P02 | C-17 / C-18 / property | proptest で encrypt 後、(a) ヘッダ `kdf_params` 任意 byte flip、(b) ヘッダ `wrapped_vek_by_pw` 任意 byte flip、(c) ヘッダ `wrapped_vek_by_recovery` 任意 byte flip、(d) **ヘッダ `nonce_counter` 任意 byte flip（u64 BE 8B、`0` 巻戻しシナリオ含む）**（Sub-D 工程5 服部指摘で追加、L1 攻撃者によるカウンタ巻戻し改竄を構造防衛、AAD 改訂 `nonce_counter (8B BE u64)` 含有と整合）、(e) per-record ciphertext 任意 byte flip、(f) ヘッダ AEAD タグ任意 byte flip、(g) per-record AEAD タグ任意 byte flip の **7 経路**全てで **`MigrationError::Crypto(AeadTagMismatch)` を確実に検出**（1000 ケース）| property | 改竄検出 |
| TC-D-E01 | 全契約 / Sub-E/F 統合 | 後続 Sub 実装者が `cargo doc -p shikomi-infra --open` で `VaultMigration` 6 メソッドの rustdoc サンプル呼出を読み、`vault encrypt → recovery-show → unlock → decrypt → rekey → change-password` の 6 連鎖フローを Sub-E/F 実装に流用できる | E2E | 逆引き可能性 |

### 13.5 Sub-D ユニットテスト詳細

#### ヘッダ独立 AEAD タグ仕様（C-17 / C-18）

| テストID | 入力 | 期待結果 |
|---|---|---|
| TC-D-U01 | `VaultEncryptedHeader { kdf_params, wrapped_pw, wrapped_recovery, nonce_counter }` 同一値で 2 回 `canonical_bytes_for_aead()` 呼出 | 戻り値 bit-exact 一致（決定論性 + フィールド順序固定）|
| TC-D-U02 | `HeaderAeadKey::from_kek_pw(&kek_pw)` → `AesGcmAeadAdapter::encrypt_record(&header_aead_key, ...)` でヘッダ AEAD タグ生成 | `decrypt_record(&header_aead_key, ...)` で復号成功、tag bit 1 つ flip → `Err(AeadTagMismatch)` |

#### `RecoveryDisclosure` / `RecoveryWords` 型レベル強制（C-19 / DC-6）

| テストID | コード片 | 期待結果 |
|---|---|---|
| TC-D-U04 | `let d = RecoveryDisclosure::new(mnemonic); let _ = d.disclose()?; let _ = d.disclose();` | compile_fail（move 後再使用、`d` は consumed）|
| TC-D-U05 | `let d = RecoveryDisclosure::new(mnemonic); d.drop_without_disclose();` 後にメモリパターン観測 | `RecoveryMnemonic` の `SecretBox<Zeroizing<...>>` が zeroize（Sub-A C-1 連鎖維持）|
| TC-D-U06 | `serde_json::to_string(&recovery_words)` | compile_fail（`Serialize` 未実装）|
| TC-D-U07 | `format!("{}", recovery_words)` | compile_fail（`Display` 未実装）|

#### `DecryptConfirmation` 二段確認（C-20）

| テストID | 入力 | 期待結果 |
|---|---|---|
| TC-D-U08 | `DecryptConfirmation::confirm()`（**引数ゼロ**、Sub-D Rev2 同期） | `DecryptConfirmation { _private: () }`（通過証跡として型レベルで閉じ込め）|
| TC-D-U09 | — | **Sub-F 引継ぎ**: 「DECRYPT」キーワード手入力 + paste 抑制 + 大文字検証は Sub-F CLI/GUI 層で `subtle::ConstantTimeEq` 比較実装、shikomi-infra `confirm()` には到達不能（Clean Arch 維持）|
| TC-D-U10 | — | **Sub-F 引継ぎ**: パスワード再入力 + `subtle::ConstantTimeEq` 比較 + 不一致時の MSG-S14 再表示は Sub-F CLI/GUI 層責務 |
| TC-D-U11 | 外部 crate（`tests/` integration test）から `DecryptConfirmation { _private: () }` 直接構築 | compile_fail（`_private` 非可視性、外部から構築不能）|

> **Sub-F 引継ぎ責務（TC-D-U09/U10 移譲先）**:
> - **`shikomi-cli`**: `vault decrypt` サブコマンドで「DECRYPT」キーワード手入力プロンプト + paste 抑制（rpassword crate）+ 大文字検証 + パスワード再入力 + `subtle::ConstantTimeEq` 比較 + 通過時に `DecryptConfirmation::confirm()` を呼出
> - **`shikomi-gui`**: 田中ペルソナ向けモーダルで同等 UI（input type="password" + paste 無効化属性 + ARIA 警告 + 通過時に IPC 経由 daemon に証跡渡し）
> - 検証 TC は Sub-F 工程の `test-design/sub-f-cli-and-gui.md` で詳細化（本 §13.10 引継ぎ表参照）

#### `MigrationError` + MSG 文言（DC-7〜DC-11）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-D-U12 | `match err: MigrationError { Crypto(_) ⇒ ..., Persistence(_) ⇒ ..., Domain(_) ⇒ ..., AlreadyEncrypted ⇒ ..., NotEncrypted ⇒ ..., PlaintextNotUtf8 ⇒ ..., RecoveryAlreadyConsumed ⇒ ..., AtomicWriteFailed { .. } ⇒ ..., }` を**ワイルドカード `_` 無し**で書く | `cargo check` 警告 0 件（**8 variant 全網羅**、Sub-D Rev2 で `ConfirmationRequired` 削除済）+ `#[non_exhaustive]` で外部 crate からの追加 variant 対応強制 |
| TC-D-U13 | i18n 翻訳辞書 `msg-s10.ja.txt` を grep | 「断定」**不在**、「可能性」「いずれにせよ」「バックアップから復元」**含有** |
| TC-D-U14 | i18n 翻訳辞書 `msg-s11.ja.txt` を grep | `\d+` 数字（`NonceCounter::current()` 由来）**不在**、「`vault rekey`」「鍵を再生成」**含有** |
| TC-D-U15 | i18n 翻訳辞書 `msg-s13.ja.txt` を grep | 「変更前の状態に戻っています」**含有** |
| TC-D-U16 | i18n 翻訳辞書 `msg-s14.ja.txt` を grep | 「DECRYPT」「パスワードを再入力」「`--force`」**含有** |
| TC-D-U17 | `MigrationError::AtomicWriteFailed { stage: AtomicWriteStage::Rename, source }` 構築 + match | stage variant が `WriteTemp` / `FsyncTemp` / `FsyncDir` / `Rename` の 4 値のみ |
| TC-D-U18 | `grep -rE "use aes_gcm::\|OsRng\|rand_core::OsRng\|getrandom::" crates/shikomi-core/src/` | 0 件（コメント除外）|

### 13.6 Sub-D 結合テスト詳細

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-D-I01 | `cargo test -p shikomi-infra --test vault_migration_integration encrypt_then_unlock_round_trip` | tempdir vault.db に暗号化書込、`unlock_with_password` で復元 records が 元 records と bit-exact 一致 |
| TC-D-I02 | 同 `encrypt_then_decrypt_round_trip` | `DecryptConfirmation::confirm("DECRYPT", &pw, &expected)?` 通過後、復号 vault が元平文 vault と一致 |
| TC-D-I03 | 同 `unlock_password_and_recovery_yield_same_vek` + `rekey_invalidates_old_vek` | (a) 同 VEK 復元、(b) rekey 後の旧 VEK で復号 → `Err(MigrationError::Crypto(AeadTagMismatch))` |
| TC-D-I04 | 同 `atomic_write_failure_preserves_original` | mock `VaultRepository::save` で I/O エラー注入 → `MigrationError::AtomicWriteFailed { stage }` + 元 vault.db バイト列維持（SHA-256 一致） |
| TC-D-I05 | 同 `req_p11_v1_accept_v999_reject` | (a) v1 暗号化 vault `load` 成功、(b) v999 偽造ヘッダ → `Err(PersistenceError::UnsupportedVaultVersion)` |

### 13.7 Sub-D property テスト詳細

| テストID | 入力空間 | invariant |
|---|---|---|
| TC-D-P01 | 任意 `records: Vec<Record>` (1..=128 件、各 label / payload 任意) + 任意 `MasterPassword` (zxcvbn 強度 ≥ 3 制約) | `encrypt_vault → unlock_with_password → records 比較` で **全件 bit-exact 一致**（1000 ケース）|
| TC-D-P02 | encrypt 完了後の vault.db バイト列に対し、(a) ヘッダ `kdf_params` 任意 byte flip、(b) ヘッダ `wrapped_vek_by_pw` 任意 byte flip、(c) ヘッダ `wrapped_vek_by_recovery` 任意 byte flip、(d) **ヘッダ `nonce_counter` 任意 byte flip（u64 BE 8B、明示的に `0` 巻戻しシナリオを含む）**、(e) per-record ciphertext 任意 byte flip、(f) ヘッダ AEAD タグ任意 byte flip、(g) per-record AEAD タグ任意 byte flip の 7 経路 | 全経路で `decrypt_vault` または `unlock_with_password` が **必ず `Err(MigrationError::Crypto(AeadTagMismatch))` を返す**（1000 ケース、改竄経路を 7 軸全網羅、`nonce_counter` 巻戻し攻撃の AEAD 検証 fail fast を確認）|

### 13.8 Sub-D E2E テストケース

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---|---|---|---|---|
| TC-D-E01 | 野木 拓海（Sub-E/F 実装者）| `VaultMigration` 6 メソッドで vault encrypt/unlock/decrypt/rekey/change-password の 6 連鎖を再構築 | (1) `cargo doc -p shikomi-infra --open` (2) `VaultMigration::{encrypt_vault, unlock_with_password, unlock_with_recovery, decrypt_vault, rekey, change_password}` の rustdoc サンプル呼出を読む (3) `RecoveryDisclosure::disclose` の所有権消費例を確認 (4) `DecryptConfirmation::confirm` の二段確認例を確認 | 30 分以内に「初回 encrypt → recovery 24 語表示 → unlock → 通常運用 → rekey or decrypt」の連鎖を Sub-E daemon / Sub-F CLI 実装に流用できる |

### 13.9 Sub-D テスト実行手順

```bash
# Rust unit + integration tests
cargo test -p shikomi-core --lib vault::recovery_disclosure
cargo test -p shikomi-infra --lib persistence::vault_migration
cargo test -p shikomi-infra --test vault_migration_integration

# property tests (1000 ケース、Sub-C と同型 ProptestConfig::with_cases(1000))
cargo test -p shikomi-infra --test vault_migration_property

# Sub-D 静的検証 (cargo 不要)
bash tests/docs/sub-d-static-checks.sh

# 既存 Sub-A/B/C static checks 再実行（回帰防止）
bash tests/docs/sub-a-static-checks.sh
bash tests/docs/sub-b-static-checks.sh
bash tests/docs/sub-c-static-checks.sh

# Sub-0 lint / cross-ref（回帰防止）
python3 tests/docs/sub-0-structure-lint.py
bash tests/docs/sub-0-cross-ref.sh

# 横断: vault-persistence integration test も再実行（REQ-P11 改訂で TC-I03/I04 新内容 + TC-I04a 新設）
cargo test -p shikomi-infra --test vault_persistence_integration
```

### 13.10 Sub-D テスト証跡

- `cargo test -p shikomi-infra --test vault_migration_*` の stdout（unit + integration + property pass 件数 + atomic write 失敗模擬の cleanup 観測）
- 静的検証スクリプト stdout（`sub-d-static-checks.sh` 4 件以上想定）
- proptest 失敗時の minimization 出力（あれば）
- vault-persistence 横断 regression 結果（TC-I03/I04 新内容 + TC-I04a 新設の pass）
- 全て `/app/shared/attachments/マユリ/sub-d-*.txt` に保存し Discord 添付

### 13.11 後続 Sub-E〜F への引継ぎ（Sub-D から派生）

| Sub | 本ファイル §13 拡張時の追加内容 |
|---|---|
| Sub-E (#43) | `VaultMigration::unlock_with_*` の戻り値 `Vek` を `tokio::sync::RwLock<Option<Vek>>` キャッシュに格納する経路、アイドル 15min / スクリーンロック / サスペンド時の `zeroize` 観測、IPC V2 `Unlock` / `Lock` / `ChangePassword` / `RotateRecovery` / `Rekey` request の `MigrationError` ↔ `IpcErrorCode` マッピング検証 |
| Sub-F (#44) | `vault encrypt` CLI で `RecoveryDisclosure::disclose` 経由 24 語表示 + MSG-S06 二段階確認、`vault decrypt` CLI で **TC-D-U09/U10 移譲分の二段確認ロジック実装**（「DECRYPT」キーワード手入力 + paste 抑制 + 大文字検証 + パスワード再入力 + `subtle::ConstantTimeEq` 比較 + 通過時に `DecryptConfirmation::confirm()` 呼出）+ MSG-S14、`vault rekey` CLI で `MigrationError::Crypto(NonceLimitExceeded)` 起点の MSG-S11 誘導、`vault change-password` で MSG-S05 成功表示、CLI 終了コード割当 + i18n 翻訳辞書統合 |

### 13.12 Sub-D 工程4 実施実績

工程4 完了後、Sub-D 実装担当（坂田銀時想定）+ テスト担当（涅マユリ想定）が本ファイルを READ → EDIT で実績を追記する。雛形は Sub-A §10.11 / Sub-B §11.11 / Sub-C §12.12 に従う。**Sub-A/B/C で観測したパターン**: 銀ちゃんは設計書の proptest / criterion bench / KAT 8 件等を**単発 fixture で省略する傾向**があるため、実装後の確認時に必ず**設計と実装の意味論的整合**を grep + Docker 再現で交叉確認すること（Bug-A-001 誤認 / Bug-B-001 bench 不在 / Bug-C-001 proptest 不在の連続パターン、テスト工程の実験データ）。
