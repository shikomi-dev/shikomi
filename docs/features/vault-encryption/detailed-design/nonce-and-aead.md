# 詳細設計書 — nonce / AEAD 境界（`nonce-and-aead`）

<!-- 親: docs/features/vault-encryption/detailed-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/detailed-design/nonce-and-aead.md -->
<!-- 主担当: Sub-A (#39) で型契約 + Verified<T>、Sub-C (#41) で AEAD 実装結合 + AeadKey trait 追加 (Boy Scout)、Sub-D (#42) で HeaderAeadKey 経路統合。 -->

## 対象型

- `shikomi_core::crypto::verified::Plaintext`
- `shikomi_core::crypto::verified::Verified<T>` + `verify_aead_decrypt` ラッパ関数
- `shikomi_core::crypto::aead_key::AeadKey` trait（**Sub-C 新規、Boy Scout Rule**）
- `shikomi_core::vault::nonce::NonceCounter`（責務再定義、Boy Scout Rule）
- `shikomi_core::vault::nonce::NonceBytes`（拡張、Boy Scout Rule）
- `shikomi_core::vault::crypto_data::WrappedVek`（内部構造分離型化、Boy Scout Rule）
- `shikomi_core::vault::crypto_data::AuthTag`（新規）
- `shikomi_core::vault::crypto_data::Aad`（既存、Sub-C は `to_canonical_bytes()` 26B 規約を消費するのみ）
- `shikomi_infra::crypto::aead::aes_gcm::AesGcmAeadAdapter`（**Sub-C 新規**）

## `Plaintext`

### 型定義

- `pub struct Plaintext { inner: SecretBytes }`

### コンストラクタ（**指摘 #5 対応で可視性変更**）

| 関数名 | 可視性 | シグネチャ概要 | 不変条件 |
|-------|------|------------|--------|
| `Plaintext::new_within_module` | **`pub(in crate::crypto::verified)`** | `(bytes: Vec<u8>) -> Plaintext` | **`shikomi-core::crypto::verified` モジュール内のみで構築可**（Rev1 で `pub(crate)` から `pub(in crate::crypto::verified)` に絞り込み）。`Verified::new_from_aead_decrypt` 経路以外からの構築を**コンパイル時禁止** |

### 公開する経路

- `expose_secret(&self) -> &[u8]`: `pub`、クリップボード投入時等に必要なため外部 crate からの read アクセスは許可
- ただし**新規構築は不可**（`Verified<Plaintext>::into_inner` 経由のみ）

### 提供トレイト（**指摘 #6 対応で本セクション追加**）

- `Debug`: **`[REDACTED PLAINTEXT]` 固定文字列**（CI grep で文字列リテラルを検証）
- `Drop`: 内部 `SecretBytes` の zeroize 経路に委譲

### 禁止トレイト（**指摘 #6 対応で本セクション追加**）

- `Clone`: **未実装**。AEAD 検証済み平文の複製を構造禁止（複製による滞留延長を防ぐ）
- `Copy`: 未実装（`Drop` 持つため不可、保証）
- `Display`: 未実装。誤フォーマット出力を**コンパイル時禁止**
- `serde::Serialize` / `serde::Deserialize`: 未実装。誤シリアライズを**コンパイル時禁止**
- `PartialEq` / `Eq`: 未実装（平文比較は `subtle::ConstantTimeEq` を Sub-D 経路で使用、通常 `==` を**コンパイル時禁止**）

### `pub(in crate::crypto::verified)` の意図（指摘 #5 対応）

- **`pub(crate)` だと crate 全体（shikomi-core 全モジュール）から `Plaintext::new_within_module` が呼べる**。Sub-B / Sub-D 実装者が `shikomi_core::vault::record::*` 等の隣接モジュールから誤って呼び出す経路が開く
- **`pub(in crate::crypto::verified)` に絞ることで、`shikomi_core::crypto::verified` モジュール内 = `Verified::new_from_aead_decrypt` を実装する同一モジュール内**からしか呼べない構造になる
- **副作用**: `Verified<Plaintext>` 経由以外で `Plaintext` を作る経路を**型システムレベルで物理封鎖**。Sub-B / Sub-D 実装者の誤実装をコンパイルエラーで弾く

## `Verified<T>`

### 型定義

- `pub struct Verified<T> { inner: T }`

### コンストラクタ

| 関数名 | 可視性 | シグネチャ概要 | 不変条件 |
|-------|------|------------|--------|
| `Verified::new_from_aead_decrypt` | **`pub(crate)`** | `(inner: T) -> Verified<T>` | crate 内のみで構築。`shikomi-infra` を含む外部 crate からは構築不可 |

### 公開する経路

- `pub fn into_inner(self) -> T`：`Verified` を消費して中身を取り出す
- `pub fn as_inner(&self) -> &T`：参照アクセス（zeroize 前にレコード投入する経路で利用）

### 提供トレイト（**指摘 #6 対応で本セクション追加**）

- `Debug`: **`Verified<T> { inner: <T の Debug 出力> }` 形式**で内部 T の Debug 実装に委譲。T = `Plaintext` の場合は `Verified<Plaintext> { inner: [REDACTED PLAINTEXT] }` となる（`Plaintext::Debug` の `[REDACTED PLAINTEXT]` 固定文字列が連鎖して秘密が漏れない）

### 禁止トレイト（**指摘 #6 対応で本セクション追加**）

- `Clone`: **未実装**。`Verified<Plaintext>` の複製を構造禁止（AEAD 検証マーカーの複製で攻撃面拡大を防ぐ、`Plaintext` の `Clone` 禁止と整合）
- `Copy`: 未実装（`T: Drop` を含むため自動で不可、保証）
- `Display`: 未実装
- `serde::Serialize` / `serde::Deserialize`: 未実装（AEAD 検証済み状態を経路外でシリアライズしてはならない）
- `PartialEq` / `Eq`: 未実装

### **`verify_aead_decrypt` ラッパ関数の契約（指摘 #4 対応で大幅改訂）**

#### 関数シグネチャ

- `pub fn verify_aead_decrypt<F, T, E>(decrypt_fn: F) -> Result<Verified<T>, E> where F: FnOnce() -> Result<T, E>`
- shikomi-infra の AES-GCM 復号は本関数の `decrypt_fn` クロージャ内で実行 → 成功時に `Verified<T>` で包まれて返る

#### **本関数の保証範囲（誇大宣伝の訂正）**

- **本関数は「AEAD 検証実行を型システムで保証する」ものではない**。型システムは「クロージャ実行成功時に `Verified<T>` を返す」しか保証しない
- shikomi-infra のクロージャ内で AEAD 検証を実装ミスで skip し `Ok(Plaintext::new_within_module(b"fake"))` を返せば、**`Verified<Plaintext>` が AEAD 検証ゼロで成立してしまう**（ただし `Plaintext::new_within_module` は `pub(in crate::crypto::verified)` 限定可視性のため、shikomi-infra からは呼べない — Rev1 で構造的封鎖済）
- したがって本関数の役割は **「呼び出し側主張マーカー（caller-asserted marker）」** である。「`Verified<T>` を返す関数を呼ぶ側は、クロージャ内で AEAD 検証を実施したことを契約として宣言する」という**契約レベルの保証**であり、**型レベルの完全保証ではない**

#### 構造的封鎖の二段防御

1. **第一層（型レベル）**: `Verified<T>::new_from_aead_decrypt` を `pub(crate)` 可視性に限定 → 外部 crate（shikomi-infra）から直接 `Verified` を構築できない
2. **第二層（モジュール内可視性）**: `Plaintext::new_within_module` を `pub(in crate::crypto::verified)` に限定 → `Verified<Plaintext>` の中身となる `Plaintext` 自体を `Verified::new_from_aead_decrypt` 関数を実装する同一モジュール内からしか作れない
3. **第三層（呼び出し側契約）**: `verify_aead_decrypt(|| ...)` のクロージャ内で AEAD 検証を実装することを **shikomi-infra の AEAD アダプタ（Sub-C）の責務として契約化**。レビューで「クロージャ内が aes-gcm crate の検証 API を呼んでいるか」を必須確認

#### Sub-C 設計時に追記すべき事項

- `aes_gcm::aead::Aead::decrypt` 系 API は内部で GMAC タグ検証を行い、検証失敗時に `Err(aead::Error)` を返す → クロージャ内では `Result` 伝播で `verify_aead_decrypt` の `E` パラメータに変換
- 「AEAD 検証 bypass」を技術的に成立させるには、Sub-C 実装者が AES-GCM 復号 API を呼ばずに任意のバイト列を `T` として返す必要がある → これは **Sub-C PR レビューでのコード読み取り**で検出する責務（型システムでは検出不可能）

#### 代替案との比較

- **代替案 A（物理結合）**: `verify_aead_decrypt` を `verify_aead_decrypt_with(ciphertext: &[u8], key: &impl AeadKey, nonce: &NonceBytes, aad: &[u8]) -> Result<Verified<Plaintext>, CryptoError>` のように shikomi-core 側で AEAD 入力を受け取り、shikomi-infra の AES-GCM 実装関数を `pub(crate)` で参照する物理結合構造に変更
  - **メリット**: AEAD 検証実装を shikomi-core の関数本体に組み込めば bypass を完全構造禁止できる
  - **デメリット**: shikomi-core が **`aes-gcm` crate に依存することになり pure Rust / no-I/O 制約と同様に「shikomi-core は暗号アルゴリズム実装を持たない」契約に違反**。Clean Architecture の依存方向（shikomi-core ← shikomi-infra）が崩れる
- **採用方針**: クロージャマーカー方式 + 二段防御 + 呼び出し側契約。Sub-C PR レビューでの構造的検証を最終防衛線とする
- **過信防止**: basic-design.md §セキュリティ設計 §Fail-Secure の `Verified<T>` 行で「**caller-asserted マーカーであり、AEAD 検証 bypass は契約レベル + Sub-C PR レビューで検出**」と注記する（basic-design.md Rev1 で更新）

## `AeadKey` trait（**Sub-C 新規、Boy Scout Rule**）

### 追加の動機

Sub-B Rev2（PR #52）で「**鍵バイト型（`Vek` / `Kek<_>` / `HeaderAeadKey`）の `expose_within_crate` は `pub(crate)` 維持、KDF 入力（`MasterPassword` / `RecoveryMnemonic`）のみ `pub` 格上げ**」という可視性ポリシー差別化が凍結された（`password.md` §可視性ポリシー差別化）。結果として **shikomi-infra の AEAD アダプタは `Vek` / `HeaderAeadKey` の内部 32B に直接アクセスできない**。要件 issue #41「`AES-256-GCM(Vek, Nonce, plaintext, Aad) → ciphertext ‖ tag`」を Clean Architecture 準拠で実装するために、**鍵側にクロージャインジェクションメソッドを追加**するのが本 trait の役割。

### 型定義

- `pub trait AeadKey { fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R; }`
- **配置先**: `crates/shikomi-core/src/crypto/aead_key.rs`（Sub-C 新規モジュール）
- **dyn-safe ではない**: `impl FnOnce` を持つため `&dyn AeadKey` には作れない（型パラメータ `R` も含めて静的ディスパッチのみ）。これは意図的（dyn 経由で trait オブジェクト化されると最適化機会を失い、`Vek` / `HeaderAeadKey` の構造が attacker-readable になる経路を増やす）

### 契約

| 項目 | 内容 |
|---|---|
| 鍵バイトの所有権 | クロージャ `f` は `&[u8; 32]` 参照のみを受け取り、**所有権は鍵側に残る**。クロージャ実行後、鍵バイト参照は dangle 不可（borrow checker が保証） |
| 鍵バイトの寿命 | クロージャ内のみで有効。クロージャ実行終了後、shikomi-infra 側で鍵バイトを保持してはならない（`for<'a> Fn(&'a [u8;32])` ライフタイム制約 + コードレビューで担保） |
| クロージャの戻り値 | `R` は呼出側が決める（AES-GCM 暗号文 `Vec<u8>` + `AuthTag` の組、または `Result<Verified<Plaintext>, CryptoError>` 等）|
| 副作用 | `with_secret_bytes` 自体は副作用を持たない。クロージャ内の AES-GCM 操作も I/O を持たない |
| Drop タイミング | 鍵バイトは `Vek` / `HeaderAeadKey` の `SecretBox<Zeroizing<[u8;32]>>` 内に保持され、**鍵自体が Drop されるまで zeroize されない**（`with_secret_bytes` 呼出毎に再 zeroize は発生しない、性能劣化を避けるため）|

### `Vek` への impl（`crypto-types.md` 側で別途記述、Boy Scout Rule）

- `impl AeadKey for Vek { fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R { f(self.expose_within_crate()) } }`
- `Vek::expose_within_crate` は **`pub(crate)` 可視性のまま**（鍵バイト外部非公開原則維持）。trait 経由のクロージャインジェクションのみが外部 crate に開放される

### `HeaderAeadKey` への impl（Sub-D 側で同パターンを Boy Scout、本 Sub-C は方針確定のみ）

- 同形の `impl AeadKey for HeaderAeadKey { ... }` を Sub-D の vault ヘッダ AEAD 検証経路で追加
- Sub-C 段階では **`Vek` impl のみ確定**、`HeaderAeadKey` impl は本書 §Sub-D 引継ぎ で予告

### 代替案との比較

| 案 | 説明 | 採否 |
|---|---|------|
| **A: `Vek::expose_within_crate` を `pub` 格上げ** | shikomi-infra から直接 `Vek` の内部 32B を取得して AES-GCM 計算 | **却下**: Sub-B Rev2 で凍結した「鍵バイト型は `pub(crate)` 維持」契約を破壊。服部・ペテルギウスから即却下確実 |
| **B: shikomi-core 側に `Vek::aes_gcm_encrypt` / `aes_gcm_decrypt` メソッド直書き** | shikomi-core が `aes-gcm` crate に直接依存し、Vek が暗号操作を提供 | **却下**: shikomi-core の no-I/O / pure Rust 制約と「shikomi-core は暗号アルゴリズム実装を持たない」契約に違反。Clean Architecture の依存方向が崩れる |
| **C: `AeadKey` trait + クロージャインジェクション（採用）** | 鍵バイト所有権は鍵側に保持、shikomi-infra がクロージャ越境で借用 | **採用**: `verify_aead_decrypt` の caller-asserted マーカー思想と完全同型、Sub-A/B 凍結契約を一切破壊しない、Tell-Don't-Ask 整合 |

## `NonceCounter`（責務再定義、Boy Scout Rule）

### 型定義（変更後）

- `pub struct NonceCounter { count: u64 }`
- 既存の `random_prefix: [u8; 8]` フィールドを**削除**（per-record nonce は完全 random 12B、prefix 共有しない）

### メソッド

| 関数名 | 可視性 | シグネチャ | 仕様 |
|-------|------|----------|------|
| `NonceCounter::new` | `pub` | `() -> NonceCounter` | `count = 0` で初期化 |
| `NonceCounter::resume` | `pub` | `(count: u64) -> NonceCounter` | vault ヘッダから読み込んだ値で再開 |
| `NonceCounter::increment` | `pub` | `(&mut self) -> Result<(), DomainError>` | `count < LIMIT` なら `count += 1; Ok(())`、`count >= LIMIT` なら `Err(DomainError::NonceLimitExceeded)`、結果は `#[must_use]` |
| `NonceCounter::current` | `pub` | `(&self) -> u64` | 永続化用に現在値を返す |
| `NonceCounter::LIMIT` | `pub` 定数 | `u64` | `1u64 << 32`（= $2^{32}$、NIST SP 800-38D §8.3 random nonce birthday bound） |

### 既存 `next() -> Result<NonceBytes, DomainError>` の扱い

- **削除**（Boy Scout Rule）。代替として `NonceBytes::from_random([u8;12])` を使用
- 既存テストコード（`nonce.rs::tests`）は **`NonceCounter::increment` ベースに書き換え**

## `NonceBytes`（拡張）

### 既存維持

- `try_new(bytes: &[u8]) -> Result<NonceBytes, DomainError>` 既存維持（永続化からの復元用）

### 新規追加

- `from_random(bytes: [u8; 12]) -> NonceBytes`：CSPRNG 呼出側（`shikomi-infra::crypto::Rng::generate_nonce_bytes`）が完全 random 12B を渡して構築
  - **失敗しない**: `[u8; 12]` の型レベルで長さ強制
  - **意味論明示**: `from_random` という名前で「これは CSPRNG 由来である」契約を呼び出し側に課す（ad-hoc な `[0u8; 12]` 等の決定論値構築をテスト以外で禁止、CI grep で検出）

## `WrappedVek`（内部構造分離型化、Boy Scout Rule）

### 型定義（変更後）

- `pub struct WrappedVek { ciphertext: Vec<u8>, nonce: NonceBytes, tag: AuthTag }`

### `AuthTag` 新規型

- `pub struct AuthTag { inner: [u8; 16] }`
- AES-GCM 認証タグ専用。長さ 16B 固定（GCM 仕様）
- `try_new(bytes: &[u8]) -> Result<AuthTag, DomainError>`：長さ検証
- `as_array(&self) -> &[u8; 16]`

### メソッド

| 関数名 | 可視性 | シグネチャ | 仕様 |
|-------|------|----------|------|
| `WrappedVek::new` | `pub` | `(ciphertext: Vec<u8>, nonce: NonceBytes, tag: AuthTag) -> Result<WrappedVek, DomainError>` | ciphertext 空なら `Err(WrappedVekEmpty)`、ciphertext 長 < 32B（VEK 32B が最小、tag は別フィールドなので含まない）なら `Err(WrappedVekTooShort)` |
| `WrappedVek::ciphertext` | `pub` | `(&self) -> &[u8]` | フィールドアクセサ |
| `WrappedVek::nonce` | `pub` | `(&self) -> &NonceBytes` | 同上 |
| `WrappedVek::tag` | `pub` | `(&self) -> &AuthTag` | 同上 |
| `WrappedVek::into_parts` | `pub` | `(self) -> (Vec<u8>, NonceBytes, AuthTag)` | 解体（永続化シリアライズ用） |

### 永続化との対応

- SQLite カラムは Sub-D で `wrapped_vek_pw BLOB` 単一カラムに **`WrappedVek` 全体を `bincode` または手書きフォーマットでシリアライズ**して保存
- Sub-A はシリアライズ実装を持たず、`WrappedVek::into_parts` / `WrappedVek::new` の境界 API のみ提供

## 設計判断の補足（nonce / AEAD 境界）

### なぜ `WrappedVek` の内部構造を分離型化するか（Boy Scout Rule）

- **既存 `WrappedVek` の問題**: 内部が `Vec<u8>` 単一フィールド（ciphertext + nonce + tag を連結）。Sub-D の AEAD 復号時に「tag は末尾 16 byte、nonce は手前 12 byte、残りが ciphertext」のような **byte offset 演算が呼び出し側に漏れ出す**
- **改善後**: `WrappedVek { ciphertext: Vec<u8>, nonce: NonceBytes, tag: AuthTag }` の 3 フィールド構造。byte offset 演算は **`WrappedVek::new` / `into_parts` に閉じる**（Tell, Don't Ask）
- **後方互換**: Issue #7 時点で `WrappedVek` を実際に AEAD 復号に使った呼出元は **存在しない**（永続化層 `vault-persistence` は `WrappedVek` を生バイトとして読み書きするのみ）。Sub-A 時点での破壊変更は安全
- **永続化フォーマット**: SQLite カラムは引き続き `wrapped_vek_pw BLOB` 単一カラムで OK（Sub-D で `WrappedVek` 全体を 1 BLOB にシリアライズ / デシリアライズする経路を確定）

### なぜ `NonceCounter` の責務を再定義するか（Boy Scout Rule）

- **Sub-0 凍結との不整合**: 既存実装は「8B random prefix + 4B u32 counter」で nonce 値そのものを生成する設計。一方 Sub-0 凍結は **「random nonce 12B + 別軸の暗号化回数監視カウンタ」**
- **新しい責務**: `NonceCounter` は「**この VEK で何回暗号化したか**を u64 で数えるだけ」。nonce 値生成には**関与しない**
- **API 破壊変更**: `NonceCounter::next() -> Result<NonceBytes, DomainError>` を削除し、`NonceCounter::increment() -> Result<(), DomainError>` に置換。`new()` / `resume()` / `current()` は維持（u32 → u64 型変更）
- **既存呼出元なし**: Issue #7 時点で `NonceCounter::next()` を呼ぶ実装は存在しない（テスト除く）。Sub-A 時点で安全に書き換え可能
- **永続化との整合**: vault ヘッダの `nonce_counter` カラムは Sub-D で **`u32` → `u64` に拡張**（既存は未使用カラムなので破壊変更影響なし）

### なぜ `Verified<T>` のコンストラクタを `pub(crate)` にするか（指摘 #4 で誇大宣伝を訂正）

- **構築可能経路の限定（型レベル保証）**: `Verified<Plaintext>` を構築できるのは `shikomi-core` crate 内の関数のみ。**外部 crate（shikomi-infra / bin crate）からは `Verified::new_from_aead_decrypt` を直接呼べない**
- **ただし AEAD 計算自体は `shikomi-infra`**: ここに本質的な制約がある。shikomi-core は AEAD アルゴリズム実装を持たない（pure Rust / no-I/O 制約）ため、AEAD 検証実行を**型レベルで完全保証することは不可能**
- **採用する解決策**: `verify_aead_decrypt(|| ...)` クロージャ経由 + `Plaintext::new_within_module` の `pub(in crate::crypto::verified)` 可視性絞り込み（指摘 #5 対応）+ Sub-C PR レビューでの「クロージャ内が AEAD 検証を実装しているか」の構造確認、の三段防御
- **誇大宣伝の訂正**: basic-design.md の従前の表記「**型レベル禁止**」は「**型レベル + 契約レベル + PR レビューレベルの三段防御**」に訂正（Rev1 で basic-design.md セキュリティ設計セクション更新）

## `AesGcmAeadAdapter`（**Sub-C 新規、shikomi-infra**）

### 型定義

- `pub struct AesGcmAeadAdapter;`（無状態 unit struct、`#[derive(Clone, Copy, Default)]`）
- **配置先**: `crates/shikomi-infra/src/crypto/aead/aes_gcm.rs`（モジュール `crypto::aead`、Sub-C 新規）
- **無状態根拠**: `aes-gcm` crate の `Aes256Gcm::new(key)` は鍵ごとに毎回構築する（軽量、内部 cipher state 不持有）。adapter 自体に state は不要、`Default::default()` で構築

### モジュール配置

```
crates/shikomi-infra/src/crypto/
  aead/                    +  本 Sub-C 新規モジュール
    mod.rs                 +  AesGcmAeadAdapter 再エクスポート
    aes_gcm.rs             +  AesGcmAeadAdapter 本体実装
    kat.rs                 +  NIST CAVP テストベクトル const 配列（test cfg）
```

### メソッド

| 関数名 | 可視性 | シグネチャ | 仕様 |
|-------|------|----------|------|
| `AesGcmAeadAdapter::encrypt_record` | `pub` | `(&self, key: &impl AeadKey, nonce: &NonceBytes, aad: &Aad, plaintext: &[u8]) -> Result<(Vec<u8>, AuthTag), CryptoError>` | per-record 暗号化。`key.with_secret_bytes(\|bytes\| ...)` クロージャ内で `Aes256Gcm::new(GenericArray::from_slice(bytes))` 構築 → `aes_gcm::aead::AeadInPlace::encrypt_in_place_detached(nonce, aad.to_canonical_bytes(), &mut buf)` で **tag 分離方式**で暗号化（`buf` は plaintext のコピー、戻り値で `(buf, AuthTag::try_new(tag.as_slice())?)`）。**AAD は `Aad::to_canonical_bytes()` の 26B 固定**（既存型再利用）。**nonce_counter の increment は呼ばない**（呼出側 = Sub-D 責務、本ファイル §nonce_counter 統合契約 参照） |
| `AesGcmAeadAdapter::decrypt_record` | `pub` | `(&self, key: &impl AeadKey, nonce: &NonceBytes, aad: &Aad, ciphertext: &[u8], tag: &AuthTag) -> Result<Verified<Plaintext>, CryptoError>` | per-record 復号 + AEAD タグ検証。`key.with_secret_bytes(\|bytes\| ...)` クロージャ内で `Aes256Gcm::new(...)` → `decrypt_in_place_detached(nonce, aad.to_canonical_bytes(), &mut buf, tag.as_array())` で検証付き復号。**タグ検証成功時のみ** `verify_aead_decrypt(\|\| Ok(Plaintext::new_within_module(buf)))?` 経由で `Verified<Plaintext>` を構築（shikomi-core の caller-asserted マーカー契約に従う）。**タグ検証失敗時** `Err(CryptoError::AeadTagMismatch)` を返し `Plaintext` を構築しない |
| `AesGcmAeadAdapter::wrap_vek` | `pub` | `(&self, kek: &impl AeadKey, nonce: &NonceBytes, vek: &Vek) -> Result<WrappedVek, CryptoError>` | VEK の wrap（Sub-D `derive_new_wrapped_pw` / `derive_new_wrapped_recovery` から呼出）。AAD は **空** `&[]`（`WrappedVek` は vault ヘッダの一部として独立 AEAD タグで保護される、record AAD とは別経路）。`vek.with_secret_bytes(\|vek_bytes\| ...)` で 32B 平文を取得 → AES-GCM 暗号化 → `WrappedVek::new(ciphertext, nonce.clone(), tag)` |
| `AesGcmAeadAdapter::unwrap_vek` | `pub` | `(&self, kek: &impl AeadKey, wrapped: &WrappedVek) -> Result<Verified<Plaintext>, CryptoError>` | VEK の unwrap。AAD は空 `&[]`、ciphertext + tag は `WrappedVek` から取得。**戻り値の `Verified<Plaintext>` 内 32B が新 `Vek::from_array` の入力**となる（呼出側 = Sub-D / Sub-E、本ファイル §AEAD 復号後の VEK 復元経路 参照） |

### `aes-gcm` crate 呼出契約

| 項目 | 値 / 方針 |
|-----|---------|
| 呼出パス | `aes_gcm::Aes256Gcm::new(GenericArray::from_slice(&[u8;32]))` → `aead::AeadInPlace::{encrypt,decrypt}_in_place_detached` の **tag 分離 API のみ**使用。`Aead::encrypt` / `Aead::decrypt` の連結返却 API（ciphertext + tag を `Vec<u8>` 末尾連結）は使わない（`WrappedVek` の構造分離型化と整合、byte offset 演算を呼出側に漏らさない、Tell-Don't-Ask） |
| Algorithm | `aes_gcm::Aes256Gcm`（AES-256 鍵長 32B、GMAC 認証タグ 16B、IV 12B 固定） |
| Cipher key 型 | `aes_gcm::Key<aes_gcm::Aes256Gcm>` = `GenericArray<u8, U32>`、内部は `&[u8;32]` ラッパ |
| Nonce 型 | `aes_gcm::Nonce<aes_gcm::Aes256Gcm>` = `GenericArray<u8, U12>`、`NonceBytes::as_array()` から構築 |
| KAT | NIST CAVP "GCM Test Vectors"（`gcmEncryptExtIV256.rsp` / `gcmDecrypt256.rsp`）から **encryption / decryption 各 8 件以上**を `crypto/aead/kat.rs` に const として埋め込み、`#[cfg(test)] fn aes_256_gcm_nist_cavp_kat()` で実行（CI ジョブ `test-infra` で必須 pass）。出典: https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/cavp-testing-block-cipher-modes |
| `subtle` v2.5+ 制約 | tag 比較は `aes-gcm` 内部の constant-time 経路に委譲。adapter 側で `==` による tag 比較を**書かない**（`tech-stack.md` §4.7 `subtle` 行の「自前で `==` 演算子を使った定数比較を書くことは禁止」契約） |
| 中間バッファ zeroize | `encrypt_in_place_detached` / `decrypt_in_place_detached` の入力 `buf: Vec<u8>` は **`Zeroizing<Vec<u8>>` で囲む**。`with_secret_bytes` クロージャを抜けるとき `Drop` で zeroize（plaintext / VEK 中間バッファの滞留時間最小化、L2 メモリスナップショット対策） |
| `aes-gcm` feature | `aes` `alloc` + `zeroize` 連携を有効化（`tech-stack.md` §4.7 凍結） |

### nonce_counter 統合契約（**Sub-D 責務との分離**）

`AesGcmAeadAdapter::encrypt_record` は **`NonceCounter::increment` を呼ばない**。理由：
- **責務分離（SRP）**: adapter は「鍵 + nonce + AAD + plaintext → ciphertext + tag」の暗号関数。nonce 上限管理は **vault リポジトリ層（Sub-D）の責務**
- **複数経路の整合**: `encrypt_record` は per-record 暗号化、`wrap_vek` は VEK wrap、両者で `NonceCounter` の更新タイミングが異なる（per-record は毎回 increment、VEK wrap は初回のみ）。adapter 内に閉じ込めると分岐が複雑化

呼出側（Sub-D）の契約：
1. **encrypt 前**: `nonce_counter.increment()?;` で上限チェック → `Err(CryptoError::NonceLimitExceeded)` なら fail fast、Sub-F の `vault rekey` フローに誘導
2. **encrypt 実行**: `let nonce = rng.generate_nonce_bytes(); adapter.encrypt_record(&vek, &nonce, &aad, plaintext)?;`
3. **永続化時**: `nonce_counter.current()` を vault ヘッダに保存

### AEAD 復号後の VEK 復元経路

`AesGcmAeadAdapter::unwrap_vek` の戻り値 `Verified<Plaintext>` から `Vek` を復元する経路：

1. `let verified = adapter.unwrap_vek(&kek_pw, &wrapped_vek_by_pw)?;` （shikomi-infra / Sub-D）
2. `let plaintext = verified.into_inner();` （shikomi-core `Verified::into_inner`）
3. `let bytes_array: [u8; 32] = plaintext.expose_secret().try_into().map_err(|_| CryptoError::AeadTagMismatch)?;` （長さ検証、32B 以外は AEAD 復号成功でも構造異常として拒否）
4. `let vek = Vek::from_array(bytes_array);` （Sub-A `Vek::from_array`）
5. `plaintext` / `bytes_array` は scope 抜けで Drop 連鎖 zeroize（L2 対策）

**注**: 上記手順 3 の長さ検証は **Sub-D で実装**（adapter の責務外）。adapter は AEAD 検証付き復号までを保証し、`Plaintext` の中身が 32B である保証は **wrap 時の構造（`WrappedVek::new(ct, nonce, tag)` で `ct.len() >= 32`）に由来する間接保証**であり、復号後の長さ検証は Sub-D の `unwrap_vek_with_password` 等の上位関数で必須。

### AAD 入れ替え攻撃検出（per-record）

| 攻撃シナリオ | 検出経路 |
|---|---|
| L1 攻撃者が record A の ciphertext + tag を record B の (record_id_B, version_B, created_at_B) と組み合わせて daemon に注入 | `decrypt_record(&vek, &nonce_A, &aad_B, &ciphertext_A, &tag_A)` の AEAD 検証で **`Aad::to_canonical_bytes()` が AAD として GMAC 計算に組み込まれる**ため、`aad_A != aad_B` で tag 不一致 → `CryptoError::AeadTagMismatch` で fail fast |
| ロールバック攻撃: 古い record ciphertext + 旧 vault_version の AAD を新 vault に注入 | `vault_version` は `Aad` 内に含まれる（既存規約、`crypto_data.rs` `Aad::to_canonical_bytes` `[16..18]`）、AEAD 検証で旧 version の AAD が新 vault key で復号失敗 |
| 同 record_id で内容差し替え | `record_id` は `Aad` 内 `[0..16]` に含まれ、ciphertext と紐付く。差し替え時 AAD は同じだが nonce が異なれば AEAD 計算結果が変わり tag 不一致。**ただし同じ nonce + 同じ AAD で別 plaintext を暗号化された場合は L1 受容範囲外**（random nonce 12B の衝突確率 $\le 2^{-32}$ で確率的排除、Sub-0 §脅威モデル §4 L1 受容） |

検証: テスト `tc_c_property_aad_swap_rejected` で property test として網羅（本書 §後続テスト設計引継ぎ + test-design.md §12.5 参照）。

## 設計判断の補足（Sub-C 追加分）

### なぜ `AesGcmAeadAdapter` を unit struct にするか

- **無状態**: `aes-gcm` crate の `Aes256Gcm::new(key)` は呼出毎に cipher state を構築する軽量操作（key schedule のみ、AES round key 展開）。adapter 内に state を持つ必要なし
- **Default 構築**: `let adapter = AesGcmAeadAdapter::default();` で即構築可能、テストで mock 不要、Sub-D / Sub-E から複数経路で並列呼出しても安全
- **Sub-B `Argon2idAdapter` / `Bip39Pbkdf2Hkdf` と命名・構築規約整合**

### なぜ `encrypt_record` / `decrypt_record` と `wrap_vek` / `unwrap_vek` を別メソッドにするか

| 観点 | per-record (`encrypt_record`/`decrypt_record`) | VEK wrap (`wrap_vek`/`unwrap_vek`) |
|---|---|---|
| 鍵 | `Vek`（VEK） | `Kek<KekKindPw>` または `Kek<KekKindRecovery>`（KEK） |
| 平文サイズ | 任意（レコード本文、可変長） | 固定 32B（VEK 本体） |
| AAD | `Aad::to_canonical_bytes()` 26B（record_id + version + created_at） | 空 `&[]`（vault ヘッダ独立 AEAD タグで別途保護、Sub-D） |
| 戻り値 | `(Vec<u8>, AuthTag)` または `Verified<Plaintext>` | `WrappedVek` または `Verified<Plaintext>` |
| 呼出頻度 | レコード暗号化のたびに `NonceCounter::increment` 必須 | 初回 vault 作成 + change-password / rekey 時のみ |

両者を 1 メソッドに統合すると分岐コードが膨らみ、AAD の有無 / 鍵型 / nonce_counter 更新タイミングが混在する。**4 メソッドに分割して責務を明示**するほうが Tell-Don't-Ask + Single Responsibility 整合。

### なぜ `decrypt_record` の戻り値を `Verified<Plaintext>` にするか

- **AEAD 検証成功の型レベル証跡**: shikomi-core の `Verified<T>` は「呼び出し側主張マーカー」。adapter が `aes_gcm::aead::AeadInPlace::decrypt_in_place_detached` で **タグ検証成功時のみ** `verify_aead_decrypt(|| Ok(Plaintext::new_within_module(buf)))` を呼ぶことで、Sub-D 以降の経路に「この `Plaintext` は AEAD 検証済み」という型レベル契約を引き渡す
- **タグ検証失敗時は `Plaintext` を構築しない**: `Err(CryptoError::AeadTagMismatch)` を返し、`Plaintext::new_within_module` を呼ばない。これにより **未検証 ciphertext を平文として扱う事故**を構造禁止（Sub-A `verify_aead_decrypt` の caller-asserted マーカー契約と整合、`pub(in crate::crypto::verified)` 可視性絞り込み）

### なぜ AAD を `Aad::to_canonical_bytes()` の `[u8; 26]` で固定するか

- **既存型の再利用（Boy Scout Rule、新規型作成禁止）**: `shikomi_core::vault::crypto_data::Aad` が **既に 26B 固定の決定論的バイト列を提供**（`record_id 16B + vault_version 2B BE + created_at_micros 8B BE`）。Sub-C で再発明すると DRY 違反、設計判断の一貫性破綻
- **vault_version の big-endian エンコーディングは既存契約**: `Aad::to_canonical_bytes()` のレイアウトを変更する場合は **`VaultVersion` のメジャーアップとセットでのみ許可**（既存 doc-comment 規約）。Sub-C は本契約を消費するのみ、変更を要求しない

## Sub-C → 後続 Sub への引継ぎ

### Sub-D（#42）への引継ぎ

1. **`HeaderAeadKey` への `AeadKey` impl 追加**: 同形パターンで Boy Scout、`crypto-types.md` 同期更新
2. **`derive_new_wrapped_pw` / `derive_new_wrapped_recovery` の AES-GCM wrap 部分**: `AesGcmAeadAdapter::wrap_vek` を呼出（`errors-and-contracts.md` §VekProvider Sub-D 追記対象）
3. **`unwrap_vek_with_password` / `unwrap_vek_with_recovery` の実装**: `unwrap_vek` 戻り値 `Verified<Plaintext>` から `Vek` 復元（本書 §AEAD 復号後の VEK 復元経路 手順）
4. **`encrypt_record` 呼出前の `NonceCounter::increment`**: vault リポジトリ層で必須、本書 §nonce_counter 統合契約 を Sub-D `repository-and-migration.md` に反映
5. **vault ヘッダ独立 AEAD タグ**: `HeaderAeadKey` を使った `AesGcmAeadAdapter::encrypt_record(&header_key, ...)` 経路、AAD は ヘッダ全体の bincode 等の正規化バイト列（Sub-D 確定）

### Sub-F（#44）への引継ぎ

- `vault rekey` フローで `CryptoError::NonceLimitExceeded` を捕捉、新 VEK 生成 + 全レコード再暗号化フロー
- MSG-S11 文言確定（nonce 上限到達時のユーザ向け案内）
