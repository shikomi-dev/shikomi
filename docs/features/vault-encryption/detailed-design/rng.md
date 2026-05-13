# 詳細設計書 — CSPRNG 単一エントリ点（`rng`）

<!-- 親: docs/features/vault-encryption/detailed-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/detailed-design/rng.md -->
<!-- 主担当: Sub-B (#40)。Sub-C / Sub-D / Sub-E は本分冊を READ → EDIT で各用途追記。 -->

## 対象型

- `shikomi_infra::crypto::rng::Rng`（Sub-B 新規）

## 設計の動機（Sub-0 凍結文言の物理実装）

Sub-0 §脅威モデル §3 保護資産インベントリ + REQ-S02 で凍結された:

> `KdfSalt` 16B、shikomi-core 側は `try_new(&[u8])` のみ、**`shikomi-infra::crypto::Rng::generate_kdf_salt() -> KdfSalt`** が単一エントリ点（Sub-0 凍結文言を Clean Architecture 整合的に再解釈）

を物理実装する。**shikomi-core は no-I/O 制約のため CSPRNG を直接呼ばない**ため、shikomi-infra に**唯一の OsRng エントリ点**を集約する。

## 型定義

- `pub struct Rng;`（無状態 struct、`#[derive(Clone, Copy, Default)]`）
- 内部に `rand_core::OsRng` を保持しない（メソッド呼出時に `OsRng` を直接使用、shared state なし）
- 全メソッドが `&self`（`&mut self` ではない）— `OsRng` は `RngCore` を `&mut` で要求するが、`OsRng` 自体は `Copy` で複製コストゼロのため、メソッド内で `OsRng` インスタンスを構築 → fill_bytes → drop の単発パターンで `&self` 維持

## メソッド

| 関数名 | 可視性 | シグネチャ | 用途 | 担当 Sub |
|-------|------|----------|------|---------|
| `Rng::generate_kdf_salt` | `pub` | `(&self) -> KdfSalt` | Argon2id 入力 salt 16B 生成。Sub-D の `vault encrypt` 初回 / `change-password` で呼出 | Sub-B（実装）/ Sub-D（呼出） |
| `Rng::generate_vek` | `pub` | `(&self) -> Vek` | VEK 32B 生成。Sub-D の `vault encrypt` / `vault rekey` で呼出 | Sub-B（実装）/ Sub-D / Sub-F（呼出） |
| `Rng::generate_nonce_bytes` | `pub` | `(&self) -> NonceBytes` | per-record AEAD nonce 12B 生成。Sub-C の AEAD 暗号化のたびに呼出 | Sub-B（実装）/ Sub-C（呼出） |
| `Rng::generate_mnemonic_entropy` | `pub` | `(&self) -> Zeroizing<[u8; 32]>` | BIP-39 24 語生成元のエントロピー 256 bit。Sub-D の `vault recovery-show` 初回で呼出 → `bip39::Mnemonic::from_entropy` で 24 語化 → `RecoveryMnemonic::from_words` で型化 | Sub-B（実装）/ Sub-D（呼出） |

## 実装契約

### `OsRng` 呼出パターン

| 項目 | 値 / 方針 |
|-----|---------|
| crate | `rand_core::OsRng`（**正確な path**、`rand::rngs::OsRng` ではない、`tech-stack.md` §4.7 凍結） |
| バックエンド | `getrandom` crate（`tech-stack.md` §4.7 独立行登録、§4.3.2 暗号クリティカル） |
| OS syscall | Linux: `getrandom(2)`、macOS: `getentropy(2)`、Windows: `BCryptGenRandom`（`getrandom` crate 内部実装） |
| `RngCore::fill_bytes` 呼出 | 各 `generate_*` メソッド内で `OsRng.fill_bytes(&mut buf)` を呼ぶ。`buf` は `Zeroizing<[u8; N]>` で囲み、生成完了 → `Vek` / `KdfSalt` / `NonceBytes` 等にラップ → 元バッファは Drop で zeroize |
| エラー処理 | `OsRng::fill_bytes` は失敗時 panic（`getrandom` crate の挙動）。本契約では「**OS syscall 失敗 = システム不具合、Fail Fast で panic 許容**」とする。これは `tech-stack.md` §4.7 `getrandom` 行で凍結の通り、shikomi 対象 OS（Linux / macOS / Windows）で `getrandom` 失敗は事実上発生しない |
| `try_fill_bytes` の検討 | `RngCore::try_fill_bytes` で `Result` を返す経路もあるが、(a) shikomi の対象 OS では失敗しない、(b) Result を返すと呼出側が誤って `unwrap` する経路を作る、の 2 点で `fill_bytes` （panic 経路）を採用。代替として `try_fill_bytes` を `crypto::Rng` 内部で `expect("OsRng failed: system entropy unavailable")` で吸収する案も検討したが、エラーハンドリング無しが本来正しい挙動 |

### shikomi-core への戻り値

| 戻り値型 | 構築経路 |
|--------|---------|
| `KdfSalt` | `KdfSalt::try_new(&buf[..])` 経由（Sub-A 既存型、`try_new` は 16B 長を検証 → 内部 `[u8;16]` にコピー）。**生 `[u8;16]` を直接外部に晒さない** |
| `Vek` | `Vek::from_array(*buf)` 経由（Sub-A 既存型、`from_array` で 32B 受取 → `SecretBox<Zeroizing<[u8;32]>>` にラップ） |
| `NonceBytes` | `NonceBytes::from_random(*buf)` 経由（Sub-A 既存型、Sub-A で from_random API 追加済み、`[u8;12]` 受取） |
| `Zeroizing<[u8; 32]>` | エントロピー 32B を直接返す（`bip39::Mnemonic::from_entropy` の入力として使う、Sub-D で消費） |

### 中間バッファ zeroize の経路

```
OsRng.fill_bytes
        ↓
local: Zeroizing<[u8; N]>  ← 中間バッファ（Drop で zeroize）
        ↓
{Vek, KdfSalt, NonceBytes}::from_*
        ↓
戻り値（呼出側に所有権移動、内部は SecretBox<Zeroizing<...>> で保護）
```

- 中間バッファ → 戻り値型へのコピー後、ローカル `Zeroizing<[u8; N]>` がスコープ抜けで `Drop` 発火 → 元バッファ zeroize
- 戻り値の `Vek` / `KdfSalt` / `NonceBytes` は呼出側の所有 → 呼出側スコープ終了で `Drop` 連鎖

## CSPRNG 取り違え事故の構造防衛

| 事故パターン | 防衛策（Sub-B 実装） |
|----------|------------------|
| 開発者が `[0u8; 32]` 等の決定論値で `Vek::from_array` を直接呼ぶ | `Vek::from_array` 自体は型レベルで防げない（`[u8; 32]` を受け取る public API のため）。**CI grep で `Vek::from_array(\[` パターンを検出**（テスト以外で禁止、Sub-B test-design.md で TC-B-I0X として追加） |
| 開発者が `rand::thread_rng()` 等の thread-local PRNG を使う | shikomi-infra の Cargo.toml に `rand` crate を**追加しない**。`rand_core` のみ依存（`tech-stack.md` §4.7 凍結）。grep で `rand::thread_rng` / `SmallRng` 等の混入を検出 |
| 開発者が `getrandom::getrandom` を直接呼ぶ | `shikomi-infra::crypto::rng` モジュール以外で `getrandom::` を呼ぶことを CI grep で禁止 |

## 設計判断の補足

### なぜ `Rng` を `pub struct Rng;` の無状態 struct にするか

- **テスト容易性**: 構築コストゼロ（`Default` で構築可能）、テストで `Rng::default()` を作って渡すだけ
- **DI 容易性**: `Argon2idAdapter::derive_kek_pw(&MasterPassword, &KdfSalt) -> Result<...>` のような関数シグネチャに `Rng` を渡す必要がない（KDF 関数自体は CSPRNG 不要、salt は呼出側が事前に `Rng::generate_kdf_salt()` で生成済）
- **代替案との比較**:
  - **`trait Rng { ... }` で抽象化案**: テストで `MockRng` を差し込める利点があるが、`Vek` / `KdfSalt` / `NonceBytes` の構築経路を trait dispatch で実装すると Sub-A 既存型の API（`from_array` / `from_random`）と二重化する。`#[cfg(test)]` で `OsRng` を `MockRng` に差し替える pattern も検討したが、production code の経路を runtime dispatch に変える正当性が弱い
  - **採用**: 具体型 `Rng` 単一実装、テストでは `Rng::default()` で実装ごと使う（`OsRng` は CI 環境でも動作）

### なぜ `Rng::generate_*` を 4 メソッドに分けるか（汎用 `generate<N>(&self) -> [u8; N]` にしない）

- **戻り値型による意味論明示**: `generate_vek() -> Vek` のシグネチャだけで「これは VEK を返す」と読める（Tell, Don't Ask）。汎用 `generate::<32>() -> [u8; 32]` だと呼出側が「これは VEK か KEK か mnemonic か」を判断するメンタルコストが発生
- **取り違え防止**: `let salt = rng.generate_vek();` のような取り違えを **戻り値型でコンパイラが検出**。汎用 `[u8; N]` だと型レベルで区別できない
- **追加の用途は遅延**: Sub-C / Sub-D / Sub-F で「もう 1 種類のランダム値が必要」になった時にメソッドを追加（YAGNI）。`generate_session_token` / `generate_record_id_seed` 等の追加余地を残す

### なぜ `RngCore` trait 引数版（`<R: RngCore>(rng: &mut R, ...)`）を提供しないか

- **OsRng 単一エントリ点契約**: Sub-0 凍結 + Sub-A 設計書で「**`shikomi-infra::crypto::Rng` が単一エントリ点**」と固定済。`RngCore` 抽象を提供すると shikomi-infra 内の他モジュール（Sub-D など）が独自 RNG を渡す経路が開く
- **テスト経路**: テストでも `Rng::default()` を使う（`OsRng` は CI 環境で利用可能）。「決定論的テスト」が必要な場合は KAT（Argon2id RFC 9106 / BIP-39 trezor 等）で十分カバー
- **将来の拡張**: 将来「ハードウェア乱数源（Intel RDRAND 等）」を使いたくなった場合は `Rng` 内部を変更（`OsRng` → ハイブリッド実装）すれば良い。`Rng` の public API は維持

## 後続 Sub への引継ぎ

- **Sub-C**: `Rng::generate_nonce_bytes` を AEAD 暗号化のたびに呼出（per-record nonce 12B、random nonce 戦略）。`Rng::generate_*` の戻り値型を AEAD 入力に直接接続
- **Sub-D**: `vault encrypt` 初回フローで `Rng::generate_kdf_salt` + `Rng::generate_vek` + `Rng::generate_mnemonic_entropy` を順次呼出
- **Sub-D**: `vault rekey` 経路で `Rng::generate_vek` で新 VEK 生成、Sub-A 既存 `Vault::rekey_with(VekProvider)` 経路に接続
- **Sub-E**: VEK キャッシュ初期化経路で `Rng` を保持しない（VEK は Sub-D の unwrap 経路で生成、daemon 内で再生成不要）
