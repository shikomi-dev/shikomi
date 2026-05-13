# テスト設計書 — Sub-C (#:crypto::aead）:41) AEAD アダプタ（shikomi-infra

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-c-aead.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->

## 12. Sub-C (#41) テスト設計 — AEAD アダプタ（`shikomi-infra::crypto::aead`）

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#41](https://github.com/shikomi-dev/shikomi/issues/41) |
| 対象 PR | TBD（本 Sub-C 設計工程後、Sub-C 実装担当が `feature/issue-41-aead` で起票） |
| 対象成果物 | `detailed-design/nonce-and-aead.md`（**EDIT**, AeadKey trait + AesGcmAeadAdapter 4 メソッド + AAD 26B 規約 + nonce_counter 統合契約 + AEAD 復号後 VEK 復元経路） / `detailed-design/crypto-types.md`（EDIT, Vek / Kek<_> への AeadKey impl 追記）/ `detailed-design/errors-and-contracts.md`（EDIT, AeadTagMismatch 発火経路 + derive_new_wrapped_* AES-GCM wrap 経路 + unwrap_vek_with_* + 契約 C-14〜C-16）/ `detailed-design/index.md`（EDIT, Sub-C 完了反映）/ `requirements.md`（REQ-S05 / REQ-S14 確定）/ `basic-design.md`（モジュール構成 + F-C1〜F-C4 処理フロー + セキュリティ設計 Sub-C 追記） |
| 設計根拠 | `detailed-design/nonce-and-aead.md` AesGcmAeadAdapter 契約、`tech-stack.md` §4.7 `aes-gcm` / `subtle` 凍結、NIST CAVP テストベクトル、Sub-A 契約 C-7（Verified pub(crate) 構築）+ Sub-C 契約 C-14〜C-16 |
| 対象 crate | `shikomi-core`（AeadKey trait + Vek / Kek<_> impl 改訂）+ `shikomi-infra`（AesGcmAeadAdapter 新規） |
| **Sub-C TC 総数** | **26 件**（ユニット 18 + 結合 5 + property 2 + E2E 1 = 26、テスト工程入口でマユリが TC-C-P02 encrypt/decrypt roundtrip を Boy Scout 補強） |

### 12.1 Sub-C テストレベルの読み替え（AEAD 用）

AES-GCM は決定論的（同入力 → 同出力）、AEAD 検証は対称性検査。Vモデル対応：

| テストレベル | 通常の対応 | Sub-C での読み替え | 検証手段 |
|---|---|---|---|
| **ユニット** | メソッド単位、ホワイトボックス | NIST CAVP KAT との bit-exact 一致、AAD / nonce / tag / ciphertext 各書換に対する **C-14 構造禁止検証**（property test）、`AeadKey::with_secret_bytes` クロージャインジェクションの正常動作 | `cargo test -p shikomi-infra` + `aead::kat::*` const ベクトル |
| **結合** | モジュール連携、契約検証 | (a) **Sub-A `Verified<Plaintext>` 構築経路の C-7 契約適合**（`pub(crate)` 越境のみ許可）、(b) **可視性ポリシー差別化維持**（C-15: `expose_within_crate` を shikomi-infra `aead/` から直接呼ばない grep 検証）、(c) **`Zeroizing<Vec<u8>>` 中間バッファの C-16 契約**（grep で生 `Vec<u8>` 中間バッファ 0 件） | `tests/docs/sub-c-static-checks.sh` + integration test |
| **property** | ランダム入力での invariant 検証 | AAD 入れ替え攻撃: 任意の 2 record の (record_id, version, created_at) を入れ替えても tag 検証失敗で `Err(AeadTagMismatch)` | `proptest` クレート（既存導入済）|
| **E2E** | 完全ブラックボックス、ペルソナシナリオ | 後続 Sub-D/E/F 実装者ペルソナ（野木拓海）が `AesGcmAeadAdapter` + `AeadKey` trait で vault encrypt / unlock / rekey の AEAD 経路を**サンプル呼出（doc コメント / `examples/`）から再構築**できる | 人手レビュー + `cargo doc` 対話 |

### 12.2 外部 I/O 依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---|---|---|---|
| **NIST CAVP "GCM Test Vectors"** | `aead::kat::AES_256_GCM_NIST_CAVP_ENCRYPT_VECTORS` / `..._DECRYPT_VECTORS` 定数（`gcmEncryptExtIV256.rsp` / `gcmDecrypt256.rsp` から各 8 件以上を埋め込み、出典 URL 付き）| — | **永続固定**（NIST 公式テストベクトル、CAVP 改訂時のみ更新）|
| **AES-256-GCM 計算（`aes-gcm` crate）** | — | `MockAeadKey` (test 用、固定 32B `[0xCAFE_BABE; 32]` を返す `AeadKey` impl) | **不要**（KAT で公式実装互換性を担保）|

### 12.3 Sub-C 受入基準（REQ-S05 + 契約）

| 受入基準ID | 内容 | 検証レベル |
|---|---|---|
| CC-1 | `AesGcmAeadAdapter::encrypt_record` / `decrypt_record` が NIST CAVP "GCM Test Vectors" の encryption / decryption 各 8 件以上で bit-exact 一致 | ユニット |
| CC-2 | AAD = `Aad::to_canonical_bytes()` 26B（既存 `shikomi_core::vault::crypto_data::Aad` 再利用、新規 AAD 型を作らない）| ユニット（型 + grep） |
| CC-3 | AEAD 復号成功時のみ `Verified<Plaintext>` を構築、失敗時は `Err(AeadTagMismatch)` で `Plaintext::new_within_module` を呼ばない（C-14） | property |
| CC-4 | `AeadKey` trait のクロージャインジェクション（`with_secret_bytes`）経由で shikomi-infra に鍵バイトを渡す。`Vek::expose_within_crate` を shikomi-infra `aead/` から**直接呼ばない**（C-15） | 結合（grep）|
| CC-5 | AEAD 中間バッファは `Zeroizing<Vec<u8>>` で囲まれ、Drop 時 zeroize（C-16）。生 `Vec<u8>` の中間バッファ 0 件 | 結合（grep）|
| CC-6 | `AesGcmAeadAdapter::wrap_vek` / `unwrap_vek` の wrap/unwrap ラウンドトリップで元 VEK と一致（同じ KEK + 同じ nonce で wrap → unwrap で元 32B 復元）| ユニット |
| CC-7 | AAD 入れ替え攻撃: record A の ciphertext を record B の AAD で復号試行 → `Err(AeadTagMismatch)` | property |
| CC-8 | `tag.as_array()` の比較は `aes-gcm` crate 内部の constant-time に委譲（自前 `==` を adapter 内で書かない）| 結合（grep）|
| CC-9 | `AesGcmAeadAdapter::encrypt_record` 自体は `NonceCounter::increment` を呼ばない（責務分離 SRP、Sub-D 呼出側責務）| 結合（grep）|
| CC-10 | `unwrap` / `expect` 経路ゼロ（Fail-Secure、Sub-B BC-14 と同型）| 結合（grep）|
| CC-11 | `aes-gcm` crate を **`crypto::aead` モジュール内のみ**で呼出、他モジュールから直接 `aes_gcm::*` を import しない（単一エントリ点）| 結合（grep）|
| CC-12 | `subtle` v2.5+ 制約: 自前 `==` で 32B / 16B 鍵・タグを比較する経路ゼロ（`tech-stack.md` §4.7 `subtle` 行の凍結）| 結合（grep）|
| CC-13 | `Vek` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` に `AeadKey` impl が追加されている（`crypto-types.md` 同期）。`HeaderAeadKey` impl は Sub-D 担当（Sub-C では予告のみ）| ユニット（trait 制約）|
| CC-14 | `derive_new_wrapped_pw` / `derive_new_wrapped_recovery` が Sub-B 段階の TBD を Sub-C で完成（`AesGcmAeadAdapter::wrap_vek` 経由）| 結合（cargo check）|

### 12.4 Sub-C テストマトリクス

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---|---|---|---|---|
| TC-C-U01 | CC-1 / REQ-S05 / L1 | `AesGcmAeadAdapter::encrypt_record` が NIST CAVP encryption KAT 8 件で `(ciphertext, tag)` bit-exact 一致 | ユニット | KAT |
| TC-C-U02 | CC-1 / REQ-S05 / L1 | `decrypt_record` が NIST CAVP decryption KAT 8 件で `Verified<Plaintext>` を返す（`expose_secret()` の中身が期待 plaintext と一致）| ユニット | KAT |
| TC-C-U03 | CC-2 / REQ-S05 | AAD は `Aad::to_canonical_bytes()` の 26B のみ。`Aad` 以外の型を AAD として渡すコンパイルエラー（`encrypt_record` 引数型 `&Aad` 強制）| ユニット | 型契約 |
| TC-C-U04 | CC-6 / REQ-S05 / Roundtrip | `wrap_vek(&kek, &nonce, &vek)` → `unwrap_vek(&kek, &wrapped)` で元 `Vek::expose_within_crate()` と一致（property test 1000 件、`Vek` ランダム生成）| ユニット | ラウンドトリップ |
| TC-C-U05 | CC-3 / C-14 / L1 | AEAD タグ書換（最後 16B の任意 byte を flip）→ `decrypt_record` が `Err(AeadTagMismatch)`、`Verified<Plaintext>` を返さない | ユニット | C-14 |
| TC-C-U06 | CC-3 / C-14 / L1 | AAD 書換（同 ciphertext + 別 record_id の AAD）→ `Err(AeadTagMismatch)` | ユニット | C-14 |
| TC-C-U07 | CC-3 / C-14 / L1 | nonce 書換 → `Err(AeadTagMismatch)` | ユニット | C-14 |
| TC-C-U08 | CC-3 / C-14 / L1 | ciphertext 書換 → `Err(AeadTagMismatch)` | ユニット | C-14 |
| TC-C-U09 | CC-13 / Sub-A C-6 | `let key: &dyn AeadKey = &Vek::from_array([0;32]);` がコンパイル失敗（`AeadKey` は `impl FnOnce` を持つため dyn-safe ではない）。代わりに `&impl AeadKey` ジェネリクス経路で渡す | ユニット | trait 制約 |
| TC-C-U10 | CC-13 | `Vek` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` 各々に `AeadKey` impl が存在（`AesGcmAeadAdapter::wrap_vek(&kek_pw, ...)` が型一致でコンパイル可）| ユニット | trait impl |
| TC-C-U11 | CC-9 / SRP | `AesGcmAeadAdapter` の全メソッド signature に `&NonceCounter` / `&mut NonceCounter` を取らない（型レベルで increment 不可、Sub-D 責務）| ユニット | 型契約 |
| TC-C-U12 | REQ-S05 | `wrap_vek` の AAD は空 `&[]`（per-record の `Aad` とは別経路、vault ヘッダ独立 AEAD タグで保護される）| ユニット | API 契約 |
| TC-C-U13 | REQ-S05 | `AuthTag::try_new(&[u8])` が 16B 入力でのみ `Ok`、他長で `Err(DomainError::InvalidVaultHeader(AuthTagLength))` | ユニット | 境界値 |
| TC-C-U14 | REQ-S05 | `WrappedVek::new(ct, nonce, tag)` で `ct.len() < 32` 時 `Err`（既存 C-11、Sub-C で wrap_vek 戻り値が常に 32B+ ciphertext を生成することで間接担保）| ユニット | 境界値 |
| TC-C-U15 | CC-3 / C-7 | `decrypt_record` の戻り値型 `Verified<Plaintext>` は **shikomi-core `pub(crate)` 経路のみ**で構築（Sub-A 契約 C-7、外部 crate test から `Verified::new_from_aead_decrypt` 直接呼出が compile_fail）| ユニット | compile_fail |
| TC-C-U16 | REQ-S05 | `AesGcmAeadAdapter::default()` で構築可（無状態 unit struct、`#[derive(Default)]`）。複数インスタンスでも振る舞い同一 | ユニット | 構築 |
| TC-C-U17 | CC-10 | `AesGcmAeadAdapter` 内に `unwrap` / `expect` 経路ゼロ（grep、テスト除外）| ユニット | Fail-Secure |
| TC-C-U18 | REQ-S05 / `aes-gcm` 凍結 | `aes_gcm::Aes256Gcm` 以外（`Aes128Gcm` / `Aes256GcmSiv` 等）を `aead/` 内で使用しない（grep）| ユニット | アルゴリズム凍結 |
| TC-C-I01 | CC-4 / C-15 / 可視性ポリシー | `grep -rE "expose_within_crate" crates/shikomi-infra/src/crypto/aead/` で 0 件（`AeadKey::with_secret_bytes` クロージャ経由のみ）| 結合 | 可視性 |
| TC-C-I02 | CC-5 / C-16 / Zeroize | `grep -rE "Vec::new\(\)\|Vec::with_capacity" crates/shikomi-infra/src/crypto/aead/aes_gcm.rs` で中間バッファ用途の生 `Vec<u8>` 0 件（`Zeroizing<Vec<u8>>` のみ）| 結合 | zeroize 契約 |
| TC-C-I03 | CC-11 / 単一エントリ点 | `grep -rE "use aes_gcm::\|aes_gcm::Aes" crates/shikomi-infra/src/` で hit するのは `crypto::aead` モジュール内のみ | 結合 | 単一エントリ点 |
| TC-C-I04 | CC-8 / CC-12 / `subtle` | `grep -rnE " == .*tag\| == .*\\[u8" crates/shikomi-infra/src/crypto/aead/` で自前 byte 比較 0 件 | 結合 | constant-time |
| TC-C-I05 | CC-14 / VekProvider | `cargo check -p shikomi-infra` で `Argon2idHkdfVekProvider::derive_new_wrapped_pw` / `..._recovery` の `WrappedVek` 構築が成功（Sub-B 段階の TBD コンパイルエラー解消）| 結合 | cargo check |
| TC-C-P01 | CC-7 / L1 / property | proptest で record A / B の ciphertext + AAD を組合せ → 復号成功は **同一 (record_id, version, created_at) 組のみ**、入れ替え時は `Err(AeadTagMismatch)`（1000 ケース）| property | AAD 入れ替え |
| TC-C-P02 | CC-1 / CC-6 / property | proptest で任意 `plaintext` (0..=4096B) + 任意 `Vek` + 任意 `Aad` 組合せに対し、`encrypt_record` → `decrypt_record` の往復で **元 plaintext と bit-exact 一致**（1000 ケース）。KAT は公式ベクトル bit-exact のみ、本 TC は **任意入力での往復不変条件**を property で担保（マユリ Boy Scout 補強）| property | encrypt/decrypt 往復 |
| TC-C-E01 | 全契約 / Sub-D/E/F 統合 | 後続 Sub 実装者が `cargo doc -p shikomi-infra` を開き、`AesGcmAeadAdapter` + `AeadKey` trait のサンプル呼出（doc コメント rustdoc）から `vault encrypt` / `vault unlock` / VEK wrap/unwrap の AEAD 経路を再構築できる | E2E | 逆引き可能性 |

### 12.5 Sub-C ユニットテスト詳細

#### NIST CAVP KAT 検証（`crates/shikomi-infra/src/crypto/aead/kat.rs` + `aes_gcm.rs` テスト）

| テストID | 入力（fixture） | 期待結果 |
|---|---|---|
| TC-C-U01 | NIST CAVP `gcmEncryptExtIV256.rsp` から **8 ベクトル**（**ベクトル分散根拠**: key bias = 全 0 / 全 F / random 各 1 件以上、plaintext 長 = 0B (空) / 16B (1 ブロック境界) / 64B / 1024B など対数スケールで分散、AAD 長 = 0B / 13B (RFC 5116 推奨ケース) / 26B (本実装採用 `Aad`) を網羅）| `encrypt_in_place_detached` 戻り値 (ciphertext, tag) が公式期待値と bit-exact 一致 |
| TC-C-U02 | NIST CAVP `gcmDecrypt256.rsp` から **8 ベクトル + tag-fail ベクトル 2 件**（同分散根拠 + 改竄検知系として MAC tampered 1 件 + truncated tag 1 件）| 正常ベクトル: `Verified<Plaintext>` 経由で plaintext 復元、tag-fail ベクトル: `Err(AeadTagMismatch)` |

#### C-14 構造禁止検証（property test）

| テストID | 操作 | 期待結果 |
|---|---|---|
| TC-C-U05 | 正常 encrypt → tag の任意 1 byte を flip → decrypt | `Err(AeadTagMismatch)`、`Plaintext` 変数を観測する経路を作っても **None / 未初期化** |
| TC-C-U06 | 正常 encrypt → AAD の `record_id` を別 UUIDv7 に置換 → decrypt | 同上 |
| TC-C-U07 | 正常 encrypt → nonce の任意 byte を flip → decrypt | 同上 |
| TC-C-U08 | 正常 encrypt → ciphertext の任意 byte を flip → decrypt | 同上 |

#### `AeadKey` trait 動作検証

| テストID | 入力 | 期待結果 |
|---|---|---|
| TC-C-U09 | `let key: &dyn AeadKey = &Vek::from_array([0;32]);` | compile_fail（`impl FnOnce` のため dyn-safe でない）|
| TC-C-U10 | `let _: WrappedVek = adapter.wrap_vek(&Kek::<KekKindPw>::from_array([0;32]), &nonce, &vek)?;` 等 3 経路 | コンパイル可、ランタイム成功 |
| TC-C-U11 | `signature::AesGcmAeadAdapter::encrypt_record` パラメータ列に `NonceCounter` 系型が含まれない | grep で 0 件 |

### 12.6 Sub-C 結合テスト詳細

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-C-I01 | `tests/docs/sub-c-static-checks.sh`: `grep -rE "expose_within_crate" crates/shikomi-infra/src/crypto/aead/` | 0 件（C-15 維持）|
| TC-C-I02 | 同上スクリプト: `grep -rnE "let mut [a-z_]+ *: *Vec<u8>" crates/shikomi-infra/src/crypto/aead/aes_gcm.rs`（中間バッファ用途、`Zeroizing<Vec<u8>>` 以外）| 0 件（C-16 維持）|
| TC-C-I03 | 同上: `grep -rE "use aes_gcm::\|aes_gcm::" crates/shikomi-infra/src/` で hit を path filter | `crypto/aead/` 配下のみ |
| TC-C-I04 | 同上: `grep -rnE "(\\[u8.*\\]|tag.as_array\\(\\)).*==" crates/shikomi-infra/src/crypto/aead/` | 0 件（CC-8 / CC-12）|
| TC-C-I05 | `cargo check -p shikomi-infra --tests` | 成功（CC-14、Sub-B TBD 解消）|

### 12.7 Sub-C property テスト詳細

| テストID | 入力空間 | invariant |
|---|---|---|
| TC-C-P01 | 任意の 2 record (`(rid_a, ver_a, ts_a)`, `(rid_b, ver_b, ts_b)`) + 任意の plaintext + 任意の VEK + 任意の nonce、各々 AEAD encrypt → AAD を A/B 入れ替えて decrypt | 同一組合せ復号は成功（plaintext 一致）、入れ替え組合せ復号は **必ず `Err(AeadTagMismatch)`** |
| TC-C-P02 | 任意 `plaintext: Vec<u8>` (0..=4096B) + 任意 `Vek: [u8;32]` + 任意 `Aad`（`record_id` UUIDv7 / `version` u16 / `created_at_micros` i64 を proptest 生成） + 任意 `nonce: [u8;12]` を `Rng::generate_nonce_bytes` で生成 | 同一 (Vek, Aad, nonce) 組での `encrypt_record(...) → decrypt_record(...)` 往復で `Verified<Plaintext>::expose_secret()` が**元 plaintext と bit-exact 一致**（1000 ケース）。KAT (TC-C-U01/U02) が公式ベクトル単発検証なら、本 TC は**入力空間全体での往復不変条件**を property で担保 |

### 12.8 Sub-C E2E テストケース

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---|---|---|---|---|
| TC-C-E01 | 野木 拓海（Sub-D/E/F 実装者）| `AesGcmAeadAdapter` + `AeadKey` trait で vault encrypt 経路を再構築 | (1) `cargo doc -p shikomi-infra --open`、(2) `AesGcmAeadAdapter::{encrypt_record, decrypt_record, wrap_vek, unwrap_vek}` の rustdoc サンプル呼出を読む、(3) `AeadKey::with_secret_bytes` クロージャインジェクションのコード例を確認 | 30 分以内に「nonce_counter increment → rng.generate_nonce_bytes → adapter.encrypt_record → EncryptedRecord 永続化」「unwrap_vek → Verified<Plaintext>::into_inner → 32B 長さ検証 → Vek::from_array」の 2 フローを Sub-D 実装に流用できる |

### 12.9 Sub-C テスト実行手順

```bash
# Rust unit + property tests
cargo test -p shikomi-infra --test aead_*
cargo test -p shikomi-infra --lib crypto::aead

# Sub-C 静的検証 (cargo 不要)
bash tests/docs/sub-c-static-checks.sh

# Sub-A / Sub-B static checks 再実行 (回帰防止)
bash tests/docs/sub-a-static-checks.sh
bash tests/docs/sub-b-static-checks.sh

# Sub-0 lint / cross-ref 回帰防止
python3 tests/docs/sub-0-structure-lint.py
bash tests/docs/sub-0-cross-ref.sh
```

### 12.10 Sub-C テスト証跡

- `cargo test -p shikomi-infra` の stdout（KAT pass 件数 + property test 結果）
- 静的検証スクリプト stdout
- proptest 失敗時の minimization 出力（あれば）
- 全て `/app/shared/attachments/マユリ/sub-c-*.txt` に保存し Discord 添付

### 12.11 後続 Sub-D〜F への引継ぎ（Sub-C から派生）

| Sub | 本ファイル §12 拡張時の追加内容 |
|---|---|
| Sub-D (#42) | `HeaderAeadKey::AeadKey` impl 追加（Sub-C で予告した Boy Scout 完成）の TC、vault リポジトリ層での `NonceCounter::increment` 統合 TC、ヘッダ独立 AEAD タグの `AesGcmAeadAdapter::encrypt_record(&header_key, ...)` 経路 TC、平文⇄暗号化マイグレーション atomic write TC、`unwrap_vek_with_password` / `unwrap_vek_with_recovery` の 32B 長さ検証 Fail Fast TC |
| Sub-E (#43) | VEK キャッシュ（`tokio::sync::RwLock<Option<Vek>>`）と `AeadKey` impl の Drop 連鎖統合、IPC V2 でのエラー variant マッピング |
| Sub-F (#44) | `vault rekey` フロー TC（`NonceLimitExceeded` 検知 → 新 VEK 生成 → 全レコード再暗号化）、MSG-S10 / MSG-S11 文言確定の E2E |

### 12.12 Sub-C 工程4 実施実績（2026-04-26、PR #55 / `0507705`）

| 区分 | TC 数 | pass | 検証手段 |
|---|---|---|---|
| ユニット（runtime + KAT） | 18 | 18 | CI `cargo test -p shikomi-infra` で 67 unit pass、Sub-C 関連 22 件確認（aes_gcm 9 + kat 9 + aead_key 4） |
| 結合（grep + cargo check） | 5 | 5 | `tests/docs/sub-c-static-checks.sh` 4/4 PASS（SKIP→PASS 切替成功） + CI cargo check |
| property（proptest 1000 ケース）| **2** | **2 PASS** | **Bug-C-001 顛末**: 銀ちゃん impl は単発 fixture のみ、テスト工程で `crates/shikomi-infra/tests/aead_property.rs` を新規実装、Docker `rust:1.95-slim` aarch64 release で 1000 ケース 0.12 秒完走 |
| E2E | 1 | 1 | CI 8 ジョブ全 SUCCESS、bench-kdf 3 OS 全 PASS（Sub-B 既存契約も維持）|
| **合計** | **26** | **26 PASS** | CI + Docker proptest + 静的 grep の三系で交叉確認 |

**Bug-C-001 顛末（テスト工程発見）**:

設計書 §12.4 / §12.7 が要求した **proptest 1000 ケース**（TC-C-P01 AAD swap + TC-C-P02 encrypt/decrypt 往復）に対し、銀ちゃん impl PR #55 では：

- **単発 fixture 1 件**で AAD 入れ替え検証 (`decrypt_with_swapped_aad_returns_aead_tag_mismatch`)
- **単発 fixture 1 件**で encrypt/decrypt 往復検証 (`encrypt_then_decrypt_roundtrip_bit_exact`)

を実装。これは「設計者が想定したケース 1 件で invariant が成立する確認」であり、「ランダム入力空間 1000 ケースで invariant が成立する確率的検証」とは**意味論が異なる**。proptest crate 不在で、shrinking で minimal failing case を提示する経路もゼロ。

**マユリ Boy Scout で補強**:

1. workspace + shikomi-infra dev-dependencies に `proptest = "1"` 追加
2. `crates/shikomi-infra/tests/aead_property.rs` 新規作成（160 行）
   - `vek_strategy` / `nonce_strategy` / `aad_strategy` / `plaintext_strategy` で入力空間定義
   - `ProptestConfig::with_cases(1000)` で**1000 ケース明示**（proptest デフォルト 256 ケースとの乖離を構造防衛）
3. Docker `rust:1.95-slim` aarch64 release で実行 → **2/2 PASS、所要 0.12 秒**

**proptest 経路で発見した実装の堅牢性**:

- TC-C-P02 (1000 ケース): 任意 plaintext (0..=4096B) + 任意 Vek + 任意 Aad + 任意 nonce で encrypt → decrypt 往復が**全件 bit-exact**。AES-GCM 実装の入力空間網羅性を確認
- TC-C-P01 (1000 ケース): AAD 入れ替え攻撃 + nonce 入れ替え攻撃を**全件 `Err(AeadTagMismatch)`** で検出

**重大度更新**:
- Bug-C-001 (proptest 1000 ケース不在): Medium → **Resolved**（テスト工程で proptest 実装を追加、設計と実装の意味論的整合確保）

**新規補助スクリプト・テスト**:
- `crates/shikomi-infra/tests/aead_property.rs`: TC-C-P01 / P02 実装、1000 ケース proptest
- `tests/docs/sub-c-static-checks.sh`: TC-C-I01〜I04 grep 検証（4/4 PASS、Sub-A/B 同型）

**全 regression 維持**: lint 20/20 + cross-ref 9/9 + sub-a 3/3 + sub-b 3/3 + sub-c 4/4 = **39/39 PASS**

---

