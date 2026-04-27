# テスト設計書 — Sub-F (#44) Issue #76 (#74-B) ユニットテスト 13 件 工程3 実装 + 工程4 検証手順

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-f-cli-subcommands/issue-76-verification.md -->
<!-- 前提: Issue #75 (#74-A) merge 済 (07ae079)、TC-F-U08/U09 は ipc_vault_repository.rs 実装側で SSoT 化済 -->
<!-- セル 81c0f7f (Issue #76 工程2 設計: TC-ID リナンバ + Sub-F TC との 1:1 整合) を反映済。本書は工程3 (テスト実装) + 工程4 (テスト担当検証) の SSoT として運用 -->

> **本書の責務**: Issue #76 (#74-B) で実装されるユニットテスト 13 件 (TC-F-U01〜U07 + U10〜U15) について、(1) 実装担当 (テスト担当=涅マユリ) の各 TC 実装 SSoT、(2) 工程3 完了判定基準、(3) 工程4 検証手順を articulate する。Sub-F 本体テスト設計 (§15.1〜§15.14) は同ディレクトリの [`index.md`](index.md) を参照。Issue #75 関連は [`issue-75-verification.md`](issue-75-verification.md)。

### 15.17 Issue #76 (#74-B) 工程3 実装スコープ + 工程4 検証手順 SSoT（2026-04-27）

§15.14b で TC-ID リナンバ (TC-F-U08/U09 を Issue #75 実装に固定、旧 U08/U09 を U14/U15 にリナンバ) が articulate 済。本節 §15.17 は **Issue #76 で新規実装する 13 件のユニット TC** の (1) 実装担当向け SSoT、(2) 完了 DoD、(3) テスト担当 (自身) の工程4 検証手順を articulate する。

> **本節の位置付け**:
> - §15.5 = 各 TC の検証内容 SSoT (操作・期待結果)
> - §15.14b = TC-ID リナンバの履歴 articulate
> - §15.17 = **工程3 実装スコープ + 工程4 完了判定 SSoT** (本節)

#### 15.17.1 実装スコープ TC 13 件マトリクス（配置先 + Issue #76 進捗）

| TC ID | 検証対象 | 配置先 (推奨) | 工程3 状態 |
|---|---|---|---|
| TC-F-U01 | clap 派生型 `VaultSubcommand` 7 variant + `--help` 7 subcommands 表示 | `crates/shikomi-cli/src/cli.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U02 | i18n `Localizer::translate` 欠落 fallback `[missing:{key}]` | `crates/shikomi-cli/src/i18n/mod.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U03 | `decrypt_confirmation::prompt` paste 抑制 4 段時刻差検証 (`< 30ms = Err / >= 30ms = Ok`) | `crates/shikomi-cli/src/input/decrypt_confirmation.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U04 | `recovery_disclosure::display(words: Vec<SerializableSecretBytes>, target)` 所有権消費 (`compile_fail` doc test) | `crates/shikomi-cli/src/presenter/recovery_disclosure.rs` の rustdoc コメント内 | ⏳ Issue #76 工程3 |
| TC-F-U05 | `mode_banner::display(ProtectionModeBanner)` 4 variant 文字列 + `NO_COLOR` 切替 | `crates/shikomi-cli/src/presenter/mode_banner.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U06 | `cache_relocked_warning::display()` MSG-S07/S19 + MSG-S20 連結 | `crates/shikomi-cli/src/presenter/cache_relocked_warning.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U07 | `match banner: ProtectionModeBanner` cross-crate enum + `_` defensive arm 許容 (Sub-E TC-E-S01 同型) | `crates/shikomi-cli/src/presenter/mode_banner.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U08 | `windows_pipe_name_from_dir` 純関数性 4 ケース | `crates/shikomi-cli/src/io/ipc_vault_repository.rs::windows_pipe_name_tests` | ✅ **Issue #75 で実装済 `07ae079`** (Issue #76 では再実装不要、参照のみ) |
| TC-F-U09 | `connect_with_vault_dir` MSG-S09(b) 強制発火 (Bug-F-009 Option α) | 同上 `windows_pipe_name_tests` | ✅ **Issue #75 で実装済 `07ae079`** (同上) |
| TC-F-U10 | `process_hardening::install()` 3 OS `#[cfg(target_os)]` 分岐シグネチャ存在 | `crates/shikomi-cli/src/process_hardening/mod.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U11 | clap 派生型に `--no-mode-banner` 等の隠蔽フラグ非定義 + `usecase::list::execute` から `mode_banner::display` 到達経路 | `crates/shikomi-cli/src/cli.rs::tests` + grep gate | ⏳ Issue #76 工程3 |
| TC-F-U12 | `recovery_disclosure::display` 関数本体 zeroize 連鎖 (`Vec<SerializableSecretBytes>` Drop 発火) | `crates/shikomi-cli/src/presenter/recovery_disclosure.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U13 | `input::password::prompt` / `input::mnemonic::prompt` 3 パターン (TTY OK / 非 TTY Err / `/dev/tty` open 失敗 Err) | `crates/shikomi-cli/src/input/password.rs::tests` / `input/mnemonic.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U14 | `accessibility::output_target::resolve()` 4 パターン (env / フラグ排他確認) **旧 TC-F-U08 リナンバ** | `crates/shikomi-cli/src/accessibility/output_target.rs::tests` | ⏳ Issue #76 工程3 |
| TC-F-U15 | `usecase::vault::unlock` 各 `Result` → ExitCode SSoT (cli-subcommands.md §終了コード SSoT) **旧 TC-F-U09 リナンバ** | `crates/shikomi-cli/src/usecase/vault/unlock.rs::tests` | ⏳ Issue #76 工程3 |
| **Issue #76 実装件数** | — | — | **13 件 (TC-F-U01〜U07 + U10〜U15)** |
| **Issue #75 既実装件数 (再実装不要)** | — | — | **2 件 (TC-F-U08, U09)** |
| **Sub-F ユニット TC 総数** | — | — | **15 件** |

#### 15.17.2 工程3 実装担当 (テスト担当=涅マユリ) 着手 Boy Scout SSoT

##### A. 配置先確定の責務

§15.17.1 「配置先 (推奨)」列はあくまで **`crates/shikomi-cli/src/**` の現状コード配置と整合する候補**。実装担当は実コードを確認し、推奨配置と差異がある場合は本表を実装事実に追従して修正する (Issue #75 §15.14b で確立した SSoT 一方向追従ポリシー継承)。

##### B. テスト命名規則

- 関数名: `tc_f_u{NN}_{snake_case_description}`（既存 `tc_f_u08_*` / `tc_f_u09_*` と整合）
- 例: `tc_f_u01_vault_subcommand_help_lists_seven_variants`、`tc_f_u02_localizer_returns_missing_marker_for_unknown_key`

##### C. モック方針

| 観点 | 方針 |
|---|---|
| I/O バウンダリ | TTY (`is-terminal::IsTerminal`)、env (`std::env::var`)、file system は **fake reader / stub** または `tempfile` で本物使用。**`mock.return_value` インライン辞書リテラルは禁止** (テスト戦略ガイド準拠、Characterization 不要だが factory 経由は推奨) |
| Characterization fixture | **不要** (CLI 内部、外部 API なし)。本書 [`index.md`](index.md) §15.2 外部 I/O 依存マップ articulate 済 |
| 時刻 | `decrypt_confirmation::prompt` は `Instant::now()` を **fake provider** 経由で注入 (TC-F-U03 の 4 段時刻差検証用) |
| `expectrl` PTY | TC-F-U13 の TTY 経路のみ。Linux / macOS で実行可、Windows は `expectrl` の Win 対応経路で OS 互換性確認 |
| serial_test | env 操作を伴う TC (TC-F-U04, U14 等) は `#[serial_test::serial(env_xdg_home)]` で直列化。Issue #75 TC-F-U09 と同 group 名で env 操作競合を排除 |

##### D. SecretString 経路の禁止事項

- **`SecretString::expose_secret()` 呼出禁止** (TC-CI-013 grep gate 違反)
- TC-F-U12 の zeroize 連鎖検証で内部バッファを観測する場合、`shikomi-cli/src/` 配下では `unsafe` hook を使わず、`shikomi-core` の test-only API 経由で観測する (Sub-A `RecoveryWords` 同型)

##### E. cross-crate dev-dep 追加時の Boy Scout

- `expectrl` (TC-F-U13 PTY 用): `crates/shikomi-cli/Cargo.toml` `[dev-dependencies]` に追加。Windows compat 確認 (Sub-D `e2e_daemon_phase15_pty.rs` で既導入実績あり)
- `serial_test`: 既導入 (Sub-E / Issue #75 TC-F-U09 で使用)
- 新規 dev-dep 追加時は `cargo deny check` (TC-CI-003) を pass させること

#### 15.17.3 工程4 検証手順 SSoT (テスト担当 = 涅マユリ 自身)

実装担当 = テスト担当の Issue 構造のため、工程3 完了 = 工程4 検証は同人物が実施。但し**自己批判の余地を確保**するため、本節の SSoT で「実装直後に自分が検証する手順」を明示し、Bug-G-005 (Issue #65 偶発 PASS 誤認) の同型再演を防ぐ。

##### 15.17.3.1 実装後 CI 観測手順

```bash
# 1. ローカル全テスト走行 (回帰確認)
cargo test --no-fail-fast --all-targets -p shikomi-cli
# 期待: TC-F-U01〜U07 + U10〜U15 (13 件) 全 pass、Issue #75 TC-F-U08/U09 (2 件) 含めて Sub-F U total 15/15
# 既存 Sub-A〜E TC への回帰なし

# 2. doc test 走行 (TC-F-U04 compile_fail)
cargo test -p shikomi-cli --doc
# 期待: TC-F-U04 が compile_fail として観測される (所有権消費の型レベル強制)

# 3. ユニットテスト件数集計
cargo test -p shikomi-cli --lib 2>&1 | grep "test result:"
# 期待: 既存件数 + 13 件 (Issue #76 新規)

# 4. 静的検査回帰 (Sub-F static checks、TC-F-S01〜S06)
bash tests/docs/sub-f-static-checks.sh  # 存在時のみ、Issue #79 (#74-E) で実装予定の場合は skip

# 5. clippy / fmt / deny
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

##### 15.17.3.2 PR 作成後の CI 必須 check 観測

PR を切ったら、Issue #75 で確立した CI 必須 check が green であることを確認:

```bash
# CI 結果確認
gh pr checks <PR番号> --repo shikomi-dev/shikomi
# 期待: 全 14 check pass、特に test-cli (3-OS マトリクス) が all green
```

`test-cli` ジョブ (Issue #75 §7.2.1 で確立) で 3-OS (Ubuntu / macOS / Windows) で 13 件全 pass を観測。Windows runner 固有の Bug-G-008 (`vault.db` lock 系) は本 Issue では発生しない (`#[ignore]` 不要、`tempfile::TempDir` も使わない pure unit のため)。

##### 15.17.3.3 自己批判チェックリスト (Bug-G-005 同型再演防止)

Issue #65 で私 (マユリ) が「偶発 PASS を対策効果と誤認した」教訓を踏まえ、以下を**実装直後に自分でレビュー**する:

- [ ] 各 TC が **assert している期待値が "PASS している風" でなく実際の挙動を assert している** (例: `assert!(result.is_ok())` ではなく `assert_eq!(result, Ok(expected))` で具体値確認)
- [ ] env 操作を伴う TC で `serial_test::serial` を忘れていない (前後 env 状態に依存しない)
- [ ] `expectrl` PTY 利用 TC で CI runner 制約 (`#[ignore]` フォールバック or skip 理由 stderr 出力) を articulate
- [ ] **fake provider 経由の依存注入**で、本物の I/O / 時刻に依存していない (TC が flaky にならない)
- [ ] テスト対象関数の戻り値型が `Result<T, E>` の場合、**Err パスも明示的にカバー** (異常系網羅)

##### 15.17.3.4 工程4 完了 DoD チェックリスト

§15.17.3.1〜.3.3 を実施後、以下を埋めて工程5 (内部レビュー) に進める:

- [ ] §15.17.3.1 ローカル全テスト 13 件追加、既存 PASS 件数 + 13 で全 pass、回帰なし
- [ ] §15.17.3.1 doc test (TC-F-U04 compile_fail) 観測
- [ ] §15.17.3.1 clippy / fmt / deny 全 pass
- [ ] §15.17.3.2 PR 作成 + CI test-cli 3-OS 全 pass 観測
- [ ] §15.17.3.3 自己批判チェックリスト 5 項目全埋め
- [ ] §15.5 マトリクスの「工程3 状態」列を `⏳ Issue #76 工程3` → `✅ 実装済 ({SHA})` に更新
- [ ] 証跡 (`cargo test` 出力 / CI ログ抜粋) を `/app/shared/attachments/マユリ/issue-76-verification-*.{md,log}` に保存し Discord 添付

##### 15.17.3.5 #74 親 Issue クローズ条件への寄与

[`issue-75-verification.md`](issue-75-verification.md) §15.15 「#74 親 Issue クローズ条件 (DoD トレース)」の **#74-B 行** (`TC-F-U01〜U13 (13/13 件) pass、cargo test -p shikomi-cli --lib で観測`) を本 Issue 完了で satisfy する。但し:

- リナンバ後の正確な記述: 「**TC-F-U01〜U07 + U10〜U15 (13/13 件) pass + TC-F-U08/U09 (Issue #75 既実装、合計 15/15 件) cargo test -p shikomi-cli --lib で観測**」
- `issue-75-verification.md` §15.15 の DoD トレース行も Issue #76 マージ時に同期更新する (Boy Scout)

#### 15.17.4 Boy Scout / 教訓 articulate（Issue #76 視点）

- **TC-ID 占有による前提整理**: Issue #75 で TC-F-U08/U09 が実装側 SSoT として固定されたため、Issue #76 はそれを尊重して U14/U15 にリナンバした。**先行 Issue 実装事実 = 後続 Issue の前提条件**として SSoT 一方向追従させる構造は、横断的な Issue 連携で頻発する TC-ID 衝突を構造的に解消する pattern として継承可能 (Issue #65 Bug-G 系列の「articulate 済 compromise」と同型)
- **テスト担当 = 実装担当 (同人物) の Issue 構造**: Issue #76 のように「ユニットテスト実装が成果物そのもの」という Issue では、実装と検証が同人物に閉じる。この場合 §15.17.3.3 自己批判チェックリストが Bug-G-005 同型再演 (偶発 PASS 誤認) の構造的防衛線になる。後続 Issue で同型構造が出る場合 (#74-C / #74-D / #74-E 等) も本 pattern を継承
- **CI 観測スコープ**: Issue #75 で確立した `test-cli` 3-OS マトリクスで Issue #76 の 13 件が観測される。Bug-F-003 解消の delivery がここで初めて値を生む (CI で実観測される TC が増えた = CI 観測スコープが実質的に広がった証拠)
