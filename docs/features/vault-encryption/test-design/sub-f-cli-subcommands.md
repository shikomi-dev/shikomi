# テスト設計書 — Sub-F (#44) shikomi-cli vault サブコマンド

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-f-cli-subcommands.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->
<!-- 横断的変更: 本書は vault-encryption feature のテスト設計だが、daemon-ipc feature の `IpcResponse::Records` 構造体化に伴う後方互換 TC（TC-IT-026..028 想定）と双方向参照する。 -->
<!-- TC-E-E01 田中ペルソナ E2E は Sub-E `sub-e-vek-cache-ipc.md` §14.4 で凍結された 6 ステップシナリオを Sub-F CLI 完成後に**実機完走**するもの。本書 §15.8 で TC-F-E01 として詳細化（同型 ID）。 -->

## 15. Sub-F (#44) テスト設計 — shikomi-cli vault サブコマンド + 既存 CRUD ロック時挙動 + 田中ペルソナ E2E

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#44](https://github.com/shikomi-dev/shikomi/issues/44) |
| 対象 PR | #69（`c3732ff`、設計フェーズ）|
| 対象成果物 | `vault-encryption/detailed-design/cli-subcommands.md`（新規 245 行）/ `requirements.md`（REQ-S15/S16 確定 + MSG-S07/S11/S14/S18/S20 CLI 文言）/ `basic-design/{processing-flows,ux-and-msg,index}.md`（EDIT、F-F1〜F-F9 + MSG 翻訳キー索引）/ `cli-vault-commands/requirements.md`（MSG-CLI-103 Boy Scout）/ **横断**: `daemon-ipc/{requirements.md, detailed-design/protocol-types.md}`（`IpcResponse::Records` 構造体化、`ProtectionModeBanner` enum 追加）|
| 設計根拠 | `cli-subcommands.md` `Subcommand::Vault(VaultSubcommand)` 8 variant / 8 usecase / 3 presenter (recovery_disclosure / mode_banner / cache_relocked_warning) / アクセシビリティ 3 経路 / i18n TOML 辞書 / 契約 **C-33〜C-37**（欠落キー fail-soft / paste 抑制 / recovery-show 1 度限り / cache_relocked: false 終了コード 0 / 隠蔽オプション禁止）|
| 対象 crate | `shikomi-cli`（`cli::Subcommand::Vault` + `cli::VaultSubcommand` enum + `usecase::vault::*` 8 件 + `presenter::{recovery_disclosure,mode_banner,cache_relocked_warning}` + `input::{password,mnemonic,decrypt_confirmation}` + `accessibility::{print_pdf,braille_brf,audio_tts}` + `i18n::Localizer`、新規）+ `shikomi-core`（`IpcResponse::Records` 構造体化 + `ProtectionModeBanner` enum 追加、横断的変更）|
| **Sub-F TC 総数** | **35 件**（ユニット 12 + 結合 13 + アクセシビリティ 4 + E2E 1 + property 0 + 静的検査 5 = 35、Sub-A〜E と同型レンジ。**TC-E-E01 を TC-F-E01 として詳細化 + 実機完走**、Sub-E §14.13 凍結シナリオの最終工程）|

### 15.1 Sub-F テストレベルの読み替え（CLI 経路 + アクセシビリティ + 田中ペルソナ E2E）

Sub-F は **Sub-A〜E で凍結した型・契約・IPC スキーマ + Sub-D `VaultMigration` + Sub-E daemon ハンドラを CLI 経路で具現化**する Sub。Vモデル対応：

| テストレベル | 通常の対応 | Sub-F での読み替え | 検証手段 |
|---|---|---|---|
| **ユニット** | メソッド単位、ホワイトボックス | (a) `clap` 派生型 `VaultSubcommand` 8 variant の構築 + 引数 parse、(b) `i18n::Localizer` fallback (C-33)、(c) `decrypt_confirmation::prompt` paste 抑制 (C-34)、(d) `recovery_disclosure::display` の zeroize 連鎖、(e) `mode_banner::display` の ANSI カラー / NO_COLOR 切替、(f) `cache_relocked_warning::display` の MSG-S20 連結、(g) `ProtectionModeBanner` 4 variant 網羅、(h) accessibility 自動切替 (SHIKOMI_ACCESSIBILITY env)、(i) 終了コード `ExitCode` マッピング | `cargo test -p shikomi-cli --lib cli::tests` + `cargo test -p shikomi-cli --lib presenter::tests` + `cargo test -p shikomi-cli --lib i18n::tests` |
| **結合** | モジュール連携、契約検証 | (a) **8 サブコマンド × IPC V2 ラウンドトリップ**: 実 daemon プロセス + tempdir vault.db で `vault encrypt/decrypt/unlock/lock/change-password/recovery-show/rekey/rotate-recovery` を `assert_cmd` 経由実行 → stdout/stderr/exit code を assert、(b) **既存 CRUD ロック時 fail fast** (REQ-S16): `shikomi list/add/edit/remove` を Locked 状態で実行 → MSG-S09(c) + 終了コード 3、(c) **shikomi list バナー 3 状態**: plaintext / encrypted-locked / encrypted-unlocked、(d) **`cache_relocked: false` 表示**: fault-injection 経由で MSG-S07 + S20 連結 + 終了コード 0 (C-31/C-36)、(e) **`recovery-show` 2 度目失敗** (C-35) | `cargo test -p shikomi-cli --test vault_subcommands` + `cargo test -p shikomi-cli --test mode_banner_integration` + `assert_cmd` + `tempfile` + 実 `shikomi-daemon` 子プロセス |
| **アクセシビリティ** | WCAG 2.1 AA 経路、stdout fixture | `vault recovery-show --print` PDF バイナリ / `--braille` BRF テキスト / `--audio` TTS パイプ / `SHIKOMI_ACCESSIBILITY=1` 自動切替の 4 経路、stdout のバイナリヘッダ / 構造で出力経路を機械検証 | `cargo test -p shikomi-cli --test accessibility_paths` + PDF/BRF magic byte assert |
| **E2E** | 完全ブラックボックス、ペルソナ | **TC-F-E01**（Sub-E `TC-E-E01` 田中ペルソナ完走の Sub-F 段階詳細化）: 田中（経理担当者、`requirements-analysis.md` ペルソナ）が daemon 起動 → `shikomi vault unlock` → `shikomi list` で業務 → アイドル 15 分自動 lock → 次操作で MSG-S09(c) → `shikomi vault unlock` 再入力 → `shikomi vault change-password` → MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」を確認、6 ステップ完走 | `bash tests/e2e/sub-f-tanaka-persona.sh` + `assert_cmd` + `tokio::time::pause/advance` 経由 idle simulation（実環境 15 分待機は CI コスト過剰、`SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` 環境変数で短縮） |
| **property** | ランダム入力での invariant | Sub-F 範囲では proptest 不要（CLI clap parse は `clap` 自体が網羅検証済、IPC ラウンドトリップは Sub-E TC-E-P01 で proptest 担保済）| — |

### 15.2 外部 I/O 依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---|---|---|---|
| **`shikomi-daemon` プロセス** | — | `tests/helpers/daemon_spawn.rs`（`assert_cmd::Command` で daemon 子プロセス起動 + tempdir socket / `SHIKOMI_VAULT_DIR` env 注入 + graceful shutdown）| **既存資産再利用**（Sub-E `crates/shikomi-daemon/tests/e2e_daemon.rs` の `daemon_spawn` ヘルパ流用、Sub-F 結合テストはそれを輸入）|
| **TTY 入力 (password / mnemonic / decrypt confirmation)** | — | `expectrl` (Unix 限定 dev-dep、Sub-D `e2e_daemon_phase15_pty.rs` で既導入) で TTY を擬似制御 | **既存資産再利用**（Sub-D PTY 経路を Sub-F 結合層で再利用、`assert_cmd` 単体で TTY emulate 不可のため）|
| **PDF / BRF / TTS 出力** | — | stdout バイナリを `bytes::Bytes` で受領後 magic byte / 構造を assert（PDF: `%PDF-1.7`、BRF: ASCII Braille range U+2800..U+28FF or `.brf` line ending）| **不要**（出力経路の自前生成、外部 API ではない）|
| **`SHIKOMI_LOCALE` / `SHIKOMI_ACCESSIBILITY` 環境変数** | — | `temp-env` crate or `std::env::set_var` 直接 (テストシリアル化、`#[serial]`) | **不要**（local env、外部 API ではない）|
| **OS スクリーンリーダー検出（macOS/Win/Linux）**| — | `--audio` 自動切替経路は OS API 依存、Sub-F 結合層では `--audio` 明示フラグで強制 → **手動 smoke テスト**は工程5 で OS 別実施 | **不要**（OS 依存経路は Sub-F 工程5 で OS 別 manual smoke）|

**理由**: Sub-F は CLI 経路 + アクセシビリティ + 田中ペルソナ E2E の組合せ層。暗号計算・マイグレーション・daemon ライフサイクル・IPC スキーマは Sub-A〜E で担保済。Sub-F 固有の検証対象は **(1) clap 派生型 8 variant の正規 parse**、**(2) i18n Localizer fallback (C-33)**、**(3) paste 抑制 (C-34)**、**(4) recovery-show 1 度限り (C-35)**、**(5) `cache_relocked: false` 表示 + 終了コード 0 (C-31/C-36)**、**(6) 隠蔽オプション禁止 (C-37)**、**(7) アクセシビリティ 3 経路**、**(8) 田中ペルソナ E2E 完走**。

### 15.3 Sub-F 受入基準（REQ-S15 / REQ-S16 + 契約 C-33〜C-37 + Sub-A〜E 累積継承）

| 受入基準ID | 内容 | 検証レベル |
|---|---|---|
| **C-33** | i18n 辞書キー欠落時もパニックさせず英語 fallback で fail-soft（`Localizer::translate("nonexistent")` → `[missing:nonexistent]` 返却、パニック無し）| ユニット（TC-F-U02） |
| **C-34** | `vault decrypt` 二段確認 paste 抑制（`decrypt_confirmation::prompt` が連続入力時刻差 < 30ms で `CliError::PasteSuspected` で fail fast、終了コード 1）| ユニット（TC-F-U03）+ 結合（TC-F-I02b） |
| **C-35** | `recovery-show` 2 度目以降は CLI 層で fail fast（C-19 整合、`RecoveryDisclosure::disclose` 所有権消費後の 2 度目呼出を daemon が拒否、CLI が MSG-S09 系に変換）| 結合（TC-F-I06b） |
| **C-36** | `cache_relocked: false` 経路で終了コード 0、Err 終了コードを返さない（C-31 整合、operation 成功）| 結合（TC-F-I07c）|
| **C-37** | 保護モードバナーの隠蔽オプションを提供しない（`--no-mode-banner` フラグ未定義、grep 静的検査で文字列不在を機械検証）| 静的検査（TC-F-S02）|
| EC-F1 | F-F1 `vault encrypt`: 平文 vault → 暗号化 vault マイグレーション + RecoveryDisclosure を 1 度のみ表示 + zeroize 連鎖 (C-19) | 結合（TC-F-I01）|
| EC-F2 | F-F2 `vault decrypt`: 暗号化 vault → 平文 vault 戻し + DECRYPT 二段確認 (C-20 + C-34 paste 抑制) | 結合（TC-F-I02 + I02b）|
| EC-F3 | F-F3 `vault unlock` 2 経路（password / `--recovery`）+ MSG-S03 表示 + 終了コード 0 / 5（バックオフ中）| 結合（TC-F-I03） |
| EC-F4 | F-F4 `vault lock`: cache 即 zeroize + MSG-S04 表示 | 結合（TC-F-I04）|
| EC-F5 | F-F5 `vault change-password`: O(1) 完了 + MSG-S05「VEK は不変のため再 unlock は不要」表示 (REQ-S10) | 結合（TC-F-I05）|
| EC-F6 | F-F6 `vault recovery-show`: 24 語表示 + 1 度限り (C-35) + アクセシビリティ 3 経路 (TC-F-A01..A03) | 結合（TC-F-I06 / I06b）+ アクセシビリティ（TC-F-A01..A03）|
| EC-F7 | F-F7 `vault rekey` + `cache_relocked: true/false` 分岐表示 (C-32 / C-36) + 終了コード 0 | 結合（TC-F-I07 / I07c）|
| EC-F8 | F-F8 `vault rotate-recovery` + `cache_relocked: true/false` 分岐表示 + 24 語 1 度返却 + zeroize | 結合（TC-F-I08）|
| EC-F9 | 既存 CRUD（`add/list/edit/remove`）ロック時 fail fast（MSG-S09(c) + `vault unlock` 誘導 + 終了コード 3、レコード内容/ID/ラベル含めない）| 結合（TC-F-I09 / I09b）|
| EC-F10 | `shikomi list` バナー 3 状態表示（`[plaintext]` / `[encrypted, locked]` / `[encrypted, unlocked]`、ANSI カラー + 文字二重符号化、`NO_COLOR` env 経路で文字のみ）| 結合（TC-F-I10）|
| EC-F11 | アクセシビリティ自動切替（`SHIKOMI_ACCESSIBILITY=1` env 経路で `--print` / `--braille` / `--audio` のいずれかが自動選択）| アクセシビリティ（TC-F-A04）|
| EC-F12 | i18n 翻訳辞書 `messages.toml`（ja-JP / en-US）に MSG-S01〜S20 全 20 キーが定義されている、欠落時 fallback 経路機能 (C-33) | ユニット（TC-F-U02）+ 静的検査（TC-F-S03）|
| EC-F13 | `presenter::recovery_disclosure` 1 度表示構造防衛: `display(words: Vec<SerializableSecretBytes>)` が**所有権消費**、関数戻り後は呼出側で words 再利用不能（型レベル強制）| ユニット（TC-F-U04）+ 静的検査（TC-F-S04）|

### 15.4 Sub-F テストマトリクス

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---|---|---|---|---|
| TC-F-U01 | REQ-S15 | `clap` 派生型 `VaultSubcommand` 8 variant が `derive(Subcommand)` で構築可、`cargo run -- vault --help` で 8 サブコマンド一覧表示 | ユニット | clap parse |
| TC-F-U02 | C-33 / EC-F12 | `i18n::Localizer::new("ja-JP")?.translate("s03_cli")` が翻訳文字列を返却 / 存在しないキー `"nonexistent_key"` で `[missing:nonexistent_key]` 返却（パニック無し）| ユニット | i18n fallback |
| TC-F-U03 | C-34 | `decrypt_confirmation::prompt` で連続入力時刻差 < 30ms を観測 → `Err(CliError::PasteSuspected)`、>= 100ms で `Ok(())` | ユニット | paste 抑制 |
| TC-F-U04 | EC-F13 | `presenter::recovery_disclosure::display(words: Vec<SerializableSecretBytes>)` の signature が **所有権を消費** (`Vec<_>` move)、戻り値型は `()` または `Result<(), DisplayError>`。compile_fail で「`words` を関数戻り後に再利用」を試行 → コンパイル失敗 | ユニット | 所有権消費 |
| TC-F-U05 | EC-F10 | `presenter::mode_banner::display(ProtectionModeBanner::Plaintext)` が `[plaintext]`、`EncryptedLocked` が `[encrypted, locked]`、`EncryptedUnlocked` が `[encrypted, unlocked]`、`Unknown` が `[unknown]` を ANSI カラー付きで返却。`NO_COLOR=1` env 経路でカラーシーケンス除去 | ユニット | バナー文字列 |
| TC-F-U06 | C-32 / C-36 / EC-F7 | `presenter::cache_relocked_warning::display()` が MSG-S07/S19 完了文言 + MSG-S20「次の操作前に `shikomi vault unlock` を再度実行してください」連結を返却、終了コード 0 を返す呼出側責務を doc コメントで明示 | ユニット | MSG 連結 |
| TC-F-U07 | EC-F10 | `ProtectionModeBanner` enum 4 variant（`Plaintext` / `EncryptedLocked` / `EncryptedUnlocked` / `Unknown`）の `match` 網羅をワイルドカード `_` 無しで書く（Sub-D Rev3 / Sub-E TC-E-S01 同型構造防衛）| ユニット | enum 網羅 |
| TC-F-U08 | EC-F11 | `accessibility::detect_route()` が `SHIKOMI_ACCESSIBILITY=1` env で `Some(AccessibilityRoute::Auto)`、明示フラグ `--print` で `Some(AccessibilityRoute::Print)`、未指定 + env 無で `None`（通常 stdout 経路）| ユニット | 自動切替 |
| TC-F-U09 | REQ-S15 | 終了コード `ExitCode` マッピング: 成功=0 / WeakPassword/UTF-8 エラー=1 / Backoff=5 / VaultLocked=3 / Internal/SystemError=2、すべて `usecase::vault::*` 戻り値型で `Result<ExitCode, CliError>` 経由で確定 | ユニット | 終了コード |
| TC-F-U10 | EC-F12 | i18n 辞書 `messages.toml` の MSG-S01〜S20 全 20 キーがロードできる（ja-JP / en-US 両方）、TOML parse エラーが起きない | ユニット | 辞書 load |
| TC-F-U11 | C-37 | clap 派生型に `--no-mode-banner` フラグが**定義されていない**こと、シンボル走査で `no_mode_banner` 識別子が CLI コードに**存在しない**ことを Rust 反射で検証 | ユニット | C-37 補強（grep gate との二段防衛） |
| TC-F-U12 | EC-F1 / C-19 | `presenter::recovery_disclosure::display` の関数本体が `mem::replace(&mut words, vec![])` 等で**確実に Drop を発火**、scope 終了時の zeroize を逃さない（メモリパターン観測 + Sub-A `RecoveryWords` 同型）| ユニット | Drop 連鎖 |
| TC-F-I01 | EC-F1 / REQ-S15 | `assert_cmd` で `shikomi vault encrypt` を強パスワード stdin 注入で実行 → stdout に MSG-S01 + 24 語表示 + 終了コード 0、vault.db が暗号化形式（`SqliteVaultRepository::load` で ProtectionMode::Encrypted）| 結合 | F-F1 正常 |
| TC-F-I02 | EC-F2 / C-20 | `shikomi vault decrypt` で正規パスワード入力 + DECRYPT 大文字確認文字列入力 → 終了コード 0 + vault.db 平文化、不正 DECRYPT 確認 (例: `decrypt`) で 終了コード 1 + DECRYPT 中止メッセージ | 結合 | F-F2 正常 + 確認 |
| TC-F-I02b | C-34 | `expectrl` で paste 模擬（30ms 以内 2 回入力）→ `Err(PasteSuspected)` + 終了コード 1 + MSG-S14 paste 検出文言 | 結合 | C-34 paste 抑制 |
| TC-F-I03 | EC-F3 / C-26 | `shikomi vault unlock` 正パスワード経路 → 終了コード 0 + MSG-S03、間違ったパスワード × 5 回 → 6 回目で終了コード 5 + `BackoffActive` メッセージ + 待機秒数表示 | 結合 | F-F3 正常 + Backoff |
| TC-F-I03b | EC-F3 | `shikomi vault unlock --recovery` で 24 語 stdin 入力 → 終了コード 0 + MSG-S03、不正 mnemonic で MSG-S12 + 終了コード 1 | 結合 | F-F3 recovery 経路 |
| TC-F-I04 | EC-F4 | `shikomi vault lock` → 終了コード 0 + MSG-S04「VEK はメモリから消去」、後続 `shikomi list` で `[encrypted, locked]` バナー + Locked 拒否経路 | 結合 | F-F4 |
| TC-F-I05 | EC-F5 / REQ-S10 | `shikomi vault change-password` で旧/新パスワード入力 → 終了コード 0 + MSG-S05「VEK は不変のため再 unlock は不要」、後続 `shikomi list` で `[encrypted, unlocked]` バナー（cache 維持確認）| 結合 | F-F5 + cache 維持 |
| TC-F-I06 | EC-F6 / C-35 | `vault encrypt` 直後 `vault recovery-show` で 24 語表示 + 終了コード 0、**2 度目以降の `recovery-show` で daemon が `IpcErrorCode::Internal { reason: "recovery-already-disclosed" }` 返却 → CLI が MSG-S09 系変換 + 終了コード 1**（C-35 構造防衛）| 結合 | F-F6 + C-35 |
| TC-F-I07 | EC-F7 | `shikomi vault rekey` 正常経路 → 終了コード 0 + MSG-S07「再暗号化完了 N 件」+ 24 語表示 + cache 維持 (`cache_relocked: true`) | 結合 | F-F7 happy path |
| TC-F-I07c | C-32 / C-36 / EC-F7 | **fault-injection** で `cache_relocked: false` 再現（Sub-E `FORCE_RELOCK_FAILURE` 経路を CLI 経由で発火、`SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` env 等の seam）→ stdout に MSG-S07 + S20 連結（「鍵情報の再キャッシュに失敗、もう一度 unlock してください」）+ **終了コード 0**（C-31/C-36 整合、operation 成功）+ 後続 `shikomi list` で `[encrypted, locked]` バナー観測 | 結合 | C-32 / C-36 Lie-Then-Surprise 防衛 |
| TC-F-I08 | EC-F8 | `shikomi vault rotate-recovery` 正常経路 → 終了コード 0 + MSG-S19 + 24 語表示 + cache 維持 (`cache_relocked: true`) | 結合 | F-F8 |
| TC-F-I09 | EC-F9 / REQ-S16 | Locked 状態で `shikomi list` → 終了コード 3 + MSG-S09(c) + 「`shikomi vault unlock` で解除してください」誘導文言、stdout/stderr に**レコード内容・ID・ラベル含まない** | 結合 | F-S16 ロック時 list |
| TC-F-I09b | EC-F9 / REQ-S16 | Locked 状態で `shikomi add Text "label" "value"` / `shikomi edit <id> ...` / `shikomi remove <id>` → いずれも終了コード 3 + MSG-S09(c) + value/label が stdout/stderr に**漏洩していない**（情報漏洩防衛、grep ゼロ件確認）| 結合 | F-S16 ロック時 CRUD |
| TC-F-I10 | EC-F10 / REQ-S16 | `shikomi list` バナー 3 状態: (a) plaintext vault → `[plaintext]` 灰色、(b) encrypted-locked → `[encrypted, locked]` 橙色、(c) encrypted-unlocked → `[encrypted, unlocked]` 緑色。`NO_COLOR=1` env 経路でカラーシーケンス無し、文字のみ表示 | 結合 | F-S16 バナー |
| TC-F-A01 | EC-F6 | `shikomi vault recovery-show --print` → stdout に PDF magic byte (`%PDF-`) + バイナリ末尾の `%%EOF`、ハイコントラスト + 36pt + 番号付き 24 語 | アクセシビリティ | --print PDF |
| TC-F-A02 | EC-F6 | `shikomi vault recovery-show --braille` → stdout に Braille Ready Format 出力（U+2800..U+28FF 範囲 or `.brf` 改行）+ 24 語が Grade 2 英語点字でエンコードされている | アクセシビリティ | --braille BRF |
| TC-F-A03 | EC-F6 | `shikomi vault recovery-show --audio` → 子プロセス `say` (macOS) / `SAPI` (Win) / `espeak` (Linux) を spawn、stdout には pid + 完了通知のみ。録音可能アプリへのパイプ拒否（fail fast）| アクセシビリティ | --audio TTS |
| TC-F-A04 | EC-F11 | `SHIKOMI_ACCESSIBILITY=1` env で `vault recovery-show`（フラグ無し）→ いずれかの代替経路に自動切替（OS 環境に応じて）、明示フラグと併用時はフラグ優先 | アクセシビリティ | 自動切替 |
| TC-F-E01 | 全契約 / Sub-E TC-E-E01 統合 | **田中（経理担当者、`requirements-analysis.md` ペルソナ）が CLI シナリオを完走**: (1) `shikomi-daemon` 起動 + `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` env で短縮、(2) `shikomi vault unlock` 入力 → MSG-S03 表示確認「vault をアンロックしました」+「アイドル 15 分」（または短縮値）+「自動的にロック」、(3) `shikomi list` で業務（複数レコード閲覧 + `[encrypted, unlocked]` バナー確認）、(4) アイドル放置（短縮で 2 秒 + 1 秒）→ 自動 lock + 次操作で MSG-S09(c) キャッシュ揮発タイムアウト「アイドル N 秒でロックしました」+ 終了コード 3、(5) `shikomi vault unlock` 再入力 → MSG-S03、(6) `shikomi vault change-password` で旧 / 新パスワード入力 → MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」確認 + `shikomi list` で再 unlock なしに業務継続できる（cache 維持） | E2E | 田中ペルソナ完走 |

### 15.5 Sub-F ユニットテスト詳細

#### `clap` 派生型 + i18n + 入力検証（C-33 / C-34 / EC-F12）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-F-U01 | `cargo run -p shikomi-cli -- vault --help` の stdout | 8 サブコマンド (`encrypt` / `decrypt` / `unlock` / `lock` / `change-password` / `recovery-show` / `rekey` / `rotate-recovery`) が一覧表示される、各々の short description が i18n 経由で表示 |
| TC-F-U02 | `Localizer::new("ja-JP")?.translate("s03_cli")` / `translate("nonexistent_key")` | 前者: 翻訳文字列を返却、後者: `[missing:nonexistent_key]` 文字列返却（パニック無し、C-33 fail-soft 維持）|
| TC-F-U03 | `decrypt_confirmation::prompt` を fake reader で 2 回連続入力（時刻差 20ms / 50ms / 200ms）| 20ms: `Err(PasteSuspected)`、50ms 以上: 同 (実装の閾値が 30ms ならば >= 30ms で Ok、ここでは閾値跨ぎ 20/50/200 で挙動分岐確認) |
| TC-F-U04 | `display(words: Vec<SerializableSecretBytes>)` を呼出後、`words` 変数を再利用しようとする compile_fail doc test | 戻り値後 `words` 再利用は所有権消費でコンパイルエラー（型レベル強制）|

#### `presenter` 層（EC-F10 / EC-F13 / C-32）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-F-U05 | `mode_banner::display(ProtectionModeBanner::Plaintext)` の戻り値文字列 | `[plaintext]` 含有 + ANSI escape (`\x1b[36m` cyan dim 等) を含む。`NO_COLOR=1` 環境変数 set 時はカラー部分が除去されることを確認 |
| TC-F-U06 | `cache_relocked_warning::display()` の戻り値文字列 | MSG-S07/S19 完了文言 + MSG-S20「次の操作前に `shikomi vault unlock` を再度実行してください」が連結されている |
| TC-F-U07 | `match banner: ProtectionModeBanner { Plaintext => "p", EncryptedLocked => "el", EncryptedUnlocked => "eu", Unknown => "u", }` をワイルドカード `_` 無しで書く | `cargo check` 警告 0 件（4 variant 全網羅）+ `#[non_exhaustive]` 適用、TC-F-S* grep gate と二段防衛 |
| TC-F-U12 | `recovery_disclosure::display` を `[String; 24]` 渡しで実行後、内部バッファの再観測（test 用 `unsafe` hook 経由）| 実行後の旧バッファ位置が `[0u8; ...]` または retrieve 不能（zeroize 連鎖発火、Sub-A `RecoveryWords` 同型）|

#### アクセシビリティ + 終了コード（EC-F11 / TC-F-U09）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-F-U08 | `accessibility::detect_route()` を以下 4 パターンで呼出: (a) `SHIKOMI_ACCESSIBILITY=1` set + フラグ無し、(b) フラグ `--print` set、(c) どちらも未設定、(d) `SHIKOMI_ACCESSIBILITY=1` + `--audio` 併用 | (a) `Some(Auto)`、(b) `Some(Print)`、(c) `None`（通常 stdout）、(d) `Some(Audio)`（明示フラグ優先）|
| TC-F-U09 | `usecase::vault::unlock` が各 `Result<(), MigrationError>` 系統に対し以下を返す: `Ok` → `ExitCode::SUCCESS`、`Err(WeakPassword)` → `ExitCode::from(1)`、`Err(BackoffActive)` → `ExitCode::from(5)`、`Err(VaultLocked)` → `ExitCode::from(3)`、`Err(Internal)` → `ExitCode::from(2)` | 各分岐で期待 ExitCode 返却 |
| TC-F-U10 | `i18n::Localizer::new("ja-JP")?` / `new("en-US")?` 双方で MSG-S01..S20 全 20 キーを `translate` 呼出 | 全キーで `[missing:...]` ではなく非空文字列を返却 (TC-F-S03 grep gate と相補) |
| TC-F-U11 | `cargo run -- vault list --no-mode-banner` 試行 / `grep -rE "no_mode_banner|--no-mode-banner" crates/shikomi-cli/src/` | clap parse エラー (`unknown flag --no-mode-banner`) + grep ゼロ件、C-37 構造防衛維持 |

### 15.6 Sub-F 結合テスト詳細

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-F-I01 | `assert_cmd` で `shikomi vault encrypt` を強パスワード stdin で実行、tempdir vault | 終了コード 0 + stdout に MSG-S01 + 24 語表示 + vault.db が `ProtectionMode::Encrypted` |
| TC-F-I02 | `vault decrypt` 正規 password + 正規 DECRYPT 大文字確認 | 終了コード 0 + vault.db 平文化 |
| TC-F-I02b | `expectrl` で paste 模擬（30ms 内 2 回入力）| 終了コード 1 + MSG-S14 paste 検出文言 stderr |
| TC-F-I03 | 正パスワード unlock / 5 連続失敗で 6 回目 BackoffActive | 前者: 終了コード 0 + MSG-S03、後者: 終了コード 5 + 待機秒数表示 |
| TC-F-I03b | `vault unlock --recovery` で 24 語入力 / 不正 mnemonic | 前者: 終了コード 0、後者: 終了コード 1 + MSG-S12 |
| TC-F-I04 | `vault lock` 後 `shikomi list` | 終了コード 3 + `[encrypted, locked]` バナー + MSG-S09(c) |
| TC-F-I05 | `vault change-password` 後 `shikomi list` | MSG-S05 表示 + `[encrypted, unlocked]` バナー（cache 維持）|
| TC-F-I06 | `vault encrypt` 直後 `vault recovery-show` | 終了コード 0 + 24 語 stdout 表示 |
| TC-F-I06b | `vault recovery-show` を 2 度目実行 | 終了コード 1 + MSG-S09 系 + recovery 既開示文言（C-35 構造防衛）|
| TC-F-I07 | `vault rekey` 正常経路 | 終了コード 0 + MSG-S07 + 24 語 + cache 維持 (`cache_relocked: true`) |
| TC-F-I07c | fault-injection (`SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` 等) 経由で `cache_relocked: false` 再現 + `vault rekey` 実行 | 終了コード **0**（C-31/C-36）+ MSG-S07/S19 + S20 連結 stderr + 後続 `shikomi list` で `[encrypted, locked]` バナー |
| TC-F-I08 | `vault rotate-recovery` 正常経路 | 終了コード 0 + MSG-S19 + 24 語 + cache 維持 |
| TC-F-I09 | Locked 状態で `shikomi list` | 終了コード 3 + MSG-S09(c) + stdout/stderr にレコード内容・ID・ラベル**含まない** (grep 0 件) |
| TC-F-I09b | Locked 状態で `add` / `edit` / `remove` | 終了コード 3 + MSG-S09(c) + value/label が stdout/stderr に**漏洩しない**（grep 0 件、情報漏洩防衛）|
| TC-F-I10 | (a) plaintext / (b) encrypted-locked / (c) encrypted-unlocked × `NO_COLOR=1` 有/無 で `shikomi list` 各 stdout | (a) `[plaintext]`、(b) `[encrypted, locked]`、(c) `[encrypted, unlocked]` + `NO_COLOR=1` 時はカラーシーケンス無し |

### 15.7 Sub-F アクセシビリティテスト詳細（TC-F-A01〜A04）

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-F-A01 | `shikomi vault recovery-show --print > out.pdf` の `out.pdf` バイナリ先頭 8 バイト + 末尾 16 バイト + 24 語の存在 | 先頭 `%PDF-1.7` (or 1.4+)、末尾 `%%EOF`、PDF 内に 24 語が **番号付き 36pt ハイコントラスト**で配置されている (PDF reader でバイナリ解析、`pdf-extract` crate or `pdfium-rs` 等で本文抽出) |
| TC-F-A02 | `shikomi vault recovery-show --braille > out.brf` のテキスト | Braille Ready Format（U+2800..U+28FF Unicode 点字 or ASCII Braille `.brf` 形式）+ Grade 2 英語点字エンコードで 24 語が読み出せる |
| TC-F-A03 | `shikomi vault recovery-show --audio` 実行 + 標準的な OS TTS バイナリの mock（fake `say` / `espeak`）| 子プロセス spawn 経路を `assert_cmd` で観測、stdout に `pid: N` + 完了通知のみ、24 語が直接 stdout に漏洩しない |
| TC-F-A04 | `SHIKOMI_ACCESSIBILITY=1 shikomi vault recovery-show` (フラグ無し) | OS 環境に応じた代替経路 (Print / Braille / Audio のいずれか) に自動切替、エラー無く完走 |

### 15.8 Sub-F E2E テストケース詳細（TC-F-E01）

`tests/e2e/sub-f-tanaka-persona.sh` (bash + assert_cmd 経由実行) に以下を実装:

| ステップ | 操作 | 期待結果 |
|---|---|---|
| Step 1 | `shikomi-daemon` を `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` 環境変数で起動 | daemon プロセス起動 + UDS / Named Pipe listening |
| Step 2 | `shikomi vault unlock`（強パスワード stdin 注入）| 終了コード 0 + stdout に MSG-S03「vault をアンロックしました」+「アイドル 2 秒で自動的にロック」（短縮値表示）|
| Step 3 | `shikomi list`（複数レコード seed 済 vault）| 終了コード 0 + stdout に `[encrypted, unlocked]` 緑色バナー + レコード一覧 |
| Step 4 | 3 秒 sleep（idle threshold 2 秒 + ポーリング余裕 1 秒）→ `shikomi list` 再実行 | 終了コード 3 + stdout/stderr に MSG-S09(c)「アイドル 2 秒でロックしました、再度 `vault unlock` してください」 |
| Step 5 | `shikomi vault unlock` 再入力 | 終了コード 0 + MSG-S03 |
| Step 6 | `shikomi vault change-password`（旧 / 新パスワード stdin） | 終了コード 0 + stdout に MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」 |
| Step 7 | `shikomi list`（再 unlock 無し）| 終了コード 0 + `[encrypted, unlocked]` バナー + レコード一覧（cache 維持確認、Sub-E REQ-S10 / EC-3 整合）|
| Cleanup | daemon に SIGTERM → graceful shutdown | 終了コード 0 + tempdir 削除 |

**証跡**: stdout / stderr / 終了コード を bash 経由で `tee` し、`/app/shared/attachments/マユリ/sub-f-e2e-tanaka.log` に保存して Discord 添付。

### 15.9 Sub-F 静的検査（grep gate）

Sub-D Rev3 / Rev4 / Sub-E TC-E-S01..S09 で凍結した「**実装直読 SSoT + grep gate による設計書-実装一致機械検証**」原則を Sub-F に継承。

| テストID | 検証対象 | grep ロジック | 失敗時 |
|---|---|---|---|
| TC-F-S01 | EC-F10 / `VaultSubcommand` 8 variant 集合整合 | `awk` で `pub enum VaultSubcommand { ... }` から variant 名抽出 → `Encrypt` / `Decrypt` / `Unlock` / `Lock` / `ChangePassword` / `RecoveryShow` / `Rekey` / `RotateRecovery` の 8 件と完全一致比較（Sub-D TC-D-S05 / Sub-E TC-E-S03 同型）| FAIL + 集合 diff |
| TC-F-S02 | C-37 隠蔽オプション禁止 | `grep -rE "no.mode.banner\|no_mode_banner\|hide.banner" crates/shikomi-cli/src/` がゼロ件であることを機械検証（誰かが `--no-mode-banner` フラグを足したら即時検出）| FAIL + 検出箇所列挙 |
| TC-F-S03 | EC-F12 i18n 辞書 MSG キー全網羅 | `crates/shikomi-cli/src/i18n/locales/{ja-JP,en-US}/messages.toml` を `awk` で解析、MSG-S01..S20 + S07_completed_records_count 等のキーを抽出して期待集合と一致比較 | FAIL + 不足キー列挙 |
| TC-F-S04 | EC-F13 `recovery_disclosure::display` 1 度表示構造防衛 | (a) 関数 signature が `display(words: Vec<SerializableSecretBytes>)` で**所有権消費**形であること（`&` 借用ではないこと）、(b) 関数本体に `mem::replace` または `drop(words)` 等の zeroize 強制経路が存在することを grep | FAIL + signature mismatch / zeroize 経路欠落 |
| TC-F-S05 | EC-F11 アクセシビリティ 3 経路 + 自動切替 | `crates/shikomi-cli/src/accessibility/` 配下に `print_pdf.rs` / `braille_brf.rs` / `audio_tts.rs` 全 3 ファイルが存在 + `accessibility::detect_route` 関数が SHIKOMI_ACCESSIBILITY env を読む経路が存在することを grep | FAIL + 欠落ファイル / 関数 |

これらは `tests/docs/sub-f-static-checks.sh` で実装する（Sub-D / Sub-E 同パターン）。Sub-F は **TC-F-S01..S05 の 5 件**。

### 15.10 Sub-F テスト実行手順

```bash
# Rust unit + integration tests
cargo test -p shikomi-cli --lib                     # TC-F-U01..U12
cargo test -p shikomi-cli --test vault_subcommands  # TC-F-I01..I10
cargo test -p shikomi-cli --test accessibility_paths  # TC-F-A01..A04

# Sub-F 静的検証 (cargo 不要、TC-F-S01..S05)
bash tests/docs/sub-f-static-checks.sh

# E2E (田中ペルソナ、TC-F-E01)
bash tests/e2e/sub-f-tanaka-persona.sh

# 既存 Sub-A〜E static checks 再実行（回帰防止）
bash tests/docs/sub-a-static-checks.sh
bash tests/docs/sub-b-static-checks.sh
bash tests/docs/sub-c-static-checks.sh
bash tests/docs/sub-d-static-checks.sh
bash tests/docs/sub-e-static-checks.sh

# Sub-0 lint / cross-ref（回帰防止）
python3 tests/docs/sub-0-structure-lint.py
bash tests/docs/sub-0-cross-ref.sh

# 横断: daemon-ipc IpcResponse::Records 構造体化の後方互換 TC（Sub-F 工程3 で同期追加）
cargo test -p shikomi-daemon --test ipc_integration
```

### 15.11 Sub-F テスト証跡

- `cargo test -p shikomi-cli --lib` の stdout（unit pass 件数）
- `cargo test -p shikomi-cli --test vault_subcommands` の stdout（integration TC-F-I01..I10 pass）
- `cargo test -p shikomi-cli --test accessibility_paths` の stdout（PDF / BRF / TTS magic byte assert pass）
- 静的検証スクリプト stdout（`sub-f-static-checks.sh` **5 件: TC-F-S01..S05**）
- **TC-F-E01 田中ペルソナ E2E**: `bash tests/e2e/sub-f-tanaka-persona.sh` の stdout / stderr / 終了コード ログ → `/app/shared/attachments/マユリ/sub-f-e2e-tanaka.log`
- daemon-ipc 横断 regression 結果（TC-IT-026..028 想定の `IpcResponse::Records` 構造体化後方互換 pass）
- 全て `/app/shared/attachments/マユリ/sub-f-*.txt` に保存し Discord 添付

### 15.12 後続 GUI feature への引継ぎ（Sub-F から派生）

| 引継ぎ項目 | 内容 |
|---|---|
| **Tauri WebView 起動経路** | `shikomi_cli::usecase::vault::*` の `pub` 公開範囲を Sub-F PR で確定 → 後続 GUI feature が再利用 |
| **MSG-S17 GUI バッジ** | 後続 GUI feature で実装、Sub-F は CLI 経路のみで TBD |
| **i18n 辞書共有** | `messages.toml` を CLI / GUI 両方で参照、`shared/i18n/` 等に再配置する Boy Scout を後続 GUI feature 工程2 で検討 |
| **アクセシビリティ拡張** | GUI 側で screen reader API（macOS `NSAccessibility` / Windows UIA / Linux AT-SPI）統合、`--print` / `--braille` / `--audio` を GUI モーダルからも呼出可能にする |

### 15.13 Sub-F 工程4 実施実績

工程4 完了後、Sub-F 実装担当 + テスト担当（涅マユリ想定）が本ファイルを READ → EDIT で実績を追記する。雛形は Sub-A §10.11 / Sub-B §11.11 / Sub-C §12.12 / Sub-D §13.12 / Sub-E §14.13 に従う。**Sub-A〜E で観測したパターン**: 銀ちゃんは設計書の proptest / criterion bench / KAT 件数等を**単発 fixture で省略する傾向**、セルは設計書の variant 数を**断定的に記述してドリフト**させる傾向、いずれも実装直読 + grep gate で構造封鎖する（Bug-A-001 / Bug-B-001 / Bug-C-001 / Bug-D-007 / Bug-E-001 連鎖、Petelgeuse Rev1〜Rev4 連続指摘の Sub-F 段階での予防）。
