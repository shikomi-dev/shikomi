# 基本設計書 — UX 設計 + メッセージ仕様（`ux-and-msg`）

<!-- 親: docs/features/vault-encryption/basic-design/index.md -->
<!-- 詳細設計書（detailed-design/ ディレクトリ）とは別ファイル。統合禁止 -->
<!-- 配置先: docs/features/vault-encryption/basic-design/ux-and-msg.md -->
<!-- 主担当: Sub-A (#39) 着手 → Sub-D (#42) MSG 文言指針追加 → Sub-E (#43) IPC V2 経路 UX 確定 + cache_relocked 仕様追記 -->
<!-- 本書は MSG-* 文言の意図 / 田中ペルソナ ジャーニー / cache_relocked 仕様の SSoT。
     文言本体（CLI / GUI 二経路）は `requirements.md` §ユーザー向けメッセージ一覧 を SSoT として参照する。
     二重メンテ防止のため、本書には文言の重複転載を避け、設計判断と意図のみを記載する。 -->

## 外部連携

該当なし — 理由: vault-encryption feature は CLI / IPC / OS シグナル以外への発信を持たない。Sub-A は型ライブラリ（pure Rust / no-I/O）、Sub-B〜D は shikomi-infra の暗号アダプタ層、Sub-E は shikomi-daemon 内に閉じる、Sub-F は shikomi-cli の clap 構造で外部 API 呼出は無し。OS スクリーンロック / サスペンド購読は `OsLockSignal` trait 抽象で daemon 内に閉じる（`detailed-design/vek-cache-and-ipc.md` §`OsLockSignal`）。

## UX設計

### Sub-A スコープ（型ライブラリ / UI 不在）

`MasterPassword::new` の構築失敗時に返す `WeakPasswordFeedback { warning, suggestions }` は **Sub-D で MSG-S08 ユーザ提示（Fail Kindly）に直接渡される構造データ**として設計する。**`warning=None` 時の代替警告文契約 + i18n 戦略責務分離（Sub-A は英語 raw のみ運ぶ、Sub-D が i18n 層を挟む）** は `detailed-design/password.md` §`warning=None` 契約 / §i18n 戦略責務分離 を参照。

### Sub-D スコープ（vault encrypt / decrypt / change-password / マイグレーション層）

Sub-D は MSG-S01 / S02 / S05 / S06 / S08 / S10 / S11 / S12 / S13 / S14 / S16 / S18 を担当。文言設計指針は **過信防止（断定禁止）+ 過小評価回避 + 次の一手提示** の 3 点を全 MSG-* で守る。

- **MSG-S08（パスワード弱い）**: zxcvbn `feedback.warning` の i18n 翻訳経路 + `warning=None` フォールバック 3 経路（既定文言 / `suggestions` 先頭文 / 強度スコア）
- **MSG-S10（AEAD 改竄）**: 「改竄の可能性 / ディスク破損 / 実装バグでも発生」を併記、断定禁止。次の一手「バックアップから復元」を必ず提示
- **MSG-S11（nonce 上限）**: `vault rekey` 誘導、残操作猶予の数値非表示（`NonceCounter::current()` を攻撃面情報として隠蔽）
- **MSG-S13（マイグレーション失敗）**: 原状復帰済み明示、段階情報はログのみ（ユーザには概要のみ）
- **MSG-S14（decrypt 確認）**: `DECRYPT` 入力 + パスワード再入力の二段確認、`subtle::ConstantTimeEq` で判定
- **MSG-S16（暗号化モード初回切替時の限界説明）**: ユーザの明示的合意（`--accept-limits` / モーダル「理解しました」）なしに次工程へ進ませない

詳細文言は `requirements.md` §ユーザー向けメッセージ一覧 を SSoT 参照。

### Sub-E スコープ（IPC V2 経路 UX、本 Sub の主担当）

Sub-E は MSG-S03 / S04 / S05（Sub-D との統合確定）/ S07 / S09(a)(b)(c) / S15 / S19 / S20 を担当。`vault unlock / lock / change-password / rotate-recovery / rekey` の **5 操作** UX を IPC V2 経路で確定する。

#### CLI / GUI 二経路分離（Sub-E 工程2 ペガサス指摘で凍結）

- **CLI 経路**（エンジニア向け、技術詳細許容）: `VEK` / `daemon` / `キャッシュ` / `zeroize` 等の技術ジャーゴンを許容、`shikomi vault status` 等の補助コマンドを併記
- **GUI / 田中ペルソナ経路**（CLI を読めない非エンジニア向け）: 技術ジャーゴンを排除、操作影響を**ユーザの手元視点**で説明（「鍵情報を完全に消去しました」「もう一度パスワードを入力してください」）

二経路は同一 MSG ID で **意味的に等価**な情報を提供する。文言は `requirements.md` §ユーザー向けメッセージ一覧 を SSoT として更新する。

#### Sub-E 担当 MSG ID 索引

下表は Sub-E が確定した文言の意図と設計判断を示す。文言本体は `requirements.md` を参照すること（DRY）。

| MSG ID | トリガ | IPC 応答 | 設計判断の要点 |
|--------|------|---------|------------|
| **MSG-S03** | `vault unlock` 成功 | `IpcResponse::Unlocked` | 自動再ロック条件（idle 15min / ScreenLock / Suspend）を必ず明示。VEK は IPC に乗らないことを CLI 経路で補足 |
| **MSG-S04** | `vault lock` 完了 | `IpcResponse::Locked` | 明示 `Lock` IPC / idle / OS スクリーンロック / サスペンド の 4 経路で **共通文言**。経路差を表に出さない（攻撃面情報の希釈） |
| **MSG-S05** | `change-password` 完了 | `IpcResponse::PasswordChanged` | **VEK 不変 / 再 unlock 不要 / レコード再暗号化なし** を強調（O(1) change-password の利点をユーザに伝達、REQ-S10 / Sub-D §F-D5） |
| **MSG-S07** | `rekey` 完了 | `IpcResponse::Rekeyed { records_count, words, cache_relocked }` | 再暗号化レコード数 + 新 24 語表示。`cache_relocked == false` 時は **MSG-S20 を連結**（後述） |
| **MSG-S09(a)** | unlock 失敗（パスワード違い） | `IpcError::BackoffActive` / `Crypto { reason:"wrong-password" }` / `RecoveryRequired` | 待機時間 N 秒を表示（`failures` カウンタ値は隠蔽）、リカバリ用 24 語経路を併記。**Bug-E-001 修正後**: `verify_header_aead` 由来の `AeadTagMismatch` も `unlock_with_password` 文脈では `WrongPassword` に変換、backoff 対象 |
| **MSG-S09(b)** | IPC 接続不能 | （クライアント側のみ、daemon 応答なし） | `daemon status` 案内 / GUI は再起動誘導 |
| **MSG-S09(c)** | キャッシュ揮発 / 自動 lock | `IpcError::VaultLocked` | 自動 lock 経路を 3 種列挙（idle 15min / 画面ロック / スリープ）、再 unlock 誘導 |
| **MSG-S15** | V1 クライアントが V2 専用 variant 送信 | `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` | 更新案内 + V1 サブセット（`list/add/edit/remove`）継続使用可の説明、田中ペルソナ向け GUI モーダル経路 |
| **MSG-S19**（新規） | `rotate_recovery` 完了 | `IpcResponse::RecoveryRotated { words, cache_relocked }` | 新 24 語表示 + MSG-S06 警告連結（写真撮影禁止 / 紙保管推奨）+ `cache_relocked == false` 時 MSG-S20 連結 |
| **MSG-S20**（新規、付帯警告） | `cache_relocked == false` | `Rekeyed` / `RecoveryRotated` 共通付帯 | 「操作は完了したが daemon 内 VEK 再キャッシュに失敗、次操作前に再 unlock してください」誘導。**Lie-Then-Surprise 防止の主役**（後述 §cache_relocked: false の UX 設計判断） |

### Sub-F スコープ（CLI 経路実装 / 後続 GUI feature）

Sub-F は MSG-S07 / S11 / S14 / S18 / S20 の CLI 最終文言、MSG-S03 / S04 / S05 / S09 / S15 / S19 の CLI 実装、MSG-S17（GUI バッジ、後続 GUI feature 連携）を担当。Sub-A〜E が確定した文言指針を i18n 翻訳辞書 `messages.toml` で具現化する。

#### Sub-F 確定 MSG ID 索引（CLI 最終文言）

下表は Sub-F 工程2 で CLI 最終文言を確定した MSG の翻訳辞書キーと意図を示す。文言本体は `requirements.md` §ユーザー向けメッセージ一覧 を SSoT とする（DRY、本書は意図と設計判断のみ）。

| MSG ID | 翻訳キー（`messages.toml`）| 意図 / 設計判断 |
|--------|------------------------|------------|
| MSG-S07 | `rekey.completed` / `rekey.completed_gui` | 再暗号化レコード数 + 新 24 語 + cache_relocked == false 時 MSG-S20 連結。終了コード 0（C-31）|
| MSG-S11 | `nonce_limit_exceeded` / `nonce_limit_exceeded_gui` | `vault rekey` 誘導、残操作猶予数値非表示、終了コード 1 |
| MSG-S14 | `decrypt.confirmation` | DECRYPT 二段確認 + `subtle::ConstantTimeEq` + paste 抑制（C-34）+ 大文字検証、`--force` 提供しない（C-20）|
| MSG-S18 | `recovery_show.print` / `braille` / `audio` | `--print` PDF / `--braille` BRF / `--audio` OS TTS、WCAG 2.1 AA、自動切替 `SHIKOMI_ACCESSIBILITY=1` env |
| MSG-S20 | `cache_relock_failed` / `cache_relock_failed_gui` | 「操作完了 + 再 unlock 必要」を Fail Kindly で連結、終了コード 0、新 24 語先表示の不変条件 (c)（`detailed-design/cli-subcommands.md` §i18n 戦略責務分離）|

#### Sub-F の CLI 文言補強（Sub-E 指針継承）

Sub-E で凍結した MSG-S03 / S04 / S05 / S09 / S15 / S19 は CLI 経路で以下のように具現化される（i18n キー命名は `detailed-design/cli-subcommands.md` §i18n 戦略責務分離 に準ずる）:

| MSG ID | CLI 翻訳キー | 終了コード |
|--------|-----------|----------|
| MSG-S03 | `unlock.completed` | 0 |
| MSG-S04 | `lock.completed` | 0 |
| MSG-S05 | `change_password.completed` | 0 |
| MSG-S09(a) | `unlock.wrong_password_with_recovery_hint` | 2 / 5（RecoveryRequired 経路）|
| MSG-S09(b) | `daemon_connection_failed` | 64（`sysexits.h::EX_USAGE` 相当、daemon 未起動）|
| MSG-S09(c) | `vault_locked_with_unlock_hint` | 3 |
| MSG-S15 | `protocol_downgrade_with_update_hint` | 4 |
| MSG-S19 | `rotate_recovery.completed` / `rotate_recovery.completed_gui` | 0（C-31、cache_relocked == false でも 0）|

## cache_relocked: false の UX 設計判断（Sub-E 工程5 ペガサス致命指摘）

### 経緯

Sub-E 工程5 のペガサス致命指摘①「成功と偽る lock」経路を本書で正式に文書化する。

旧設計（PR #68 `cc06de7` 時点）では `v2_handler/rotate_recovery.rs:62-69` および `rekey.rs:57-63` で以下の経路を持っていた:

1. atomic write 成功
2. `cache.lock()` で旧 VEK を破棄
3. `unlock_with_password()` で新パスワードを使い再キャッシュを試行
4. **3 が失敗した場合、`tracing::warn!` のみ出力して `IpcResponse::RecoveryRotated` / `Rekeyed` を成功として返却**

ユーザは「成功！新 24 語はこれ」を見た直後、次の `list` で突如 `VaultLocked` を受けて詰む。**Fail Kindly の真逆 = Lie-Then-Surprise**、田中ペルソナ即死。

### 修正方針（Sub-E 採用、PR #68 `143e8eb` で実装）

1. **`IpcResponse::Rekeyed` / `RecoveryRotated` に `cache_relocked: bool` フィールド追加**（必須）
2. `cache.lock() → unlock_with_password()` の再キャッシュが失敗した場合、`cache_relocked: false` を **応答に明示**して返却
3. atomic write は成功している → vault.db は新 mnemonic + 新 VEK で wrap 済の正常状態 → **`Err` ではなく `Ok` 系応答**で返す（操作は成功した）
4. Sub-F CLI / GUI は `cache_relocked == false` を検出して **MSG-S20 を MSG-S07 / S19 に連結表示**
5. daemon 側は `tracing::warn!(target="shikomi_daemon::v2_handler", ...)` で診断ログを残す（運用維持）

### MSG-S20 文言指針（Sub-E 確定、Sub-F が CLI 最終文言確定）

**CLI 経路**（エンジニア向け）:

> **注意**: 操作は完了しましたが、daemon の VEK 再キャッシュに失敗しました。次の操作前に `shikomi vault unlock` を再度実行してください。

**GUI / 田中ペルソナ経路**（技術ジャーゴン排除）:

> **鍵情報の再読み込みに失敗しました**。次の操作前にもう一度パスワードを入力してください。

文言の不変条件:

- 「操作は完了した」を**先**に伝える（vault.db は正常、ロールバック不要）
- 「再 unlock が必要」を**次に**伝える（次の一手の明示）
- **失敗の内部詳細**（I/O エラー詳細 / KEK_pw 導出失敗詳細）は**含めない**（攻撃面情報の希釈、MSG-S10 / S11 と同方針）
- `cache_relocked: false` 時も新 24 語は表示する（rekey / rotate_recovery の主目的、ユーザは紙にメモする責務）

### 設計上の不変条件（Sub-E Rev2 追加、`detailed-design/vek-cache-and-ipc.md` §不変条件と同期）

- **C-30**: `IpcResponse::Rekeyed` / `RecoveryRotated` は `cache_relocked: bool` フィールド必須。フィールド欠落時はコンパイルエラー（型レベル強制）
- **C-31**: `cache_relocked: false` は **vault.db atomic write が成功した後の経路でのみ発生**。atomic write 失敗時は `Err(IpcError::Persistence)` で返り、`Rekeyed` / `RecoveryRotated` 応答自体が構築されない
- **C-32**: `cache_relocked: false` 経路でユーザが次の read/write IPC を送ると `IpcError::VaultLocked` が返る。Sub-F は MSG-S20 表示後、**ユーザが次操作を行う前に再 unlock 経路を能動的に提示**する責務（CLI: `vault unlock` 案内、GUI: パスワード入力モーダル自動表示）

### 却下案

| 案 | 説明 | 採否 |
|---|------|------|
| **A: `cache_relocked: false` 時は `Err(IpcError::Internal)` で返す** | 「再キャッシュ失敗は内部エラー」として失敗応答 | **却下**: vault.db は正常状態 = 操作は成功している。`Err` で返すと Sub-F が「操作失敗」とユーザに伝え、ユーザが再 rekey / rotate_recovery を実行 → 不整合の連鎖。新 24 語が表示されない経路もできる → ユーザは旧 mnemonic で復号できなくなり完全敗北 |
| **B: `tracing::warn!` のみ（旧設計）** | 成功応答 + ログ警告のみ | **却下**: ペガサス致命指摘の元経路、Lie-Then-Surprise |
| **C: `cache_relocked: bool` を応答に追加（採用）** | 操作成功を保ちつつ再 unlock 必要を明示 | **採用**: vault.db 正常状態を尊重、ユーザに「成功 + 注意」を Fail Kindly で伝達、田中ペルソナ守護 |
| **D: daemon 側で再 unlock を 3 回リトライ** | 一時 I/O エラー耐性 | **保留**: 設計穴の根本解決にならない（3 回失敗時の経路は同じ問題）。Sub-F 工程で運用観測後に検討 |

## 田中ペルソナ ジャーニー（Sub-E 工程2 ペガサス指摘で凍結）

田中ペルソナ A（CLI を読めない非エンジニア、GUI のみ使用）が暗号化モードで vault を扱う典型ジャーニーを以下に示す。各ステップで表示される MSG ID と GUI 経路文言指針を明示する。

```mermaid
flowchart TD
    Start([田中: shikomi-gui 起動]) --> Encrypt[vault encrypt 初回実行]
    Encrypt -->|MSG-S16 限界説明| Consent{理解しました ボタン押下}
    Consent -->|MSG-S08 弱い場合| WeakPw[パスワード強度ゲート再入力]
    WeakPw --> Encrypt
    Consent -->|強い場合| ShowMnemonic[MSG-S06 24語表示警告]
    ShowMnemonic --> Confirm[書き写し完了 ボタン押下]
    Confirm -->|MSG-S01| Encrypted([暗号化完了])
    Encrypted --> Locked([daemon: Locked 状態])
    Locked --> Unlock[GUI ロック解除パスワード入力 試行]
    Unlock -->|正パスワード MSG-S03| Unlocked([daemon: Unlocked])
    Unlock -->|誤パスワード MSG-S09 a| WrongPw[パスワード違い 待機案内]
    WrongPw -->|失敗カウンタ < 5| Locked
    WrongPw -->|失敗 5 回連続 MSG-S09 a + wait_secs| Backoff[次の試行まで 30 秒]
    Backoff --> Locked
    Unlocked --> List[レコード一覧画面]
    List -->|MSG-S17 [encrypted] バッジ常時| List
    List -->|15 min 操作なし MSG-S04| AutoLock([自動 lock])
    AutoLock --> Locked
    Unlocked --> ChangePw[パスワード変更画面]
    ChangePw -->|MSG-S05 VEK 不変| Unlocked
    Unlocked --> Rekey[鍵を再生成 ボタン]
    Rekey -->|atomic write OK + 再キャッシュ OK<br/>MSG-S07| ShowNewMnemonic[新 24 語表示 MSG-S06 連結]
    Rekey -->|atomic write OK + 再キャッシュ NG<br/>MSG-S07 + MSG-S20| ShowNewMnemonicAndRelock[新 24 語表示 + 再ログイン誘導]
    ShowNewMnemonicAndRelock --> Locked
    ShowNewMnemonic --> Unlocked
```

### ジャーニー上の Fail Kindly 守護ポイント

1. **MSG-S16 → MSG-S06 → MSG-S01** の連鎖: 暗号化モード初回は限界説明 → 24 語警告 → 完了通知の 3 段で「**写真撮影禁止 / 紙保管 / 物理保管**」の合意を取得
2. **MSG-S03 / S04** の自動再ロック条件明示: ユーザが「いつロックされるか」を予期できる
3. **MSG-S05** の「再 unlock 不要」明示: O(1) change-password の利点を伝達、田中ペルソナの「パスワード変えたら全部やり直し？」誤解を未然防止
4. **MSG-S07 / S19 + MSG-S20** の連結: rekey / rotate_recovery 完了時に再キャッシュ失敗を**先制的に明示**、Lie-Then-Surprise を構造的に封鎖
5. **MSG-S09(a)** の待機時間表示: brute force backoff 中も「いつ試せるか」を伝える、ユーザの不安を最小化
6. **MSG-S15** の V1 クライアント拒否: 田中ペルソナは GUI のみのため通常到達しないが、shikomi-cli を別途使った場合の救済経路として GUI モーダルで自動更新案内

## i18n 戦略責務分離

Sub-A の `WeakPasswordFeedback` は zxcvbn の英語 raw `warning` / `suggestions` を運ぶ純粋なデータ構造。Sub-D の MSG-S08 文言確定経路で **i18n 翻訳辞書**（`shikomi-cli` / `shikomi-gui` 側）を挟んで日本語化する。

- **Sub-A 責務**: 英語 raw を一切翻訳しない、`warning=None` 時の代替警告文契約のみ提供（`detailed-design/password.md` §`warning=None` 契約）
- **Sub-D 責務**: i18n 翻訳辞書経由で MSG-S08 を組み立てる、`feedback.warning` の翻訳キーマッピングを Sub-D 設計書 `password.md` §i18n 戦略責務分離に凍結
- **Sub-E 責務**: MSG-S03 / S04 / S05 / S07 / S09 / S15 / S19 / S20 の i18n キーを `requirements.md` MSG 表で定義（CLI / GUI 二経路で同キー使用）
- **Sub-F 責務**: CLI 経路の最終文言（MSG-S07 / S11 / S14 / S20 CLI 経路）と i18n 辞書ファイル（`shikomi-cli/locales/ja-JP/messages.toml` 等）の実装

i18n 辞書の格納場所と key 命名規則は Sub-F 工程で確定する。Sub-E は **MSG ID + CLI / GUI 二経路の意味論等価性**のみを SSoT として凍結する。

## 受入基準・非機能要件との対応

`requirements-analysis.md` §受入基準 / §非機能要件と本書で確定した UX / MSG の対応関係を以下に示す。

| 出典 | UX / MSG での担保 |
|------|---------------|
| `requirements-analysis.md` §受入基準 #9（過信なく / 過小評価なく伝わる） | MSG-S10（断定禁止 + バックアップ復元）/ MSG-S16（限界説明 + 明示合意） |
| `requirements-analysis.md` §非機能要件「ユーザ理解担保 / 暗号化モード切替時の限界説明」 | MSG-S16（4 点限界説明 + `--accept-limits` / モーダル明示同意） |
| `requirements-analysis.md` §非機能要件「ユーザ理解担保 / 保護モード可視化」 | MSG-S17 GUI バッジ + REQ-S16 CLI `[encrypted]` / `[plaintext]` バナー |
| `requirements-analysis.md` §非機能要件「アクセシビリティ / リカバリ 24 語の視覚障害ユーザ向け代替経路（WCAG 2.1 AA）」 | MSG-S18 `--print` / `--braille` / `--audio` 代替経路 |
| **田中ペルソナ A（CLI を読めない非エンジニア）の Fail Kindly 守護**（脅威モデル L1〜L4 横断、`threat-model.md` §7.0 既定 Fail Kindly） | CLI / GUI 二経路分離（全 MSG-S03/S04/S05/S07/S09/S15/S19/S20）、田中ジャーニー §1〜6 守護ポイント |
| **Lie-Then-Surprise 防止**（Sub-E 工程5 ペガサス致命指摘で追加要請、Fail Kindly 補強） | MSG-S20 + `cache_relocked: bool` 応答スキーマ + C-30/C-31/C-32 不変条件 |
