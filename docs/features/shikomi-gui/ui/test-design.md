# テスト設計書 — ui（shikomi-gui）

<!-- feature: shikomi-gui / sub-feature: ui / Issue #96 -->
<!-- 配置先: docs/features/shikomi-gui/ui/test-design.md -->
<!-- システムテストは system-test-design.md に記述。本ファイルは IT + UT のみ -->
<!-- 参照: basic-design.md §モジュール契約 / detailed-design.md §1〜7 -->

## 0. テスト方針参照

本テスト設計書は `config/prompts/test_strategy.md` に定めるテスト戦略（Vモデル階層化・ダブル方針・CI ワークフロー対応）に準拠する。本ファイルは IT + UT のみを記述し、システムテストは親 `system-test-design.md` に委ねる。

---

## 1. 外部 I/O 依存マップ

| テスト | 外部 I/O | 依存対象 | 対処 | Fixture 状態 |
|-------|---------|---------|------|------------|
| IT（コンポーネント → Command） | `@tauri-apps/api/core` の `invoke` | Tauri IPC ブリッジ（Sub-B Commands） | `vi.mock('@tauri-apps/api/core')` で `invoke` を factory stub に差し替え | 不要：factory は Sub-B `detailed-design.md §2.3` の凍結 API 契約を型として使用。raw fixture は Sub-B IPC 結合テスト（`it_ipc_commands.rs`）が担保済み |
| UT（コンポーネント純粋レンダリング） | なし | 純粋 UI 計算（props → DOM） | モック不要 | 不要 |
| UT（`errors.ts` 変換） | なし | 純粋関数（GUIError → 日本語文字列） | モック不要 | 不要 |
| UT（`PasswordStrengthMeter`） | `zxcvbn` ライブラリ | 純粋計算（パスワード強度評価） | **実ライブラリ使用**（モック不要）。ピュア関数かつ外部 I/O なし | 不要 |
| UT（機密ライフサイクル） | DOM `<input>` ref | jsdom/happy-dom の仮想 DOM | Vitest 組込み DOM 環境で検証 | 不要 |

> **assumed mock 禁止**: MockIPC の `invoke` stub が返す値は Sub-B `detailed-design.md §2.3` の凍結 API 契約に基づく型付き factory で生成する。
> インラインオブジェクトリテラル（`{ kind: "ipc_error", ... }` 直書き）は factory 経由必須。

---

## 2. テスト配置方針

| テストレベル | 配置先 | 実行コマンド |
|------------|--------|------------|
| UT（コンポーネント + `errors.ts`） | `crates/shikomi-gui/ui/src/**/*.test.tsx` | `npm test` (vitest) |
| IT（MockIPC コンポーネント統合） | `crates/shikomi-gui/ui/src/**/*.it.test.tsx` | `npm test` (vitest) |

**テストフレームワーク（SolidJS 向け）**:

| ライブラリ | 用途 |
|-----------|------|
| `vitest` | テストランナー + `vi.mock` |
| `@solidjs/testing-library` | SolidJS コンポーネント render/cleanup |
| `@testing-library/user-event` | ユーザ操作シミュレーション（クリック・入力等） |
| `happy-dom` | DOM 環境（Vitest の `environment: 'happy-dom'`） |

---

## 3. テスト用ダブルの方針

### 3.1 MockIPC（IT 専用）

IT テストでのみ使用する。`@tauri-apps/api/core` の `invoke` を `vi.mock` で差し替え。

| 項目 | 仕様 |
|------|------|
| 実装 | `vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))` でモジュール全体を差し替え |
| 返却値 | `vi.mocked(invoke).mockResolvedValueOnce(factory.xxx())` のように factory 経由で設定 |
| factory | Sub-B 凍結 API 契約（`GUIError` JSON / `ListEntriesOutput` / `EntryIdOutput` 等）を型ガード付きで生成する `tests/factories/ipc.ts` に配置 |

### 3.2 `zxcvbn` — 実ライブラリ使用

`PasswordStrengthMeter` テストでは `zxcvbn` をモックせず実ライブラリを呼び出す。score の境界値（2 / 3）は実際に score を誘発するパスワードを入力する。

| score | サンプルパスワード（境界値用） | 強度ラベル |
|------|-------------------------------|-----------|
| 2（disabled 最大）| `"password123"` 相当 | 普通（`disabled`）|
| 3（enabled 最小）| `"correctHorseBatteryStaple"` 相当 | 強い（`enabled`）|

**注意**: `zxcvbn` の実 score は入力値に依存するため、境界値テストは実観測した入力値を fixture としてコメントに記録しておくこと。

---

## 4. テストマトリクス（トレーサビリティ）

### 4.1 ユニットテスト

| テスト ID | REQ-UI | 設計根拠 | テスト内容 | 種別 |
|---------|---------|--------|----------|------|
| TC-GUI-UI-UT01 | REQ-UI-03 | `detailed-design.md §1.3` | `VaultStatusBanner` — `mode="plaintext"` → 「[平文]」テキスト表示 | 正常系 |
| TC-GUI-UI-UT02 | REQ-UI-03 | `detailed-design.md §1.3` | `VaultStatusBanner` — `mode="encrypted_locked"` → 「[暗号化済・ロック中]」 | 正常系 |
| TC-GUI-UI-UT03 | REQ-UI-03 | `detailed-design.md §1.3` | `VaultStatusBanner` — `mode="encrypted_unlocked"` → 「[暗号化済・解除済]」 | 正常系 |
| TC-GUI-UI-UT04 | REQ-UI-03 | `detailed-design.md §1.3` | `VaultStatusBanner` — `mode="unknown"` → 「[不明]」 | 正常系 |
| TC-GUI-UI-UT05 | REQ-UI-08 | `detailed-design.md §1.8 / §5` | `PasswordStrengthMeter` — score 0 → 「非常に脆弱」ラベル + `onScore(0)` 呼び出し | 正常系 |
| TC-GUI-UI-UT06 | REQ-UI-08 | `detailed-design.md §5` | `PasswordStrengthMeter` — score 2（disabled 上限境界値）→ 「普通」ラベル + `onScore(2)` | 正常系（境界値）|
| TC-GUI-UI-UT07 | REQ-UI-08 | `detailed-design.md §5` | `PasswordStrengthMeter` — score 3（enabled 下限境界値）→ 「強い」ラベル + `onScore(3)` | 正常系（境界値）|
| TC-GUI-UI-UT08 | REQ-UI-08 | `detailed-design.md §5` | `PasswordStrengthMeter` — score 4 → 「非常に強い」ラベル + `onScore(4)` | 正常系 |
| TC-GUI-UI-UT09 | REQ-UI-08 | `detailed-design.md §1.7 / §5` | `VaultEncryptPanel` — score < 3 の間は「暗号化」ボタンが `disabled` | 正常系 |
| TC-GUI-UI-UT10 | REQ-UI-10 | `detailed-design.md §1.9` | `VaultDecryptPanel` — チェックボックス未チェック → 「解除する」ボタン `disabled` | 正常系 |
| TC-GUI-UI-UT11 | REQ-UI-10 | `detailed-design.md §1.9` | `VaultDecryptPanel` — チェックボックスチェック後 → 「解除する」ボタン enabled | 正常系（境界値）|
| TC-GUI-UI-UT12 | REQ-UI-04, REQ-UI-12 | `detailed-design.md §1.5` | `EntryForm`（追加モード）— ラベル空文字送信 → 「ラベルを入力してください」フィールド直下表示・`add_entry` 未呼び出し | 異常系 |
| TC-GUI-UI-UT13 | REQ-UI-04, REQ-UI-12 | `detailed-design.md §1.5` | `EntryForm`（追加モード）— 値空文字送信 → 「値を入力してください」フィールド直下表示・`add_entry` 未呼び出し | 異常系 |
| TC-GUI-UI-UT14 | REQ-UI-05 | `detailed-design.md §1.5` | `EntryForm`（編集モード）— 初期値から変更なし → フォーム送信しても `update_entry` invoke を呼ばない | 異常系（Silent Skip） |
| TC-GUI-UI-UT15 | REQ-UI-09, REQ-UI-14 | `detailed-design.md §1.11 / §4` | `RecoveryPhraseDisplay` — 24 語 props を受け取り番号付きで全語表示する | 正常系 |
| TC-GUI-UI-UT16 | REQ-UI-09 | `detailed-design.md §1.11` | `RecoveryPhraseDisplay` — 「転記完了」ボタン押下 → `onConfirmed()` 呼び出し | 正常系 |
| TC-GUI-UI-UT17 | REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "daemon_not_running" }` → 「daemon が起動していません。`shikomi start` を実行してください」 | 正常系 |
| TC-GUI-UI-UT18 | REQ-UI-07, REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "ipc_error", ipc_code: "hotkey_conflict", hotkey_conflict_entry: "my-entry" }` → 「`{combo}` は別エントリ（`my-entry`）に割り当て済みです」（`message` フィールド不使用） | 正常系 |
| TC-GUI-UI-UT19 | REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "ipc_error", ipc_code: "crypto", crypto_reason: "wrong-password" }` → 「パスワードが一致しません」 | 正常系 |
| TC-GUI-UI-UT20 | REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "ipc_error", ipc_code: "crypto", crypto_reason: "weak-password" }` → 「パスワードが脆弱すぎます」 | 正常系 |
| TC-GUI-UI-UT21 | REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "ipc_error", ipc_code: "crypto", crypto_reason: "nonce-limit-exceeded" }` → 「vault の再暗号化が必要です…」 | 正常系 |
| TC-GUI-UI-UT22 | REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "ipc_error", ipc_code: "backoff_active", wait_secs: 30 }` → 「`30` 秒後に再試行してください」（`wait_secs` 補間） | 正常系 |
| TC-GUI-UI-UT23 | REQ-UI-11, REQ-UI-13 | `detailed-design.md §6` | `errors.ts` — `{ kind: "ipc_error", ipc_code: "vault_locked" }` → 制御フロー信号（`UnlockModal` 表示用、日本語文字列でない） | 正常系 |
| TC-GUI-UI-UT24 | REQ-UI-14 | `detailed-design.md §4` | `VaultEncryptPanel` — `invoke` 呼び出し直後にパスワード DOM ref が `""` に上書きされること（機密変数ゼロ化、R1-GUI-18） | 正常系（機密ライフサイクル）|
| TC-GUI-UI-UT25 | REQ-UI-14 | `detailed-design.md §4` | `UnlockModal` — `unlock_vault` invoke 後にパスワード DOM ref が `""` に上書きされること（R1-GUI-18）| 正常系（機密ライフサイクル）|
| TC-GUI-UI-UT26 | REQ-UI-02 | `detailed-design.md §1.4` | `EntryList` — 種別 `text` → 「テキスト」/ 種別 `secret` → 「シークレット」表示 | 正常系 |
| TC-GUI-UI-UT27 | REQ-UI-02 | `detailed-design.md §1.4` | `EntryList` — ホットキー設定済みエントリに「Ctrl+Alt+X」バッジ表示。未設定エントリは空欄 | 正常系 |

### 4.2 結合テスト（MockIPC）

| テスト ID | REQ-UI | 設計根拠 | テスト内容 | 種別 |
|---------|---------|--------|----------|------|
| TC-GUI-UI-IT01 | REQ-UI-01 | `detailed-design.md §1.1` | `App` 起動 → `list_entries` invoke 成功 → `connected` 遷移、`EntryList` + `VaultStatusBanner` が表示される | 正常系 |
| TC-GUI-UI-IT02 | REQ-UI-01 | `detailed-design.md §1.2` | `App` 起動 → `list_entries` → `daemon_not_running` → `DaemonConnectionPanel` 表示、全操作ボタン無効 | 異常系 |
| TC-GUI-UI-IT03 | REQ-UI-11 | `detailed-design.md §3 / §1.1` | `App` 起動後、任意 Command が `vault_locked` を返す → `UnlockModal` がオーバーレイ表示される | 異常系 |
| TC-GUI-UI-IT04 | REQ-UI-04 | `detailed-design.md §1.5` | `EntryForm`（追加モード）— ラベル・値入力後に送信 → `add_entry` invoke 呼び出し成功 → `onSuccess()` 呼び出し | 正常系 |
| TC-GUI-UI-IT05 | REQ-UI-05 | `detailed-design.md §1.5` | `EntryForm`（編集モード）— ラベル変更後に送信 → `update_entry` invoke 呼び出し成功 → `onSuccess()` 呼び出し | 正常系 |
| TC-GUI-UI-IT06 | REQ-UI-06 | `detailed-design.md §1.4` | `EntryList` — 削除ボタン押下 → 確認ダイアログ → 確認 → `delete_entry` invoke 成功 → `list_entries` 再取得 | 正常系 |
| TC-GUI-UI-IT07 | REQ-UI-07 | `detailed-design.md §1.6` | `HotkeySelector` — `Ctrl+Alt+3` 選択 → `assign_hotkey` invoke 成功 → `onChanged()` 呼び出し | 正常系 |
| TC-GUI-UI-IT08 | REQ-UI-07 | `detailed-design.md §1.6` | `HotkeySelector` — `assign_hotkey` → `hotkey_conflict { hotkey_conflict_entry: "passwd-entry" }` → 「`Ctrl+Alt+X` は別エントリ（`passwd-entry`）に割り当て済みです」インライン表示（`message` 不使用） | 異常系 |
| TC-GUI-UI-IT09 | REQ-UI-07 | `detailed-design.md §1.6` | `HotkeySelector` — 「解除」ボタン押下 → `remove_hotkey` invoke 成功 → `onChanged()` 呼び出し | 正常系 |
| TC-GUI-UI-IT10 | REQ-UI-08 | `detailed-design.md §1.7` | `VaultEncryptPanel` — score ≥ 3 のパスワード入力後に送信 → `encrypt_vault` invoke 成功 → `onEncrypted(phrases)` 呼び出し（phrases 24 件）| 正常系 |
| TC-GUI-UI-IT11 | REQ-UI-10 | `detailed-design.md §1.9` | `VaultDecryptPanel` — チェックボックスチェック + パスワード入力 + 送信 → `decrypt_vault(confirmed: true)` invoke 成功 → `onDecrypted()` 呼び出し | 正常系 |
| TC-GUI-UI-IT12 | REQ-UI-11 | `detailed-design.md §1.10` | `UnlockModal` — `unlock_vault` → `wrong-password` → 「パスワードが一致しません」インライン表示。再入力可能状態 | 異常系 |
| TC-GUI-UI-IT13 | REQ-UI-11 | `detailed-design.md §1.10` | `UnlockModal` — `unlock_vault` → `backoff_active { wait_secs: 30 }` → 「30 秒後に再試行してください」インライン表示 | 異常系 |
| TC-GUI-UI-IT14 | REQ-UI-11 | `detailed-design.md §3` | `vault_locked` フロー — `UnlockModal` でアンロック成功 → `onUnlocked()` → 元操作 `pendingOperation` が再試行される | 正常系（回復フロー） |
| TC-GUI-UI-IT15 | REQ-UI-13 | `basic-design.md §3.2` | 全エラー経路で `message` フィールドが画面上に表示されないこと（`errors.ts` 一元変換の契約検証） | 正常系（API 契約） |

---

## 5. ユニットテスト詳細設計（抜粋）

### TC-GUI-UI-UT05〜08: `PasswordStrengthMeter` — score 0〜4 境界値

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-UI-UT05 〜 UT08 |
| 対応する要件ID | REQ-UI-08（R1-GUI-10） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §5`） |
| 種別 | 正常系（境界値） |
| 前提条件 | `zxcvbn` 実ライブラリを使用（モックなし）。Vitest `happy-dom` 環境 |
| 操作 | score 0/2/3/4 を誘発するパスワードを入力し `PasswordStrengthMeter` にレンダリング |
| 期待結果 | 各 score に対応するラベル文言表示、`onScore(n)` が正しい引数で呼ばれること |
| **重点**: score 2→3 の境界が `disabled` / `enabled` 切替の分岐点であるため UT06 + UT07 を必ず対にすること |

### TC-GUI-UI-UT14: `EntryForm` 編集モード — 変更なし Silent Skip

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-UI-UT14 |
| 対応する要件ID | REQ-UI-05（R1-GUI-06、ipc-client `basic-design.md §3.3` Sub-C 契約） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §1.5`） |
| 種別 | 異常系（Silent Skip） |
| 前提条件 | `mode="edit"`、`entry` に初期値（label="foo", value="bar"）を渡す |
| 操作 | label / value を変更せずフォーム送信 |
| 期待結果 | `invoke("update_entry", ...)` が呼ばれない。`onCancel()` が呼ばれる |
| **重点**: ipc-client Sub-C 契約（変更なし時に `update_entry` 呼ばない）の UI 側履行確認 |

### TC-GUI-UI-UT18〜22: `errors.ts` — `ipc_code` 別変換

| テスト ID | `ipc_code` | 入力追加フィールド | 期待日本語メッセージ |
|---------|-----------|-----------------|------------------|
| TC-GUI-UI-UT18 | `hotkey_conflict` | `hotkey_conflict_entry: "my-entry"` | 競合エントリ名 `my-entry` を含む文字列 |
| TC-GUI-UI-UT19 | `crypto` | `crypto_reason: "wrong-password"` | 「パスワードが一致しません」 |
| TC-GUI-UI-UT20 | `crypto` | `crypto_reason: "weak-password"` | 「パスワードが脆弱すぎます」 |
| TC-GUI-UI-UT21 | `crypto` | `crypto_reason: "nonce-limit-exceeded"` | 「vault の再暗号化が必要です」を含む文字列 |
| TC-GUI-UI-UT22 | `backoff_active` | `wait_secs: 30` | 数値 `30` を補間した文字列 |

**共通事項**: 各テストで `message` フィールドを参照せず `ipc_code` / 専用フィールドのみで変換することを確認する。`message` が戻り値に混入する実装は UT でここで検出する。

### TC-GUI-UI-UT24: 機密変数ゼロ化（R1-GUI-18）

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-UI-UT24 |
| 対応する要件ID | REQ-UI-14（R1-GUI-18） |
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §4`） |
| 種別 | 正常系（機密ライフサイクル） |
| 前提条件 | `VaultEncryptPanel` を MockIPC 環境でレンダリング。DOM ref で `<input type="password">` を保持 |
| 操作 | パスワード入力 → score ≥ 3 → 「暗号化」ボタン押下 → `encrypt_vault` mock が `Ok({disclosure: [24語]})` を返す |
| 期待結果 | `invoke` 呼び出し後に `<input>` の `value` が `""` になっていること（`createSignal` に機密値が残らない）|
| **重点**: R1-GUI-18 は「`createSignal` / `createStore` state に機密値を格納しない」制約。DOM ref の即破棄が実装されているかの防衛線 |

---

## 6. 結合テスト詳細設計（抜粋）

### TC-GUI-UI-IT08: HotkeySelector — `hotkey_conflict` 競合エントリ名表示

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-UI-IT08 |
| 対応する要件ID | REQ-UI-07（R1-GUI-08）|
| 対応する工程 | 階層 3 基本設計（`basic-design.md §3.2` / `detailed-design.md §1.6`） |
| 種別 | 異常系 |
| 前提条件 | `HotkeySelector` に `entryId` を渡してレンダリング。MockIPC: `assign_hotkey` が `{ kind: "ipc_error", ipc_code: "hotkey_conflict", hotkey_conflict_entry: "passwd-entry" }` を返す |
| 操作 | `Ctrl+Alt+3` を選択して割当 |
| 期待結果 | セレクタ直下に「`Ctrl+Alt+3` は別エントリ（`passwd-entry`）に割り当て済みです」が表示される。`message` フィールドの文字列は表示されない |
| **重点**: `hotkey_conflict_entry` フィールドを使った競合エントリ名表示（R1-GUI-08）。`message` parse 依存の旧パターンを検出する防衛線 |

### TC-GUI-UI-IT14: `vault_locked` フロー — 元操作再試行

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-UI-IT14 |
| 対応する要件ID | REQ-UI-11（R1-GUI-13）|
| 対応する工程 | 階層 3 詳細設計（`detailed-design.md §3`） |
| 種別 | 正常系（回復フロー） |
| 前提条件 | `App` コンポーネント + MockIPC。初回 `delete_entry` が `vault_locked` を返し、続く `unlock_vault` が `Ok` を返すよう設定 |
| 操作 | ① エントリ削除 → ② `UnlockModal` が自動表示 → ③ パスワード入力して「アンロック」 → ④ `unlock_vault` 成功 |
| 期待結果 | ④ 成功後、`delete_entry` が再試行され削除が完了する。`UnlockModal` が非表示になる。`list_entries` で一覧が更新される |
| **重点**: `pendingOperation` ストアに保存された元操作が `handleUnlockSuccess()` で再実行されることの確認（`detailed-design.md §3 sequenceDiagram` 全ステップ検証）|

### TC-GUI-UI-IT15: 全エラー経路で `message` フィールド表示禁止

| 項目 | 内容 |
|------|------|
| テストID | TC-GUI-UI-IT15 |
| 対応する要件ID | REQ-UI-13（ipc-client `detailed-design.md §2.2` 凍結 API 契約）|
| 対応する工程 | 階層 3 基本設計（`basic-design.md §3.2`）|
| 種別 | 正常系（API 契約） |
| 前提条件 | 各コンポーネントを MockIPC 環境でレンダリング。`errors.ts` が各 `ipc_code` → 日本語変換を返す |
| 操作 | `daemon_not_running` / `hotkey_conflict` / `crypto` / `backoff_active` の各エラーをコンポーネントに受け取らせる |
| 期待結果 | いずれのエラー経路でも `GUIError.message` の英語文字列（例: `"ipc error: vault is locked"` 等）が DOM に出現しない |
| **重点**: `errors.ts` 一元変換の守備範囲確認。`message` をコンポーネント直書きするバグを CI で検出する防衛線 |

---

## 7. モック方針まとめ

| テスト対象 | モック要否 | 実装方法 |
|----------|---------|---------|
| `@tauri-apps/api/core` の `invoke` | **IT で差し替え** | `vi.mock('@tauri-apps/api/core')` + `vi.mocked(invoke).mockResolvedValueOnce(factory.xxx())` |
| `zxcvbn` ライブラリ | **差し替え不要** | 純粋計算。実ライブラリを使い実観測 score でテストする |
| DOM ref（パスワード入力）| **差し替え不要** | happy-dom の仮想 DOM で `<input>` を実レンダリングし `.value` を assert |
| `store/vault.ts` | **IT は実ストア** | コンポーネント統合テストでは実ストアを通す。UT は props 直渡しで隔離 |

**assumed mock 禁止**: MockIPC factory は Sub-B `detailed-design.md §2.3` の型定義から生成する。`{ kind: "ipc_error" }` のインラインリテラル直書きは却下対象。

---

## 8. CI ワークフロー対応

| テスト | ワークフロー | 備考 |
|-------|------------|------|
| TC-GUI-UI-UT01〜UT27（計 27 件） | `test-gui.yml`（拡張）または新設 `test-gui-ui.yml` | happy-dom 環境でヘッドレス実行可能 |
| TC-GUI-UI-IT01〜IT15（計 15 件）| 同上 | MockIPC で Tauri プロセス不要、ヘッドレス実行可能 |
| Windows IT | `windows.yml`（拡張）| Windows Named Pipe 経路でも同一テストが通ること |

> **Tauri プロセス不要**: UI テストは `vi.mock` で `invoke` を差し替えるため Tauri バイナリのビルドは不要。Rust ライブラリのビルドが不要であることを意味し、CI 上は `npm test` のみで完結する。

---

## 9. カバレッジ基準

| 観点 | 基準 |
|------|------|
| REQ-UI 全件網羅 | REQ-UI-01〜13 全件が IT または UT でカバーされること |
| 正常系 | 全コンポーネントの主要フロー（IT）必須 |
| 異常系 | Fail Fast（JS validation / disabled ボタン）、エラーインライン表示、`vault_locked` 回復フローを網羅 |
| 境界値 | `zxcvbn` score 2（disabled 上限）/ 3（enabled 下限）を必ず含む |
| 機密ライフサイクル | R1-GUI-18 対象コンポーネント（`VaultEncryptPanel`, `VaultDecryptPanel`, `UnlockModal`）全件で DOM ref ゼロ化を UT 検証 |
| API 契約 | `message` フィールド非表示（凍結 API 契約 §2.3）を IT15 で構造的に検証 |
| `hotkey_conflict_entry` 表示 | R1-GUI-08 の UI 表示（IT08）が Sub-B 凍結フィールドから直接取得していることを確認 |

---

*作成: 涅マユリ（テスト担当）/ 2026-05-11*
*設計根拠: `docs/features/shikomi-gui/ui/basic-design.md` §モジュール契約 / `detailed-design.md` §1〜7 / Issue #96*
