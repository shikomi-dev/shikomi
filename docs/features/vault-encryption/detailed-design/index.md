# 詳細設計書（インデックス）

<!-- 基本設計書とは別ディレクトリ。統合禁止 -->
<!-- feature: vault-encryption / Epic #37 -->
<!-- 配置先: docs/features/vault-encryption/detailed-design/index.md -->
<!-- 本ディレクトリは Sub-A (#39) Rev1 で `detailed-design.md` を分割した結果。
     Sub-B〜F の本文は各 Sub の設計工程で本ディレクトリ内の各分冊を READ → EDIT で追記する。
     新規分冊が必要になった場合は本 index.md からの索引を更新すること。 -->

## 記述ルール（必ず守ること）

詳細設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

## 分冊構成（Sub-E 完了時点、**Sub-D の 7 分冊 + vek-cache-and-ipc.md 新設で 8 分冊**）

| 分冊 | 主担当範囲 | 主な対象型・契約 |
|-----|---------|--------------|
| [`crypto-types.md`](./crypto-types.md) | 鍵階層型（Sub-A） | `Vek` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` / `HeaderAeadKey`（**Sub-C で `AeadKey` impl 追加 Boy Scout**） |
| [`password.md`](./password.md) | パスワード認証境界（Sub-A trait + Sub-B `ZxcvbnGate` 実装） | `MasterPassword` / `PasswordStrengthGate` trait / `WeakPasswordFeedback`（**`warning=None` 契約 + i18n 責務分離**） / **`ZxcvbnGate`（Sub-B 新規）** |
| [`nonce-and-aead.md`](./nonce-and-aead.md) | nonce / AEAD 境界（Sub-A 型 + **Sub-C 実装結合 + `AeadKey` trait + `AesGcmAeadAdapter`**） | `NonceCounter`（責務再定義） / `NonceBytes::from_random` / `WrappedVek` / `AuthTag` / `Verified<T>` / `Plaintext` / `verify_aead_decrypt`（**呼び出し側主張マーカー契約 + 可視性 `pub(in crate::crypto::verified)`**） / **`AeadKey` trait（Sub-C、クロージャインジェクション）** / **`AesGcmAeadAdapter`（Sub-C、`encrypt_record` / `decrypt_record` / `wrap_vek` / `unwrap_vek` 4 メソッド + NIST CAVP KAT + AAD 26B 規約 + nonce_counter 統合契約）** |
| [`errors-and-contracts.md`](./errors-and-contracts.md) | エラー型 / リカバリ / 契約サマリ（Sub-A 型 + Sub-B `KdfErrorKind` 詳細 + `InvalidMnemonic` variant + **Sub-C `AeadTagMismatch` 発火経路 + `derive_new_wrapped_*` AES-GCM wrap 経路 + `unwrap_vek_with_*`**） | `RecoveryMnemonic` / `CryptoOutcome<T>` / `CryptoError` / `DomainError` 拡張 / `VekProvider`（**Sub-B 具象 `Argon2idHkdfVekProvider` + Sub-C で wrap/unwrap 経路確定**） / 設計判断の補足 / **契約 C-1〜C-16 サマリ表（Sub-C で C-14〜C-16 追加）** |
| **[`kdf.md`](./kdf.md)（Sub-B 新規）** | KDF アダプタ（shikomi-infra） | `Argon2idAdapter`（`m=19456, t=2, p=1`、RFC 9106 KAT、criterion p95 1 秒） / `Bip39Pbkdf2Hkdf`（24 語 → seed → KEK_recovery、HKDF info `b"shikomi-kek-v1"`、trezor + RFC 5869 KAT） / `Argon2idParams::FROZEN_OWASP_2024_05` const |
| **[`rng.md`](./rng.md)（Sub-B 新規）** | CSPRNG 単一エントリ点（shikomi-infra） | `Rng`（`rand_core::OsRng` + `getrandom` バックエンド） / `generate_kdf_salt` / `generate_vek` / `generate_nonce_bytes` / `generate_mnemonic_entropy`（Sub-0 凍結文言「KdfSalt::generate() 単一コンストラクタ」の Clean Arch 整合的物理実装） |
| **[`repository-and-migration.md`](./repository-and-migration.md)（Sub-D 新規）** | 暗号化 Vault リポジトリ + 平文⇄暗号化マイグレーション（shikomi-infra） | `VaultMigration` service（`encrypt_vault` / `decrypt_vault` / `unlock_with_*` / `rekey` / `change_password` の 6 メソッド）/ `VaultEncryptedHeader`（`KdfParams` + `HeaderAeadEnvelope` 含む）/ `EncryptedRecord` / `RecoveryDisclosure` + `RecoveryWords`（24 語初回 1 度表示の型レベル強制 C-19）/ `DecryptConfirmation`（型レベル二段確認証跡 C-20）/ `MigrationError` 9 variants / `Aad::HeaderEnvelope(Vec<u8>)` 拡張 / SQLite DDL 拡張（`kdf_params` / `header_aead_*` カラム、`PRAGMA user_version` bump） |
| **[`vek-cache-and-ipc.md`](./vek-cache-and-ipc.md)（Sub-E 新規）** | VEK キャッシュ + IPC V2 拡張（shikomi-daemon、横断的変更で daemon-ipc feature と双方向同期） | `VekCache` + `VaultUnlockState` enum（`Locked` / `Unlocked { vek, last_used }`、`#[non_exhaustive]`、契約 C-22〜C-25）/ `IdleTimer`（60 秒ポーリング、アイドル 15 分タイムアウト）/ `OsLockSignal` trait（`#[cfg]` 分割、macOS / Windows / Linux 具象実装 + Mock）/ `UnlockBackoff`（連続失敗 5 回で指数バックオフ、契約 C-26）/ IPC V2 5 新 variant ハンドラ（unlock / lock / change_password / rotate_recovery / rekey）/ `MigrationError → IpcError` マッピング（9 → 4 集約、`RecoveryRequired → MSG-S09(a)` 統合、契約 C-27）/ V1 クライアント拒否（`ProtocolDowngrade`、契約 C-28） |

**Sub-C で新規分冊を追加しない理由**: AEAD 設計は既存 `nonce-and-aead.md` の延長線上にある（`Verified<T>` / `Plaintext` / `verify_aead_decrypt` クロージャマーカーがすべて Sub-C `AesGcmAeadAdapter` に直結）。`aead-adapter.md` を別ファイルに切り出すと **Verified<T> 契約と AEAD 実装が物理的に分離**し、設計の縦串整合（型契約 → 実装具象）が崩れる。1 分冊 400 行以内のソフトキャップを超えない範囲で `nonce-and-aead.md` 内に集約する判断（Boy Scout Rule、不要な分冊増加を回避）。

**Sub-D で新規分冊 `repository-and-migration.md` を追加する理由**: 暗号化 Vault リポジトリ + 双方向マイグレーション + ヘッダ AEAD タグ + RecoveryDisclosure + DecryptConfirmation の 5 軸が同一責務領域（Sub-A/B/C で凍結された型を**消費する側のサービス層**）を構成。既存 6 分冊（鍵階層 / パスワード / nonce-AEAD / エラー契約 / KDF / RNG）はすべて**型・契約レベルの分冊**であり、サービス層の意味論を既存分冊に詰め込むと**型契約とサービス実装が混在して設計の縦串整合が崩れる**。Sub-C の判断（既存分冊延長で集約）と対称的に、Sub-D は**新規責務領域**のため新規分冊が正しい。`repository-and-migration.md` は約 470 行（5 軸 × 6 メソッド + 横断的変更で 400 行ソフトキャップを意図的に許容超過、これ以上の分割は責務横断で意味論が分散するため不採用）。

```
ディレクトリ構造:
docs/features/vault-encryption/detailed-design/
  index.md                   # 本ファイル（分冊索引 + 全 public API クラス図 + データ構造表）
  crypto-types.md            # Vek / Kek<Kind> / HeaderAeadKey
  password.md                # MasterPassword / PasswordStrengthGate / WeakPasswordFeedback / ZxcvbnGate
  nonce-and-aead.md          # Verified<T> / Plaintext / NonceCounter / NonceBytes / WrappedVek / AuthTag
  errors-and-contracts.md    # RecoveryMnemonic / CryptoError / CryptoOutcome / VekProvider 具象 / 契約サマリ
  kdf.md                     # Argon2idAdapter / Bip39Pbkdf2Hkdf / Argon2idParams const  [Sub-B 新規]
  rng.md                     # Rng (OsRng 単一エントリ点) / generate_*  [Sub-B 新規]
```

**分割方針**:

- 1 分冊あたり原則 **400 行以内**を目標とし、超過したら更に分割を検討（Sub-B〜F 拡張への余地を確保）
- **サフィックス分割禁止**（`crypto-types-vek.md` のような形は不可）。型グループでファイル分け
- 各分冊は冒頭に「対象型・主担当 Sub」を明示し、独立して読めるようにする（Boy Scout Rule の継承）

## クラス設計（詳細・全 public API 統合 Mermaid 図）

各型のメソッドシグネチャ詳細は分冊参照。本図は型相互の関係を示す全体俯瞰。

```mermaid
classDiagram
    class Vek {
        +SecretBox~Zeroizing[u8;32]~
        +from_array(bytes)
        +Drop : zeroize 32B
        +Clone : NOT IMPLEMENTED
    }
    class Kek~Kind~ {
        +SecretBox~Zeroizing[u8;32]~
        +PhantomData~Kind~
        +from_array(bytes)
        +Drop : zeroize 32B
        +Clone : NOT IMPLEMENTED
    }
    class KekKindPw {
        <<phantom marker>>
    }
    class KekKindRecovery {
        <<phantom marker>>
    }
    class HeaderAeadKey {
        +SecretBox~Zeroizing[u8;32]~
        +from_kek_pw(kek)
        +Drop : zeroize 32B
        +Clone : NOT IMPLEMENTED
    }
    class MasterPassword {
        +SecretBytes
        +new(s, gate) Result
        +Drop : zeroize
        +Clone : NOT IMPLEMENTED
    }
    class PasswordStrengthGate {
        <<trait>>
        +validate(s) Result
        +dyn-safe
    }
    class WeakPasswordFeedback {
        +warning Option_String
        +suggestions Vec_String
        +Sub-D fallback契約
        +Sub-D i18n責務
    }
    class RecoveryMnemonic {
        +SecretBox~Zeroizing[String;24]~
        +from_words(words) Result
        +Drop : zeroize each word
        +Clone : NOT IMPLEMENTED
    }
    class Plaintext {
        +SecretBytes
        +pub_in_crate_crypto_verified
        +Drop : zeroize
        +Clone : NOT IMPLEMENTED
    }
    class Verified~T~ {
        +inner T
        +pub_crate_constructor
        +Caller_Asserted_Marker
        +into_inner() T
        +Clone : NOT IMPLEMENTED
    }
    class CryptoOutcome~T~ {
        <<enumeration>>
        +TagMismatch
        +NonceLimit
        +KdfFailed
        +WeakPassword
        +Verified
        +non_exhaustive
    }
    class NonceCounter {
        +count u64
        +increment Result
        +current u64
        +LIMIT 2_pow_32
        +must_use
    }
    class NonceBytes {
        +inner [u8;12]
        +from_random
        +try_new
    }
    class WrappedVek {
        +ciphertext Vec_u8
        +nonce NonceBytes
        +tag AuthTag
        +new Result
        +into_parts
    }
    class AuthTag {
        +inner [u8;16]
        +try_new
    }

    Kek~Kind~ ..> KekKindPw : phantom
    Kek~Kind~ ..> KekKindRecovery : phantom
    MasterPassword ..> PasswordStrengthGate : requires
    PasswordStrengthGate --> WeakPasswordFeedback : Err
    HeaderAeadKey ..> Kek~KekKindPw~ : derives
    Verified~T~ --> Plaintext : wraps
    CryptoOutcome~T~ --> Verified~T~ : variant
    CryptoOutcome~T~ --> WeakPasswordFeedback : variant
    WrappedVek --> NonceBytes : owns
    WrappedVek --> AuthTag : owns
```

## データ構造（全分冊横断）

| 名前 | 型 | 用途 | デフォルト値 | 詳細分冊 |
|------|---|------|------------|---------|
| `Vek` 内部表現 | `SecretBox<Zeroizing<[u8; 32]>>` | VEK 本体（AES-256 鍵） | コンストラクタで明示供給（CSPRNG 経由 / unwrap 結果経由） | `crypto-types.md` |
| `Kek<Kind>` 内部表現 | `(SecretBox<Zeroizing<[u8; 32]>>, PhantomData<Kind>)` | KEK 本体（KDF 出力） | コンストラクタで明示供給 | `crypto-types.md` |
| `HeaderAeadKey` 内部表現 | `SecretBox<Zeroizing<[u8; 32]>>` | ヘッダ AEAD タグ検証用鍵 | `Kek<KekKindPw>` から `from_kek_pw` で派生 | `crypto-types.md` |
| `MasterPassword` 内部表現 | `SecretBytes` | ユーザ入力パスワード | コンストラクタで明示供給（強度ゲート通過後のみ） | `password.md` |
| `WeakPasswordFeedback` フィールド | `{ warning: Option<String>, suggestions: Vec<String> }` | zxcvbn の `feedback` 構造をそのまま運ぶ。**`warning=None` 時の代替警告文責務は Sub-D**、**i18n 層は Sub-D 担当**（Sub-A は英語 raw のみ運ぶ） | `PasswordStrengthGate::validate` の `Err` 内 | `password.md` |
| `RecoveryMnemonic` 内部表現 | `SecretBox<Zeroizing<[String; 24]>>` | BIP-39 24 語 | コンストラクタで明示供給（BIP-39 検証通過後のみ、Sub-B で詳細化） | `errors-and-contracts.md` |
| `Plaintext` 内部表現 | `SecretBytes`、コンストラクタ可視性は **`pub(in crate::crypto::verified)` 限定** | レコード復号後の平文 | `Verified<Plaintext>::into_inner()` 経由でのみ取り出し可 | `nonce-and-aead.md` |
| `Verified<T>` 内部表現 | `T`（ジェネリクス）。**呼び出し側主張マーカー**（型システムは AEAD 検証実行を保証しない、契約レベルの保証） | AEAD 検証済みマーカ | `pub(crate) fn new_from_aead_decrypt(t: T)` でのみ構築 | `nonce-and-aead.md` |
| `NonceCounter::count` | `u64` | この VEK での暗号化回数 | `0`（`NonceCounter::new()`） | `nonce-and-aead.md` |
| `NonceCounter::LIMIT` | `u64` 定数 | 上限値 | `1u64 << 32`（= $2^{32}$） | `nonce-and-aead.md` |
| `NonceBytes::inner` | `[u8; 12]` | per-record AEAD nonce | コンストラクタ供給（`from_random` / `try_new`） | `nonce-and-aead.md` |
| `WrappedVek::ciphertext` | `Vec<u8>` | AEAD 暗号文 | `WrappedVek::new` | `nonce-and-aead.md` |
| `WrappedVek::tag` | `AuthTag([u8; 16])` | GCM 認証タグ | `WrappedVek::new` | `nonce-and-aead.md` |
| `KdfSalt::inner` | `[u8; 16]` | Argon2id 入力 salt | 既存維持（`KdfSalt::try_new`） | `errors-and-contracts.md`（既存型のため軽量参照） |

## ビジュアルデザイン

該当なし — 理由: UIなし

## 不変条件・契約サマリ（Sub-A の検証可能条件）

| 契約 | 強制方法 | 検証手段 | 詳細分冊 |
|-----|--------|--------|---------|
| C-1: Tier-1 揮発型は `Drop` 時 zeroize される | 内部 `SecretBox<Zeroizing<...>>` / `SecretBytes` の `Drop` 経路 | ユニットテスト: `Drop` 後のメモリパターン検証 | `crypto-types.md` / `password.md` / `errors-and-contracts.md` |
| C-2: Tier-1 揮発型は `Clone` 不可 | `Clone` 未実装 | compile_fail doc test | 同上 |
| C-3: Tier-1 揮発型は `Debug` で秘密値を出さない | `Debug` 実装が `[REDACTED ...]` 固定 | ユニットテスト: `format!("{:?}", vek)` の戻り値検証 | 同上 |
| C-4: Tier-1 揮発型は `Display` 不可 | `Display` 未実装 | compile_fail doc test | 同上 |
| C-5: Tier-1 揮発型は `serde::Serialize` 不可 | `Serialize` 未実装 | compile_fail doc test | 同上 |
| C-6: `Kek<KekKindPw>` と `Kek<KekKindRecovery>` は混合不可 | phantom-typed + Sealed trait | compile_fail doc test | `crypto-types.md` |
| C-7: `Verified<T>` は AEAD 復号関数からのみ構築可 | コンストラクタ `pub(crate)` 可視性 | 外部 crate からの構築 compile_fail doc test | `nonce-and-aead.md` |
| C-8: `MasterPassword::new` は `PasswordStrengthGate::validate` 通過必須 | コンストラクタが `&dyn PasswordStrengthGate` を要求 | ユニットテスト: `AlwaysRejectGate` で `Err(WeakPassword)` を確認、`AlwaysAcceptGate` で `Ok(MasterPassword)` | `password.md` |
| C-9: `NonceCounter::increment` は上限到達で `Err(NonceLimitExceeded)` | `if count >= LIMIT` 分岐 + `#[must_use]` | ユニットテスト: `LIMIT - 1` まで OK、`LIMIT` で `Err` | `nonce-and-aead.md` |
| C-10: `NonceBytes::from_random` は失敗しない（型レベル長さ強制） | 引数 `[u8; 12]` | コンパイラ強制（テスト不要だが回帰テストで `from_random([0u8;12])` が構築できることを確認） | `nonce-and-aead.md` |
| C-11: `WrappedVek::new` は ciphertext 空 / 短すぎを拒否 | `ciphertext.is_empty()` / `ciphertext.len() < 32` | ユニットテスト: 各境界条件 | `nonce-and-aead.md` |
| C-12: `RecoveryMnemonic::from_words` は 24 語固定（型レベル） | 引数 `[String; 24]` | コンパイラ強制 | `errors-and-contracts.md` |
| C-13: 既存 `DomainError::NonceOverflow` は `NonceLimitExceeded` に rename されている | grep + cargo check | CI で variant 名の一致検証 | `errors-and-contracts.md` |
| **C-14**: AEAD 検証失敗時に `Plaintext` を構築しない（Sub-C 新規） | `AesGcmAeadAdapter::decrypt_record` / `unwrap_vek` 内で `decrypt_in_place_detached` の `Err` 時に `verify_aead_decrypt` クロージャに到達しない | property test（タグ / AAD / nonce / ciphertext 4 系列書換）で `Verified<Plaintext>` 不在を assert | `nonce-and-aead.md` / `errors-and-contracts.md` |
| **C-15**: AEAD 鍵バイトの可視性ポリシー差別化維持（Sub-C 新規） | `AeadKey::with_secret_bytes` クロージャインジェクション経由でのみ shikomi-infra に `&[u8;32]` を渡す。`Vek` / `HeaderAeadKey::expose_within_crate` は `pub(crate)` 維持 | grep: shikomi-infra `aead/` 配下で `expose_within_crate` 直接呼出が 0 件 | `nonce-and-aead.md` / `crypto-types.md` |
| **C-16**: AEAD 中間バッファ zeroize（Sub-C 新規） | `encrypt_in_place_detached` / `decrypt_in_place_detached` の入力 `buf` を `Zeroizing<Vec<u8>>` で囲む | grep: shikomi-infra `aead/aes_gcm.rs` で `Zeroizing<Vec<u8>>` 使用、生 `Vec<u8>` の中間バッファ 0 件 | `nonce-and-aead.md` |
| **C-17**: ヘッダ AEAD タグの AAD はヘッダ全フィールド正規化バイト列を含む（Sub-D 新規） | `VaultEncryptedHeader::canonical_bytes_for_aad()` 実装で全フィールド連結強制 | ユニット bit-exact + property test（任意 byte 書換 → AEAD 検証失敗）| `repository-and-migration.md` / `errors-and-contracts.md` |
| **C-18**: `kdf_params` 改竄をヘッダ AEAD タグで検出（Sub-D 新規） | C-17 経路で AAD に `kdf_params` を含める | property test: kdf_params 任意書換 → `MigrationError::Crypto(AeadTagMismatch)` | `repository-and-migration.md` |
| **C-19**: `RecoveryDisclosure::disclose` は 1 度しか呼べない（Sub-D 新規） | `disclose(self)` 所有権消費 + `Display` / `Serialize` 未実装 | compile_fail doc test | `repository-and-migration.md` |
| **C-20**: `vault decrypt` は `DecryptConfirmation` 引数必須、`--force` でも省略不可（Sub-D 新規） | `VaultMigration::decrypt_vault(.., confirmation: DecryptConfirmation)` 型シグネチャ強制 | compile_fail doc test（confirmation なしでの呼出）| `repository-and-migration.md` |
| **C-21**: 平文⇄暗号化マイグレーション中の atomic write 失敗で原状復帰（Sub-D 新規） | vault-persistence の `.new` cleanup 経路継承（REQ-P04/P05） | integration test: SIGKILL 論理等価フック（`vault-persistence` TC-I06 同型）| `repository-and-migration.md` |
| **C-22**: `Locked` 状態で read/write IPC は型レベル拒否（Sub-E 新規） | 各 IPC ハンドラ入口の `match VaultUnlockState` でワイルドカード `_` 禁止、`Locked => Err(IpcError::VaultLocked)` 強制 | grep 静的検査（TC-E-S01）+ ユニットテスト（Locked で各 IPC を呼出 → `Err(VaultLocked)`）| `vek-cache-and-ipc.md` |
| **C-23**: `Unlocked → Locked` 遷移時に `Vek` を即 zeroize（Sub-E 新規） | `mem::replace(&mut state, Locked)` で旧 state 取り出し → Drop 連鎖 → `Vek::Drop` で 32B zeroize | ユニットテスト: lock 後にメモリ走査で旧 VEK が残らないことを確認 | `vek-cache-and-ipc.md` |
| **C-24**: アイドル 15min タイムアウトで自動 lock（Sub-E 新規） | `IdleTimer` バックグラウンド task が 60 秒ポーリングで `now - last_used >= 15min` 検出 → `cache.lock()` | integration test: `MockClock` で 15min 進行 → cache が `Locked` になることを確認 | `vek-cache-and-ipc.md` |
| **C-25**: OS スクリーンロック / サスペンド受信で 100ms 以内に lock（Sub-E 新規） | `OsLockSignal::next_lock_event` 受信 → `cache.lock()` 呼出 | integration test: `MockLockSignal` から `LockEvent::ScreenLocked` 注入 → 100ms 以内に `Locked` 遷移 | `vek-cache-and-ipc.md` |
| **C-26**: 連続失敗 5 回で指数バックオフ発動、unlock 成功でリセット（Sub-E 新規） | `UnlockBackoff::record_failure` / `record_success` の状態遷移 | ユニットテスト: 5 回連続失敗 → `BackoffActive` 返却 / unlock 成功 → カウンタゼロ | `vek-cache-and-ipc.md` |
| **C-27**: `MigrationError::RecoveryRequired` を `IpcError::RecoveryRequired` 透過 → MSG-S09 (a)（Sub-E 新規） | `From<MigrationError> for IpcError` 実装で transparent 透過 | ユニットテスト: パスワード失敗で `RecoveryRequired` 発火 → IPC 応答が `RecoveryRequired` variant、MSG-S09 (a) 文言を含む | `vek-cache-and-ipc.md` |
| **C-28**: V1 クライアントが V2 専用 variant 送信時に `ProtocolDowngrade` で拒否（Sub-E 新規） | handshake で client_version 確認、V1 クライアントには V2 variant を deserialize 失敗させる | integration test: V1 セッションで V2 variant 送信 → `IpcResponse::Error(ProtocolDowngrade)` 返却 | `vek-cache-and-ipc.md` |

## 後続 Sub-B〜F の TBD ブロック

各 Sub の設計工程で本ディレクトリ内の対応分冊を READ → EDIT で以下を追記する。

- **Sub-B（完了、本書 Rev により本項目は履歴）**: KDF アダプタの詳細クラス図（`Argon2idAdapter` / `Bip39Pbkdf2Hkdf`）、`PasswordStrengthGate` の `ZxcvbnGate` 実装詳細、KAT データ取得経路、CSPRNG 単一エントリ点 `Rng` → **新規 `kdf.md` + `rng.md` を追加**、`password.md` に `ZxcvbnGate` 章追加、`errors-and-contracts.md` に `KdfErrorKind` source 型詳細 + `InvalidMnemonic` variant + `Argon2idHkdfVekProvider` 具象 を追加
- **Sub-C（完了、本書 Rev により本項目は履歴）**: AEAD アダプタの詳細設計（`AesGcmAeadAdapter` の 4 メソッド + NIST CAVP KAT + AAD 26B 規約 + nonce_counter 統合契約 + AEAD 復号後の VEK 復元経路）、`AeadKey` trait（クロージャインジェクション、Sub-B Rev2 可視性ポリシー差別化との整合）、`verify_aead_decrypt` ラッパ関数の呼び出し経路の補強、`derive_new_wrapped_*` の AES-GCM wrap 経路、`unwrap_vek_with_*` の VEK 復元 + 長さ検証 Fail Fast、契約 C-14〜C-16 追加 → **`nonce-and-aead.md` 拡張**（新規分冊なし、`aead-adapter.md` 不要）+ `errors-and-contracts.md` 補強 + `crypto-types.md` で `Vek` / `Kek<_>` への `AeadKey` impl 追記（Boy Scout）
- **Sub-D（完了、本書 Rev により本項目は履歴）**: `VaultMigration` service（`encrypt_vault` / `decrypt_vault` / `unlock_with_*` / `rekey` / `change_password` の 6 メソッド）、SQLite スキーマ拡張（`kdf_params` / `header_aead_*` カラム、`PRAGMA user_version` bump）、`VaultEncryptedHeader` / `KdfParams` / `HeaderAeadEnvelope` / `EncryptedRecord` 完成、`HeaderAeadKey::AeadKey` impl 追加（Sub-C 予告 Boy Scout 完成）、`RecoveryDisclosure` + `RecoveryWords`（24 語初回 1 度表示の型レベル強制 C-19）、`DecryptConfirmation`（二段確認 C-20）、`Aad::HeaderEnvelope(Vec<u8>)` 拡張（既存 `Aad::Record` と並列）、`MigrationError` 列挙型、ヘッダ独立 AEAD タグ AAD 規約（C-17/C-18）、契約 C-17〜C-21 追加、MSG-S01/S02/S05 部分/S06/S08/S10/S11/S12/S13/S14/S16/S18 文言確定 → **新規 `repository-and-migration.md`** + `errors-and-contracts.md` Sub-D 完了反映 + `crypto-types.md` `HeaderAeadKey::AeadKey` impl 同期 + `requirements.md` REQ-S06/S07/S13 確定 + `basic-design.md` Sub-D モジュール構成 + F-D1〜F-D5 処理フロー追記。**横断的変更**: `vault-persistence` の REQ-P11 改訂（「暗号化モード即時拒否」→「未対応バージョン拒否」）、`flows.md` の `UnsupportedYet` 即 return 削除、`integration.md` 旧 TC 退役 + 新 TC 置換
- **Sub-E（完了、本書 Rev により本項目は履歴）**: VEK キャッシュの `tokio::sync::RwLock<VaultUnlockState>` 設計（`Locked` / `Unlocked { vek, last_used }` enum、`#[non_exhaustive]`）、`VekCache` + `IdleTimer` + `OsLockSignal` trait（`#[cfg]` 分割、macOS / Windows / Linux 具象実装）、IPC V2 `IpcRequest` 5 variant 追加（`Unlock` / `Lock` / `ChangePassword` / `RotateRecovery` / `Rekey`）、`UnlockBackoff` 連続失敗 5 回で指数バックオフ、`change-password` の `wrapped_VEK_by_pw` 単独更新フロー（VEK 不変、daemon キャッシュ維持）、`MigrationError → IpcError` マッピング（9 → 4 集約、`RecoveryRequired → MSG-S09(a)` 統合）、契約 C-22〜C-28 追加、MSG-S03/S04/S05/S09/S15 文言確定 → **新規 `vek-cache-and-ipc.md`** + `requirements.md` REQ-S09/S10/S11/S12 確定 + `basic-design/architecture.md` Sub-E モジュール構成 + `basic-design/processing-flows.md` F-E1〜F-E5 処理フロー追記 + `basic-design/security.md` L2 / A07 Sub-E 追記。**横断的変更**: `daemon-ipc` feature の `IpcProtocolVersion::V2` 非破壊昇格 + `IpcRequest` / `IpcResponse` / `IpcError` の V2 variant 追加（SSoT は `daemon-ipc/detailed-design/protocol-types.md`）
- **Sub-F**: `shikomi vault {encrypt, decrypt, unlock, lock, change-password, recovery-show, rekey}` の clap サブコマンド構造、IPC V2 リクエスト発行経路、MSG-S* 文言テーブル → 新規 `cli-subcommands.md`
