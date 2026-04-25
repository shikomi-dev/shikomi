# 詳細設計書 — nonce / AEAD 境界（`nonce-and-aead`）

<!-- 親: docs/features/vault-encryption/detailed-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/detailed-design/nonce-and-aead.md -->
<!-- 主担当: Sub-A (#39) で型契約 + Verified<T>、Sub-C (#41) で AEAD 実装結合。 -->

## 対象型

- `shikomi_core::crypto::verified::Plaintext`
- `shikomi_core::crypto::verified::Verified<T>` + `verify_aead_decrypt` ラッパ関数
- `shikomi_core::vault::nonce::NonceCounter`（責務再定義、Boy Scout Rule）
- `shikomi_core::vault::nonce::NonceBytes`（拡張、Boy Scout Rule）
- `shikomi_core::vault::crypto_data::WrappedVek`（内部構造分離型化、Boy Scout Rule）
- `shikomi_core::vault::crypto_data::AuthTag`（新規）

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
