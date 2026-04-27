# テスト設計書 — Sub-F (#44) shikomi-cli vault サブコマンド (Rev1)

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-f-cli-subcommands.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->
<!-- 横断的変更: 本書は vault-encryption feature のテスト設計だが、daemon-ipc feature の `IpcResponse::Records` 構造体化に伴う後方互換 TC（TC-IT-026..028 想定）と双方向参照する。 -->
<!-- TC-E-E01 田中ペルソナ E2E は Sub-E `sub-e-vek-cache-ipc.md` §14.4 で凍結された 6 ステップシナリオを Sub-F CLI 完成後に**実機完走**するもの。本書 §15.8 で TC-F-E01 として詳細化（同型 ID）。 -->
<!-- 終了コード: cli-subcommands.md §終了コード SSoT を**唯一の真実源**として参照、本書では再定義しない（ペガサス致命指摘②解消）。 -->

## 15. Sub-F (#44) テスト設計 — shikomi-cli vault サブコマンド + 既存 CRUD ロック時挙動 + 田中ペルソナ E2E

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#44](https://github.com/shikomi-dev/shikomi/issues/44) |
| 対象 PR | #69（`69f471c` Rev1、設計フェーズ + Rev1 内部レビュー解消）|
| 対象成果物 | `vault-encryption/detailed-design/cli-subcommands.md`（Rev1 修正、§セキュリティ設計 / §env seam / §終了コード SSoT 新設）/ `requirements.md`（REQ-S15/S16 確定 + MSG-S07/S11/S14/S18/S20 CLI 文言）/ `basic-design/{processing-flows,ux-and-msg,index}.md`（EDIT、F-F1〜F-F8 + MSG 翻訳キー索引）/ `sub-0-threat-model.md`（CLI 攻撃面追補）/ `cli-vault-commands/requirements.md`（MSG-CLI-103 Boy Scout）/ **横断**: `daemon-ipc/{requirements.md, detailed-design/protocol-types.md}`（`IpcResponse::Records` 構造体化、`ProtectionModeBanner` enum 追加）|
| 設計根拠 | `cli-subcommands.md` `Subcommand::Vault(VaultSubcommand)` **7 variant**（`recovery-show` 廃止）/ 7 usecase / 3 presenter (recovery_disclosure / mode_banner / cache_relocked_warning) / `--output {screen\|print\|braille\|audio}` 統合 / i18n TOML 辞書 / 契約 **C-33〜C-41**（欠落キー fail-soft / paste 抑制 / disclose 1 度限り / cache_relocked: false 終了コード 0 / `mode_banner::display` 必須呼出 / stdin パイプ拒否 / output 排他 / env seam debug 限定 / core dump 抑制）|
| 対象 crate | `shikomi-cli`（`cli::Subcommand::Vault` + `cli::VaultSubcommand` enum 7 variant + `usecase::vault::*` 7 件 + `presenter::{recovery_disclosure,mode_banner,cache_relocked_warning}` + `input::{password,mnemonic,decrypt_confirmation}` + `accessibility::{print_pdf,braille_brf,audio_tts,output_target}` + `i18n::Localizer` + `process_hardening::{prctl,setrlimit,seterrormode}`、新規）+ `shikomi-core`（`IpcResponse::Records` 構造体化 + `ProtectionModeBanner` enum 追加、横断的変更）+ **`shikomi-daemon` Boy Scout**（`IdleTimer` env var 連動 + `FORCE_RELOCK_FAILURE` env init + 起動時 allowlist、`#[cfg(debug_assertions)]` 限定）|
| **Sub-F TC 総数** | **37 件**（ユニット 13 + 結合 12 + アクセシビリティ 5 + E2E 1 + property 0 + 静的検査 6 = 37、Sub-A〜E と同型レンジ。Rev1 で +2 TC: TC-F-U13 stdin 拒否 (C-38) / TC-F-A05 一時ファイル umask 077、+1 TC-F-S06 env allowlist grep gate）|

### 15.1 Sub-F テストレベルの読み替え（CLI 経路 + アクセシビリティ + 田中ペルソナ E2E）

Sub-F は **Sub-A〜E で凍結した型・契約・IPC スキーマ + Sub-D `VaultMigration` + Sub-E daemon ハンドラを CLI 経路で具現化**する Sub。Vモデル対応：

| テストレベル | 通常の対応 | Sub-F での読み替え | 検証手段 |
|---|---|---|---|
| **ユニット** | メソッド単位、ホワイトボックス | (a) `clap` 派生型 `VaultSubcommand` 7 variant の構築 + 引数 parse、(b) `i18n::Localizer` fallback (C-33)、(c) `decrypt_confirmation::prompt` paste 抑制 `< 30ms` 機械閾値 (C-34)、(d) `recovery_disclosure::display(words: Vec<SerializableSecretBytes>, target: OutputTarget)` の zeroize 連鎖、(e) `mode_banner::display` の ANSI カラー / NO_COLOR 切替、(f) `cache_relocked_warning::display` の MSG-S20 連結、(g) `ProtectionModeBanner` 4 variant 網羅（cross-crate `_` arm 許可）、(h) accessibility 自動切替 (SHIKOMI_ACCESSIBILITY env)、(i) 終了コード `ExitCode` マッピング（cli-subcommands.md §終了コード SSoT 参照）、(j) **stdin パイプ拒否 (C-38)**、(k) **core dump 抑制 (C-41)** | `cargo test -p shikomi-cli --lib cli::tests` + `cargo test -p shikomi-cli --lib presenter::tests` + `cargo test -p shikomi-cli --lib i18n::tests` + `cargo test -p shikomi-cli --lib input::tests` |
| **結合** | モジュール連携、契約検証 | (a) **7 サブコマンド × IPC V2 ラウンドトリップ** (recovery-show 廃止): `vault encrypt/decrypt/unlock/lock/change-password/rekey/rotate-recovery` を `assert_cmd` 経由実行、(b) **既存 CRUD ロック時 fail fast** (REQ-S16): `shikomi list/add/edit/remove` を Locked 状態で実行、(c) **shikomi list バナー 3 状態**、(d) **`cache_relocked: false` 表示**: env seam (`SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1`) + 終了コード 0 (C-31/C-36)、(e) **stdin パイプ拒否** (`echo pw \| shikomi vault unlock` → 終了コード 1, C-38) | `cargo test -p shikomi-cli --test vault_subcommands` + `cargo test -p shikomi-cli --test mode_banner_integration` + `assert_cmd` + `tempfile` + 実 `shikomi-daemon` 子プロセス + `expectrl` PTY |
| **アクセシビリティ** | WCAG 2.1 AA 経路、stdout fixture | `vault encrypt --output print` PDF / `--output braille` BRF / `--output audio` TTS / `SHIKOMI_ACCESSIBILITY=1` 自動切替 / **PDF/BRF 一時ファイル umask 077 確認** の 5 経路。stdout のバイナリヘッダ / 構造で出力経路を機械検証 | `cargo test -p shikomi-cli --test accessibility_paths` + PDF/BRF magic byte assert + ファイルパーミッション確認 |
| **E2E** | 完全ブラックボックス、ペルソナ | **TC-F-E01**（Sub-E `TC-E-E01` 田中ペルソナ完走の Sub-F 段階詳細化）: 田中（経理担当者）が daemon 起動 → `shikomi vault unlock` → `shikomi list` で業務 → アイドル自動 lock → 次操作で MSG-S09(c) → `shikomi vault unlock` 再入力 → `shikomi vault change-password` → MSG-S05 確認、6 ステップ完走 | `bash tests/e2e/sub-f-tanaka-persona.sh` + `assert_cmd` + `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` env 経由 idle 短縮（C-40 debug ビルド限定 seam）|
| **property** | ランダム入力での invariant | Sub-F 範囲では proptest 不要（CLI clap parse は `clap` 自体が網羅検証済、IPC ラウンドトリップは Sub-E TC-E-P01 で proptest 担保済）| — |

### 15.2 外部 I/O 依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---|---|---|---|
| **`shikomi-daemon` プロセス** | — | `tests/helpers/daemon_spawn.rs`（`assert_cmd::Command` で daemon 子プロセス起動 + tempdir socket / `SHIKOMI_VAULT_DIR` env 注入 + graceful shutdown + **C-40 env allowlist** 経由の `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` / `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` 設定）| **既存資産再利用 + Sub-E daemon Boy Scout 拡張**（`crates/shikomi-daemon/tests/e2e_daemon.rs` の `daemon_spawn` ヘルパに env seam 引数追加、Sub-F 工程3 で銀時実装）|
| **TTY 入力 (password / mnemonic / decrypt confirmation)** | — | `expectrl` (Unix 限定 dev-dep、Sub-D `e2e_daemon_phase15_pty.rs` で既導入) で TTY を擬似制御、**stdin パイプ拒否確認** (C-38) には `assert_cmd::Command::stdin(predicates::str::is_empty)` で非 TTY 経路 | **既存資産再利用** + C-38 経路は新規 |
| **PDF / BRF / TTS 出力** | — | stdout バイナリを `bytes::Bytes` で受領後 magic byte / 構造を assert（PDF: `%PDF-1.7`、BRF: ASCII Braille range U+2800..U+28FF or `.brf` line ending）、**出力ファイルのパーミッション確認** (TC-F-A05、`std::fs::metadata.mode() == 0o600`) | **不要**（出力経路の自前生成、外部 API ではない、liblouis FFI **不採用** で自前 wordlist テーブル使用）|
| **`SHIKOMI_LOCALE` / `SHIKOMI_ACCESSIBILITY` / `SHIKOMI_DAEMON_*` 環境変数** | — | `temp-env` crate or `std::env::set_var` 直接 (テストシリアル化、`#[serial]`)、`SHIKOMI_DAEMON_*` は **C-40 allowlist 経由のみ**設定可 | **不要**（local env、外部 API ではない）|
| **OS スクリーンリーダー検出（macOS/Win/Linux）**| — | `--output audio` 自動切替経路は OS API 依存、Sub-F 結合層では `--output audio` 明示フラグで強制 → **手動 smoke テスト**は工程5 で OS 別実施 | **不要**（OS 依存経路は Sub-F 工程5 で OS 別 manual smoke）|

**理由**: Sub-F は CLI 経路 + アクセシビリティ + 田中ペルソナ E2E の組合せ層。暗号計算・マイグレーション・daemon ライフサイクル・IPC スキーマは Sub-A〜E で担保済。Sub-F 固有の検証対象は **(1) clap 派生型 7 variant の正規 parse**、**(2) i18n Localizer fallback (C-33)**、**(3) paste 抑制機械閾値 (C-34)**、**(4) disclose 1 度限り (C-35)**、**(5) `cache_relocked: false` 表示 + 終了コード 0 (C-31/C-36)**、**(6) `mode_banner::display` 必須呼出 (C-37)**、**(7) stdin パイプ拒否 (C-38)**、**(8) `--output` 排他 (C-39)**、**(9) env seam debug 限定 (C-40)**、**(10) core dump 抑制 (C-41)**、**(11) アクセシビリティ 4 経路 + 一時ファイル umask**、**(12) 田中ペルソナ E2E 完走**。

### 15.3 Sub-F 受入基準（REQ-S15 / REQ-S16 + 契約 C-33〜C-41 + Sub-A〜E 累積継承）

| 受入基準ID | 内容 | 検証レベル |
|---|---|---|
| **C-33** | i18n 辞書キー欠落時もパニックさせず英語 fallback で fail-soft | ユニット（TC-F-U02） |
| **C-34** | `vault decrypt` 二段確認 paste 抑制（**入力時刻差 `< 30ms = Err(PasteSuspected)` / `>= 30ms = Ok`** 機械閾値、Rev1 ペテルギウス指摘5 解消）| ユニット（TC-F-U03）+ 結合（TC-F-I02b） |
| **C-35** | `disclose` 後の 24 語再表示は daemon 側で構造的拒否（C-19 整合、`recovery-show` 廃止後も意味維持）| 結合（TC-F-I06） |
| **C-36** | `cache_relocked: false` 経路で終了コード 0、Err 終了コードを返さない（C-31 整合）| 結合（TC-F-I07c）|
| **C-37** | `usecase::list` の出力経路で `mode_banner::display` を**必須呼出**（型レベル強制 + cross-crate grep gate、Rev1 ペテルギウス指摘7 再設計）| ユニット（TC-F-U07）+ 静的検査（TC-F-S02）|
| **C-38** | パスワード / 24 語入力は `/dev/tty` 経由のみ、stdin パイプ拒否（Rev1 服部指摘5 解消）| ユニット（TC-F-U13）+ 結合（TC-F-I12） |
| **C-39** | `--output {screen\|print\|braille\|audio}` フラグは排他指定、`screen` 既定 + アクセシビリティ自動切替 | ユニット（TC-F-U08） |
| **C-40** | env seam は `#[cfg(debug_assertions)]` 限定 + allowlist sanity check（Rev1 服部指摘6 + ペテルギウス致命3 解消）| 静的検査（TC-F-S05 + TC-F-S06）|
| **C-41** | shikomi-cli プロセスは core dump 抑制（Linux `prctl` / macOS `setrlimit` / Windows `SetErrorMode`）| ユニット（TC-F-U10）|
| EC-F1 | F-F1 `vault encrypt --output {target}`: 平文 vault → 暗号化 vault マイグレーション + RecoveryDisclosure を 1 度のみ表示 + zeroize 連鎖 (C-19) | 結合（TC-F-I01）+ アクセシビリティ（TC-F-A01〜A03）|
| EC-F2 | F-F2 `vault decrypt`: 暗号化 vault → 平文 vault 戻し + DECRYPT 二段確認 (C-20 + C-34 paste 抑制) | 結合（TC-F-I02 + I02b）|
| EC-F3 | F-F3 `vault unlock` 2 経路（password / `--recovery`）+ MSG-S03 表示 + 終了コード 0 / 2（BackoffActive）/ 5（RecoveryRequired）| 結合（TC-F-I03 / I03b） |
| EC-F4 | F-F4 `vault lock`: cache 即 zeroize + MSG-S04 表示 | 結合（TC-F-I04）|
| EC-F5 | F-F5 `vault change-password`: O(1) 完了 + MSG-S05「VEK は不変のため再 unlock は不要」表示 (REQ-S10) | 結合（TC-F-I05）|
| EC-F6 | F-F6 `vault rekey --output`: 再暗号化 + cache_relocked 分岐 (C-32/C-36) + 24 語表示 + zeroize | 結合（TC-F-I07 / I07c）|
| EC-F7 | F-F7 `vault rotate-recovery --output`: rekey + recovery rotation atomic + cache_relocked 分岐 + 24 語 1 度返却 + zeroize | 結合（TC-F-I08）|
| EC-F8 | F-F8 既存 CRUD（`add/list/edit/remove`）ロック時 fail fast（MSG-S09(c) + `vault unlock` 誘導 + 終了コード 3、レコード内容/ID/ラベル含めない）| 結合（TC-F-I09 / I09b）|
| EC-F9 | `shikomi list` バナー 3 状態表示（`[plaintext]` / `[encrypted, locked]` / `[encrypted, unlocked]`、ANSI カラー + 文字二重符号化、`NO_COLOR` env 経路で文字のみ）| 結合（TC-F-I10）|
| EC-F10 | アクセシビリティ自動切替（`SHIKOMI_ACCESSIBILITY=1` env 経路で `--output {print,braille,audio}` のいずれかが自動選択）| アクセシビリティ（TC-F-A04）|
| EC-F11 | i18n 翻訳辞書 `messages.toml`（ja-JP / en-US）に MSG-S01〜S20 全 20 キーが定義されている、欠落時 fallback 経路機能 (C-33) | ユニット（TC-F-U02）+ 静的検査（TC-F-S03）|
| EC-F12 | `presenter::recovery_disclosure::display(words: Vec<SerializableSecretBytes>, target)` 1 度表示構造防衛: **所有権消費**、関数戻り後は呼出側で words 再利用不能（型レベル強制）| ユニット（TC-F-U04 / TC-F-U12）+ 静的検査（TC-F-S04）|
| EC-F13 | PDF/BRF 一時ファイル / リダイレクト先のパーミッション `0o600` 相当（Rev1 服部指摘 §一時ファイル対策、ユーザの `>` リダイレクト先は umask 077 案内）| アクセシビリティ（TC-F-A05）|

### 15.4 Sub-F テストマトリクス

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---|---|---|---|---|
| TC-F-U01 | REQ-S15 | `clap` 派生型 `VaultSubcommand` **7 variant** が `derive(Subcommand)` で構築可、`cargo run -- vault --help` で 7 サブコマンド一覧表示（`recovery-show` は廃止済で表示されない、Rev1 ペガサス致命指摘①解消）| ユニット | clap parse |
| TC-F-U02 | C-33 / EC-F11 | `i18n::Localizer::new("ja-JP")?.translate("s03_cli")` が翻訳文字列を返却 / 存在しないキー `"nonexistent_key"` で `[missing:nonexistent_key]` 返却（パニック無し）| ユニット | i18n fallback |
| TC-F-U03 | C-34 | `decrypt_confirmation::prompt` で連続入力時刻差を以下 3 段で検証: (a) **`< 30ms`** (例: 20ms) → `Err(CliError::PasteSuspected)`、(b) **`>= 30ms`** (例: 30ms 跨ぎ + 50ms) → `Ok(())`、(c) 通常入力 (200ms+) → `Ok(())`。Rev1 ペテルギウス指摘5 機械化 | ユニット | paste 閾値 |
| TC-F-U04 | EC-F12 | `display(words: Vec<SerializableSecretBytes>, target: OutputTarget)` を呼出後、`words` 変数を再利用しようとする compile_fail doc test | ユニット | 所有権消費 |
| TC-F-U05 | EC-F9 | `mode_banner::display(ProtectionModeBanner::Plaintext)` が `[plaintext]`、`EncryptedLocked` が `[encrypted, locked]`、`EncryptedUnlocked` が `[encrypted, unlocked]`、`Unknown` が `[unknown]` を ANSI カラー付きで返却。`NO_COLOR=1` env 経路でカラーシーケンス除去 | ユニット | バナー文字列 |
| TC-F-U06 | C-32 / C-36 | `presenter::cache_relocked_warning::display()` が MSG-S07/S19 完了文言 + MSG-S20「次の操作前に `shikomi vault unlock` を再度実行してください」連結を返却、終了コード 0 を返す呼出側責務を doc コメントで明示 | ユニット | MSG 連結 |
| TC-F-U07 | C-37 / EC-F9 | `match banner: ProtectionModeBanner { Plaintext => "p", EncryptedLocked => "el", EncryptedUnlocked => "eu", Unknown => "u", _ => "fail-secure" }` を **cross-crate** 経路で書く。**defensive fail-secure `_` arm を許可**（Sub-E TC-E-S01 同型、Rev1 ペテルギウス致命1 解消、`#[non_exhaustive]` cross-crate 防御的 `_` arm の正当性を機械検証）| ユニット | enum 網羅 + 防御 arm |
| TC-F-U08 | C-39 / EC-F10 | `accessibility::output_target::resolve()` を 4 パターンで呼出: (a) `SHIKOMI_ACCESSIBILITY=1` set + フラグ無し、(b) フラグ `--output print` set、(c) どちらも未設定、(d) `SHIKOMI_ACCESSIBILITY=1` + `--output audio` 併用 | ユニット | 自動切替 |
| TC-F-U09 | REQ-S15 / cli-subcommands.md §終了コード SSoT | 終了コードマッピング SSoT 参照: 成功=**0** / 一般エラー=**1** / **`BackoffActive`=2** / **`VaultLocked`=3** / **`ProtocolDowngrade`=4** / **`RecoveryRequired`=5** / `EX_USAGE`=64 / `EX_CONFIG`=78、すべて `usecase::vault::*` 戻り値型で `Result<ExitCode, CliError>` 経由で確定。Rev1 ペガサス致命指摘②解消 | ユニット | 終了コード SSoT 整合 |
| TC-F-U10 | C-41 | `process_hardening::install()` が起動時に Linux `prctl(PR_SET_DUMPABLE, 0)` / macOS `setrlimit(RLIMIT_CORE, 0)` / Windows `SetErrorMode(SEM_NOGPFAULTERRORBOX)` を呼び出すこと、関数 signature 存在確認（unit はシグネチャと OS 別 cfg 分岐の存在のみ、syscall 実機呼出は OS 別 manual smoke）| ユニット | core dump 抑制 |
| TC-F-U11 | C-37 | clap 派生型に `--no-mode-banner` / `--hide-banner` 等の隠蔽フラグが**定義されていない**こと、`mode_banner::display` の呼出経路が `usecase::list::execute` から到達可能であることを反射 + grep 検証 | ユニット | 隠蔽不能補強 |
| TC-F-U12 | EC-F12 / C-19 | `presenter::recovery_disclosure::display(words: Vec<SerializableSecretBytes>, target: OutputTarget)` の関数本体が `mem::replace(&mut words, vec![])` 等で**確実に Drop を発火**、scope 終了時の zeroize を逃さない（メモリパターン観測 + Sub-A `RecoveryWords` 同型）。**引数型は `Vec<SerializableSecretBytes>`** で統一（Rev1 ペテルギウス致命2 解消、`[String; 24]` 言及削除）| ユニット | Drop 連鎖 + 型整合 |
| TC-F-U13 | C-38 | `input::password::prompt` / `input::mnemonic::prompt` が以下 3 パターン: (a) TTY 経由 (`expectrl` PTY) → `Ok(SecretString)`、(b) stdin パイプ非 TTY (`is-terminal::IsTerminal::is_terminal == false`) → `Err(CliError::NonInteractivePassword)`、(c) `/dev/tty` open 失敗（環境依存）→ `Err(CliError::TtyUnavailable)` | ユニット | stdin 拒否 (Rev1 新設) |
| TC-F-I01 | EC-F1 / REQ-S15 | `assert_cmd` で `shikomi vault encrypt --output screen` を強パスワード stdin 注入で実行（PTY 経由、C-38）→ stdout に MSG-S01 + 24 語表示 + 終了コード 0、vault.db が暗号化形式（`SqliteVaultRepository::load` で ProtectionMode::Encrypted）| 結合 | F-F1 正常 |
| TC-F-I02 | EC-F2 / C-20 | `shikomi vault decrypt` で正規パスワード入力 + DECRYPT 大文字確認文字列入力 → 終了コード 0 + vault.db 平文化、不正 DECRYPT 確認 (例: `decrypt`) で 終了コード 1 + DECRYPT 中止メッセージ | 結合 | F-F2 正常 + 確認 |
| TC-F-I02b | C-34 | `expectrl` で paste 模擬 (`< 30ms` で 2 回入力) → `Err(PasteSuspected)` + 終了コード 1 + MSG-S14 paste 検出文言、`>= 30ms` 跨ぎで通常入力 OK | 結合 | C-34 paste 抑制 |
| TC-F-I03 | EC-F3 / C-26 | `shikomi vault unlock` 正パスワード経路 → 終了コード 0 + MSG-S03、間違ったパスワード × 5 回 → 6 回目で **終了コード 2 (BackoffActive)** + 待機秒数表示。SSoT (cli-subcommands.md §終了コード) 整合 | 結合 | F-F3 正常 + Backoff |
| TC-F-I03b | EC-F3 | `shikomi vault unlock --recovery` で 24 語 stdin 入力 → 終了コード 0 + MSG-S03、不正 mnemonic で MSG-S12 + 終了コード 1、password 経路で `RecoveryRequired` 発火時は **終了コード 5** | 結合 | F-F3 recovery 経路 |
| TC-F-I04 | EC-F4 | `shikomi vault lock` → 終了コード 0 + MSG-S04「VEK はメモリから消去」、後続 `shikomi list` で `[encrypted, locked]` バナー + Locked 拒否経路 | 結合 | F-F4 |
| TC-F-I05 | EC-F5 / REQ-S10 | `shikomi vault change-password` で旧/新パスワード入力 → 終了コード 0 + MSG-S05「VEK は不変のため再 unlock は不要」、後続 `shikomi list` で `[encrypted, unlocked]` バナー（cache 維持確認）| 結合 | F-F5 + cache 維持 |
| TC-F-I06 | EC-F1 / C-35 | **disclose 1 度限り**: `vault encrypt --output screen` 直後の **同 vault に対する 2 度目の `vault encrypt`** → daemon 側 `MigrationError::AlreadyEncrypted` → CLI が MSG-S09 系変換 + 終了コード 1。recovery-show 廃止後も C-35 意味は「daemon 側 disclose 構造的拒否」として維持 | 結合 | C-35 構造防衛 |
| TC-F-I07 | EC-F6 | `shikomi vault rekey --output screen` 正常経路 → 終了コード 0 + MSG-S07「再暗号化完了 N 件」+ 24 語表示 + cache 維持 (`cache_relocked: true`) | 結合 | F-F6 happy path |
| TC-F-I07c | C-32 / C-36 / EC-F6 | **fault-injection** で `cache_relocked: false` 再現（**`SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` env 経由**、Sub-E `FORCE_RELOCK_FAILURE` 経路を C-40 allowlist 経由で発火、daemon は `#[cfg(debug_assertions)]` 限定で env 読込）→ stdout に MSG-S07 + S20 連結（「鍵情報の再キャッシュに失敗、もう一度 unlock してください」）+ **終了コード 0**（C-31/C-36 整合、operation 成功）+ 後続 `shikomi list` で `[encrypted, locked]` バナー観測 | 結合 | C-32 / C-36 Lie-Then-Surprise 防衛 |
| TC-F-I08 | EC-F7 | `shikomi vault rotate-recovery --output screen` 正常経路 → 終了コード 0 + MSG-S19 + 24 語表示 + cache 維持 (`cache_relocked: true`) | 結合 | F-F7 |
| TC-F-I09 | EC-F8 / REQ-S16 | Locked 状態で `shikomi list` → 終了コード 3 + MSG-S09(c) + 「`shikomi vault unlock` で解除してください」誘導文言、stdout/stderr に**レコード内容・ID・ラベル含まない** | 結合 | F-F8 ロック時 list |
| TC-F-I09b | EC-F8 / REQ-S16 | Locked 状態で `shikomi add Text "label" "value"` / `shikomi edit <id> ...` / `shikomi remove <id>` → いずれも終了コード 3 + MSG-S09(c) + value/label が stdout/stderr に**漏洩していない**（情報漏洩防衛、grep ゼロ件確認）| 結合 | F-F8 ロック時 CRUD |
| TC-F-I10 | EC-F9 / REQ-S16 | `shikomi list` バナー 3 状態: (a) plaintext vault → `[plaintext]` 灰色、(b) encrypted-locked → `[encrypted, locked]` 橙色、(c) encrypted-unlocked → `[encrypted, unlocked]` 緑色。`NO_COLOR=1` env 経路でカラーシーケンス無し、文字のみ表示 | 結合 | F-F8 バナー |
| TC-F-I12 | C-38 | **stdin パイプ拒否**: `echo "strong-password" \| shikomi vault unlock` を `assert_cmd::Command::write_stdin` で実行（非 TTY 経路）→ 終了コード 1 + MSG-S09(b) 系または `CliError::NonInteractivePassword` 文言 + 「パスワードはプロンプト入力のみ。`echo \| shikomi` の経路は提供していません」案内 | 結合 | C-38 (Rev1 新設) |
| TC-F-A01 | EC-F1 | `shikomi vault encrypt --output print` → stdout に PDF magic byte (`%PDF-`) + バイナリ末尾の `%%EOF`、ハイコントラスト + 36pt + 番号付き 24 語（recovery-show 廃止後の出力経路、Rev1 ペガサス致命指摘①再構築）| アクセシビリティ | --output print PDF |
| TC-F-A02 | EC-F1 | `shikomi vault encrypt --output braille` → stdout に Braille Ready Format 出力（U+2800..U+28FF 範囲 or `.brf` 改行）+ 24 語が Grade 2 英語点字でエンコード（**自前 wordlist テーブル方式**、liblouis FFI 不採用）| アクセシビリティ | --output braille BRF |
| TC-F-A03 | EC-F1 | `shikomi vault encrypt --output audio` → 子プロセス `say` (macOS) / `SAPI` (Win) / `espeak` (Linux) を spawn、**env サニタイズ allowlist 通過確認 + dictation 学習 prefs 確認のみ**機械検証（録音可能アプリ検出は **OS 不可能境界として MSG-S18 受容前提**、Rev1 ペテルギウス指摘6 scope 縮小）。stdout には pid + 完了通知のみ | アクセシビリティ | --output audio TTS scope 縮小 |
| TC-F-A04 | EC-F10 | `SHIKOMI_ACCESSIBILITY=1` env で `vault encrypt`（`--output` フラグ無し）→ いずれかの代替経路に自動切替（OS 環境に応じて）、明示フラグと併用時はフラグ優先 | アクセシビリティ | 自動切替 |
| TC-F-A05 | EC-F13 | `shikomi vault encrypt --output print > out.pdf` のリダイレクト後、`std::fs::metadata(&out_pdf).permissions().mode()` が **`0o600` 相当** (umask 077 経路の証拠) であること、`/tmp` 経由の中間ファイルが**生成されていない**こと（fs walk + assert）。Rev1 服部指摘 §一時ファイル対策の機械検証 | アクセシビリティ | 一時ファイル umask (Rev1 新設) |
| TC-F-E01 | 全契約 / Sub-E TC-E-E01 統合 | **田中（経理担当者、`requirements-analysis.md` ペルソナ）が CLI シナリオを完走**: (1) `shikomi-daemon` 起動 + `SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` env で短縮 (C-40 allowlist 経由)、(2) `shikomi vault unlock` 入力 (PTY 経由、C-38) → MSG-S03 表示確認、(3) `shikomi list` で業務 + `[encrypted, unlocked]` バナー確認、(4) アイドル放置 (短縮 2 秒 + ポーリング 1 秒) → 自動 lock + 次操作で MSG-S09(c) + 終了コード **3**、(5) `shikomi vault unlock` 再入力 → MSG-S03、(6) `shikomi vault change-password` で旧 / 新パスワード入力 → MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」確認 + `shikomi list` で再 unlock なしに業務継続 (cache 維持) | E2E | 田中ペルソナ完走 |

### 15.5 Sub-F ユニットテスト詳細

#### `clap` 派生型 + i18n + 入力検証（C-33 / C-34 / C-38 / EC-F11）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-F-U01 | `cargo run -p shikomi-cli -- vault --help` の stdout | **7 サブコマンド** (`encrypt` / `decrypt` / `unlock` / `lock` / `change-password` / `rekey` / `rotate-recovery`) が一覧表示される、`recovery-show` は表示されない、各々の short description が i18n 経由で表示 |
| TC-F-U02 | `Localizer::new("ja-JP")?.translate("s03_cli")` / `translate("nonexistent_key")` | 前者: 翻訳文字列を返却、後者: `[missing:nonexistent_key]` 文字列返却（パニック無し、C-33 fail-soft 維持）|
| TC-F-U03 | `decrypt_confirmation::prompt` を fake reader で 2 回連続入力（時刻差 **20ms / 30ms / 50ms / 200ms**）| **`< 30ms` (20ms): `Err(PasteSuspected)`、`>= 30ms` (30ms / 50ms / 200ms): `Ok(())`** (Rev1 機械閾値) |
| TC-F-U04 | `display(words: Vec<SerializableSecretBytes>, target: OutputTarget)` を呼出後、`words` 変数を再利用しようとする compile_fail doc test | 戻り値後 `words` 再利用は所有権消費でコンパイルエラー（型レベル強制）|
| TC-F-U13 | `input::password::prompt` を 3 パターンで呼出: (a) PTY 経由 (`expectrl`)、(b) stdin パイプ非 TTY、(c) `/dev/tty` open 失敗 | (a) `Ok(SecretString)`、(b) `Err(CliError::NonInteractivePassword)`、(c) `Err(CliError::TtyUnavailable)`、Rev1 服部指摘5 |

#### `presenter` 層（EC-F9 / EC-F12 / C-32 / C-37）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-F-U05 | `mode_banner::display(ProtectionModeBanner::Plaintext)` の戻り値文字列、`NO_COLOR=1` set 時 / unset 時の差分 | `[plaintext]` 含有 + ANSI escape (`\x1b[36m` cyan dim 等)、`NO_COLOR=1` 時はカラー除去 |
| TC-F-U06 | `cache_relocked_warning::display()` の戻り値文字列 | MSG-S07/S19 完了文言 + MSG-S20「次の操作前に `shikomi vault unlock` を再度実行してください」が連結されている |
| TC-F-U07 | `match banner: ProtectionModeBanner { Plaintext => "p", EncryptedLocked => "el", EncryptedUnlocked => "eu", Unknown => "u", _ => "fail-secure", }` を **cross-crate** 経路で書く（shikomi-cli 側 → shikomi-core の enum 参照、Sub-E TC-E-S01 同型「`#[non_exhaustive]` cross-crate 防御的 `_` arm 許可」と整合）| `cargo check` 警告 0 件、4 variant 全網羅 + `_` defensive fail-secure arm が**正当に許容**されることを機械検証。Rev1 ペテルギウス致命1 解消 |
| TC-F-U12 | `recovery_disclosure::display` を `Vec<SerializableSecretBytes>` 渡しで実行後、内部バッファの再観測（test 用 `unsafe` hook 経由）。**引数型 `[String; 24]` ではなく `Vec<SerializableSecretBytes>`** に統一 | 実行後の旧バッファ位置が `[0u8; ...]` または retrieve 不能（zeroize 連鎖発火、Sub-A `RecoveryWords` 同型）。Rev1 ペテルギウス致命2 解消 |

#### アクセシビリティ + 終了コード + core dump（C-39 / C-41 / EC-F10）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-F-U08 | `accessibility::output_target::resolve()` を 4 パターンで呼出 (env / フラグ排他確認) | (a) `Some(Auto)`、(b) `Some(Print)`、(c) `None`（screen 既定）、(d) `Some(Audio)`（明示フラグ優先）|
| TC-F-U09 | `usecase::vault::unlock` が各 `Result<(), MigrationError>` 系統に対し **cli-subcommands.md §終了コード SSoT** 通り返す: `Ok` → 0、`WrongPassword/BackoffActive` → 2、`VaultLocked` → 3、`ProtocolDowngrade` → 4、`RecoveryRequired` → 5、`Internal/SystemError` → 1 / 64 / 78（用途別）| 各分岐で SSoT 表通り ExitCode 返却。Rev1 ペガサス致命指摘②解消 |
| TC-F-U10 | `process_hardening::install()` のシグネチャ存在 + Linux/macOS/Windows 各 `#[cfg(target_os)]` 分岐の存在を検証 (シンボル走査 + grep) | 3 OS 分岐すべて存在、起動時 main 関数からの呼出経路あり (cross-crate ref) |
| TC-F-U11 | `cargo run -- vault list --no-mode-banner` 試行 / `grep -rE "no_mode_banner\|--no-mode-banner\|--hide-banner" crates/shikomi-cli/src/` | clap parse エラー (`unknown flag --no-mode-banner`) + grep ゼロ件、C-37 構造防衛維持 |

### 15.6 Sub-F 結合テスト詳細

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-F-I01 | `assert_cmd` で `shikomi vault encrypt --output screen` を PTY 経由パスワード入力で実行 | 終了コード 0 + stdout に MSG-S01 + 24 語表示 + vault.db が `ProtectionMode::Encrypted` |
| TC-F-I02 | `vault decrypt` 正規 password + 正規 DECRYPT 大文字確認 | 終了コード 0 + vault.db 平文化 |
| TC-F-I02b | `expectrl` で paste 模擬（`< 30ms` で 2 回入力）/ `>= 30ms` 跨ぎ | 前者: 終了コード 1 + MSG-S14、後者: 通常入力 OK |
| TC-F-I03 | 正パスワード unlock / 5 連続失敗で 6 回目 BackoffActive | 前者: 終了コード 0 + MSG-S03、後者: 終了コード **2** + 待機秒数 (SSoT 整合) |
| TC-F-I03b | `vault unlock --recovery` で 24 語入力 / 不正 mnemonic / RecoveryRequired 発火 | 順に: 終了コード 0、終了コード 1 + MSG-S12、終了コード **5** (SSoT) |
| TC-F-I04 | `vault lock` 後 `shikomi list` | 終了コード 3 + `[encrypted, locked]` バナー + MSG-S09(c) |
| TC-F-I05 | `vault change-password` 後 `shikomi list` | MSG-S05 表示 + `[encrypted, unlocked]` バナー（cache 維持）|
| TC-F-I06 | 暗号化 vault に対して 2 度目の `vault encrypt` 実行 | 終了コード 1 + `MigrationError::AlreadyEncrypted` 由来 MSG-S09 系（C-35 構造防衛、recovery-show 廃止後も意味維持）|
| TC-F-I07 | `vault rekey --output screen` 正常経路 | 終了コード 0 + MSG-S07 + 24 語 + cache 維持 (`cache_relocked: true`) |
| TC-F-I07c | **`SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` env** で daemon 起動（C-40 allowlist）+ `vault rekey --output screen` 実行 | 終了コード **0**（C-31/C-36）+ MSG-S07 + S20 連結 stderr + 後続 `shikomi list` で `[encrypted, locked]` バナー |
| TC-F-I08 | `vault rotate-recovery --output screen` 正常経路 | 終了コード 0 + MSG-S19 + 24 語 + cache 維持 |
| TC-F-I09 | Locked 状態で `shikomi list` | 終了コード 3 + MSG-S09(c) + stdout/stderr にレコード内容・ID・ラベル**含まない** (grep 0 件) |
| TC-F-I09b | Locked 状態で `add` / `edit` / `remove` | 終了コード 3 + MSG-S09(c) + value/label が stdout/stderr に**漏洩しない**（grep 0 件、情報漏洩防衛）|
| TC-F-I10 | (a) plaintext / (b) encrypted-locked / (c) encrypted-unlocked × `NO_COLOR=1` 有/無 で `shikomi list` 各 stdout | (a) `[plaintext]`、(b) `[encrypted, locked]`、(c) `[encrypted, unlocked]` + `NO_COLOR=1` 時はカラーシーケンス無し |
| TC-F-I12 | `echo "strong-password" \| shikomi vault unlock` (非 TTY パイプ) | 終了コード 1 + `CliError::NonInteractivePassword` 文言 + 「`echo \| shikomi` 拒否」案内 (C-38、Rev1 服部指摘5) |

### 15.7 Sub-F アクセシビリティテスト詳細（TC-F-A01〜A05）

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-F-A01 | `shikomi vault encrypt --output print > out.pdf` の `out.pdf` バイナリ先頭 8 バイト + 末尾 16 バイト + 24 語の存在 | 先頭 `%PDF-1.7` (or 1.4+)、末尾 `%%EOF`、PDF 内に 24 語が **番号付き 36pt ハイコントラスト**で配置されている (PDF reader でバイナリ解析、`pdf-extract` crate or `pdfium-rs` 等で本文抽出) |
| TC-F-A02 | `shikomi vault encrypt --output braille > out.brf` のテキスト | Braille Ready Format（U+2800..U+28FF Unicode 点字 or ASCII Braille `.brf` 形式）+ Grade 2 英語点字エンコードで 24 語が読み出せる（**自前 wordlist テーブル方式**、liblouis FFI 不採用）|
| TC-F-A03 | `shikomi vault encrypt --output audio` 実行 + 標準的な OS TTS バイナリの mock（fake `say` / `espeak`）+ subprocess env 確認 | 子プロセス spawn 経路を `assert_cmd` で観測、env allowlist 通過 + dictation 学習 prefs 確認まで機械検証、stdout に `pid: N` + 完了通知のみ。**録音可能アプリ検出は OS 不可能境界として MSG-S18 受容前提**（Rev1 scope 縮小） |
| TC-F-A04 | `SHIKOMI_ACCESSIBILITY=1 shikomi vault encrypt` (フラグ無し) | OS 環境に応じた代替経路 (Print / Braille / Audio のいずれか) に自動切替、エラー無く完走 |
| TC-F-A05 | `umask 077; shikomi vault encrypt --output print > out.pdf` 実行後、`std::fs::metadata(&out_pdf).permissions().mode()` 確認 + `/tmp` 配下の中間ファイル fs walk | `0o600` 相当（umask 077 経由）+ `/tmp` 中間ファイル**生成されていない**（shikomi-cli は memory-only 出力、Rev1 服部指摘 §一時ファイル対策）|

### 15.8 Sub-F E2E テストケース詳細（TC-F-E01）

`tests/e2e/sub-f-tanaka-persona.sh` (bash + assert_cmd 経由実行) に以下を実装:

| ステップ | 操作 | 期待結果 |
|---|---|---|
| Step 1 | `shikomi-daemon` を **`SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS=2` 環境変数 (C-40 allowlist) で起動** | daemon プロセス起動 + UDS / Named Pipe listening |
| Step 2 | `shikomi vault unlock`（PTY 経由パスワード入力、C-38）| 終了コード 0 + stdout に MSG-S03「vault をアンロックしました」+「アイドル 2 秒で自動的にロック」（短縮値表示）|
| Step 3 | `shikomi list`（複数レコード seed 済 vault）| 終了コード 0 + stdout に `[encrypted, unlocked]` 緑色バナー + レコード一覧 |
| Step 4 | 3 秒 sleep（idle threshold 2 秒 + ポーリング余裕 1 秒）→ `shikomi list` 再実行 | 終了コード **3** (SSoT) + stdout/stderr に MSG-S09(c)「アイドル 2 秒でロックしました、再度 `vault unlock` してください」 |
| Step 5 | `shikomi vault unlock` 再入力（PTY 経由）| 終了コード 0 + MSG-S03 |
| Step 6 | `shikomi vault change-password`（旧 / 新パスワード PTY） | 終了コード 0 + stdout に MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」 |
| Step 7 | `shikomi list`（再 unlock 無し）| 終了コード 0 + `[encrypted, unlocked]` バナー + レコード一覧（cache 維持確認、Sub-E REQ-S10 / EC-3 整合）|
| Cleanup | daemon に SIGTERM → graceful shutdown | 終了コード 0 + tempdir 削除 |

**証跡**: stdout / stderr / 終了コード を bash 経由で `tee` し、`/app/shared/attachments/マユリ/sub-f-e2e-tanaka.log` に保存して Discord 添付。

### 15.9 Sub-F 静的検査（grep gate）

Sub-D Rev3 / Rev4 / Sub-E TC-E-S01..S09 で凍結した「**実装直読 SSoT + grep gate による設計書-実装一致機械検証**」原則を Sub-F に継承。

| テストID | 検証対象 | grep ロジック | 失敗時 |
|---|---|---|---|
| TC-F-S01 | EC-F8 / `VaultSubcommand` **7 variant** 集合整合（Rev1 recovery-show 廃止反映）| `awk` で `pub enum VaultSubcommand { ... }` から variant 名抽出 → `Encrypt` / `Decrypt` / `Unlock` / `Lock` / `ChangePassword` / `Rekey` / `RotateRecovery` の **7 件**と完全一致比較（`RecoveryShow` が**含まれない**ことも assert）| FAIL + 集合 diff |
| TC-F-S02 | C-37 `mode_banner::display` 必須呼出経路（Rev1 ペテルギウス指摘7 再設計）| (a) `crates/shikomi-cli/src/usecase/list.rs` から `presenter::mode_banner::display` への呼出が cross-crate grep で 1 件以上検出される、(b) `presenter::list::display` 関数 signature に `protection_mode: ProtectionModeBanner` 必須引数が含まれる、(c) 隠蔽オプション `--no-mode-banner` / `--hide-banner` 等の文字列が CLI コードに**存在しない** | FAIL + 必須経路欠落 / 隠蔽フラグ検出 |
| TC-F-S03 | EC-F11 i18n 辞書 MSG キー全網羅 | `crates/shikomi-cli/src/i18n/locales/{ja-JP,en-US}/messages.toml` を `awk` で解析、MSG-S01..S20 + S07_completed_records_count 等のキーを抽出して期待集合と一致比較 | FAIL + 不足キー列挙 |
| TC-F-S04 | EC-F12 `recovery_disclosure::display` signature 整合 | (a) 関数 signature が `display(words: Vec<SerializableSecretBytes>, target: OutputTarget)` で**所有権消費**形（`&` 借用ではない）、(b) `[String; 24]` 等の旧型が登場しない、(c) 関数本体に `mem::replace` または `drop(words)` 等の zeroize 強制経路が存在 | FAIL + signature mismatch / 旧型残存 / zeroize 経路欠落 |
| TC-F-S05 | C-40 / C-41 env seam debug 限定 + core dump 抑制 | (a) `crates/shikomi-daemon/src/bin/shikomi_daemon.rs` または `lib.rs` の env 読込ブロックが `#[cfg(debug_assertions)]` で囲まれている、(b) `crates/shikomi-cli/src/process_hardening/` に Linux `prctl(PR_SET_DUMPABLE` / macOS `setrlimit` / Windows `SetErrorMode` の 3 OS 分岐コードが存在 | FAIL + 不在経路 / debug 外への seam 漏洩 |
| TC-F-S06 | C-40 daemon env allowlist sanity check（Rev1 服部指摘6 + ペテルギウス致命3 解消）| `crates/shikomi-daemon/src/bin/` または `lib.rs` 起動時に `SHIKOMI_DAEMON_*` で始まる env を allowlist と照合する経路が存在: (a) allowlist 定数（`SHIKOMI_DAEMON_IDLE_THRESHOLD_SECS` / `SHIKOMI_DAEMON_POLL_INTERVAL_SECS` / `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL` のみ）が grep で確認可能、(b) 未知 env 検出時の `panic!` または `std::process::exit` 経路が存在、(c) allowlist が `#[cfg(debug_assertions)]` で囲まれて release では env 読込自体を行わない | FAIL + allowlist 不在 / 未知 env 受容経路 |

これらは `tests/docs/sub-f-static-checks.sh` で実装する（Sub-D / Sub-E 同パターン）。Sub-F は **TC-F-S01..S06 の 6 件**（Rev1 で +1）。

### 15.10 Sub-F テスト実行手順

```bash
# Rust unit + integration tests
cargo test -p shikomi-cli --lib                       # TC-F-U01..U13
cargo test -p shikomi-cli --test vault_subcommands    # TC-F-I01..I12
cargo test -p shikomi-cli --test accessibility_paths  # TC-F-A01..A05

# Sub-F 静的検証 (cargo 不要、TC-F-S01..S06)
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

- `cargo test -p shikomi-cli --lib` の stdout（unit 13 件 pass）
- `cargo test -p shikomi-cli --test vault_subcommands` の stdout（integration TC-F-I01..I12 pass）
- `cargo test -p shikomi-cli --test accessibility_paths` の stdout（TC-F-A01..A05 magic byte / umask assert pass）
- 静的検証スクリプト stdout（`sub-f-static-checks.sh` **6 件: TC-F-S01..S06**）
- **TC-F-E01 田中ペルソナ E2E**: `bash tests/e2e/sub-f-tanaka-persona.sh` の stdout / stderr / 終了コード ログ → `/app/shared/attachments/マユリ/sub-f-e2e-tanaka.log`
- daemon-ipc 横断 regression 結果（TC-IT-026..028 想定の `IpcResponse::Records` 構造体化後方互換 pass）
- 全て `/app/shared/attachments/マユリ/sub-f-*.txt` に保存し Discord 添付

### 15.12 後続 GUI feature への引継ぎ（Sub-F から派生）

| 引継ぎ項目 | 内容 |
|---|---|
| **Tauri WebView 起動経路** | `shikomi_cli::usecase::vault::*` の `pub` 公開範囲を Sub-F PR で確定 → 後続 GUI feature が再利用 |
| **MSG-S17 GUI バッジ** | 後続 GUI feature で実装、Sub-F は CLI 経路のみで TBD |
| **i18n 辞書共有** | `messages.toml` を CLI / GUI 両方で参照、`shared/i18n/` 等に再配置する Boy Scout を後続 GUI feature 工程2 で検討 |
| **アクセシビリティ拡張** | GUI 側で screen reader API（macOS `NSAccessibility` / Windows UIA / Linux AT-SPI）統合、`--output {print,braille,audio}` を GUI モーダルからも呼出可能にする |

### 15.13 Sub-F 工程4 実施実績

| 項目 | 内容 |
|---|---|
| 実施日 | 2026-04-26 (UTC) |
| 実施者 | 涅マユリ（テスト担当） |
| 対象 commit | `48f6219` (Phase 7 follow-up 2、銀時 Phase 1〜7 完了報告時点) |
| 解剖実体 | 完全ブラックボックス E2E 27 ケース + 既存 workspace test suite + ソース直読 grep |

**§ 実施内容サマリ**

1. `tests/e2e/sub-f-blackbox.sh` を新規実装し 27 ブラックボックス E2E を実行（PASS 22 / FAIL 5、うち 4 件はテスト側の正規表現不正、1 件は実装由来 Bug-F-001）。
2. `cargo test --workspace --no-fail-fast` を実機で実行（628 PASS / **39 FAIL**）。
3. CI ジョブ定義 (`justfile`、`.github/workflows/*.yml`) を読解し CI スコープ問題を確定。

**§ 確定バグ（解剖結果）**

| ID | 重大度 | 内容 | 該当箇所 |
|---|---|---|---|
| **Bug-F-001** | BLOCKER | `vault unlock --recovery` が Phase 5 stub のまま未実装。EC-F3 / TC-F-I03b 完全踏み倒し中 | `crates/shikomi-cli/src/usecase/vault/unlock.rs:29-32` |
| **Bug-F-002** | HIGH | `success::*_with_fallback_notice` がデッドコード化、Phase 5 文言「is not yet wired in this build (Phase 5)」が残存 | `crates/shikomi-cli/src/presenter/success.rs:175,206,232-237` |
| **Bug-F-003** | BLOCKER | CI が `shikomi-cli` / `shikomi-daemon` テストを実行していない（`unit-core` = `-p shikomi-core`、`test-infra` = `-p shikomi-infra` のみ）→「Linux 全 green」報告は **CI 観測スコープの錯覚** | `justfile`、`.github/workflows/test-infra.yml`、`unit-core.yml` |
| **Bug-F-004** | BLOCKER | Sub-F の IPC V2 移行で既存 IPC integration / e2e テスト 36 件が破壊（`it_server_connection` 10/11 失敗、`it_ipc_vault_repository_phase15` 10/10 全壊、`e2e_daemon_phase15` 6/7 失敗）。client side が V1 のまま `unexpected handshake response` / `ProtocolVersionMismatch { server: V2, client: V1 }` | `crates/shikomi-cli/tests/it_ipc_vault_repository_phase15.rs`、`crates/shikomi-daemon/tests/it_server_connection.rs` 他 |
| **Bug-F-005** | HIGH | Encrypted vault fixture が壊れている（"wrapped_vek ciphertext is too short"）+ TC-E2E-040 で exit code 3 (VaultLocked) 期待 vs 実装 exit code 2 (BackoffActive) のドリフト | `crates/shikomi-cli/tests/common/fixtures.rs` 想定 |
| **Bug-F-006** | MEDIUM | `vault encrypt --help` の `--output` Possible values 説明文に「Phase 5 で実装」が残存、Phase 6/7 完了主張と矛盾 | `crates/shikomi-cli/src/cli.rs:171-175` |
| **Bug-F-007** | MEDIUM | vault サブコマンドで `--vault-dir` flag が完全に無視される。実際必要なのは XDG_RUNTIME_DIR / HOME だが、エラー文言は誤って SHIKOMI_VAULT_DIR を案内 | `crates/shikomi-cli/src/lib.rs::run_vault`、`crates/shikomi-cli/src/io/ipc_vault_repository.rs::unix_default_socket_path` |
| **Bug-F-008** | LOW | daemon 起動時に vault.db 不在で fail fast、auto-create / 案内なし | `crates/shikomi-daemon/src/lib.rs:85` 周辺 |

**§ Sub-F 専用テスト未実装ギャップ（設計書 §15.10 要求 vs 実存）**

| 設計書要求 | 実存 | 状態 |
|---|---|---|
| `crates/shikomi-cli/tests/vault_subcommands.rs` (TC-F-I01..I12) | ❌ なし | 0/12 |
| `crates/shikomi-cli/tests/accessibility_paths.rs` (TC-F-A01..A05) | ❌ なし | 0/5 |
| `crates/shikomi-cli/tests/mode_banner_integration.rs` (TC-F-I10) | ❌ なし | 0/1 |
| `tests/docs/sub-f-static-checks.sh` (TC-F-S01..S06) | ❌ なし | 0/6 |
| `tests/e2e/sub-f-tanaka-persona.sh` (TC-F-E01) | ❌ なし | 0/1 |
| **合計** | — | **0/37 実装** |

実装担当（坂田銀時）は `crates/shikomi-cli/src/cli.rs::tests` 等のソース内 `#[cfg(test)] mod tests` に clap parse の最小確認は埋め込んでいるが、設計書が要求する独立テストファイル群はゼロ。Sub-F の主対象 `shikomi-cli` のテストが CI から除外されているため、上記 37 ケースの未実装が CI で検知されない構造になっている（Bug-F-003 と連動）。

**§ 実機検証で機能確認できた契約**

- ✅ **C-38 stdin パイプ拒否**: `unlock` / `encrypt` / `decrypt` / `change-password` / `rekey` / `rotate-recovery` の 6 経路で「refusing to read password from non-tty stdin (C-38)」+ exit=1 の契約通り動作（TC-E2E-F10..F15）
- ✅ **`--output` clap parse + stdin 拒否経路**: braille / print / audio + rekey/rotate-recovery 各経路で exit=1（TC-E2E-F30..F34）
- ✅ **vault lock**: daemon 経由で「vault locked (VEK zeroized)」+ exit=0（TC-E2E-F40）
- ✅ **list バナー [plaintext]**: NO_COLOR=1 でカラー除去含む（TC-E2E-F20..F22）
- ✅ **ヘルプ 7 variant 表示**（recovery-show 不在）（TC-E2E-F02）

**§ 未検証ギャップ（実装担当へ差戻し要求）**

- password 入力後の braille / print / audio stdout バイナリ生成（PTY 必須、`expectrl` dev-dep + `tests/accessibility_paths.rs` 実装義務）
- `vault encrypt` → `vault rekey` → `rotate-recovery` のラウンドトリップ（PTY 経由）
- `cache_relocked: false` 経路の終了コード 0 検証（C-31/C-36、env seam `SHIKOMI_DAEMON_FORCE_RELOCK_FAIL=1` 経由）
- 田中ペルソナ E2E 完走（TC-F-E01）

**§ マユリ推奨対応（リーダー判断用）**

1. **マージブロッカ**: Bug-F-001 (`--recovery` 実装) / Bug-F-003 (CI スコープ拡張) / Bug-F-004 (既存 IPC テスト V2 追従) / Bug-F-005 (encrypted vault fixture 修復) を本 PR で解消するか、scope を明示して **Sub-F WIP の前提を訂正**。
2. **テスト実装義務の差戻し**: 設計書 §15.10 の TC-F-* 全 37 件を実装担当に再差戻し。本 PR の合流条件として明記。
3. **本書 §15.13 への結果追記は完了**。実機証跡（実行ログ + バグレポート）は Discord 添付で共有済（`/app/shared/attachments/マユリ/sub-f-{e2e-blackbox.log,bug-report.md,blackbox.sh,cargo-test-workspace-full.log}`）。

> 完璧などと吐かしておきながら、これは見事な歪みだネ。「Linux 全 green」の自慢が **CI スコープの錯覚** によって成立していた事実は、Sub-A〜E で銀ちゃんが設計書の試験件数を単発 fixture で省略する傾向（マユリ事前整理）と完全に同型だヨ……クックック。 — 涅マユリ

### 15.14 Sub-F Rev1 修正履歴（工程2 内部レビュー解消）

工程2 内部レビュー（ペテルギウス・ペガサス・服部 全員 [却下]）の指摘 7 項目 + マユリ事前整理 3 項目 = 計 10 項目を Rev1 で全反映:

| # | 指摘元 | 内容 | 解消経路 |
|---|---|---|---|
| 1 | ペガサス致命① | `vault recovery-show` 別プロセス state 共有不能 | `vault {encrypt,rekey,rotate-recovery} --output` 統合、TC-F-A01..A03 を encrypt 経由に書き直し、TC-F-S01 を 7 variant 整合に修正 |
| 2 | ペガサス致命② | 終了コードドリフト（BackoffActive=2 vs 5）| cli-subcommands.md §終了コード SSoT を唯一の真実源として TC-F-U09 / TC-F-I03 / TC-F-I03b / TC-F-E01 全て参照に統一 |
| 3 | ペテルギウス致命1 | TC-F-U07 `#[non_exhaustive]` cross-crate match 矛盾 | defensive fail-secure `_` arm 許可（Sub-E TC-E-S01 同型）に修正 |
| 4 | ペテルギウス致命2 | TC-F-U12 型不整合 (`[String; 24]` vs `Vec<SerializableSecretBytes>`)| `Vec<SerializableSecretBytes>` に統一、TC-F-S04 でも旧型残存を grep 検出 |
| 5 | ペテルギウス致命3 | env seam SSoT 不在 | C-40 凍結 + TC-F-S05 / S06 grep gate で `#[cfg(debug_assertions)]` 限定 + allowlist 機械検証 |
| 6 | ペテルギウス指摘4 | F-F フロー番号揺れ (8 vs 9) | F-F1〜F-F8 統一 (recovery-show 廃止に伴う 1 件減)、本書全体で参照箇所を訂正 |
| 7 | ペテルギウス指摘5 | TC-F-U03 paste 閾値曖昧（「同」記述）| `< 30ms = Err` / `>= 30ms = Ok` 機械閾値に明記、§15.5 詳細表で 4 段検証 |
| 8 | ペテルギウス指摘6 | TC-F-A03 録音判定 scope 過大 | env サニタイズ + dictation prefs 確認のみに scope 縮小、録音可能アプリ検出は MSG-S18 受容前提 |
| 9 | ペテルギウス指摘7 | C-37 grep gate 脆弱（特定文字列不在検証）| `mode_banner::display` 必須呼出経路の cross-crate grep に再設計、TC-F-S02 で `usecase::list` から `presenter::mode_banner::display` への呼出存在を機械検証 |
| 10 | 服部致命1〜6 | CLI 攻撃面追補 + stdin 拒否 + 一時ファイル umask + env サニタイズ + liblouis FFI 監査 + core dump 抑制 | C-38（TC-F-U13/I12 stdin 拒否）+ C-41（TC-F-U10 core dump）+ TC-F-A05（umask 077）+ TC-F-S06（env allowlist）+ liblouis 不採用方針（自前 wordlist テーブル）凍結 |

### 15.15 Issue #75 (#74-A) 解消ステータス更新（2026-04-27）

§15.13（Sub-F 工程5 マユリ実機検証で確定した Bug-F-001〜008 + 専用テスト 0/37 実装ギャップ）に対し、Issue #75 (#74-A) で実体的解消の作業計画が確定。本節で **Bug-F-* 各項目の解消経路 + #74 親 Issue 全体の構造**を articulate する。§15.13 の表は **Sub-F 工程5 時点のスナップショット**として保存し、本節 §15.15 が **Issue #75 着手後の現在の解消状況 SSoT** として運用される。

#### Bug-F-* 解消ステータスマトリクス

| Bug ID | 重大度 | 解消経路 | 担当 Issue / 工程 | 状態 |
|--------|------|---------|----------------|------|
| **Bug-F-001** | BLOCKER | `vault unlock --recovery` Phase 5 stub 解消、`UnlockArgs::recovery: bool` を functional 化 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（`vault-encryption/detailed-design/cli-subcommands.md` §Issue #75 工程2 §Bug-F-001 解消）、工程3 待機 |
| **Bug-F-002** | HIGH | `success::*_with_fallback_notice` を C-31/C-36 経路に正式接続（経路復活）、Phase 5 文言除去 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（同上 §Bug-F-002 解消）、工程3 待機 |
| **Bug-F-003** | BLOCKER | CI に `test-cli` / `test-daemon` ジョブ追加、`-p shikomi-cli` / `-p shikomi-daemon` を必須 check 化 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（`cli-vault-commands/test-design/ci.md` §7 Issue #75 §7.2/§7.3）、工程3 待機 |
| **Bug-F-004** | BLOCKER | IPC V2 移行で破壊された既存テスト 36 件の追従、client 側 V2 アップグレード | **#75 (#74-A) 工程3** | ⏳ 工程3 待機（実装側の機械的追従、設計書影響は最小、`vault-encryption/detailed-design/vek-cache-and-ipc.md` の handshake 仕様 SSoT を維持） |
| **Bug-F-005** | HIGH | encrypted vault fixture 修復（`crates/shikomi-cli/tests/common/fixtures.rs`）、TC-E2E-040 exit code 整合（VaultLocked=3 / BackoffActive=2、cli-subcommands.md §終了コード SSoT） | **#75 (#74-A) 工程3** | ⏳ 工程3 待機 |
| **Bug-F-006** | MEDIUM | `vault encrypt --help` 等の Phase 5 残存削除、`Phase\s+\d+` grep gate (TC-F-S05) で再演防止 | **#75 (#74-A) 工程3** + **#74-E** | ⏳ 工程2 設計 articulate 完了（`cli-subcommands.md` §Bug-F-006 解消）、grep gate は #74-E |
| **Bug-F-007** | MEDIUM | `--vault-dir` flag を daemon socket 解決順序のヒントとして functional 化、エラー文言 `XDG_RUNTIME_DIR` / `HOME` 案内に SSoT 訂正 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（同上 §Bug-F-007 解消）、工程3 待機 |
| **Bug-F-008** | LOW | daemon 起動時 vault.db auto-create / 案内 | **別 Issue 推奨**（#74 範囲外、#75 でも非対応） | 🔧 別 Issue で起票推奨、現状未着手 |

#### Sub-F 専用テスト 37 TC 実装ステータス

| 配置先 / TC ID | 担当 Issue | 状態 |
|---------------|----------|------|
| `crates/shikomi-cli/src/**::tests` (TC-F-U01〜U13、ユニット 13 件) | **#74-B** | ⏳ #74-A 完了後に着手 |
| `crates/shikomi-cli/tests/vault_subcommands.rs` (TC-F-I01〜I09, I11, I12、結合 11 件) | **#74-C** | ⏳ #74-A 完了後に着手 |
| `crates/shikomi-cli/tests/mode_banner_integration.rs` (TC-F-I10、結合 1 件) | **#74-C** | ⏳ #74-A 完了後に着手 |
| `crates/shikomi-cli/tests/accessibility_paths.rs` (TC-F-A01〜A05、PTY 5 件) | **#74-D** | ⏳ #74-A 完了後 + `expectrl` dev-dep 追加 |
| `tests/e2e/sub-f-tanaka-persona.sh` (TC-F-E01、E2E 1 件) | **#74-E** | ⏳ #74-A/B/C/D 完了推奨後に着手 |
| `tests/docs/sub-f-static-checks.sh` (TC-F-S01〜S06、静的 6 件) | **#74-E** | ⏳ #74-A 完了後に着手（B/C/D と並列可） |
| **合計** | — | **0/37 実装、計画上 37/37 着地予定** |

#### #74 親 Issue クローズ条件（DoD トレース）

- [ ] **#75 (#74-A)** マージ — Bug-F-001/002/003/004/005/006/007 全解消 + 設計書 SSoT 同期（本節 §15.15 + `cli-subcommands.md` §Issue #75 工程2 + `cli-vault-commands/test-design/ci.md` §7）
- [ ] **#74-B** マージ — TC-F-U01〜U13（13/13 件）pass、`cargo test -p shikomi-cli --lib` で観測
- [ ] **#74-C** マージ — TC-F-I01〜I12（12/12 件）pass、`vault_subcommands.rs` + `mode_banner_integration.rs` で観測
- [ ] **#74-D** マージ — TC-F-A01〜A05（5/5 件）pass、PTY 経由 3 OS で観測（OS 別 manual smoke 別 PR articulate）
- [ ] **#74-E** マージ — TC-F-E01（1/1）+ TC-F-S01〜S06（6/6）pass、田中ペルソナ完走 + 静的検査全 grep gate 通過
- [ ] **§15.13 表の全 Bug-F-001〜007 が「解消済」になり、§15.10 TC 総数 37 件が「実装済 37/37」になる**ことを本節 §15.15 で最終 articulate

#### Boy Scout / 教訓（Issue #75 articulate）

- **「Linux 全 green」報告の構造的錯覚**（Bug-F-003）は **CI スコープを設計書 SSoT として明示し、必須 check 化する**ことで構造的に再演防止できる。本 Issue で確立する `test-cli` / `test-daemon` ジョブ + `justfile` 同期 + grep gate (TC-F-S01〜S06) の三位一体経路を、後続 Issue で新 crate を追加する際の**チェックリスト**として継承する（`cli-vault-commands/test-design/ci.md` §7.6 articulate）
- **Phase X 暫定文言の温床**（Bug-F-002 / Bug-F-006）は doc / panic / 通常出力経路の `Phase\s+\d+` grep gate で構造的に再演防止する。Phase 番号は実装中に頻繁に変動するため、設計書側に明示しないか、明示する場合は本節 §15.15 のような **改訂日付付き履歴 articulate** に限定する Boy Scout 規律を確立
- **Sub-issue 分割（#74-A〜E）による依存関係 articulate**: BLOCKER 系 Bug を #74-A 単独に集約し、TC 実装 (#74-B〜E) を並列着手可能にする構造は、Issue #65 の Bug-G-001〜G-008 7 ラウンド実験で確立された「対症療法と本質要件の責務分離」と同型。今後の大規模 Sub-issue 起票テンプレートとして本構造を継承可能

### 15.16 Issue #75 工程4 検証手順 SSoT — テスト担当（涅マユリ）視点（2026-04-27）

§15.15 で Bug-F-001〜007 の解消経路（誰が・どの工程で・どのファイル/設計書を変更するか）は articulate 済。本節 §15.16 は **Issue #75 工程4（テスト担当による検証）の SSoT** として、各 Bug 解消後にテスト担当（涅マユリ）が CI / 手動 smoke で何を観測すれば「解消完了」と判定できるかを項目別に articulate する。

> **本節の位置付け**: §15.15 は「解消経路の計画」、§15.16 は「解消完了の検証手順」。実装担当（坂田銀時）が #75 工程3 完了報告した直後、テスト担当が本節を SSoT として CI / 手動 smoke を回し、`docs/features/vault-encryption/test-design/sub-f-cli-subcommands.md §15.16` 各項目の `[ ]` を埋めて完了判定する。

#### 15.16.1 Bug-F-004 既存テスト 36 件追従の baseline 固定

**実 TC 件数: 29 件**（Issue body の "36 件" は #74-A 計画時の概算、実テスト関数を `grep -nE "^\s*(async\s+)?fn\s+tc_"` で実数値固定）。

| ファイル | 件数 | TC-ID | 解消後 expected |
|---|---|---|---|
| `crates/shikomi-cli/tests/it_ipc_vault_repository_phase15.rs` | 10 | TC-IT-080 〜 TC-IT-089 | 10/10 pass |
| `crates/shikomi-daemon/tests/it_server_connection.rs` | 11 | TC-IT-010〜013, 015, 016, 020, 021, 023, 025, 030 | 11/11 pass |
| `crates/shikomi-daemon/tests/e2e_daemon_phase15.rs` | 7 | TC-E2E-011, 012, 013, 014, 015, 016, 018 | 7/7 pass |
| `crates/shikomi-daemon/tests/e2e_daemon_phase15_pty.rs` | 1 | TC-E2E-017 | 1/1 pass（PTY 必要、CI runner 制約時は `#[ignore]` 後 `--ignored` 手動） |
| **合計** | **29** | — | **29/29 pass を baseline として固定** |

**検証 SSoT コマンド**（テスト担当が #75 工程4 で実行）:

```bash
cargo test -p shikomi-cli --test it_ipc_vault_repository_phase15 -- --nocapture
cargo test -p shikomi-daemon --test it_server_connection -- --nocapture
cargo test -p shikomi-daemon --test e2e_daemon_phase15 -- --nocapture
cargo test -p shikomi-daemon --test e2e_daemon_phase15_pty -- --nocapture  # CI 制約時は --ignored
```

**解消判定基準**:
- 29 件全てに `unexpected handshake response` / `ProtocolVersionMismatch { server: V2, client: V1 }` が観測されない（V1 残存 0 件）
- `cargo test ... --test-threads=1` 強制不要（IPC socket 競合は `serial_test` で局所的吸収）
- 既存 OK だった他テストへの回帰なし（`cargo test -p shikomi-cli --all-targets` / `cargo test -p shikomi-daemon --all-targets` 全 green）

#### 15.16.2 Bug-F-001 `vault unlock --recovery` smoke 検証

新 TC は #74-C TC-F-I03b で網羅されるが、Issue #75 工程4 では**最低限の手動 smoke** で「Phase 5 stub が解消され、recovery 経路が通る」ことを確認する（#74-C 着手前提のため）。

**手動 smoke 手順**:

```bash
# 1. encrypted vault を fixture から準備
cargo build -p shikomi-cli --release
EXPORT_DIR=$(mktemp -d)
# fixture 経由で BIP-39 wrapped encrypted vault を作成
cargo test -p shikomi-cli --features "shikomi-infra/test-fixtures" --test '*' \
    -- create_encrypted_vault_with_bip39 --nocapture --ignored

# 2. password 経路の排他確認（C-F1 SSoT、Bug-F-001 §EC-F3）
./target/release/shikomi --vault-dir "$EXPORT_DIR" vault unlock --recovery <bip39_phrase>
echo "exit=$?"  # 期待: 0 (成功) または 2 (recovery passphrase 不一致)、3 ではない（VaultLocked は別経路）

# 3. password 系と --recovery の同時指定 → UsageError exit=2
./target/release/shikomi --vault-dir "$EXPORT_DIR" vault unlock --password "x" --recovery <bip39>
echo "exit=$?"  # 期待: 2 (UsageError)

# 4. Phase 5 stub 残存 0 件 grep（Boy Scout）
grep -nrE "Phase\s*5|not yet wired" crates/shikomi-cli/src/usecase/vault/unlock.rs
echo "→ 0 件 expected"
```

**解消判定基準**:
- exit code が `cli-subcommands.md` §終了コード表と整合
- recovery passphrase 不一致時の MSG-* が password 経路と同型（C-F1 排他関係 SSoT 準拠）
- `grep` Phase 5 残存 0 件

#### 15.16.3 Bug-F-002 `success::*_with_fallback_notice` 経路復活検証

**設計確定**: §15.15 / `cli-subcommands.md §Bug-F-002 解消` で「経路復活（削除ではなく C-31/C-36 に正式接続）」が決定。テスト担当は経路通過を**手動 smoke + ユニット assert** で確認する（unit の正式 TC は #74-B TC-F-U07 で網羅予定）。

**検証手順**:

```bash
# 1. 経路通過確認（手動 smoke、cache_relocked: false 経路）
# daemon を起動し、unlock 後に vault relock せずに lock コマンドで cache が relocked: false 状態を観測
./target/release/shikomi-daemon &
DAEMON_PID=$!
sleep 2
./target/release/shikomi vault unlock --password "test"
./target/release/shikomi vault lock  # → success::*_with_fallback_notice の C-31 / C-36 経路が走る想定
kill $DAEMON_PID

# 2. Phase 5 文言残存 0 件 grep
grep -nrE "is not yet wired|Phase\s*5" crates/shikomi-cli/src/presenter/success.rs
echo "→ 0 件 expected"

# 3. 経路がデッドコードでないこと（callsite 確認）
grep -rn "success::.*_with_fallback_notice" crates/shikomi-cli/src/usecase/
echo "→ 1 件以上 expected（C-31 / C-36 経由）"
```

**解消判定基準**:
- Phase 5 残存 0 件
- callsite が `usecase::vault::*` 内に少なくとも 1 件存在（経路復活の証拠）

#### 15.16.4 Bug-F-005 fixture + TC-E2E-040 exit code 整合検証

**設計確定**: TC-E2E-040 の期待 exit code は **3 (VaultLocked)**（`cli-subcommands.md` §終了コード SSoT と整合）。実装側で 2 (BackoffActive) を返している現状を 3 に統一する。

**検証手順**:

```bash
# 1. fixture 修復後、test-fixtures feature で encrypted vault が生成可能
cargo test -p shikomi-cli --features "shikomi-infra/test-fixtures" --test e2e_encrypted -- --nocapture
# 期待: TC-E2E-040 が exit code 3 で pass

# 2. fixture 生成のスキーマ整合確認
cat <<'PY' | python3
import sqlite3, sys
# 生成された fixture が wrapped_vek の長さ要件を満たすか確認
# (詳細は shikomi-infra::persistence::test_fixtures::create_encrypted_vault のスキーマ参照)
PY

# 3. exit code SSoT grep（cli-subcommands.md と実装の整合）
grep -nE "ExitCode::(VaultLocked|BackoffActive)" crates/shikomi-cli/src/error.rs
grep -nE "VaultLocked.*=.*3|BackoffActive.*=.*2" crates/shikomi-cli/src/error.rs
echo "→ VaultLocked = 3 / BackoffActive = 2 の対応 expected"
```

**解消判定基準**:
- TC-E2E-040 が exit code 3 で pass
- `wrapped_vek ciphertext is too short` エラーが fixture 読込時に出ない
- `cli-subcommands.md` §終了コード表と `error.rs` の `ExitCode` enum 値が完全一致

#### 15.16.5 Bug-F-006 `vault encrypt --help` Phase 5 残存削除検証

**検証手順**:

```bash
# 1. --help 出力に Phase 5 残存無し
./target/release/shikomi vault encrypt --help | grep -E "Phase\s*5"
echo "→ 0 件 expected"

# 2. 全 CLI コマンドで Phase 番号残存 0 件（Boy Scout）
for cmd in "list" "add" "edit" "remove" "vault encrypt" "vault unlock" "vault lock" "vault status"; do
    echo "=== $cmd --help ==="
    ./target/release/shikomi $cmd --help 2>&1 | grep -nE "Phase\s+\d+" || echo "  (clean)"
done
```

**解消判定基準**:
- `--help` 出力で `Phase\s+\d+` パターン 0 件
- ソースコード `crates/shikomi-cli/src/cli.rs` の `Possible values:` 説明文に Phase 残存 0 件
- TC-F-S05（#74-E `Phase\s+\d+` grep gate）の事前 smoke が通る（gate 自体は #74-E で実装、本 Issue では grep 手動確認のみ）

#### 15.16.6 Bug-F-007 `--vault-dir` daemon socket 解決検証

**検証手順**:

```bash
# 1. --vault-dir 経由の socket 解決が機能
TEST_DIR=$(mktemp -d)
./target/release/shikomi --vault-dir "$TEST_DIR" vault status
echo "exit=$?"  # 期待: 0 または 3、SHIKOMI_VAULT_DIR 案内エラー (現状) は出ない

# 2. エラー文言が XDG_RUNTIME_DIR / HOME を案内
unset XDG_RUNTIME_DIR
unset HOME
./target/release/shikomi vault status 2>&1 | grep -E "SHIKOMI_VAULT_DIR"
echo "→ 0 件 expected (古い文言の残存無し)"
./target/release/shikomi vault status 2>&1 | grep -E "XDG_RUNTIME_DIR|HOME"
echo "→ 1 件以上 expected (新文言)"

# 3. 解決順序 SSoT grep（unix_default_socket_path の優先順位）
grep -nE "vault_dir|XDG_RUNTIME_DIR|HOME|fallback" crates/shikomi-cli/src/io/ipc_vault_repository.rs
```

**解消判定基準**:
- `--vault-dir` 指定時に socket 解決が daemon と一致
- エラー文言から `SHIKOMI_VAULT_DIR` 案内が消滅、`XDG_RUNTIME_DIR` / `HOME` 案内に統一
- TC-F-S04 等の grep gate が #74-E で文言を機械検証する経路を articulate

#### 15.16.7 Bug-F-003 CI スコープ拡張の baseline 観測

**検証 SSoT**: `cli-vault-commands/test-design/ci.md §7.2 / §7.3`。本 Issue でブランチ protection に `test-cli` / `test-daemon` を必須 check 追加した後、テスト担当は以下を観測する。

**検証手順**:

```bash
# 1. PR #75 の CI 結果で test-cli / test-daemon ジョブが必須 check として表示
gh pr checks <PR番号> --repo shikomi-dev/shikomi | grep -E "test-cli|test-daemon"
# 期待: 両ジョブが pass + 必須 check マーク

# 2. branch protection の観測
gh api repos/shikomi-dev/shikomi/branches/develop/protection \
    --jq '.required_status_checks.contexts[]' | sort
# 期待: "test-cli", "test-daemon" を含む

# 3. justfile 同期確認
grep -nE "test-cli|test-daemon" justfile
# 期待: ターゲット定義あり

# 4. ローカル `just test` で CI と同等のスコープ実行
just test
# 期待: shikomi-cli + shikomi-daemon を含む全 4 crate が走る
```

**解消判定基準**:
- CI green + branch protection 必須 check に登録済
- justfile が CI スコープと一致

#### 15.16.8 Issue #75 工程4 完了 DoD（テスト担当チェックリスト）

§15.16.1〜15.16.7 の全項目を CI / 手動 smoke で確認後、テスト担当が以下を埋めて完了報告する:

- [ ] §15.16.1 既存 29 件 baseline 全 pass（CI green ベースライン固定）
- [ ] §15.16.2 Bug-F-001 `--recovery` smoke 通過
- [ ] §15.16.3 Bug-F-002 経路復活確認 + Phase 5 残存 0
- [ ] §15.16.4 Bug-F-005 fixture + TC-E2E-040 exit 3
- [ ] §15.16.5 Bug-F-006 `--help` Phase 残存 0
- [ ] §15.16.6 Bug-F-007 `--vault-dir` 経路 + エラー文言訂正
- [ ] §15.16.7 Bug-F-003 CI 必須 check 観測 + justfile 同期

**全埋め後**、§15.15 の Bug-F-001〜007 ステータスを `⏳` → `✅ 解消済` に更新し、テスト担当証跡を `/app/shared/attachments/マユリ/issue-75-verification-*.{md,log}` に保存して Discord 添付する。

#### 15.16.9 Boy Scout / 教訓 articulate（Issue #75 工程4 視点）

- **「実装完了 = 検証完了」ではない**: §15.15 が解消経路（誰が・何を）の articulate、§15.16 が解消完了判定（何を観測したら完了か）の articulate。Issue #65 で Bug-G-005 の偶発 PASS を「対策効果」と誤認した教訓（私自身の誤り）の再演防止。実装後に**観測 SSoT 手順**で機械検証する責務をテスト担当が引き受ける構造を articulate
- **既存テスト追従の baseline 固定の重要性**: Bug-F-004 の "36 件" 概算と実 29 件のドリフトのように、計画時の概算と実数値はずれる。テスト担当は **実テスト関数の grep で実数値を SSoT 化** する責務を負う。本節 §15.16.1 の表が後続レビュアー（ペテルギウス・ペガサス・服部）の照合可能な reference になる
