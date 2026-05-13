# 結合テスト設計 — vault-persistence（TC-I24〜I29-D: Windows DACL 検証）

> 前提条件・外部I/O依存マップ・DACL正規化要件は [overview.md](./overview.md) を参照。
> 全 TC に `#[cfg(windows)]` ガード必須。Linux / macOS CI では自動スキップ。

---

## TC-I24: save 後の vault.db は owner-only DACL（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I24 |
| 対応する受入基準ID | REQ-P07 受入観点① |
| 対応する工程 | 基本設計（REQ-P07、save フロー step 6「作成直後にファイルパーミッションを所有者 ACL 設定」） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir` を使用 |
| 操作 | 1. `repo.save(&vault)` 2. `GetNamedSecurityInfoW` で `vault.db` の DACL と所有者 SID を取得 |
| 期待結果 | `save()` が `Ok(())` を返す。vault.db の DACL が 4 不変条件を満たす: ①`SE_DACL_PROTECTED` bit が立っている ②`AceCount == 1` かつ `ACCESS_ALLOWED_ACE_TYPE` ③ACE トラスティ SID が所有者 SID と `EqualSid` で一致 ④`AccessMask == FILE_GENERIC_READ \| FILE_GENERIC_WRITE`（`DELETE` / `WRITE_DAC` 等の追加ビットなし） |

---

## TC-I25: vault.db の DACL 破損後 load → InvalidPermission（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I25 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 基本設計（REQ-P07、load フロー step 4「ファイルのパーミッション確認」） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`repo.save(&vault)` 完了済み。vault.db に対し、テストコード内で `BUILTIN\Users` への `GENERIC_READ` Allow ACE を `SetNamedSecurityInfoW` で追加し DACL を壊す（ACE 数 = 2 かつ `PROTECTED_DACL_SECURITY_INFORMATION` なし） |
| 操作 | `repo.load()` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE)", actual, .. })` が返る。`actual` フィールドに全 ACE の列挙文字列（`trustee_sid=<SID>, ace_type=..., access_mask=0x<hex>` の形式 2 行分）が含まれる——不変条件②（`ace_count`）違反時のラベル形式（`flows.md §OS 別パーミッション実装詳細 §Windows` 参照）。秘密値を含まない |

---

## TC-I26: 継承 ACE 破棄の確認 — ensure_dir 後に SE_DACL_PROTECTED が設定される（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I26 |
| 対応する受入基準ID | REQ-P07 受入観点③ |
| 対応する工程 | 基本設計（REQ-P07、save フロー step 3「PermissionGuard::ensure_dir — DACL 適用」） |
| 種別 | 正常系 |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir` 直下に vault ディレクトリパスを指定（親 `%TEMP%` から ACE を継承した状態が初期値）。`repo.save` の前に vault ディレクトリが存在しないことを確認済み |
| 操作 | 1. `repo.save(&vault)`（内部で `ensure_dir` が vault ディレクトリを作成・DACL 適用） 2. `GetNamedSecurityInfoW` で vault ディレクトリの Control Flags を取得 |
| 期待結果 | `save()` が `Ok(())` を返す。取得した Control Flags に `SE_DACL_PROTECTED` bit が立っている（親 `%TEMP%` からの継承 ACE が破棄されている）。vault ディレクトリの ACE 数は 1 |

---

## TC-I27: vault dir DACL 破損後 load → InvalidPermission（Windows）

| 項目 | 内容 |
|------|------|
| テストID | TC-I27 |
| 対応する受入基準ID | REQ-P07 受入観点② |
| 対応する工程 | 基本設計（REQ-P07、load フロー step 1「PermissionGuard::verify_dir」） |
| 種別 | 異常系 |
| 前提条件 | `#[cfg(windows)]`。`repo.save(&vault)` 完了済み。vault ディレクトリに対し、`SetNamedSecurityInfoW` で `DACL_SECURITY_INFORMATION`（`PROTECTED_DACL_SECURITY_INFORMATION` を除く）で書き換えることで `SE_DACL_PROTECTED` bit を意図的に落とす |
| 操作 | `repo.load()` を呼ぶ |
| 期待結果 | `Err(PersistenceError::InvalidPermission { path, expected: "owner-only DACL (FILE_GENERIC_READ\|FILE_GENERIC_WRITE\|FILE_TRAVERSE)", actual, .. })` が返る。`actual` フィールドが `"inherited DACL (SE_DACL_PROTECTED not set)"` と等しい——不変条件①（`inherited`）違反時の確定ラベル（`flows.md §OS 別パーミッション実装詳細 §Windows` 参照）。`vault.db` は変更されていない |

---

## TC-I28: Sub-D `vault_migration_integration` 5 件 green 化（Windows、Issue #65 受入）

> **背景**: Sub-D（Issue #42 / PR #58）由来の integration test `crates/shikomi-infra/tests/vault_migration_integration.rs` が **Windows ランナーのみ** で 5 件全失敗していた（PR #64 CI ログ参照）。Issue #65 修正のミニマム受入条件として、これら 5 件が修正後の Windows CI で PASS することを本 TC で明示的に検証対象化する（既存テスト = 受入観点の SSoT、新規テスト追加なしで AC を満たす）。

| 項目 | 内容 |
|------|------|
| テストID | TC-I28 |
| 対応する受入基準ID | AC-18（Issue #65 受入、新規） |
| 対応する工程 | 詳細設計（REQ-P04、`AtomicWriter::write_new` クローズ順序契約 / `fsync_and_rename` Win 限定 retry、`../detailed-design/flows.md` §`save` step 6.10〜6.13 / step 7.3 / `../detailed-design/classes.md` §設計判断 §3.1） |
| 種別 | 異常系の green 化（修正前は Windows で `AtomicWriteFailed { stage: Rename, source: code:5 PermissionDenied }`、修正後は PASS） |
| 前提条件 | `feature/issue-65-windows-atomic-rename` ブランチ。`AtomicWriter::write_new` に `PRAGMA wal_checkpoint(TRUNCATE)` + `PRAGMA journal_mode = DELETE` + `Connection::close()` 明示呼出が実装されている。`AtomicWriter::fsync_and_rename` に `cfg(windows)` 限定の指数バックオフ rename retry（`50ms × 2^(n-1)` ± `25ms` jitter × 5、最悪 ~1675ms、Bug-G-001 反映後）が実装されている。raw fixture `tests/fixtures/characterization/raw/issue65/pr64_failure_log.txt` がベースライン保存されている |
| 操作 | Windows CI ランナー上で `cargo test -p shikomi-infra --test vault_migration_integration` を実行（テスト関数: `tc_d_i01_encrypt_then_unlock_password_roundtrip` / `tc_d_i02_encrypt_then_decrypt_roundtrip` / `tc_d_i03_rekey_then_unlock_with_same_password_observation` / `tc_d_i04_rekey_then_decrypt_vault_all_records_succeed` / `tc_d_i05_req_p11_v1_accepted_via_vault_migration` の 5 件） |
| 期待結果 | 5 件全て PASS（exit code == 0、`test result: ok. 5 passed; 0 failed`）。Linux / macOS でも引き続き PASS。raw fixture（PR #64 失敗ログ）と CI ログ diff を比較し「`AtomicWriteFailed { stage: Rename, code: 5 }` パターンが消えた」ことを証跡として記録する。**`#[cfg(windows)] #[ignore]` で 5 件を回避する PR は問答無用で却下**（防衛線、本ファイル冒頭注記参照） |

---

## TC-I29: 並行 read open 中の rename race を retry で吸収（Windows、Issue #65 補強検証）

> **背景**: Issue #65 の根本対策（`Connection::close()` 明示 + WAL checkpoint + `journal_mode=DELETE`）に加えて、Win Indexer / Defender 等の一過性ハンドル残存に対する補強として実装される `cfg(windows)` 限定 rename retry（50ms × 5 回）の機能を**決定的に再現するテスト**。並行スレッドが `vault.db` を read open している短時間ウィンドウ中に save を発火させ、retry が成功して save が `Ok(())` を返すことを直接検証する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29 |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、新規） |
| 対応する工程 | 詳細設計（REQ-P04、`AtomicWriter::fsync_and_rename` step 7.3 Windows 分岐、`../detailed-design/flows.md`） |
| 種別 | 異常系（race 状態下での正常完了検証） |
| 前提条件 | `#[cfg(windows)]` ガード付き。`tempfile::TempDir` を使用。初期 `vault.db` を save 済（記録済レコード 1 件）。`std::thread::spawn` で補助スレッドを起動できる |
| 操作 | 1. メインスレッドで初期 vault を save 完了 2. 補助スレッドを起動し、`std::fs::OpenOptions::new().read(true).share_mode(0)` 相当（`FILE_SHARE_NONE`）で `vault.db` を open し、**短時間保持**（指数バックオフ込み最悪 ~1675ms の内側、典型 200ms で retry 3 回目（累積 ~350ms）までに吸収される設計）してから drop する 3. 補助スレッドの open 直後にメインスレッドで別内容の vault を `repo.save(&new_vault)` する 4. save の戻り値と `vault.db` 内容を確認 |
| 期待結果 | `repo.save()` が `Ok(())` を返す（補助スレッドが drop した後、retry の 1〜4 回目で rename が成功する。CI Defender 介入時は 4〜5 回目で吸収）。`repo.load()` で復元した vault が新内容と一致する（最終的に `.new` から `vault.db` への置換が完了している）。**retry が機能していなければ `Err(AtomicWriteFailed { stage: Rename, source: code:5 })` で fail する**（修正前の挙動）。タイムアウト記録: **約 1675ms 超過なら fail**（指数バックオフの retry 上限契約違反、`../basic-design/security.md` §atomic write の二次防衛線 §jitter — `50ms × 2^(n-1)` ± `25ms` jitter × 5 = 最悪 ~1675ms / 平均 ~1550ms、Bug-G-001 反映後）|

**実装上の注意（Win API 直叩き、unsafe）**:
- `std::fs::OpenOptions` は標準では `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` を立てるため race 再現にならない。`std::os::windows::fs::OpenOptionsExt::share_mode(0)` で **share_mode = 0**（排他 open）を指定する必要がある
- 補助スレッドの保持時間は **典型 200ms 程度**（指数バックオフ後の SSoT に追従、Bug-G-001 反映後）。retry 3 回目（累積中央値 ~350ms）までに吸収される設計。CI ランナー (windows-latest) で `drop(File)` の close 遅延 + Defender/Indexer の追加 lock を考慮しても retry 4 回目（累積 ~750ms）までには確実に吸収される
- 経過時間 deadline は **3000ms 程度**（指数バックオフ最悪 ~1675ms × 1.8 buffer + write_new + thread spawn / channel 同期の余裕を考慮）。これを超えるなら指数バックオフ SSoT 上限契約違反
- 並行スレッドが指数バックオフ込み最悪 ~1675ms を超えて保持し続けると `Err(AtomicWriteFailed { stage: Rename })` が返る（**意図通りの fail fast**）。これを直接検証するのが TC-I29-A
- 3 ケース（TC-I29 / TC-I29-A / TC-I29-B）は `#[serial_test::serial(windows_atomic_rename_retry)]` で直列化。並列実行時に補助スレッドの share_mode(0) ロックが他テスト (別 TempDir) の Defender scan 経路を経由して干渉する可能性を排除
- `tracing_test` は **integration テスト crate では既定で対象 crate のログを env filter で弾く**ため、workspace `Cargo.toml` で `features = ["no-env-filter"]` を有効化する。これがないと `Audit::retry_event` の emit が `logs_contain` で観測できない（公式注記）

---

## TC-I29-A: retry 5 回全敗で `outcome=exhausted` が **error レベル**で発火する（Windows、Issue #65 DoS 兆候）

> **背景**: Issue #65 retry 補強の **DoS 兆候側 emit 経路**を直接検証する。補助スレッドが `vault.db` を `share_mode(0)` で **指数バックオフ最悪 ~1675ms を確実に超える時間**保持し、retry を 5 回全敗に追い込む。`Audit::retry_event` の `outcome=exhausted` 経路 (error レベル、`%outcome` Display 経由のクォート無し wire format、`../../basic-design/security.md` §retry 監査ログ) が発火し、daemon 側 subscriber が DoS 兆候として OWASP A09 連携で上位通報できる起点を担保する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29-A |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、DoS 兆候側） |
| 対応する工程 | 基本設計（`../basic-design/security.md` §atomic write の二次防衛線 §retry 監査ログ §rename retry 全敗 / 詳細設計 `../detailed-design/flows.md` §`save` step 7.3） |
| 種別 | 異常系（fail fast の意図確認 + 監査ログ error 経路の発火確認） |
| 前提条件 | `#[cfg(windows)]` ガード。`tempfile::TempDir`。初期 `vault.db` を save 済。`tracing_test::traced_test` でログ収集 |
| 操作 | 1. 初期 vault を save 完了 2. 補助スレッドが `share_mode(0)` で `vault.db` を **2500ms 保持**（v8 で 800ms から拡張、`>1675ms` で retry を 5 回全敗させる、Bug-G-001 反映後の指数バックオフ拡張に追従） 3. 補助スレッド ready 直後に `repo.save(&new_vault)` 4. save 戻り値とトレーシングログを検証 |
| 期待結果 | `repo.save()` が `Err(AtomicWriteFailed { stage: Rename, source: code:5/32/33 })` を返す。監査ログに `"rename retry exhausted"`（error レベル）+ `outcome=exhausted`（`%outcome` Display 経由のクォート無し wire format）が emit される。`outcome=pending` も併発するが `outcome=succeeded` は emit されない（fail 経路）|

**実装上の注意**:
- `tracing_test::traced_test` は **DEBUG 以上**の events を捕捉する。`Audit::retry_event` の error 分岐は `tracing::error!` を発行するため `logs_contain("rename retry exhausted")` で観測可能
- 補助スレッドの 2500ms は指数バックオフ最悪 `~1675ms` に対して `+50%` 余裕（Bug-G-001 反映後）。CI ランナーの sleep 精度揺らぎ (±50ms) と Defender 介入による追加待機を吸収する

---

## TC-I29-B: race 不在の通常 save では retry が exhaust まで到達しない（Windows、回帰防止）

> **背景**: `windows_rename_retry` の 5 回 retry が race 無し時に exhaust 経路まで到達する**異常を検出する sanity check**。CI ランナー (windows-latest) では Defender / Indexer 介入で通常 save でも一過性 race が発生し得る (Issue #65 の根源そのもの) ため、retry 経路自体は許容する。**本 TC の責務は「exhausted まで到達しない = 正常吸収範疇」の確認**であり、retry 経路への偽 emit の厳密検証は unit test 側に委譲する（v7.1 で「retry 経路自体を NG」から緩和、CI 実測の Defender 介入を反映）。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29-B |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、回帰防止） |
| 対応する工程 | 詳細設計（`../detailed-design/flows.md` §`save` step 7、`../detailed-design/classes.md` §`AtomicWriter::rename_atomic` 制御フロー） |
| 種別 | 正常系（race 無し経路の sanity check） |
| 前提条件 | `#[cfg(windows)]`。`tempfile::TempDir`。`tracing_test::traced_test` |
| 操作 | 1. race 無しで `repo.save(&vault)` を呼ぶ（初回作成）2. race 無しで `repo.save(&updated)` を呼ぶ（置換）3. CI 環境の偶発失敗時は 200ms 待機 + 1 回再試行で吸収 4. トレーシングログを検証 |
| 期待結果 | 最終的に置換 save が `Ok(())`。監査ログに `"rename retry exhausted"` / `outcome=exhausted`（クォート無し wire format）が **emit されていない**。`outcome=pending` / `outcome=succeeded` 経路の emit は**許容**（CI 環境の Defender 介入で偶発 retry が起こり得るため、retry 経路自体は NG にしない）|

---

## TC-I29-D (unit): `reverify_no_reparse_point` の TOCTOU 判定単体検証（Windows、`atomic.rs` 内 `#[cfg(test)]`）

> **背景**: Issue #65 retry 補強の二次防衛線 §`Win retry 中 TOCTOU` を担保する `reverify_no_reparse_point` を**ユニットレベルで決定的に検証**する。retry sleep 窓中に junction を差し替える race は非決定的で flaky になりやすいため、判定単体を直接呼び出して 4 経路（通常ファイル / 未存在 / junction / dir symlink）を網羅する。

| 項目 | 内容 |
|------|------|
| テストID | TC-I29-D-1 〜 TC-I29-D-4 |
| 対応する受入基準ID | AC-19（Issue #65 retry 補強、TOCTOU 二次防衛線） |
| 対応する工程 | 基本設計（`../basic-design/security.md` §atomic write の二次防衛線 §Win retry 中 TOCTOU）/ 詳細設計（`AtomicWriter::reverify_no_reparse_point`） |
| 種別 | 正常系 (D-1, D-2) / 異常系 (D-3, D-4) |
| 配置 | `crates/shikomi-infra/src/persistence/sqlite/atomic.rs` の `#[cfg(test)] mod tests` 内 `#[cfg(windows)]` ガード（関数が `pub(crate)` 未満で integration 不可） |
| 操作 | D-1: 通常ファイル → `Ok` / D-2: 未存在パス → `Ok`（初回 save の `final_path` 経路）/ D-3: `mklink /J` で junction → `Err(InvalidVaultDir { reason: SymlinkNotAllowed })` / D-4: `symlink_dir` で dir symlink → 同上 |
| 期待結果 | 上記 4 経路すべて期待値通り。D-3 / D-4 は `mklink /J` / `symlink_dir` が失敗する制約付きランナー（権限不足）では skip（`stderr` に skip 理由を出力）|

**実装上の注意**:
- D-3 (junction) は **管理者権限不要** で作成可能（`FILE_ATTRIBUTE_REPARSE_POINT (0x400)` ビット検出経路）
- D-4 (dir symlink) は **Developer Mode 有効または管理者権限**が必要（`is_symlink()` 検出経路、`windows-latest` GA runner は Developer Mode 有効）
- D-3 と D-4 で **検出経路が異なる**（reparse point ビット vs symlink フラグ）ため両方検証が必要

---

*概要・前提条件: [overview.md](./overview.md) / [TC-I01〜I11](./tc-i01-i11.md) / [TC-I12〜I23](./tc-i12-i23.md) / 改訂履歴: [../changelog.md](../changelog.md)*
*対応 Issue: #10, #14, #65 / 親ドキュメント: `../../index.md`*
