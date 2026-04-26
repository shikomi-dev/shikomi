# テスト設計書 — Sub-E (#43) VEK キャッシュ + IPC V2 拡張

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->
<!-- 横断的変更: 本書は vault-encryption feature のテスト設計だが、daemon-ipc feature の `test-design/integration.md` の V2 ラウンドトリップ TC（TC-IT-021..025 想定、Sub-E 工程3 で同期追加）と双方向参照する。 -->

## 14. Sub-E (#43) テスト設計 — VEK キャッシュ + IPC V2 拡張

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#43](https://github.com/shikomi-dev/shikomi/issues/43) |
| 対象 PR | #66（`ec773c5`、設計フェーズ）|
| 対象成果物 | `vault-encryption/detailed-design/vek-cache-and-ipc.md`（新規 332 行）/ `basic-design/{architecture,processing-flows,security}.md`（EDIT）/ `requirements.md`（REQ-S09〜S12 / MSG-S03/S04/S05/S09/S15 確定）/ **横断**: `daemon-ipc/{requirements.md, detailed-design/protocol-types.md}`（IPC V2 非破壊昇格、`IpcRequest`/`IpcResponse`/`IpcErrorCode` の V2 variant 追加）|
| 設計根拠 | `vek-cache-and-ipc.md` `VekCache` / `VaultUnlockState` enum / `IdleTimer` 60 秒ポーリング / `OsLockSignal` trait + `#[cfg]` 分割 / `UnlockBackoff` 指数バックオフ / IPC V2 ハンドラ 5 variant、契約 C-22〜C-28（型レベル fail-secure + zeroize + アイドル + OS シグナル + バックオフ + RecoveryRequired 透過 + V1 拒否）|
| 対象 crate | `shikomi-core`（`IpcProtocolVersion::V2` + `IpcRequest`/`IpcResponse`/`IpcErrorCode` の V2 variant、横断的変更）+ `shikomi-daemon`（`cache::vek::{VekCache, VaultUnlockState, CacheError}` / `cache::lifecycle::{IdleTimer, OsLockSignal, LockEvent}` / `backoff::unlock::UnlockBackoff` / `ipc::v2_handler::{unlock, lock, change_password, rotate_recovery, rekey}`、新規）|
| **Sub-E TC 総数** | **27 件**（ユニット 16 + 結合 9 + property 1 + E2E 1 = 27、Sub-A/B/C/D と同型レンジ。**工程2 Rev1 で +3 TC**: U16 backoff カテゴリ限定 / I08 rekey-recovery 整合性 / I09 handshake バイパス拒否）+ 静的検査 7 件（TC-E-S01..S07、Rev1 で TC-E-S07 handshake 許可リスト境界 grep gate を新設）|

### 14.1 Sub-E テストレベルの読み替え（VEK キャッシュ + daemon プロセス用）

Sub-E は **Sub-A/B/C で凍結した型・契約 + Sub-D `VaultMigration` 6 メソッドを daemon プロセスに統合**する Sub。Vモデル対応：

| テストレベル | 通常の対応 | Sub-E での読み替え | 検証手段 |
|---|---|---|---|
| **ユニット** | メソッド単位、ホワイトボックス | 新規 5 型（`VekCache` / `VaultUnlockState` / `IdleTimer` / `UnlockBackoff` / `OsLockSignal` trait + `LockEvent`）の不変条件、`MigrationError → IpcError` マッピング、IPC V2 variant の variant_name 全網羅、`#[non_exhaustive]` enum の exhaustive match（ワイルドカード `_` 禁止 = C-22 構造防衛）| `cargo test -p shikomi-daemon --lib cache::vek::tests` + `cargo test -p shikomi-daemon --lib backoff::unlock::tests` + `cargo test -p shikomi-core --lib ipc::tests` |
| **結合** | モジュール連携、契約検証 | (a) **モック `MockVaultMigration` 経由で IPC V2 5 variant の往復**を `tokio` runtime 上で実行、(b) **`MockClock` で 15min 経過を simulate** → `IdleTimer` 自動 lock 観測、(c) **`MockLockSignal` から `LockEvent::ScreenLocked` / `SystemSuspended` 注入** → 100ms 以内 lock 観測、(d) **モック failure 注入で `UnlockBackoff` 5 連続失敗** → `BackoffActive { wait_secs: 30 }` 返却 | `cargo test -p shikomi-daemon --test ipc_v2_integration` + `tokio::test` runtime + `MockVaultMigration` / `MockLockSignal` / `MockClock` |
| **property** | ランダム入力での invariant | 任意の `LockEvent` シーケンス注入で **100ms 以内 cache.is_unlocked == false** を全例で観測（1000 ケース、契約 C-25）| `proptest` 1000 ケース（Sub-C/D と同型 ProptestConfig::with_cases(1000)）|
| **E2E** | 完全ブラックボックス、ペルソナ | 田中（経理担当者）が `shikomi vault unlock → 業務 → アイドル 15min → 自動 lock → 再 unlock → change-password → cache 維持確認`の 6 ステップシナリオを CLI から実行できる（**MSG-S03 / S04 / S05 文言が表示**、Sub-F 実装後に E2E 実行可能、Sub-E 工程5 で詳細化）| 人手シナリオテスト + Sub-F 工程5 で `tokio::test` 経由 in-process 統合再現 |

### 14.2 外部 I/O 依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---|---|---|---|
| **OS スクリーンロック / サスペンドシグナル** | — | `MockLockSignal { tx, rx }`（`tokio::sync::mpsc::channel` 経由でテスト側から `LockEvent` 注入）| **不要**（実 OS API 直接利用は具象 `MacOsLockSignal` / `WindowsLockSignal` / `LinuxLockSignal` のみ、テストは trait `OsLockSignal` 抽象層で完結。実 OS シグナル integration は Sub-F 工程5 で OS 別 manual smoke 実施）|
| **時刻 (`Instant::now`)** | — | `MockClock` trait（`fn now() -> Instant`、テスト側で `advance(15min)` 操作）| **不要**（`tokio::time::pause` + `tokio::time::advance` で `tokio` runtime 内時刻を進める標準パターン、新規 fixture 不要）|
| **IPC ストリーム (`tokio::io::DuplexStream`)** | — | 既存 `daemon-ipc/tests/` の `DuplexStream` ヘルパ再利用 | **既存資産再利用**（`daemon-ipc/test-design/integration.md` §4 既存 `Framed<DuplexStream, LengthDelimitedCodec>` 経由）|
| **Sub-D `VaultMigration` 6 メソッド** | — | `MockVaultMigration { unlock_returns: Result<(Vault, Vek), MigrationError>, .. }` （trait 抽象 + 戻り値ストアド）| **不要**（Sub-D で実装担保済、本 Sub-E は呼出側の責務分離テスト、`MigrationError` 9 variant のマッピング検証のみ）|

**理由**: Sub-E は Sub-A/B/C/D の組合せ層 + daemon ライフサイクル制御。暗号計算の正しさは Sub-B/C で、マイグレーション契約は Sub-D で担保済。Sub-E 固有の検証対象は **(1) `VaultUnlockState` 型遷移と zeroize 経路**、**(2) アイドル / OS シグナルによる自動 lock**、**(3) バックオフ指数増加**、**(4) `MigrationError → IpcError` マッピング**、**(5) IPC V2 ハンドラの match 強制**、**(6) V1 非破壊**。

### 14.3 Sub-E 受入基準（REQ-S09 / S10 / S11 / S12 + 契約 C-22〜C-28）

| 受入基準ID | 内容 | 検証レベル |
|---|---|---|
| **C-22** | `Locked` 状態で read/write IPC は型レベル拒否（各 V2 ハンドラ入口の `match VaultUnlockState` でワイルドカード `_` 禁止、`Locked => Err(IpcError::VaultLocked)` 強制）| ユニット（match 網羅 + compile_fail）+ 静的検査（grep gate TC-E-S01）|
| **C-23** | `Unlocked → Locked` 遷移時に `Vek` を即 zeroize（`mem::replace(&mut state, Locked)` で旧 state 取り出し → `Vek::Drop` で 32B zeroize）| ユニット（メモリパターン観測）+ 結合（IPC `Lock` 後の cache 状態確認）|
| **C-24** | アイドル 15min タイムアウトで自動 lock（`IdleTimer` バックグラウンド task が 60 秒ポーリングで `now - last_used >= 15min` 検出、最大遅延 60 秒受容）| 結合（`MockClock` で 15min + 1 秒 advance → cache が `Locked` に遷移）|
| **C-25** | OS スクリーンロック / サスペンド受信から **100ms 以内** に cache が Locked（`OsLockSignal::next_lock_event` 受信 → `cache.lock()` 呼出）| 結合 + property（`MockLockSignal` から `LockEvent::ScreenLocked` / `SystemSuspended` 注入、計時）|
| **C-26** | 連続失敗 5 回で指数バックオフ発動（5→30s, 6→60s, 7→120s, ... 最大 1h クランプ）、unlock 成功でリセット | ユニット（`UnlockBackoff::record_failure` × 5 → `check` Err、`record_success` → `check` Ok）+ 結合（`MockVaultMigration::unlock_returns: Err(_)` × 5 → 6 回目 `BackoffActive { 30 }` 返却）|
| **C-27** | `MigrationError::RecoveryRequired` を `IpcError::RecoveryRequired` 透過 → MSG-S09 (a) リカバリ経路案内（Sub-D Rev5 ペガサス指摘契約の Sub-E 実装）| ユニット（`From<MigrationError> for IpcError` の variant 対応）+ 結合（IPC `Unlock` 失敗時に `RecoveryRequired` 応答 + i18n 文言確認）|
| **C-28** | V1 クライアントが V2 専用 variant 送信時に `IpcErrorCode::ProtocolDowngrade` で拒否（handshake で `client_version=V1` 検出後の保護経路）| ユニット（`serde` deserialize 失敗 → ProtocolDowngrade）+ 結合（`MockClient { version: V1 }` から `IpcRequest::Unlock` 送信 → `IpcResponse::Error(ProtocolDowngrade)` 受信）|
| EC-1 | F-E1 Unlock 経路: `backoff.check` → `cache.state()` → `unlock_with_password` → `cache.unlock(vek)` → `backoff.record_success` の順序と早期 return 経路維持 | 結合 |
| EC-2 | F-E2 Lock 経路: 明示 IPC / アイドル / OS シグナル 3 経路全てが `cache.lock()` を呼び出し、`Vek` Drop 連鎖 zeroize | 結合 |
| EC-3 | F-E3 ChangePassword: VEK 不変、`wrapped_VEK_by_pw` のみ更新、daemon キャッシュ維持（`cache.is_unlocked() == true` 継続）| 結合 |
| EC-4 | F-E4 RotateRecovery: 新 24 語が `IpcResponse::RecoveryRotated { disclosure }` で **初回 1 度のみ**返却（C-19 所有権消費を IPC 経路で表現）| 結合 |
| EC-5 | F-E5 Rekey: 旧 VEK で全レコード復号 → 新 VEK で再暗号化、`cache.lock` → `cache.unlock(new_vek)` で cache 更新 | 結合 |
| EC-6 | MSG-S03（unlock 成功）/ MSG-S04（lock 完了）/ MSG-S05（change-password 完了 + cache 維持明示）/ MSG-S09 カテゴリ別ヒント (a) パスワード違い + RecoveryRequired 統合 / MSG-S15（V1 拒否 + 更新案内）の文言が i18n 翻訳辞書に含まれる | ユニット（grep）|
| EC-7 | `VaultUnlockState` / `VekCache` / `CacheError` / `LockEvent` / `IpcRequest`(V2) / `IpcResponse`(V2) / `IpcErrorCode`(V2) 全 enum に `#[non_exhaustive]` 適用 | 静的検査（grep gate TC-E-S02..S05）|
| EC-8 | shikomi-core / shikomi-infra に OS API（`DistributedNotificationCenter` / `WTSRegisterSessionNotification` / `zbus` / `dbus` / `objc`）の直接 import 不混入（Clean Arch 維持、Sub-A/B/C/D 累積契約継承）| 静的検査（grep gate TC-E-S06）|
| **C-29 候補** | **handshake 必須契約** + **handshake 完了前の全 IPC variant 拒否** + **handshake 後の `client_version × request.variant_name()` 許可リスト検証**（V1 許可セット 5 件 + V2 専用セット 5 件、不一致時 `ProtocolDowngrade` 返却）。Petelgeuse 工程2 Rev1 指摘で誤認した「`#[non_exhaustive]` の serde 経路保護」を**正規化**、Sub-D Rev3 ワイルドカード排除原則の Sub-E 段階継承 | ユニット（TC-E-U14）+ 結合（TC-E-I07 / I09）+ 静的検査（grep gate TC-E-S07）|
| EC-9 | **rekey 後の `wrapped_vek_by_recovery` 整合性破壊ウィンドウ封鎖**（atomic 化 OR invalidate マーカー方式）。服部工程2 Rev1 指摘で発見した正規ユーザ誤認 + L2 攻撃面拡大経路を Sub-E 結合層で封鎖 | 結合（TC-E-I08）|
| EC-10 | **`UnlockBackoff::record_failure` 呼出経路を「パスワード違い」カテゴリのみに限定**（`Crypto(_)` ワイルドカード backoff 禁止、`AeadTagMismatch` / `NonceLimitExceeded` / `KdfFailed` / `InvalidMnemonic` / `RecoveryRequired` / `Persistence(_)` / `Domain(_)` は backoff カウント対象外）。L2 DoS 嫌がらせ防衛 + 正規ユーザ誤バックオフ防止、Sub-D に `Crypto(WrongPassword)` variant 追加要求が Sub-E から発生（PR #66 Rev2 で Sub-D `MigrationError` 拡張要求を明記）| ユニット（TC-E-U16）|

### 14.4 Sub-E テストマトリクス

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---|---|---|---|---|
| TC-E-U01 | REQ-S09 / C-22 | `VekCache::new()` の初期状態が `VaultUnlockState::Locked`（`is_unlocked() == false`、`Locked` 状態で read/write 拒否の起点） | ユニット | 初期状態 |
| TC-E-U02 | REQ-S09 / EC-1 | `VekCache::unlock(vek)` で `Locked → Unlocked { vek, last_used: Instant::now() }` 遷移、`is_unlocked() == true`、`with_vek(\|v\| v.expose_within_crate())` で同一バイト列取得（クロージャインジェクション、Sub-C `AeadKey::with_secret_bytes` 同型）| ユニット | 状態遷移 |
| TC-E-U03 | C-23 / REQ-S09 | `VekCache::lock()` 呼出で `Unlocked → Locked` 遷移、`mem::replace` で旧 state 取り出し → `Vek::Drop` 連鎖 → 32B zeroize（メモリパターン: 旧 VEK バイト列が `[0u8; 32]` または完全に retrieve 不能）。Sub-A C-1 zeroize 連鎖の Sub-E 段階での維持確認 | ユニット | Drop 連鎖 |
| TC-E-U04 | C-22 / REQ-S09 | `VekCache::with_vek(\|v\| f(v))` が `Locked` 状態で `Err(CacheError::VaultLocked)`、`Unlocked` でクロージャ実行 + `last_used` 更新（呼出前後で `state.last_used` の `Instant` が進む）| ユニット | クロージャ実行 |
| TC-E-U05 | C-22 / EC-7 | `VaultUnlockState` への `match` 網羅（`Locked` + `Unlocked { vek, last_used }`）を**ワイルドカード `_` 無し**で書く + `#[non_exhaustive]` 適用。defining crate 内では `_` 不要で exhaustive、将来 variant 追加時は本テストが**実際に**先に壊れる（Sub-D Rev4 TC-D-U12 同型構造防衛、グレ gate は TC-E-S01 で機械強制）| ユニット | enum 網羅 |
| TC-E-U06 | REQ-S09 | `VaultUnlockState` への `Clone` / `Copy` / `Display` / `serde::Serialize` 実装は **compile_fail**（`Vek` の禁止トレイト連鎖が `VaultUnlockState` に伝播、誤コピー / 誤シリアライズ / 誤表示を型レベル禁止、Sub-A C-1 / Sub-D DC-6 同型）| ユニット | compile_fail |
| TC-E-U07 | EC-7 / REQ-S09 | `CacheError` 2 variant（`VaultLocked` / `AlreadyUnlocked`）+ `#[non_exhaustive]` 適用、`match` 網羅でワイルドカード `_` 無し（Sub-D `MigrationError` 同型）| ユニット | enum 網羅 |
| TC-E-U08 | C-26 / REQ-S11 | `UnlockBackoff::record_failure` を 4 回呼出後 `check()` が `Ok(())`、5 回目で `next_allowed_at = Some(now + 30s)` 設定、`check()` が `Err(BackoffActive { wait_secs: 30 })` 返却 | ユニット | 境界 |
| TC-E-U09 | C-26 / REQ-S11 | `UnlockBackoff::record_failure` を 6 回 → `wait_secs: 60`、7 回 → `120`、8 回 → `240`、... 連続失敗で**指数増加**、最大 1 時間（3600 秒）でクランプ（`failures >= 12` でも `wait_secs == 3600`）| ユニット | 指数増加 + クランプ |
| TC-E-U10 | C-26 / REQ-S11 | `UnlockBackoff::record_success()` 呼出で `failures = 0` + `next_allowed_at = None`、直後の `check()` が `Ok(())`（unlock 成功でカウンタリセット）| ユニット | リセット |
| TC-E-U11 | C-27 / REQ-S12 | `From<MigrationError> for IpcErrorCode` 実装で `MigrationError::RecoveryRequired` → `IpcErrorCode::RecoveryRequired` 透過変換、変換後の `Display` 文字列が「recovery path required」を含む（Sub-D Rev5 ペガサス指摘契約の Sub-E 実装、MSG-S09 (a) 経路）| ユニット | 透過変換 |
| TC-E-U12 | REQ-S12 / EC-6 | `MigrationError → IpcErrorCode` マッピング **9 variant 全網羅**: (1) `Crypto(CryptoError::WeakPassword(_))` → `Crypto { reason: "weak-password" } + Feedback`、(2) `Crypto(CryptoError::AeadTagMismatch)` → `Crypto { reason: "aead-tag-mismatch" }`、(3) `Crypto(CryptoError::NonceLimitExceeded)` → `Crypto { reason: "nonce-limit-exceeded" }`、(4) `Crypto(_)` その他 → `Crypto { reason: "kdf-failed" }`、(5) `Persistence(_)` → `Persistence { reason }` 透過、(6) `Domain(_)` → `Domain { reason }` 透過、(7) `AlreadyEncrypted` / `NotEncrypted` / `PlaintextNotUtf8` / `RecoveryAlreadyConsumed` → `Internal { reason }`、(8) `AtomicWriteFailed { stage, .. }` → `Persistence { reason: "atomic-write-{stage}" }`、(9) `RecoveryRequired` → `RecoveryRequired`。**ワイルドカード `_` 無し**で書く（Sub-D Rev3 TC-D-S05 同型）| ユニット | enum 網羅 |
| TC-E-U13 | REQ-S12 / EC-7 | `IpcRequest` V2 5 新 variant（`Unlock` / `Lock` / `ChangePassword` / `RotateRecovery` / `Rekey`）+ `IpcResponse` V2 5 新 variant（`Unlocked` / `Locked` / `PasswordChanged` / `RecoveryRotated { disclosure }` / `Rekeyed { records_count }`）+ `IpcErrorCode` V2 4 新 variant（`VaultLocked` / `BackoffActive { wait_secs }` / `RecoveryRequired` / `ProtocolDowngrade`）の `variant_name()` ヘルパ全網羅、`#[non_exhaustive]` 適用 | ユニット | 列挙 |
| TC-E-U14 | C-28 / REQ-S12 | (Petelgeuse 工程2 Rev1 指摘で意味論誤認訂正) V1 クライアント handshake 完了後（`HandshakeState { client_version: V1 }` を daemon セッション状態に保持）、V2 専用 variant `IpcRequest::Unlock` を受信 → daemon ハンドラが **「`client_version` × `request.variant_name()` の許可リスト」を機械検証** → V1 セッションで V2 専用 variant 検出時に `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` 返却。**`#[non_exhaustive]` 属性は Rust API stability 用で serde 動作とは独立**、実態は handshake 許可リスト方式での拒否（誤認訂正済）| ユニット | 非破壊 + 許可リスト |
| TC-E-U15 | EC-6 / REQ-S09 / S10 / S11 / S12 | i18n 翻訳辞書（`shikomi-cli/i18n/{ja,en}.toml` または同等）経由 MSG-S03（「vault をアンロックしました」+「アイドル 15 分 / スクリーンロック / サスペンド/ スリープで自動的にロック」+ **CLI/GUI 二経路文言**: GUI 経路は「VEK」「daemon」「キャッシュ」「サスペンド」「zeroize」を排除した田中ペルソナ向け平易表現を併記）/ MSG-S04（CLI: 「VEK はメモリから消去」/ GUI: 「鍵情報をメモリから完全に消去」）/ MSG-S05（CLI: 「VEK は不変のため再 unlock は不要」/ GUI: 「レコードはそのまま使えます。再ログインも不要」）/ MSG-S09 (a) パスワード違い（「リカバリ用 24 語（リカバリニーモニック）での復号 (`vault unlock --recovery`) も可能」+ **`MigrationError::RecoveryRequired` 統合** + 平易表現「リカバリ用 24 語」併記）/ MSG-S15（「V1 クライアントは V2 専用機能を使用できません」+「`cargo install --force shikomi-cli`」更新案内）の文言を grep。**ペガサス工程2 Rev1 指摘**で田中ペルソナ向け CLI/GUI 二経路文言確定 | ユニット | 文言 |
| TC-E-U16 | C-26 / REQ-S11 / 服部工程2 Rev1 指摘 | `UnlockBackoff::record_failure` の**呼出経路を「パスワード違い」カテゴリのみに限定**することを機械検証。`MigrationError::Crypto(CryptoError::WrongPassword)` 相当の variant でのみ `record_failure` 発火、以下は backoff カウントしないこと: (a) `Crypto(AeadTagMismatch)` (vault.db 改竄、MSG-S10 経路、L2 攻撃者の DoS 嫌がらせ防衛)、(b) `Crypto(NonceLimitExceeded)` (rekey 必要、MSG-S11 経路)、(c) `Crypto(KdfFailed)` (実装バグ / リソース枯渇)、(d) `Crypto(InvalidMnemonic)` (リカバリ単語不正、別カテゴリ)、(e) `RecoveryRequired` (経路誘導、C-27)、(f) `Persistence(_)` / `Domain(_)` (ストレージ層)。**ワイルドカード `_` 拒否原則を Sub-E backoff トリガにも適用**、`Crypto(_)` 全般を一括カウントする経路は禁止（Sub-D Rev3 ワイルドカード排除原則の Sub-E 段階継承）| ユニット | カテゴリ限定 |
| TC-E-I01 | EC-1 / REQ-S09 / S12 | `MockVaultMigration::unlock_with_password` 成功 → `IpcRequest::Unlock` 送信 → daemon が `cache.unlock(vek)` 実行 → `IpcResponse::Unlocked {}` 返却、その後 `cache.is_unlocked() == true` 維持 + MSG-S03 表示 | 結合 | F-E1 正常 |
| TC-E-I02 | C-26 / REQ-S11 | `MockVaultMigration::unlock_with_password` を 5 回連続 `Err(MigrationError::Crypto(_))` 設定 → 5 回目で `UnlockBackoff::record_failure` × 5 → 6 回目の `IpcRequest::Unlock` 送信時に**ハンドラ入口**で `backoff.check()?` が `Err(BackoffActive { wait_secs: 30 })` → `IpcResponse::Error(BackoffActive { 30 })` 即返却（`MockVaultMigration` は呼ばれない、即拒否経路の確認）| 結合 | F-E1 バックオフ |
| TC-E-I03 | C-27 / REQ-S12 | `MockVaultMigration::unlock_with_password` で `Err(MigrationError::RecoveryRequired)` 設定 → `IpcRequest::Unlock` 送信 → `IpcResponse::Error(IpcErrorCode::RecoveryRequired)` 受信 + i18n 経由文言が「リカバリ経路 (`vault unlock --recovery`) も可能」を含む（Sub-D Rev5 ペガサス指摘契約の Sub-E 実装統合確認）| 結合 | F-E1 リカバリ誘導 |
| TC-E-I04 | C-23 / C-24 / EC-2 | (a) `IpcRequest::Lock` 送信 → `IpcResponse::Locked {}` + `cache.is_unlocked() == false`、(b) **`tokio::time::pause` + `advance(15min + 1s)`** で `IdleTimer` バックグラウンド task が 60 秒ポーリング検出 → `cache.lock()` 自動呼出 → `cache.is_unlocked() == false`、両経路で `Vek` Drop 連鎖 zeroize 観測（メモリパターン `[0u8; 32]`）| 結合 | F-E2 lock 2 経路 |
| TC-E-I05 | C-25 / EC-2 | `MockLockSignal` から (a) `LockEvent::ScreenLocked`、(b) `LockEvent::SystemSuspended` 注入 → 各々 100ms 以内に `cache.is_unlocked() == false` 観測（`tokio::time::Instant` で計時）| 結合 | F-E2 OS シグナル |
| TC-E-I06 | EC-3 / EC-4 / EC-5 / REQ-S10 / S12 | Unlocked 状態で IPC V2 5 variant 全ラウンドトリップ: (a) `Unlock` → `Unlocked`（既に Unlocked のため `Internal { reason: "already-unlocked" }` 拒否経路含む）、(b) `Lock` → `Locked`、(c) `ChangePassword { old, new }` → `PasswordChanged` + cache 維持（`is_unlocked` true 継続）、(d) `RotateRecovery { master_password }` → `RecoveryRotated { disclosure: RecoveryWords }` で**新 24 語を初回 1 度のみ**返却（disclosure は IPC 応答で受信側が即 Drop）、(e) `Rekey { master_password }` → `Rekeyed { records_count }` + `cache.lock → cache.unlock(new_vek)` で cache 更新 | 結合 | V2 全 5 variant 往復 |
| TC-E-I07 | C-28 / REQ-S12 / EC-6 | (a) `MockClient { handshake_version: V1 }` で接続成功（V1 サブセット `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord` のみ受理）、(b) V1 接続後に `IpcRequest::Unlock`（V2 専用）を MessagePack 送信 → daemon ハンドラの `check_request_allowed(V1, "unlock")` が `Err(ProtocolDowngrade)` 返却（許可リスト方式での拒否、TC-E-U14 の結合層対応）→ `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` 受信 + i18n 経由 MSG-S15「V1 クライアントは V2 専用機能を使用できません」「`cargo install --force shikomi-cli` で最新版に更新」を含む | 結合 | V1 非破壊 |
| TC-E-I08 | 服部工程2 Rev1 指摘 / C-29 候補 / REQ-S12 | **rekey 後の `wrapped_vek_by_recovery` 整合性破壊ウィンドウ封鎖**を結合層で検証: (a) 平文 vault → `vault encrypt` で 2 経路 wrap (`wrapped_vek_by_pw` + `wrapped_vek_by_recovery`) 生成、(b) `IpcRequest::Rekey { master_password }` 実行 → 新 VEK + 全レコード再暗号化、(c) **rekey 完了直後の `wrapped_vek_by_recovery` の状態を確認**: 設計判断 (A) atomic 化選択時 → 旧 mnemonic で unwrap 不能（atomic write 1 トランザクションで両方更新）、設計判断 (B) invalidate マーカー方式選択時 → recovery 経路 unlock 試行で `Err(MigrationError::RecoveryRequired)` または専用 variant で MSG-S09 (a) + 「`vault rotate-recovery` 実行を推奨」誘導文言、(d) いずれの方式でも **rekey 後の旧 mnemonic 経路 unlock で AEAD tag mismatch (MSG-S10「改竄の可能性」) が発火しない**ことを確認（正規ユーザを「vault 改竄」と誤認させない、Fail Kindly 維持）。Sub-F 丸投げ禁止、Sub-E 結合層で整合性確認 | 結合 | 整合性ウィンドウ封鎖 |
| TC-E-I09 | 服部工程2 Rev1 指摘 / C-29 候補 / REQ-S12 | **handshake バイパス拒否（C-29 候補）**: (a) クライアントが handshake をスキップしていきなり `IpcRequest::ListRecords` / `AddRecord` / `Unlock` 等を送信 → daemon は `HandshakeState` を初期 `NotEstablished` で保持しており、`check_request_allowed` が **handshake 必須契約**を機械検証 → `Err(IpcErrorCode::HandshakeRequired)` または `ProtocolDowngrade` 返却、(b) handshake 後でも他クライアントの session_id を spoofing する経路は localhost UDS / Named Pipe + UID 一致で L1 同ユーザ範囲に閉じる（脅威モデル §4 整合）。**handshake 完了前は全 IPC variant 拒否**を結合層で検証 | 結合 | handshake 必須 |
| TC-E-P01 | C-25 / REQ-S09 | proptest で任意の `LockEvent` シーケンス（`{ScreenLocked, SystemSuspended}` の 1..=10 件、間隔 0..=500ms）を `MockLockSignal` から注入 → **全ケースで `cache.is_unlocked() == false` への遷移が 100ms 以内**（1000 ケース、Sub-C/D と同型 ProptestConfig::with_cases(1000)）| property | OS シグナル不変条件 |
| TC-E-E01 | 全契約 / Sub-F 統合 | 田中（経理担当者）が CLI シナリオを完走: `shikomi vault unlock` 入力 → MSG-S03 表示 → `shikomi list` で業務 → アイドル 15 分放置 → 自動 lock + 次操作で MSG-S09 (c) キャッシュ揮発タイムアウト → `shikomi vault unlock` 再入力 → `shikomi vault change-password` → MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」の文言確認 | E2E | 田中ペルソナ |

### 14.5 Sub-E ユニットテスト詳細

#### `VekCache` + `VaultUnlockState` 型遷移（C-22 / C-23）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-E-U01 | `let cache = VekCache::new(); cache.is_unlocked()` | `false`（初期 Locked、read/write IPC は型レベル拒否の起点）|
| TC-E-U02 | `cache.unlock(Vek::from_array([1u8;32]))?; cache.with_vek(\|v\| v.expose_within_crate().to_vec())` | 戻り値が `[1u8; 32]` 一致、`cache.is_unlocked() == true`、`with_vek` 呼出で `last_used` が `Instant::now()` に更新 |
| TC-E-U03 | (1) `cache.unlock(Vek::from_array([0xAB; 32]))?;` (2) cache 内 `Vek` の生バイト位置を `unsafe` で記録（テスト専用 hook）(3) `cache.lock()?;` (4) 旧バイト位置を再観測 | 旧 `Vek` 32B が `[0x00; 32]` または retrieve 不能（`Vek::Drop` の `Zeroize::zeroize` 連鎖発火、Sub-A C-1 維持）|
| TC-E-U04 | (1) `let r = cache.with_vek(\|v\| 42)`（Locked 時）(2) `cache.unlock(vek)?;` (3) `let r = cache.with_vek(\|v\| 42)`（Unlocked 時） | (1) `Err(CacheError::VaultLocked)` (2) `Ok(42)` + `last_used` 更新確認（呼出前後の `Instant` 差 > 0）|
| TC-E-U05 | `match state: VaultUnlockState { Locked => "locked", Unlocked { vek: _, last_used: _ } => "unlocked", }` を**ワイルドカード `_` 無し**で書く | `cargo check` 警告 0 件（**2 variant 全網羅**）+ `#[non_exhaustive]` 適用。defining crate 内では `_` なしで exhaustive、将来 variant 追加時は本テストが先に壊れる。**ワイルドカード排除は TC-E-S01 grep gate で機械強制**（Sub-D Rev4 TC-D-S07 同型構造防衛）|
| TC-E-U06 | (1) `serde_json::to_string(&state)` (2) `state.clone()` (3) `let s = state; let _ = s; let _ = s;`（move 後再使用）(4) `format!("{state}")` | (1)..(4) 全て **compile_fail**（`Serialize` / `Clone` / `Display` 未実装、move 後再使用は所有権消費、Sub-A `Vek` 禁止トレイト連鎖が `VaultUnlockState` に伝播）|

#### `CacheError` 列挙（EC-7）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-E-U07 | `match err: CacheError { VaultLocked => "locked", AlreadyUnlocked => "already-unlocked", }` を**ワイルドカード `_` 無し**で書く | `cargo check` 警告 0 件（**2 variant 全網羅**）+ `#[non_exhaustive]` 適用。Sub-D `MigrationError` の Rev3 TC-D-S05 同型「実装直読 SSoT」原則を Sub-E 段階で継承 |

#### `UnlockBackoff` 指数バックオフ（C-26）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-E-U08 | (1) `let mut b = UnlockBackoff::default();` (2) `for _ in 0..4 { b.record_failure(); }` (3) `b.check()` (4) `b.record_failure();` (5) `b.check()` | (3) `Ok(())`（4 回までバックオフ非発動）(5) `Err(IpcErrorCode::BackoffActive { wait_secs: 30 })`（5 回目で発動、30 秒）|
| TC-E-U09 | `b.record_failure()` を計 6/7/8/9/10/11/12 回呼出後、各 `b.check()` の `wait_secs` を取得 | 6→60s, 7→120s, 8→240s, 9→480s, 10→960s, 11→1920s, **12→3600s（最大クランプ）**、`failures >= 12` でも常に `wait_secs == 3600`（指数増加 + 1 時間 hard クランプ）|
| TC-E-U10 | (1) 5 回失敗 → `BackoffActive` 状態 (2) `b.record_success()` (3) `b.check()` (4) `b.record_failure()` × 4 → `b.check()` | (3) `Ok(())`（カウンタゼロリセット）(4) `Ok(())`（リセット後の 4 回目までは Ok）|

#### `MigrationError → IpcErrorCode` マッピング（C-27 / REQ-S12）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-E-U11 | `let e: IpcErrorCode = MigrationError::RecoveryRequired.into(); format!("{e}")` | `IpcErrorCode::RecoveryRequired` variant、`Display` 文字列が「recovery path required」を含む（C-27 透過、Sub-D Rev5 ペガサス指摘契約の Sub-E 実装）|
| TC-E-U12 | `match err: MigrationError { Crypto(WeakPassword(_)) ⇒ ..., Crypto(AeadTagMismatch) ⇒ ..., Crypto(NonceLimitExceeded) ⇒ ..., Crypto(_) ⇒ ..., Persistence(_) ⇒ ..., Domain(_) ⇒ ..., AlreadyEncrypted ⇒ ..., NotEncrypted ⇒ ..., PlaintextNotUtf8 ⇒ ..., RecoveryAlreadyConsumed ⇒ ..., AtomicWriteFailed { stage, .. } ⇒ ..., RecoveryRequired ⇒ ..., }` を**ワイルドカード `_` 無し**（`Crypto(_)` 内部の網羅は別 match で詳細化）で書き、各分岐が期待 `IpcErrorCode` variant を返す | `cargo check` 警告 0 件（**Sub-D 9 variant + `Crypto` 内 4 variant 全網羅**、9→4 集約完成）|

#### IPC V2 variant + Wire-format（REQ-S12 / EC-7 / C-28）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-E-U13 | `IpcRequest::Unlock { master_password, recovery: None }.variant_name()` 等を全 V2 variant で呼出、`#[non_exhaustive]` 適用確認 | `"unlock" / "lock" / "change_password" / "rotate_recovery" / "rekey"` + `IpcResponse` `"unlocked" / "locked" / "password_changed" / "recovery_rotated" / "rekeyed"` + `IpcErrorCode` `"vault_locked" / "backoff_active" / "recovery_required" / "protocol_downgrade"`、各 enum に `#[non_exhaustive]` 維持 |
| TC-E-U14 | (1) daemon が `HandshakeState { client_version: V1 }` セッション状態を `IpcRequest::Handshake { client_version: V1 }` 受信時に保存 (2) 同一セッションで `IpcRequest::Unlock { .. }` を受信 (3) daemon ハンドラ入口の **`fn check_request_allowed(client_version, request) -> Result<(), IpcErrorCode>`** で許可リスト検証: V1 許可セット = `{Handshake, ListRecords, AddRecord, EditRecord, RemoveRecord}`、V2 専用セット = `{Unlock, Lock, ChangePassword, RotateRecovery, Rekey}` | (3) `Err(IpcErrorCode::ProtocolDowngrade)` 返却（V1 セッション × V2 専用 variant の組合せが許可リスト不一致、handshake 許可リスト方式の境界検証）。**Petelgeuse 工程2 Rev1 指摘で誤認訂正**: `#[non_exhaustive]` は Rust API stability 用、serde の `unknown variant` 拒否動作とは独立。本 TC が検証するのは daemon 側の許可リスト機械検証であり、serde の挙動ではない |

#### MSG 文言（EC-6）

| テストID | 検証手段 | 期待結果 |
|---|---|---|
| TC-E-U15 | i18n 翻訳辞書 `msg-s03.ja.txt` / `msg-s04.ja.txt` / `msg-s05.ja.txt` / `msg-s09.ja.txt` / `msg-s15.ja.txt` を grep | (S03) 「vault をアンロックしました」+「アイドル 15 分」+「自動的にロック」**含有**、(S04) 「vault をロックしました」+「VEK はメモリから消去」**含有**、(S05) 「VEK は不変のため再 unlock は不要」+「daemon キャッシュも維持」**含有**、(S09 a) 「リカバリニーモニックでの復号」+「`vault unlock --recovery`」**含有** + 「次の試行可能まで N 秒」**含有**、(S15) 「V1 クライアントは V2 専用機能を使用できません」+「`cargo install --force shikomi-cli`」**含有** |

### 14.6 Sub-E 結合テスト詳細

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-E-I01 | `cargo test -p shikomi-daemon --test ipc_v2_integration unlock_round_trip` | `MockVaultMigration` 成功シナリオで `IpcRequest::Unlock` → `IpcResponse::Unlocked` + `cache.is_unlocked() == true` 維持、`backoff.failures == 0`（成功でリセット）|
| TC-E-I02 | 同 `unlock_backoff_after_5_failures` | 5 回 `MockVaultMigration::unlock_with_password` 失敗注入 → 6 回目の `IpcRequest::Unlock` 送信時に**ハンドラ入口** `backoff.check()` で `Err(BackoffActive { wait_secs: 30 })` → `MockVaultMigration` は呼ばれない（即拒否観測）|
| TC-E-I03 | 同 `unlock_recovery_required_path` | `MockVaultMigration::unlock_with_password` で `Err(MigrationError::RecoveryRequired)` → `IpcResponse::Error(IpcErrorCode::RecoveryRequired)` 受信 + i18n 経由 MSG-S09 (a) 文言確認（C-27 統合）|
| TC-E-I04 | 同 `lock_explicit_and_idle_timeout` | (a) `IpcRequest::Lock` → `IpcResponse::Locked` + `cache.is_unlocked() == false`、(b) `tokio::time::pause + advance(Duration::from_secs(15*60 + 1))` → `IdleTimer` 60 秒ポーリング検出 → `cache.lock()` 自動呼出 → `cache.is_unlocked() == false`、両経路で旧 VEK 32B が zeroize（メモリパターン `[0u8; 32]`）|
| TC-E-I05 | 同 `os_lock_signal_within_100ms` | `MockLockSignal::send(LockEvent::ScreenLocked)` 注入 → `tokio::time::Instant::now()` 経過時間 < 100ms で `cache.is_unlocked() == false`、`SystemSuspended` でも同等（C-25）|
| TC-E-I06 | 同 `v2_handlers_round_trip_5_variants` | (a) Unlock 既に Unlocked 状態で送信 → `Internal { reason: "already-unlocked" }`、(b) Lock → Locked、(c) ChangePassword → PasswordChanged + `cache.is_unlocked() == true` 維持（VEK 不変、O(1)、REQ-S10）、(d) RotateRecovery → `RecoveryRotated { disclosure: RecoveryWords }` 受信、`disclosure` は受信側で即 Drop 観測（zeroize）、(e) Rekey → `Rekeyed { records_count: N }` + `cache` の VEK が新 VEK に置換（旧 VEK Drop 連鎖 zeroize → 新 VEK で `cache.unlock`）|
| TC-E-I07 | 同 `v1_client_protocol_downgrade` | `MockClient { handshake_version: V1 }` 接続後、`IpcRequest::Unlock { .. }` を MessagePack 送信 → daemon は V1 接続として deserialize、V2 専用 variant 検出 → `IpcResponse::Error(IpcErrorCode::ProtocolDowngrade)` 返却 + 後続接続継続（強制切断ではなく拒否のみ、V1 既存 variant は引き続き受理）|

### 14.7 Sub-E property テスト詳細

| テストID | 入力空間 | invariant |
|---|---|---|
| TC-E-P01 | `MockLockSignal` から `proptest::collection::vec(prop_oneof![Just(LockEvent::ScreenLocked), Just(LockEvent::SystemSuspended)], 1..=10)` ＋ 各イベント間隔 `0..=500ms`、cache は事前に `Unlocked` 状態 | 全シーケンスで**最初の `LockEvent` 受信から 100ms 以内に `cache.is_unlocked() == false`**（1000 ケース、契約 C-25、Sub-C TC-C-P02 / Sub-D TC-D-P01 と同型 ProptestConfig::with_cases(1000)）|

### 14.8 Sub-E E2E テストケース

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---|---|---|---|---|
| TC-E-E01 | 田中 一郎（経理担当者、`requirements-analysis.md` ペルソナ）| daemon 経由で日常業務（unlock → 業務 → 自動 lock → 再 unlock → change-password）を完走、各段階で正しい MSG が表示される | (1) `shikomi-daemon` 起動 (2) `shikomi vault unlock` 入力 → MSG-S03 表示確認「vault をアンロックしました」+「アイドル 15 分」(3) `shikomi list` で業務（複数レコード閲覧）(4) 端末を 15 分放置 → 次操作で MSG-S09 (c) キャッシュ揮発タイムアウト「アイドル 15 分でロックしました、再度 `vault unlock` してください」(5) `shikomi vault unlock` 再入力 → MSG-S03 (6) `shikomi vault change-password` で旧 / 新パスワード入力 → MSG-S05「VEK は不変のため再 unlock は不要、daemon キャッシュも維持」確認 (7) `shikomi list` で再 unlock なしに業務継続できる | 6 ステップ完走、MSG-S03 / S09 (c) / S05 が正しい順序で表示、step (7) で `cache.is_unlocked()` 相当の状態が継続（Sub-F CLI 実装後に E2E 実行可能、Sub-E 工程5 で詳細化）|

### 14.9 Sub-E 静的検査（grep gate）

Sub-D Rev3 / Rev4 で凍結した「**実装直読 SSoT + grep gate による設計書-実装一致機械検証**」原則を Sub-E に継承。**4 度目以降の同型ドリフト**を Sub-E 段階で構造封鎖する。

| テストID | 検証対象 | grep ロジック | 失敗時 |
|---|---|---|---|
| TC-E-S01 | C-22 ワイルドカード排除 | `crates/shikomi-daemon/src/ipc/v2_handler/{unlock,lock,change_password,rotate_recovery,rekey}.rs` 内の `match cache.state()` / `match state` / `match VaultUnlockState` ブロックに bare wildcard arm `^[[:space:]]+_[[:space:]]*=>` が存在しないことを awk + grep で機械検証（Sub-D TC-D-S07 同型）| FAIL + 行番号 + remediation「2 variant exhaustive match を完成させ '_ => ...' arm を削除せよ」|
| TC-E-S02 | EC-7 `VaultUnlockState` variant 集合整合 | `awk` で `pub enum VaultUnlockState { ... }` から variant 名抽出（`Locked` / `Unlocked`）→ 期待集合と完全一致比較（Sub-D TC-D-S05 同型）| FAIL + impl set / expected set 表示 |
| TC-E-S03 | EC-7 `IpcRequest` variant 集合整合 | `awk` で `pub enum IpcRequest { ... }` から variant 名抽出 → V1 既存 5 件（`Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord`）+ V2 新 5 件（`Unlock` / `Lock` / `ChangePassword` / `RotateRecovery` / `Rekey`）= **10 variants** と完全一致比較 | FAIL + 集合 diff |
| TC-E-S04 | EC-7 `IpcResponse` variant 集合整合 | 同様に V1 既存 7 件（`Handshake` / `ProtocolVersionMismatch` / `Records` / `Added` / `Edited` / `Removed` / `Error`）+ V2 新 5 件（`Unlocked` / `Locked` / `PasswordChanged` / `RecoveryRotated` / `Rekeyed`）= **12 variants** | FAIL + 集合 diff |
| TC-E-S05 | EC-7 `IpcErrorCode` V2 4 新 variant 含有 | 同様に `IpcErrorCode` から V2 新 4 件（`VaultLocked` / `BackoffActive` / `RecoveryRequired` / `ProtocolDowngrade`）が存在することを confirm（既存 V1 variant の retain も同時確認、count は SSoT に従う）| FAIL + 集合 diff |
| TC-E-S06 | EC-8 Clean Arch 維持 | shikomi-core / shikomi-infra に OS API（`DistributedNotificationCenter` / `WTSRegisterSessionNotification` / `zbus::` / `dbus::` / `objc::` / `windows::Win32::System::RemoteDesktop`）の直接 import 不混入（grep -rE）、shikomi-daemon 内のみで完結 | FAIL + import 漏洩箇所列挙、Sub-A/B/C/D 累積 Clean Arch 契約の Sub-E 段階での回帰検証 |
| TC-E-S07 | C-28 / C-29 候補 / handshake 許可リスト境界 / Petelgeuse 工程2 Rev1 指摘 | (a) shikomi-daemon に **`fn check_request_allowed(client_version, request) -> Result<(), IpcErrorCode>`** または同等関数が存在することを grep 検証、(b) 該当関数本体に V1 許可セット（`Handshake` / `ListRecords` / `AddRecord` / `EditRecord` / `RemoveRecord`）と V2 専用セット（`Unlock` / `Lock` / `ChangePassword` / `RotateRecovery` / `Rekey`）が**明示列挙**されていること、(c) 関数本体に `ProtocolDowngrade` または同等の拒否経路が存在、(d) handshake 未完了時の全 variant 拒否経路（`HandshakeState::NotEstablished` 等）が存在することを awk + grep で機械検証。**「`#[non_exhaustive]` の serde 経路保護」誤認実装が紛れ込む経路を構造封鎖**（Petelgeuse 工程2 Rev1 で指摘した意味論誤認の実装段階での再発防止、Sub-D Rev4 TC-D-S07 ワイルドカード gate 同型）| FAIL + 関数欠落 / variant 列挙不足 / 拒否経路不在を行番号付き列挙、remediation「handshake 許可リスト関数を追加し、V1/V2 variant 集合を明示列挙、`HandshakeState::NotEstablished` で全拒否経路を実装せよ」|

これらは `tests/docs/sub-e-static-checks.sh` で実装する（Sub-D TC-D-S01..S07 と同パターン、Sub-E は **TC-E-S01..S07 の 7 件**）。**Sub-D Rev3 / Rev4 で凍結した実装直読 + 機械検証**原則を Sub-E から継承し、`Locked → Unlocked → Locked` 型遷移の意味論ドリフト + handshake 許可リストの誤認実装ドリフトを Petelgeuse 5 度目を待たず構造封鎖する。**Petelgeuse 工程2 Rev1 指摘** で「`#[non_exhaustive]` の serde 経路保護」誤認を訂正、TC-E-S07 を新設して handshake 許可リストの境界を機械検証。

### 14.10 Sub-E テスト実行手順

```bash
# Rust unit + integration tests
cargo test -p shikomi-core --lib ipc::tests
cargo test -p shikomi-daemon --lib cache::vek::tests
cargo test -p shikomi-daemon --lib cache::lifecycle::tests
cargo test -p shikomi-daemon --lib backoff::unlock::tests
cargo test -p shikomi-daemon --lib ipc::v2_handler::tests
cargo test -p shikomi-daemon --test ipc_v2_integration

# property tests (1000 ケース、Sub-C/D と同型 ProptestConfig::with_cases(1000))
cargo test -p shikomi-daemon --test ipc_v2_property

# Sub-E 静的検証 (cargo 不要、TC-E-S01..S07、Rev1 で S07 handshake 許可リスト gate 追加)
bash tests/docs/sub-e-static-checks.sh

# 既存 Sub-A/B/C/D static checks 再実行（回帰防止）
bash tests/docs/sub-a-static-checks.sh
bash tests/docs/sub-b-static-checks.sh
bash tests/docs/sub-c-static-checks.sh
bash tests/docs/sub-d-static-checks.sh

# Sub-0 lint / cross-ref（回帰防止）
python3 tests/docs/sub-0-structure-lint.py
bash tests/docs/sub-0-cross-ref.sh

# 横断: daemon-ipc V2 round-trip も再実行（IpcRequest/IpcResponse/IpcErrorCode の V2 variant 追加で TC-IT-021..025 想定、Sub-E 工程3 で同期追加）
cargo test -p shikomi-daemon --test ipc_integration
```

### 14.11 Sub-E テスト証跡

- `cargo test -p shikomi-daemon --test ipc_v2_integration` の stdout（unit + integration + property pass 件数 + idle/OS-signal の計時 + backoff の指数増加観測）
- 静的検証スクリプト stdout（`sub-e-static-checks.sh` **7 件: TC-E-S01..S07**、Sub-D Rev4 同型構造防衛 + Rev1 で S07 handshake 許可リスト境界 gate 追加）
- proptest 失敗時の minimization 出力（あれば）
- daemon-ipc 横断 regression 結果（TC-IT-021..025 想定の V2 variant ラウンドトリップ pass）
- 全て `/app/shared/attachments/マユリ/sub-e-*.txt` に保存し Discord 添付

### 14.12 後続 Sub-F への引継ぎ（Sub-E から派生）

| Sub | 本ファイル §14 拡張時の追加内容 |
|---|---|
| Sub-F (#44) | (a) `vault encrypt` / `vault decrypt` / `vault unlock` / `vault lock` / `vault change-password` / `vault rotate-recovery` / `vault rekey` CLI サブコマンド経路で Sub-E IPC V2 ハンドラを呼出、(b) `vault decrypt` の二段確認（Sub-D TC-D-U09/U10 移譲分: `subtle::ConstantTimeEq` 比較 + paste 抑制 + 大文字検証 + パスワード再入力）→ 通過時に `DecryptConfirmation::confirm()` 呼出 → IPC 経由で `decrypt_vault` 呼出、(c) MSG-S07（rekey 完了レコード数 = `Rekeyed.records_count`）/ MSG-S11（nonce 上限到達文言）/ MSG-S14（DECRYPT 確認モーダル）/ MSG-S18（リカバリ表示アクセシビリティ PDF/braille/audio）の文言確定、(d) `shikomi list` ヘッダ `[plaintext]` / `[encrypted]` バナー（REQ-S16）、(e) E2E TC-E-E01 の Sub-F CLI 実装後に `tokio::test` 経由 in-process 統合再現 |

### 14.13 Sub-E 工程4 実施実績

工程4 完了後、Sub-E 実装担当（坂田銀時想定）+ テスト担当（涅マユリ想定）が本ファイルを READ → EDIT で実績を追記する。雛形は Sub-A §10.11 / Sub-B §11.11 / Sub-C §12.12 / Sub-D §13.12 に従う。**Sub-A〜D で観測したパターン**: 銀ちゃんは設計書の proptest / criterion bench / KAT 件数等を**単発 fixture で省略する傾向**、セルは設計書の variant 数を**断定的に記述してドリフト**させる傾向、いずれも実装直読 + grep gate で構造封鎖する（Bug-A-001 / Bug-B-001 / Bug-C-001 / Bug-D-007 連鎖、Petelgeuse Rev1〜Rev4 連続指摘の Sub-E 段階での予防）。
