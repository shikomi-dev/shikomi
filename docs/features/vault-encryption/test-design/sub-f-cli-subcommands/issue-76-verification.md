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

> **§15.17.2 §A SSoT 一方向追従**: 「配置先 (推奨)」は工程2 設計時の **想定** であり、工程3
> 実装時に**実コード配置と整合**するよう本表を実装事実に追従して修正する (Issue #75 §15.14b
> で確立した一方向追従ポリシー継承)。下表「配置先 (実装後 SSoT)」列が工程3 完了時点の真実源。

| TC ID | 検証対象 | 配置先 (実装後 SSoT) | 工程3 状態 |
|---|---|---|---|
| TC-F-U01 | clap 派生型 `VaultSubcommand` 7 variant + `--help` 7 subcommands 表示 | `crates/shikomi-cli/src/cli.rs::tests::tc_f_u01_vault_subcommand_help_lists_seven_variants_recovery_show_absent` | ✅ **Issue #76 で実装済** |
| TC-F-U02 | i18n fallback (現実装事実: `Locale::detect_from_lang_env_value` 未知 LANG 値 fail-soft → English)。**設計書 §15.5 #2 の `Localizer::translate` 経路は Phase 6/7 で `shikomi_cli::i18n::Localizer` モジュール導入時に集約予定** (`presenter/success.rs` 内 doc コメント SSoT 参照)、現実装は `Locale::detect_from_lang_env_value` 経由の fail-soft 経路のみ。Phase 6/7 移行時に本 TC を `Localizer::translate("nonexistent_key") == "[missing:nonexistent_key]"` 検証に差し替える Boy Scout 必要 | `crates/shikomi-cli/src/presenter/mod.rs::tests::tc_f_u02_locale_detect_falls_back_to_english_for_unknown_lang_value_without_panic` | ✅ **Issue #76 で実装済** (推奨配置 `i18n/mod.rs::tests` を未導入実装事実に追従) |
| TC-F-U03 | `vault decrypt` 確認文字列 `"DECRYPT"` 完全一致契約 (現実装事実: `usecase::vault::decrypt::is_decrypt_confirmation_literal` + `CONFIRMATION_LITERAL`)。**設計書 §15.5 #3 の paste 抑制 4 段時刻差検証は Phase 5 タスク**として `input::decrypt_confirmation::prompt` モジュール導入時に集約予定 (本ファイル冒頭 doc コメント「Phase 5 で paste 抑制 + ConstantTimeEq」)、現実装は Phase 2 単純文字列比較で wire。Phase 5 移行時に paste 抑制 4 段時刻差検証へ差し替える Boy Scout 必要 | `crates/shikomi-cli/src/usecase/vault/decrypt.rs::tests::tc_f_u03_decrypt_confirmation_literal_compare_only_accepts_uppercase_decrypt` | ✅ **Issue #76 で実装済** (推奨配置 `input/decrypt_confirmation.rs::tests` を未導入実装事実に追従) |
| TC-F-U04 | 24 語表示 presenter API 不変条件 (現実装事実: `render_recovery_disclosure_screen(&[SerializableSecretBytes], Locale)` 借用形)。**設計書 §15.5 #4 の `Vec<SerializableSecretBytes>` 所有権消費形は Phase 8+ で `recovery_disclosure` モジュール集約時に再検討**、現実装は 24 語の所有権を呼出側 `usecase::vault::encrypt::execute` が保持し presenter は借用のみ参照する構造を SSoT とする。`compile_fail` doctest は Phase 8+ 移行時に追加 | `crates/shikomi-cli/src/presenter/success.rs::tests::tc_f_u04_render_recovery_disclosure_screen_signature_borrows_words_slice_for_reuse` | ✅ **Issue #76 で実装済** (推奨配置 `presenter/recovery_disclosure.rs` rustdoc を未導入実装事実に追従) |
| TC-F-U05 | `mode_banner::display(ProtectionModeBanner)` 4 variant 文字列 + `NO_COLOR` 切替 | `crates/shikomi-cli/src/presenter/mode_banner.rs::tests::tc_f_u05_display_renders_four_variants_with_no_color_toggle` | ✅ **Issue #76 で実装済** |
| TC-F-U06 | `cache_relocked_warning::display()` MSG-S07/S19 + MSG-S20 連結 + `success::render_rekeyed_with_fallback_notice` SSoT 整合 | `crates/shikomi-cli/src/presenter/cache_relocked_warning.rs::tests::tc_f_u06_display_concatenates_msg_s20_warning_for_both_locales` | ✅ **Issue #76 で実装済** |
| TC-F-U07 | `match banner: ProtectionModeBanner` cross-crate enum + `_` defensive arm 許容 (Sub-E TC-E-S01 同型) | `crates/shikomi-cli/src/presenter/mode_banner.rs::tests::tc_f_u07_protection_mode_banner_match_with_defensive_underscore_arm_compiles` | ✅ **Issue #76 で実装済** |
| TC-F-U08 | `windows_pipe_name_from_dir` 純関数性 4 ケース | `crates/shikomi-cli/src/io/ipc_vault_repository.rs::windows_pipe_name_tests::tc_f_u08_*` (3 件 + 補助 `vault_dir_socket_path_is_pure` 1 件) | ✅ **Issue #75 で実装済 `07ae079`** (Issue #76 では再実装不要、参照のみ) |
| TC-F-U09 | `connect_with_vault_dir` MSG-S09(b) 強制発火 (Bug-F-009 Option α) | 同上 `windows_pipe_name_tests::tc_f_u09_connect_with_vault_dir_returns_daemon_not_running_with_primary_path` | ✅ **Issue #75 で実装済 `07ae079`** (同上) |
| TC-F-U10 | `hardening::core_dump::suppress` 3 OS `#[cfg(target_os)]` 分岐シグネチャ存在 (現実装事実: `process_hardening` ではなく `hardening::core_dump`) | `crates/shikomi-cli/src/hardening/core_dump.rs::tests::tc_f_u10_suppress_signature_exists` | ✅ **Issue #75 Phase 5 で実装済** (推奨配置 `process_hardening/mod.rs::tests` を実モジュール名 `hardening::core_dump` に追従) |
| TC-F-U11 | clap 派生型に `--no-mode-banner` / `--hide-banner` 非定義 + `presenter::list::render_list` 必須引数 `ProtectionModeBanner` 型レベル強制 | `crates/shikomi-cli/src/cli.rs::tests::tc_f_u11_vault_list_rejects_no_mode_banner_flag_and_render_list_requires_protection_mode` | ✅ **Issue #76 で実装済** (grep gate は TC-F-S02 補完範疇) |
| TC-F-U12 | 24 語表示経路で `SerializableSecretBytes::to_lossy_string_for_handler` 経由表示 + scope 終了 `Drop` 発火構造 (現実装事実: `Vec<SerializableSecretBytes>` 所有権を呼出側保持、scope 終了 Drop で `secrecy` crate 経由 zeroize)。**設計書 §15.5 #12 の `mem::replace` パターンは Phase 8+ で `recovery_disclosure::display` モジュール集約時に追加**、現実装は呼出側 scope の通常 Drop で zeroize 委譲 | `crates/shikomi-cli/src/presenter/success.rs::tests::tc_f_u12_render_recovery_disclosure_lossy_string_path_preserves_word_visibility` | ✅ **Issue #76 で実装済** (推奨配置 `presenter/recovery_disclosure.rs::tests` を未導入実装事実に追従) |
| TC-F-U13 | `input::password::prompt` / `input::mnemonic::prompt` C-38 stdin 非 TTY 拒否 (PTY OK 経路は結合 TC-F-I12 で実機検証) | `crates/shikomi-cli/src/input/password.rs::tests::tc_f_u13_password_prompt_returns_non_interactive_password_when_stdin_not_tty` + `crates/shikomi-cli/src/input/mnemonic.rs::tests::tc_f_u13_mnemonic_prompt_returns_non_interactive_password_when_stdin_not_tty` (2 関数で 1 TC、両 prompt 並列保証) | ✅ **Issue #76 で実装済** |
| TC-F-U14 | `accessibility::output_target::resolve()` 明示フラグ最優先 3 variant + env-driven 切替 pure 関数代替 **旧 TC-F-U08 リナンバ** | `crates/shikomi-cli/src/accessibility/output_target.rs::tests::tc_f_u14_resolve_explicit_flag_takes_precedence_over_env_for_three_variants` | ✅ **Issue #76 で実装済** |
| TC-F-U15 | `CliError` 全 variant → `ExitCode` SSoT 写像マトリクス (cli-subcommands.md §終了コード SSoT) **旧 TC-F-U09 リナンバ** | `crates/shikomi-cli/src/error.rs::tests::tc_f_u15_exit_code_ssot_mapping_for_all_cli_error_variants_in_one_matrix` | ✅ **Issue #76 で実装済** (推奨配置 `usecase/vault/unlock.rs::tests` を「終了コード SSoT は `error::ExitCode` で集約、7 vault サブコマンド全てが共有」事実に追従) |
| **Issue #76 実装件数** | — | — | **13 件 (TC-F-U01〜U07 + U10〜U15)** |
| **Issue #75 既実装件数 (再実装不要)** | — | — | **2 件 (TC-F-U08, U09)** |
| **Sub-F ユニット TC 総数** | — | — | **15 件** |

**実装後検出 Bug articulate (Issue #76 工程3 副産物)**:

- **Bug-F-010** (NEW、TC-F-U02 articulate 由来): `Locale::detect_from_lang_env_value(Some("🦀"))` のような **2 バイト目が char boundary 外の非 ASCII LANG 値**で `s[..2]` byte slice が panic する経路がある。C-33 fail-soft 契約違反 (`presenter/mod.rs` line 42、`s[..2].eq_ignore_ascii_case("ja")` を char boundary 不問で評価)。修正は別 Issue で `s.is_char_boundary(2)` ガード追加または `s.chars().take(2).collect::<String>()` 経由比較。Issue #76 工程3 完了報告に記載。

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
