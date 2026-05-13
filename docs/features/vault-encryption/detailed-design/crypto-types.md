# 詳細設計書 — 鍵階層型（`crypto-types`）

<!-- 親: docs/features/vault-encryption/detailed-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/detailed-design/crypto-types.md -->
<!-- 主担当: Sub-A (#39)。Sub-B〜F は本分冊を READ → EDIT で各 KEK 派生実装に追記する。 -->

## 対象型

- `shikomi_core::crypto::key::Vek`
- `shikomi_core::crypto::key::Kek<Kind>` + phantom marker `KekKindPw` / `KekKindRecovery`
- `shikomi_core::crypto::header_aead::HeaderAeadKey`

## `Vek`

### 型定義

- `pub struct Vek { inner: SecretBox<Zeroizing<[u8; 32]>> }`

### コンストラクタ

| 関数名 | 可視性 | シグネチャ概要 | 不変条件 / 契約 |
|-------|------|------------|--------------|
| `Vek::from_array` | `pub` | `[u8; 32]` を受取り `Vek` を返す。失敗しない（型レベルで 32B 強制） | 受取った `[u8; 32]` は内部 `SecretBox<Zeroizing<[u8;32]>>` に**ムーブ**される（呼び出し側のローカル変数も即 zeroize 対象に入るよう、呼び出し側は `[u8; 32]` を構築直後に `Vek::from_array` へ渡し、ローカルを `Zeroizing` でラップしておくこと。Sub-B 設計時に呼び出し側パターンを明示） |

### 公開しない経路

- `expose_within_crate(&self) -> &[u8; 32]`: `pub(crate)` 可視性。`shikomi-core` 内部からのみ呼出可。`shikomi-infra` の AEAD 実装は本関数を呼べないが、Sub-C 設計時に「**`shikomi-core::crypto` 経由のラッパ関数**経由で間接アクセスする経路」を確定する（外部 crate に生バイトを渡す API は提供しない、`Verified<Plaintext>` で抽象化）

### Drop 契約

- `Drop` 実装は内部 `SecretBox<Zeroizing<[u8;32]>>` の `Drop` に委譲。`Zeroizing` の `Drop` で 32B 全 zeroize
- `Drop` 順序保証: Rust 標準のドロップ順序（フィールド宣言順の逆順）。`Vek` は単一フィールドなので順序依存なし

### 禁止トレイト

- `Clone`: **未実装**。複製禁止により VEK のメモリ滞留範囲を1箇所に限定
- `Copy`: 未実装（`Drop` 持つため不可、保証）
- `Display`: 未実装。誤フォーマット出力を**コンパイル時禁止**
- `serde::Serialize` / `serde::Deserialize`: 未実装。誤シリアライズを**コンパイル時禁止**
- `PartialEq` / `Eq`: 未実装（VEK 比較は `subtle::ConstantTimeEq` を Sub-B/C で使う、通常 `==` を**コンパイル時禁止**）

### 提供トレイト

- `Debug`: **`[REDACTED VEK]` 固定文字列**を出力（CI grep で文字列リテラルを検証）
- `Drop`: 上記
- **`AeadKey`**（Sub-C 新規、Boy Scout Rule、`crypto::aead_key::AeadKey` trait に対する impl）: `fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8;32]) -> R) -> R { f(self.expose_within_crate()) }`。クロージャインジェクション経由で shikomi-infra の `AesGcmAeadAdapter` に **借用のみ**を渡す（所有権は `Vek` に保持）。`expose_within_crate` の `pub(crate)` 可視性は変更せず、trait 経由のクロージャでのみ外部 crate に開放（`nonce-and-aead.md` §`AeadKey` trait + 契約 C-15 参照）

## `Kek<Kind>`

### 型定義

- `pub struct Kek<Kind: KekKind>(SecretBox<Zeroizing<[u8;32]>>, PhantomData<Kind>);`
- `pub trait KekKind: 'static + Sealed {}` — Sealed トレイトパターンで外部 crate での `KekKind` 実装を**禁止**
- `pub struct KekKindPw;` / `pub struct KekKindRecovery;` — 各々 `KekKind` を実装（Sealed trait は `shikomi_core::crypto::key` 内の `mod sealed { pub trait Sealed {} }` で隠蔽、外部 crate からは `KekKind` を継承不可）

### コンストラクタ

| 関数名 | 可視性 | シグネチャ概要 | 不変条件 |
|-------|------|------------|--------|
| `Kek::<KekKindPw>::from_array` | `pub` | `[u8; 32]` 受取 | 32B 強制、`Vek` と同様 |
| `Kek::<KekKindRecovery>::from_array` | `pub` | `[u8; 32]` 受取 | 同上 |

### 公開しない経路

- `expose_within_crate(&self) -> &[u8; 32]`: `pub(crate)`、Sub-B / Sub-C の AEAD 操作実装からのみ呼出可

### Drop 契約

- `Vek` と同等。内部 `SecretBox<Zeroizing<[u8;32]>>` の `Drop` で 32B 全 zeroize

### 禁止トレイト

- `Clone` / `Copy` / `Display` / `serde::Serialize` / `PartialEq` / `Eq`: いずれも未実装（`Vek` と同等）

### 提供トレイト

- `Debug`: **型パラメータごとに区別**して出力（Sub-D 設計時に Debug 文字列の機械検証ルール確定）
  - `Kek<KekKindPw>` → `[REDACTED KEK<Pw>]`
  - `Kek<KekKindRecovery>` → `[REDACTED KEK<Recovery>]`
- `Drop`: 上記
- **`AeadKey`**（Sub-C 新規、Boy Scout Rule）: `Kek<KekKindPw>` / `Kek<KekKindRecovery>` 両方に impl。`AesGcmAeadAdapter::wrap_vek` / `unwrap_vek` への `key: &impl AeadKey` 引数を満たす。phantom 型による KEK 取り違え禁止契約（C-6）は **trait 制約レベルでも継続**（`wrap_vek(&kek_pw, ...)` と `wrap_vek(&kek_recovery, ...)` は両方コンパイル可能だが、呼出側の関数シグネチャが `&Kek<KekKindPw>` を要求すれば C-6 で型レベル弾き）

### Sealed trait の意図

- 外部 crate（`shikomi-infra` / bin crate）が独自の `KekKind` を定義して `Kek<MyCustomKind>` を構築する経路を**コンパイル時禁止**。鍵階層の用途追加は必ず `shikomi-core` 側の Sub-A 設計改訂を経由させる
- Sealed trait のパターンは `https://rust-lang.github.io/api-guidelines/future-proofing.html#sealed-traits-protect-against-downstream-implementations-c-sealed` 参照

## `HeaderAeadKey`

### 型定義

- `pub struct HeaderAeadKey { inner: SecretBox<Zeroizing<[u8;32]>> }`

### コンストラクタ

- `pub fn from_kek_pw(kek: &Kek<KekKindPw>) -> HeaderAeadKey`
  - **同じ鍵バイトをコピー**して構築（`expose_within_crate` 経由で `[u8;32]` を読み出し、`HeaderAeadKey` 内に新 `SecretBox` を作る）
  - 受け取り元の `Kek<KekKindPw>` は呼び出し側のスコープで生き続ける（参照のみ取る）。`HeaderAeadKey` の Drop は独立に発火し、`HeaderAeadKey` 用の 32B が独立に zeroize される
  - **`Kek<KekKindRecovery>` を引数に取るオーバーロードは提供しない**（コンパイルエラーで弾く、Sub-0 凍結のヘッダ AEAD 鍵経路 = KEK_pw のみ）

### 公開しない経路

- `expose_within_crate(&self) -> &[u8; 32]`: `pub(crate)`、ヘッダ AEAD 検証関数（Sub-D 実装）からのみ呼出可

### Drop 契約

- `Vek` / `Kek<Kind>` と同等。内部 `SecretBox<Zeroizing<[u8;32]>>` の `Drop` で 32B 全 zeroize

### 禁止トレイト（**指摘 #6 対応で本セクション追加**）

- `Clone`: **未実装**。複製禁止
- `Copy`: 未実装（`Drop` 持つため不可、保証）
- `Display`: 未実装
- `serde::Serialize` / `serde::Deserialize`: 未実装
- `PartialEq` / `Eq`: 未実装（鍵比較は `subtle::ConstantTimeEq`、通常 `==` を**コンパイル時禁止**）

### 提供トレイト（**指摘 #6 対応で本セクション追加**）

- `Debug`: **`[REDACTED HEADER AEAD KEY]` 固定文字列**を出力（CI grep で文字列リテラルを検証、`Vek` / `Kek<_>` と同等の機械検証ルール）
- `Drop`: 上記
- **`AeadKey`**（**Sub-D で Boy Scout 完成**、Sub-C で予告した同形パターンを実装）: `fn with_secret_bytes<R>(&self, f: impl FnOnce(&[u8;32]) -> R) -> R { f(self.expose_within_crate()) }`。Sub-D の vault ヘッダ独立 AEAD 検証経路で `AesGcmAeadAdapter::encrypt_record(&header_key, ..., &Aad::HeaderEnvelope(canonical_bytes), &[])` / `decrypt_record(&header_key, ..., &Aad::HeaderEnvelope(...), ...)` を呼ぶために必要。`Vek` / `Kek<_>` impl と同形（Sub-C 凍結契約 C-15 維持、`expose_within_crate` の `pub(crate)` 可視性は変更せず）。Sub-D で `repository-and-migration.md` §`HeaderAeadEnvelope` と同期

### Sub-0 凍結の型表現

- Sub-0 §脅威モデル §4 L1 §対策(c) で凍結した「**ヘッダ AEAD タグの鍵 = KEK_pw 流用**、kdf_params 改竄は KDF 出力変化で間接検出」を**型レベルで強制**
- 具体的には: ヘッダ AEAD 検証関数（Sub-D 実装）は `unverify_header(&HeaderAeadKey, &EncryptedHeader) -> Result<Verified<DecryptedHeader>, CryptoError>` のシグネチャを取り、**`Kek<KekKindPw>` 直接ではなく `HeaderAeadKey` を要求**する。これにより「ヘッダ復号には KEK_pw 派生鍵」という設計制約が型レベルで明示される

## 設計判断の補足（鍵階層型）

### なぜ `Vek` を独立 newtype にするか（既存 `SecretBytes` で済まさない）

- **意味論的区別**: `SecretBytes` は「任意長の秘密バイト列」。`Vek` は「**32 byte 固定の VEK 本体**」。型シグネチャ上で「VEK を渡す」と「任意秘密を渡す」が混同されると、Sub-B の `wrap_with_kek_pw(vek: &SecretBytes, kek: &Kek<KekKindPw>) -> WrappedVek` のような関数でうっかり `MasterPassword` を渡しても通ってしまう
- **Drop 時 zeroize の経路保証**: `SecretBytes` の Drop は内部 `String` の zeroize を経由する間接経路。`Vek` は **`[u8;32]` を直接 `Zeroizing<[u8;32]>` で保持**することで、`Drop` 時の zeroize 範囲が静的に確定する（ヒープ上の連続 32 byte のみ）
- **`expose_within_crate()` の境界制御**: `Vek` 内部の `[u8;32]` 取り出しは `pub(crate)` 可視性に制限。**外部 crate からは取り出せない**ため、誤って `bin` crate（CLI / GUI）が VEK 生バイトを触る経路を**型レベルで禁止**。Sub-B / Sub-D の AEAD 操作実装のみが crate 内アクセス可能
- **可視性ポリシー差別化**: 鍵バイト型（`Vek` / `Kek<_>` / `HeaderAeadKey`）は `pub(crate)` で外部非公開、KDF 入力型（`MasterPassword` / `RecoveryMnemonic`）は `pub` で shikomi-infra への正規経路として開放。差別化の根拠と判断基準は `password.md` §可視性ポリシー差別化（鍵バイト vs KDF 入力）に集約（Sub-B Rev2 工程5 服部・ペテルギウス指摘で明文化）

### なぜ `Kek<Kind>` を phantom-typed にするか

- **取り違え禁止**: KEK_pw（Argon2id 由来）と KEK_recovery（PBKDF2+HKDF 由来）は同じ 32 byte の鍵だが、**用途が異なる**。`wrapped_VEK_by_pw` は `KekPw` でしか復号してはいけない、`wrapped_VEK_by_recovery` は `KekRecovery` でしか復号してはいけない。実装ミスで取り違えると、復号は AEAD タグ検証で失敗するが、エラー文言が「タグ不一致」となり**真因（鍵経路の取り違え）が隠蔽される**
- **コンパイルエラーで弾く**: `unwrap_with(wrapped: &WrappedVek, kek: &Kek<KekKindPw>)` のような関数シグネチャに `Kek<KekKindRecovery>` を渡すと**コンパイルエラー**になる
- **代替案との比較**: enum `enum Kek { Pw([u8;32]), Recovery([u8;32]) }` 案も検討したが、(a) match のたびに「これは Pw か Recovery か」の runtime 分岐が増える、(b) フィールド型は同じなので enum での区別は意味論のみで型安全性ゼロ、(c) phantom-typed なら関数シグネチャで型を確定でき DRY、の 3 点で phantom-typed を採用

### なぜ `HeaderAeadKey` を独立型にするか（`Kek<KekKindPw>` を直接使わない）

- **用途分離**: `Kek<KekKindPw>` は `wrapped_VEK_by_pw` の AES-GCM unwrap に使う鍵。`HeaderAeadKey` は vault ヘッダ全体の独立 AEAD タグ検証に使う鍵。**同じ 32 byte だが、暗号操作の対象が異なる**
- **Drop タイミングの独立**: ヘッダ検証完了 → `HeaderAeadKey` 即 Drop（zeroize）→ そのあと `Kek<KekKindPw>` で `wrapped_VEK_by_pw` unwrap という**段階的な滞留時間最小化**を、型ごとの `Drop` 経路で構造的に担保
- **Sub-0 凍結の明示**: 「ヘッダ AEAD 鍵 = KEK_pw 流用」を**コードを読まずに型シグネチャだけで認識**できる。Sub-D 実装者が誤って `Kek<KekKindRecovery>` を渡そうとすると `from_kek_pw` の引数型でコンパイルエラー
