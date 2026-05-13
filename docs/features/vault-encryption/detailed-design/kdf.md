# 詳細設計書 — KDF アダプタ（`kdf`）

<!-- 親: docs/features/vault-encryption/detailed-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/detailed-design/kdf.md -->
<!-- 主担当: Sub-B (#40)。Sub-D 以降は本分冊を READ → EDIT で `vault encrypt` 入口統合や rekey 経路に追記。 -->

## 対象型

- `shikomi_infra::crypto::kdf::Argon2idAdapter`（Sub-B 新規）
- `shikomi_infra::crypto::kdf::Bip39Pbkdf2Hkdf`（Sub-B 新規）
- `shikomi_infra::crypto::kdf::Argon2idParams`（const 凍結値、Sub-B 新規）
- `shikomi_infra::crypto::kdf::HKDF_INFO`（const 凍結値、Sub-B 新規）

## モジュール配置と責務

```
crates/shikomi-infra/src/
  crypto/                      +  本 Sub-B 新規モジュール
    mod.rs                     +  rng / kdf / password の再エクスポート
    kdf/                       +  KDF アダプタ
      mod.rs                   +  Argon2idAdapter / Bip39Pbkdf2Hkdf 再エクスポート
      argon2id.rs              +  Argon2idAdapter + Argon2idParams const
      bip39_pbkdf2_hkdf.rs     +  Bip39Pbkdf2Hkdf + HKDF_INFO const
      kat.rs                   +  KAT データ取得 (test cfg)
```

**Clean Architecture の依存方向**:

- shikomi-infra の本モジュールは shikomi-core の暗号ドメイン型（`MasterPassword` / `KdfSalt` / `RecoveryMnemonic` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` / `CryptoError`）に依存
- shikomi-core への逆依存は禁止（Sub-A の `crypto::*` モジュールは KDF アルゴリズム実装に依存しない）
- `argon2` / `pbkdf2` / `hkdf` / `bip39` / `sha2` / `hmac` crate に shikomi-infra のみが依存（shikomi-core の Cargo.toml には**追加しない**、no-I/O 制約継承）

## `Argon2idAdapter`

### 型定義

- `pub struct Argon2idAdapter { params: Argon2idParams }`
- 無状態 struct（`#[derive(Clone, Copy, Default)]`、フィールドは const 値の保持のみ）
- `Default` 実装で `Argon2idParams::FROZEN_OWASP_2024_05` を使う

### `Argon2idParams` 凍結値

- `pub struct Argon2idParams { pub m: u32, pub t: u32, pub p: u32, pub output_len: usize }`
- `pub const FROZEN_OWASP_2024_05: Argon2idParams = Argon2idParams { m: 19_456, t: 2, p: 1, output_len: 32 };`
- **凍結根拠**: `tech-stack.md` §4.7 `argon2` 行 + OWASP Password Storage Cheat Sheet（2024-05 改訂版）+ `requirements.md` REQ-S03
- **再評価サイクル**: 4 年または criterion ベンチで p95 1 秒逸脱時。Sub-B 設計書凍結後の変更は `tech-stack.md` 同時改訂を必須とする
- **派生**: `Debug, Clone, Copy, PartialEq, Eq`（const 値、秘密でない）

### メソッド

| 関数名 | 可視性 | シグネチャ | 仕様 |
|-------|------|----------|------|
| `Argon2idAdapter::new` | `pub` | `(params: Argon2idParams) -> Argon2idAdapter` | 任意 params 構築（テスト用に小さい params を渡す経路を許容） |
| `Argon2idAdapter::default` | `pub` | `() -> Argon2idAdapter` | 凍結値で構築 |
| `Argon2idAdapter::derive_kek_pw` | `pub` | `(&self, password: &MasterPassword, salt: &KdfSalt) -> Result<Kek<KekKindPw>, CryptoError>` | 本モジュールの主目的。`password.expose_secret_bytes()` で取得した `&[u8]`（**Sub-A `MasterPassword` の `pub` 可視性正規経路**、`password.md` §可視性ポリシー差別化 を参照） を `argon2::Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(self.params.m, self.params.t, self.params.p, Some(self.params.output_len))?).hash_password_into(password_bytes, salt.as_array(), &mut out)` に渡す。`out: Zeroizing<[u8; 32]>` で中間バッファを保護、成功時 `Kek::<KekKindPw>::from_array(*out)` で返す（中間 32B は Drop で zeroize） |

### `argon2` crate 呼出契約

| 項目 | 値 / 方針 |
|-----|---------|
| 呼出パス | `argon2::Argon2 → hash_password_into` の **raw API のみ**使用。`password-hash` の PHC 文字列 API は使わない（`tech-stack.md` §4.7 `argon2` 行、DRY 違反防止） |
| Algorithm | `argon2::Algorithm::Argon2id`（固定、Argon2i / Argon2d は使わない） |
| Version | `argon2::Version::V0x13`（最新 stable、固定） |
| 出力長 | `output_len = 32`（`Vek` / `Kek` と同じ 32B、AES-256 鍵長） |
| KAT | RFC 9106 Appendix A の Test Vectors を `kdf/kat.rs` に埋め込み、`#[cfg(test)] fn argon2id_rfc9106_kat()` で実行（CI ジョブ `test-infra` で必須 pass） |
| 中間バッファ zeroize | `Zeroizing<[u8; 32]>` で 32B 出力バッファを包む。`Argon2::hash_password_into` 戻り後即座に `Kek::from_array(*zeroized_buf)` でラップ → 元バッファは `Zeroizing::Drop` で zeroize |

### **性能契約（criterion ベンチ p95 1 秒）**

- **ベンチ対象**: `Argon2idAdapter::derive_kek_pw` の単一呼出時間
- **測定環境**: GitHub Actions の 3 OS runner（Ubuntu / macOS / Windows）でそれぞれ計測
- **判定基準**:
  - **p95 ≤ 1.0 秒** で pass（ホットキー UX を阻害しない上限）
  - **p95 > 1.0 秒** で **リリースブロッカ**（CI fail）
  - **p50 < 100 ms** は逆に「弱すぎ」警告（`m=19456` が効いていない可能性、L3 brute force 耐性低下）
- **再評価トリガ**: (a) 4 年経過（OWASP 推奨更新サイクル）、(b) ベンチが p95 1 秒を超えた、(c) 新しい RFC または OWASP 改訂が出た — いずれかで `tech-stack.md` §4.7 + 本書 + `Argon2idParams::FROZEN_OWASP_2024_05` を同時改訂
- **実装**: `crates/shikomi-infra/benches/argon2id.rs` に criterion benchmark を新設、`cargo bench -p shikomi-infra` で実行可能
- **CI ジョブ**: 既存 `test-infra` ジョブとは別の `bench-kdf` ジョブを追加（OS マトリクス 3、所要 30〜60 秒/ OS、`cargo bench` 実行）
- **fixture**: テスト用パスワードは zxcvbn 強度 ≥ 3 のサンプル（criterion seed は固定、再現性確保）

## `Bip39Pbkdf2Hkdf`

### 型定義

- `pub struct Bip39Pbkdf2Hkdf;`（無状態 struct、`#[derive(Clone, Copy, Default)]`）
- HKDF info 定数: `pub const HKDF_INFO: &[u8] = b"shikomi-kek-v1";`
- HKDF info 凍結根拠: `tech-stack.md` §2.4 KEK_recovery 行 + Sub-0 凍結値

### メソッド

| 関数名 | 可視性 | シグネチャ | 仕様 |
|-------|------|----------|------|
| `Bip39Pbkdf2Hkdf::derive_kek_recovery` | `pub` | `(&self, recovery: &RecoveryMnemonic) -> Result<Kek<KekKindRecovery>, CryptoError>` | **変数名規約**: 引数 `recovery` は Sub-A `shikomi_core::crypto::recovery::RecoveryMnemonic`、内部で派生する `bip39::Mnemonic` インスタンスは `bip39_mnemonic` で受ける（同一 `mnemonic` 識別子の取り違え禁止）。**処理手順**: (1) `recovery.expose_words()` で `&[String; 24]` 取得（`recovery.rs` 実装と一致、**Sub-A `RecoveryMnemonic` の `pub` 可視性正規経路**、`password.md` §可視性ポリシー差別化 を参照）、(2) 24 語をスペース区切りで連結し `let bip39_mnemonic = bip39::Mnemonic::parse_in(Language::English, joined)?;` で wordlist + checksum 検証、失敗時 `CryptoError::InvalidMnemonic`、(3) `let seed = bip39_mnemonic.to_seed("");` で 64B seed（PBKDF2-HMAC-SHA512 2048iter `salt="mnemonic"+""`、内部実装は `bip39` crate に委譲）、(4) `hkdf::Hkdf::<Sha256>::new(None, &seed).expand(HKDF_INFO, &mut [u8; 32])`、(5) 32B 出力を `Kek::<KekKindRecovery>::from_array` でラップ。中間 seed 64B + KEK 32B は `Zeroizing` で囲む。**`bip39_mnemonic` の zeroize**: `bip39` crate v2 系の `zeroize` feature を有効化（`tech-stack.md` §4.7 `bip39` 行）し、スコープ抜けで `bip39_mnemonic` の Drop 経路で wordlist 由来の文字列バッファを zeroize |

### `bip39` / `pbkdf2` / `hkdf` crate 呼出契約

| 項目 | 値 / 方針 |
|-----|---------|
| `bip39` crate | major ピン v2 系（rust-bitcoin、`tech-stack.md` §4.7）。`tiny-bip39` 不採用（unmaintained） |
| Mnemonic 言語 | English のみ（`Language::English` 固定）。日本語 wordlist は将来 Sub-D 以降の検討、初期実装は英語のみ |
| Mnemonic 構築経路 | `Mnemonic::parse_in(Language::English, joined_words)` → `to_seed("")` の 2 段。BIP-39 passphrase は **空文字列固定**（Sub-D 以降でユーザ追加 passphrase の検討余地あり、初期実装はなし） |
| PBKDF2 呼出 | `bip39` crate 内部で実装、shikomi-infra 側で `pbkdf2` crate を直接呼ばない（依存方向: `bip39` → `pbkdf2`、shikomi-infra は `bip39` 経由のみ） |
| HKDF 呼出 | `hkdf::Hkdf::<Sha256>::new(salt: Option<&[u8]>, ikm: &[u8]).expand(info, okm)` の `expand` API。`salt = None` で `[0u8; 32]` 等価扱い（HKDF-Extract 段階を内部 default salt で実行）、`info = HKDF_INFO` 固定 |
| KAT | (a) BIP-39 trezor 公式 `vectors.json`（24 語 → seed の経路、`bip39` crate のテストでも実施されているが本リポジトリでも独立実行）、(b) RFC 5869 Appendix A の HKDF-SHA256 テストベクトル（`kdf/kat.rs` に埋め込み、`#[cfg(test)]`） |
| 中間バッファ zeroize | seed 64B + KEK 32B を `Zeroizing<[u8; N]>` で包む。`Mnemonic` 自体の zeroize は `bip39` crate v2 の `zeroize` feature を有効化（`tech-stack.md` §4.7 `bip39` 行） |

### Mnemonic 検証の責務分担（Sub-A との境界）

- **Sub-A `RecoveryMnemonic::from_words`**: 配列長 24 + 各単語の文字列長 / ASCII 性のみ軽量検証（pure Rust、`bip39` crate 不在）
- **Sub-B `Bip39Pbkdf2Hkdf::derive_kek_recovery`**: BIP-39 wordlist 一致 + checksum 検証を実施（`bip39` crate に委譲）
- **責務分離の意図**: Sub-A は no-I/O 制約のため大きな wordlist データ（2048 語）を持たない。Sub-B（shikomi-infra）が wordlist を保有し、検証経路を集約。Sub-D の `vault recovery-show` 後の入力検証もこの経路を通る

### 設計判断の補足

#### なぜ `bip39::Mnemonic::to_seed` を使い、`pbkdf2` crate を直接呼ばないか

- **DRY 原則**: BIP-39 標準は「PBKDF2-HMAC-SHA512 2048iter, salt='mnemonic'+passphrase」を**仕様の一部**として規定（BIP-39 Specification §"From mnemonic to seed"）。`bip39` crate v2 の `Mnemonic::to_seed` はこの標準を実装している。shikomi-infra で再実装すると BIP-39 仕様変更時の追従負担が増える
- **責務分離**: `bip39` crate は wordlist + checksum + seed 導出を**ひとつの仕様準拠ライブラリ**として提供。shikomi-infra は「seed 取得後の HKDF アプリ固有派生」のみ責務を持つ
- **テスト**: BIP-39 trezor 公式 vectors.json は `Mnemonic::to_seed` の正しさを **bip39 crate 内部でも検証済み** + 本リポジトリでも独立 KAT 実行（二重検証）

#### なぜ HKDF info 定数を `b"shikomi-kek-v1"` 固定で持つか

- **ドメイン分離**: HKDF info パラメータはアプリ固有のラベルであり、同じ seed を別アプリで使った時に異なる KEK が出る（暗号的ドメイン分離、RFC 5869 §3.2 Recommendations）
- **バージョニング**: `-v1` サフィックスで将来 KDF アルゴリズム変更時の段階的移行を可能にする（v2 への移行は `b"shikomi-kek-v2"` で別 const 定義し、ヘッダの `kdf_version` で分岐）
- **凍結根拠**: `tech-stack.md` §2.4 で凍結値として明示、Sub-B 設計書でも const として固定し、PR レビューで変更を検出

## エラーハンドリング

| エラー variant | 発火条件 | Sub-B での扱い |
|------------|---------|--------------|
| `CryptoError::KdfFailed { kind: KdfErrorKind::Argon2id, source }` | `argon2::Argon2::hash_password_into` が `Err` を返した（メモリ不足 / Params 構築失敗 / 出力長不正） | `Argon2idAdapter::derive_kek_pw` から即返却、リトライしない |
| `CryptoError::KdfFailed { kind: KdfErrorKind::Pbkdf2, source }` | `bip39::Mnemonic::to_seed` 失敗（`bip39` crate v2 では実質発生しないが、将来の lib 仕様変更に備えて variant を保持） | `Bip39Pbkdf2Hkdf::derive_kek_recovery` から即返却 |
| `CryptoError::KdfFailed { kind: KdfErrorKind::Hkdf, source }` | `hkdf::Hkdf::<Sha256>::expand` が出力長エラー（`okm.len() > 255 * Hash::OutputSize` 超過時） | 32B 出力では発生しないが、enum variant 完全性のため定義 |
| `CryptoError::InvalidMnemonic` | `bip39::Mnemonic::parse_in` 失敗（wordlist 不一致 / checksum 不一致 / 単語数不正） | `Bip39Pbkdf2Hkdf::derive_kek_recovery` から即返却、Sub-D の `vault unlock --recovery` 経路で MSG-S12 に変換 |

詳細な variant 定義 + KdfErrorKind は `errors-and-contracts.md` を参照。

## 後続 Sub への引継ぎ

- **Sub-C**: AEAD 実装側で `Kek<KekKindPw>` / `Kek<KekKindRecovery>` を受け取り `WrappedVek` 復号する経路を本書 `derive_kek_*` の戻り値に直接接続
- **Sub-D**: `EncryptedSqliteVaultRepository` の `unwrap_with_password` / `unwrap_with_recovery` ユースケースから `Argon2idAdapter::derive_kek_pw` / `Bip39Pbkdf2Hkdf::derive_kek_recovery` を呼び出し、結果を Sub-C の AEAD アダプタに渡す
- **Sub-D / Sub-F**: `vault encrypt` 初回フロー（新規 `KdfSalt::generate` → `Argon2idAdapter::derive_kek_pw` → `Vek` 生成 → `wrap_with_kek_pw`）の連結
