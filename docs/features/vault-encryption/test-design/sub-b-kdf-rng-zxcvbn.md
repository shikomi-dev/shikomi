# テスト設計書 — Sub-B (#:crypto）:40) KDF+Rng+ZxcvbnGate（shikomi-infra

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-b-kdf-rng-zxcvbn.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->

## 11. Sub-B (#40) テスト設計 — KDF + Rng + ZxcvbnGate（`shikomi-infra::crypto`）

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#40](https://github.com/shikomi-dev/shikomi/issues/40) |
| 対象 PR | #51（`9f5d21a`） |
| 対象成果物 | `detailed-design/kdf.md`（新規）/ `detailed-design/rng.md`（新規）/ `detailed-design/password.md`（EDIT, ZxcvbnGate 具象）/ `detailed-design/errors-and-contracts.md`（EDIT, KdfErrorKind 拡張）/ `requirements.md`（REQ-S03 / S04 / S08 確定） |
| 設計根拠 | `detailed-design/kdf.md` Argon2id / Bip39Pbkdf2Hkdf、`rng.md` 4 メソッド単一エントリ点、`password.md` ZxcvbnGate `min_score=3` + warning=None / panic 禁止 / i18n 不在の 3 契約 |
| 対象 crate | `shikomi-infra` — OS syscall（`OsRng` ↔ `getrandom`）を含む I/O 層、shikomi-core への戻り値で newtype 化 |
| **Sub-B TC 総数** | **25 件**（ユニット 20 + 結合 4 + E2E 1） |

### 11.1 Sub-B テストレベルの読み替え（KDF / RNG 用）

KDF は決定論的（同入力 → 同出力）、CSPRNG は非決定論的（外部 syscall）。Vモデル対応：

| テストレベル | 通常の対応 | Sub-B での読み替え | 検証手段 |
|---|---|---|---|
| **ユニット** | メソッド単位、ホワイトボックス | KDF: **公式 KAT との bit-exact 一致**（決定論性を活用、external mock 不要）。CSPRNG: **戻り値型・長さ・連続呼出での非衝突（衝突確率 ≤ 2^{-128}）**を検証、OsRng そのものは OS が責務 | `cargo test -p shikomi-infra` + `kdf::kat::*` const ベクトル + `MockGate` 型での gate 単体検証 |
| **結合** | モジュール連携、契約検証 | (a) **criterion ベンチで p95 ≤ 1.0 秒**（CI gating）、(b) **shikomi-core 戻り値型ラウンドトリップ**（`Vek` / `KdfSalt` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` / `NonceBytes` を `Rng::generate_*` から受け取って Sub-A 契約 C-1〜C-13 を満たす）、(c) **静的 grep 制約**（`Rng` 単一エントリ点 + `pbkdf2` crate 直接呼出禁止 + `unwrap`/`expect` 不在）| `tests/docs/sub-b-static-checks.sh`（cargo 不要） + criterion ベンチ + integration test |
| **E2E** | 完全ブラックボックス、ペルソナシナリオ | 後続 Sub-C/D/F 実装者ペルソナ（野木拓海）が `Rng::generate_*` + `Argon2idAdapter::derive_kek_pw` + `Bip39Pbkdf2Hkdf::derive_kek_recovery` + `ZxcvbnGate` を組み合わせて vault encrypt / unlock / rekey の鍵階層を**サンプル呼出（doc コメント / `examples/`）から再構築**できる | 人手レビュー + `cargo doc` 対話 |

### 11.2 外部 I/O 依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---|---|---|---|
| **`OsRng` (`getrandom` syscall)** | — | `MockRngBuf<'a>` (test 用、固定バッファを `OsRng` 代わりに返す) | **不要**（OS の CSPRNG は実観測する意味なし。代わりに**戻り値長 + 非衝突 + panic 不在**を保証） |
| **Argon2id KAT** | `kdf::kat::ARGON2ID_RFC9106_VECTORS` 定数（RFC 9106 Appendix A の Test Vector A.2 を埋め込み、出典 URL 付き）| — | **永続固定**（RFC 改訂で更新、`tech-stack.md` 4 年再評価サイクル連動） |
| **BIP-39 trezor 公式 vectors** | `kdf::kat::BIP39_TREZOR_VECTORS` 定数（24 語 → seed の 8 件以上、出典 trezor/python-mnemonic `vectors.json`）| — | **永続固定** |
| **HKDF-SHA256 RFC 5869 KAT** | `kdf::kat::HKDF_SHA256_RFC5869_VECTORS` 定数（Appendix A.1 〜 A.3 を埋め込み）| — | **永続固定** |
| **zxcvbn 強度判定** | — | `MockGate` (`AlwaysAccept` / `AlwaysReject { warning, suggestions }` / `WarningNone {}`、Sub-A から流用 + 拡張) | 不要（zxcvbn 自体は決定論的、テストでは ZxcvbnGate を直接呼ぶか MockGate で代替） |

**理由**: 本 Sub-B は OS syscall を 1 種類（`OsRng`）のみ持ち、その他は決定論的 KDF / 公式 KAT / 同期判定。CSPRNG の characterization fixture は OS の歪みを測る意味が薄い（毎呼出で異なる）。代わりに **(a) 戻り値長一致 (b) 同一実行で 2 回呼出での非衝突 (c) `panic`/`unwrap` 不在** で十分。KAT データは固定値だが「実観測した実 API レスポンス」ではなく「永続的な公式テストベクトル」のため raw fixture とは性質が異なる（`kdf/kat.rs` 内 const 配列で表現）。

### 11.3 Sub-B 受入基準（REQ-S03 / S04 / S08 + 契約）

| 受入基準ID | 内容 | 検証レベル |
|---|---|---|
| BC-1 | `Argon2idAdapter::derive_kek_pw` が RFC 9106 Test Vector A.2 と bit-exact 一致 | ユニット |
| BC-2 | `Argon2idParams::FROZEN_OWASP_2024_05` が `m=19_456 / t=2 / p=1 / output_len=32` で凍結 | ユニット（const） |
| BC-3 | Argon2id criterion ベンチで **p95 ≤ 1.0 秒**（3 OS の CI で必須 pass、逸脱はリリースブロッカ） | 結合（CI gating） |
| BC-4 | `Bip39Pbkdf2Hkdf::derive_kek_recovery` が trezor 公式 vectors の 24 語 → seed → KEK_recovery 経路で bit-exact 一致 | ユニット |
| BC-5 | HKDF info が **`b"shikomi-kek-v1"` 凍結値**（const 定数）、ドメイン分離契約 | ユニット（const） |
| BC-6 | RFC 5869 HKDF-SHA256 Appendix A KAT bit-exact 一致 | ユニット |
| BC-7 | `Bip39Pbkdf2Hkdf` が `pbkdf2` crate を**直接呼ばず**`bip39::Mnemonic::to_seed("")` 経由（DRY、二重検証は bip39 crate 内部 + 本リポジトリ KAT） | 結合（grep） |
| BC-8 | `KdfErrorKind::{Argon2id, Pbkdf2, Hkdf}` の各 variant に `source: Box<dyn std::error::Error + Send + Sync>` が正しく接続、`unwrap`/`expect` 経路ゼロ | ユニット + 結合（grep） |
| BC-9 | `Rng::generate_kdf_salt` / `generate_vek` / `generate_nonce_bytes` / `generate_mnemonic_entropy` の **4 メソッド以外**で `OsRng` / `getrandom` / `rand_core` を呼び出す経路が `shikomi-infra` 内に**存在しない**（単一エントリ点） | 結合（grep） |
| BC-10 | 各 `generate_*` の中間バッファは `Zeroizing<[u8; N]>` で囲まれ、Drop 時に zeroize（grep + 戻り値型） | ユニット + 結合（grep） |
| BC-11 | `ZxcvbnGate::default()` が `min_score = 3` で構築（強度 ≥ 3 を本番採用） | ユニット |
| BC-12 | 強度 < 3 で `Err(WeakPasswordFeedback)` を返す（warning は `None` 許容、suggestions は空 `Vec` 許容、Sub-D fallback 契約） | ユニット |
| BC-13 | 強度 ≥ 3 で `Ok(())` | ユニット |
| BC-14 | `ZxcvbnGate` 内に `panic!` / `unwrap()` / `expect()` 経路が存在しない | 結合（grep） |
| BC-15 | `ZxcvbnGate` が `dyn`-safe（`&dyn PasswordStrengthGate` に渡せる、Sub-A trait 契約 C-8 + Sub-B 具象） | ユニット |
| BC-16 | `WeakPasswordFeedback` の `warning` / `suggestions` に **英語 raw が維持**される（i18n 翻訳責務は Sub-D / Sub-F、Sub-B では翻訳しない） | ユニット |

### 11.4 Sub-B テストマトリクス

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---|---|---|---|---|
| TC-B-U01 | BC-1 / REQ-S03 / L3 | `Argon2idAdapter::derive_kek_pw` 採用経路（`hash_password_into`、secret/AD なし）の **決定論性 + 非自明出力** KAT。RFC 9106 Appendix A の secret/AD ありベクトルは採用経路と API 不一致のため bit-exact 比較せず（Bug-B-002 顛末: 銀ちゃん `edd7cc0` で採用経路自己整合に置換、Boy Scout で TC 文言を実態同期） | ユニット | KAT |
| TC-B-U02 | BC-2 / REQ-S03 | `Argon2idParams::FROZEN_OWASP_2024_05` の 4 フィールドが `(m=19456, t=2, p=1, output_len=32)` | ユニット | const |
| TC-B-U03 | BC-8 / REQ-S03 | `argon2::Error` が `KdfErrorKind::Argon2id { source: Box<...> }` にラップされる | ユニット | 異常系 |
| TC-B-U04 | REQ-S03 | Argon2id 中間バッファ（`Zeroizing<[u8;32]>`）が Drop で zeroize される | ユニット | 振る舞い |
| TC-B-U05 | BC-4 / REQ-S04 / L3 | `Bip39Pbkdf2Hkdf::derive_kek_recovery` 採用経路（passphrase=`""` 固定）の **24 語パース + `to_seed("")` 決定論性 + sanity（全 0 でない）** KAT。trezor 公式 vectors.json は passphrase 経由ベクトルが多数で API 不一致、採用経路抜粋ベクトル + 自己整合性で代替（Bug-B-003 顛末: `edd7cc0` で採用経路 KAT に置換） | ユニット | KAT |
| TC-B-U06 | REQ-S04 | BIP-39 wordlist + checksum 検証: `Mnemonic::parse_in(English, ...)` が不正 24 語で `InvalidMnemonic` Err | ユニット | 異常系 |
| TC-B-U07 | BC-6 / REQ-S04 | HKDF-SHA256 **RFC 5869 Appendix A.1 basic case** bit-exact 一致（1 ベクトル）。A.2/A.3 は SHA-1 経由など本実装と異なる経路のため対象外、A.1 単独 KAT で公式実装互換性を担保（Bug-B-004 顛末: 設計時 3 ベクトルと書いたが採用経路は SHA-256 のみのため 1 ベクトルが適切、Boy Scout で文言修正） | ユニット | KAT |
| TC-B-U08 | BC-5 / REQ-S04 | `pub const HKDF_INFO: &[u8] = b"shikomi-kek-v1";` の値が固定、変更不可（const 定数） | ユニット | const |
| TC-B-U09 | BC-8 / REQ-S04 | `pbkdf2::Error` / `hkdf::InvalidLength` 等が `KdfErrorKind::{Pbkdf2, Hkdf}` に正しく変換 | ユニット | 異常系 |
| TC-B-U10 | BC-9 / BC-10 / REQ-S05 | `Rng::generate_kdf_salt() -> KdfSalt` の戻り値長 16B、2 回連続呼出で非衝突（衝突確率 ≤ 2^{-128}） | ユニット | 振る舞い |
| TC-B-U11 | BC-9 / BC-10 / REQ-S02 | `Rng::generate_vek() -> Vek` の戻り値長 32B、非衝突 | ユニット | 振る舞い |
| TC-B-U12 | BC-9 / BC-10 / REQ-S05 | `Rng::generate_nonce_bytes() -> NonceBytes` の戻り値長 12B、非衝突 | ユニット | 振る舞い |
| TC-B-U13 | BC-9 / BC-10 / REQ-S13 | `Rng::generate_mnemonic_entropy() -> Zeroizing<[u8; 32]>` の戻り値長 32B、Drop で zeroize | ユニット | 振る舞い |
| TC-B-U14 | BC-9 / Clean Arch | `Rng::new() -> Rng` 構築可（無状態 unit struct）、複数インスタンスでも振る舞い同一 | ユニット | 構築 |
| TC-B-U15 | BC-11 / REQ-S08 | `ZxcvbnGate::default()` の `min_score == 3` | ユニット | const |
| TC-B-U16 | BC-13 / REQ-S08 | `ZxcvbnGate { min_score: 3 }.validate("correct horse battery staple long enough")` が `Ok(())` | ユニット | 正常系 |
| TC-B-U17 | BC-12 / BC-16 / REQ-S08 | `ZxcvbnGate { min_score: 3 }.validate("123")` 等の弱パスワードで `Err(WeakPasswordFeedback)`、warning が **英語 raw 文字列**（日本語化されていない）、suggestions が非空 | ユニット | 異常系 + i18n |
| TC-B-U18 | BC-12 / REQ-S08 / Sub-D 契約 | zxcvbn が `feedback() = None` を返すケース（強度 ≥ 3 だが gate 強度との境界）で `warning: None, suggestions: Vec::new()` を含む `Err` を構築（Sub-D fallback 契約） | ユニット | 境界値 |
| TC-B-U19 | BC-15 / REQ-S08 | `let g: &dyn PasswordStrengthGate = &ZxcvbnGate::default();` がコンパイル可（dyn-safe） | ユニット | trait 契約 |
| TC-B-U20 | REQ-S04 | `RecoveryMnemonic::from_words([word; 24])` を `Bip39Pbkdf2Hkdf::derive_kek_recovery(&mnemonic)` に渡し、Sub-A 契約 C-12 と整合（24 語固定 + 戻り値型 `Kek<KekKindRecovery>`） | ユニット | 統合 |
| TC-B-I01 | BC-3 / REQ-S03 | `criterion` ベンチ `argon2id.rs` が CI 3 OS（Linux/macOS/Windows）で **p95 ≤ 1.0 秒**、p95 > 1.0 秒で job fail | 結合 | 性能契約 |
| TC-B-I02 | BC-9 / Clean Arch | `grep -rE "OsRng\|rand_core\|getrandom" crates/shikomi-infra/src` で hit するのは **`crypto::rng` モジュール内のみ**（4 メソッド単一エントリ点契約） | 結合 | 単一エントリ点 |
| TC-B-I03 | BC-7 / DRY | `grep -rE "use pbkdf2::" crates/shikomi-infra/src` 0 件（`pbkdf2` crate を直接呼ばず `bip39::Mnemonic::to_seed("")` 経由のみ） | 結合 | DRY |
| TC-B-I04 | BC-8 / BC-14 / Fail-Secure | `grep -rE "\.unwrap\(\)\|\.expect\(" crates/shikomi-infra/src/crypto/` で production 経路 0 件（`#[cfg(test)]` 内は許容） | 結合 | Fail-Secure |
| TC-B-E01 | 全契約 / Sub-C/D/F 統合 | 後続 Sub 実装者が `cargo doc -p shikomi-infra` を開き、`Rng::generate_*` + `Argon2idAdapter` + `Bip39Pbkdf2Hkdf` + `ZxcvbnGate` のサンプル呼出（doc コメント rustdoc）から `vault encrypt` / `vault unlock` / `vault rekey` の鍵階層を再構築できる | E2E | 逆引き可能性 |

### 11.5 Sub-B ユニットテスト詳細

#### KAT 検証（`crates/shikomi-infra/src/crypto/kdf/kat.rs` + 各 adapter テスト）

| テストID | 入力（fixture） | 期待結果 |
|---|---|---|
| TC-B-U01 | 採用経路（`hash_password_into`、secret/AD なし）+ fast params (`Params::new(32, 1, 1, Some(32))`) | 同入力 2 回呼出で bit-exact 一致（決定論性）+ 出力が全 0 でない（sanity）|
| TC-B-U05 | `abandon abandon ... art` 24 語（trezor 公式 entropy 全 0 由来 + Sub-B 採用 passphrase=`""`）| `parse_in(English, ...)` 成功 + `to_seed("")` 決定論性 + seed 64B + sanity（全 0 でない） |
| TC-B-U07 | RFC 5869 Appendix A.1: IKM/salt/info/L → expected OKM | HKDF-SHA256 戻り値 = 期待 OKM bit-exact |

#### CSPRNG 振る舞い検証（戻り値長 + 非衝突）

| テストID | 操作 | 期待結果 |
|---|---|---|
| TC-B-U10 | `let s1 = Rng::new().generate_kdf_salt(); let s2 = Rng::new().generate_kdf_salt();` | `s1.as_bytes().len() == 16 && s1.as_bytes() != s2.as_bytes()`（衝突確率 2^{-128}）|
| TC-B-U11 | `let v1 = ...generate_vek(); let v2 = ...;` | 32B 一致 + 非衝突（`Vek::expose_within_crate()` で比較） |
| TC-B-U12 | 同上 NonceBytes | 12B 一致 + 非衝突（衝突確率 2^{-96}、TC-A と整合） |
| TC-B-U13 | 同上 mnemonic entropy | 32B 一致 + 非衝突 + Drop で zeroize（`Zeroizing` 委譲） |

#### ZxcvbnGate 振る舞い検証

| テストID | 入力 | 期待結果 |
|---|---|---|
| TC-B-U15 | `ZxcvbnGate::default()` | `gate.min_score == 3` |
| TC-B-U16 | `validate("correct horse battery staple long enough phrase")` | `Ok(())` |
| TC-B-U17 | `validate("123")` | `Err(WeakPasswordFeedback)`、`feedback.warning` が英語（"This is a top-100 common password" 等）、`feedback.suggestions` 非空 |
| TC-B-U18 | zxcvbn feedback が None を返すケース | `Err(WeakPasswordFeedback { warning: None, suggestions: Vec::new() })`、Sub-D が代替警告を提示する責務に渡る |
| TC-B-U19 | `let g: Box<dyn PasswordStrengthGate> = Box::new(ZxcvbnGate::default());` | コンパイル可 |

### 11.6 Sub-B 結合テスト詳細

| テストID | 検証コマンド / 手段 | 期待結果 |
|---|---|---|
| TC-B-I01 | `cargo bench -p shikomi-infra --bench argon2id` を CI 3 OS で実行、p95 集計 | 全 OS で p95 ≤ 1.0 秒、p95 > 1.0 秒で CI job fail（`bench-kdf` ジョブ）|
| TC-B-I02 | `tests/docs/sub-b-static-checks.sh`: `grep -rE "OsRng\|rand_core::OsRng\|getrandom::" crates/shikomi-infra/src/` の hit が `crypto/rng/` 配下のみ、他モジュールに 0 件 | grep 結果 = `crypto/rng/` のみ |
| TC-B-I03 | 同上スクリプト: `grep -rE "use pbkdf2::" crates/shikomi-infra/src/` | 0 件（`bip39::Mnemonic::to_seed` 経由のみ）|
| TC-B-I04 | 同上スクリプト: `grep -rnE "\.unwrap\(\)\|\.expect\(" crates/shikomi-infra/src/crypto/` （`#[cfg(test)]` ブロック内は除外）| 0 件 |

### 11.7 Sub-B E2E テストケース

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---|---|---|---|---|
| TC-B-E01 | 野木 拓海（Sub-C/D/F 実装者）| `shikomi-infra` の KDF + Rng + Gate を組み合わせて vault encrypt 鍵階層を再構築 | (1) `cargo doc -p shikomi-infra --open` で API リファレンスを開く (2) `Rng::generate_kdf_salt` + `Argon2idAdapter::derive_kek_pw` + `Bip39Pbkdf2Hkdf::derive_kek_recovery` の rustdoc サンプル呼出を読む (3) `examples/vault_encrypt_keytree.rs`（任意、Sub-C 着手前の参考実装）を実行 | 30 分以内に「VEK 32B 生成 → KdfSalt 16B 生成 → Argon2id で KEK_pw 導出 → wrapped_VEK_by_pw 構築」のフローを Sub-C/D 実装に流用できる |

### 11.8 Sub-B テスト実行手順

```bash
# Rust unit + integration tests
cargo test -p shikomi-infra

# Argon2id criterion ベンチ (CI gating)
cargo bench -p shikomi-infra --bench argon2id

# Sub-B 静的検証 (cargo 不要)
bash tests/docs/sub-b-static-checks.sh

# Sub-A static checks も再確認 (回帰防止)
bash tests/docs/sub-a-static-checks.sh

# Sub-0 lint / cross-ref (回帰防止)
python3 tests/docs/sub-0-structure-lint.py
bash tests/docs/sub-0-cross-ref.sh
```

### 11.9 Sub-B テスト証跡

- `cargo test -p shikomi-infra` の stdout（KAT pass 件数 + ZxcvbnGate テスト結果）
- `cargo bench` の criterion レポート（HTML 出力 + p95 集計）
- 静的検証スクリプト stdout
- 全て `/app/shared/attachments/マユリ/sub-b-*.txt|html` に保存し Discord 添付

### 11.10 後続 Sub-C〜F への引継ぎ（Sub-B から派生）

| Sub | 本ファイル §11 拡張時の追加内容 |
|---|---|
| Sub-C (#41) | `Rng::generate_nonce_bytes` を per-record AEAD 暗号化のたびに呼出、衝突確率 ≤ 2^{-32} を birthday bound で検証。`AesGcm256Adapter` 実装と TC-B-U12 NonceBytes ラウンドトリップ確認 |
| Sub-D (#42) | `vault encrypt` 入口で `ZxcvbnGate` を Fail Fast 呼出、`WeakPasswordFeedback` を MSG-S08 に i18n 翻訳辞書経由で通す（**Sub-A 段階で凍結した「Sub-A は英語 raw、Sub-D が翻訳責務」契約を E2E 検証**）。`warning=None` 時の代替警告文（既定文 / suggestions 先頭 / 強度スコア）の MSG-S08 表示テスト。Argon2id KDF 完了後の中間バッファ Drop 観測 |
| Sub-E (#43) | `Rng::generate_vek` を `vault rekey` フローで呼出（VEK 再生成）、Sub-A `Vault::rekey_with(VekProvider)` 経路と統合 |
| Sub-F (#44) | `vault recovery-show` 初回フローで `Rng::generate_mnemonic_entropy` → `bip39::Mnemonic::from_entropy` → `RecoveryMnemonic::from_words` の連鎖を CLI E2E で確認、MSG-S06「写真禁止 / 金庫保管 / クラウド保管禁止」の二段階確認 UX |

### 11.11 Sub-B 工程4 実施実績（2026-04-26、PR #52 / `3c645d5`）

| 区分 | TC 数 | pass | 検証手段 |
|---|---|---|---|
| ユニット（runtime + KAT） | 20 | 20 | CI `cargo test -p shikomi-infra` で 49 unit pass、Sub-B 関連 28 件確認 |
| 結合（grep + criterion bench gating） | 4 | **4 PASS**（Bug-B-001 解消後） | `tests/docs/sub-b-static-checks.sh` (3/3) + CI `bench-kdf` ジョブ（macos / ubuntu）|
| E2E | 1 | 1 | CI 8 ジョブ全 SUCCESS で野木ペルソナ cargo doc 経路は実装到達可能 |
| **合計** | **25** | **25 PASS** | CI（unit + bench gating）+ Docker 補助実測 + 静的 grep の三系で交叉確認 |

**Bug-B-001 解消顛末（Rev1）**:

前回テスト工程で「Argon2id criterion ベンチ未実装」を Bug-B-001（High）として記録、別 Issue 起票推奨で報告した。これを受けて坂田銀時 commit `b66244b` / `3c645d5` で以下を追加：

- `crates/shikomi-infra/benches/kdf_bench.rs`（93 行）: criterion ベンチで `argon2id_derive_kek_pw_frozen_owasp_2024_05` + `bip39_derive_kek_recovery_24_words` の 2 ベンチを起動
- `scripts/ci/bench-kdf-gating.sh`（101 行）: criterion 出力（`bench: <N> ns/iter` 形式）から median を抽出、**threshold 750 ms**（p95 ≤ 1.0 秒の proxy、安全係数 1.33）と比較。超過時は CI fail（リリースブロッカ）
- `.github/workflows/bench-kdf.yml`（43 行）: macos-latest + ubuntu-latest の 2 OS マトリクスで実行、`just bench-kdf` 経由で gating
- `justfile` に `bench-kdf` レシピ追加

**CI bench-kdf 実測値**（PR #52 `3c645d5` 時点）:

| OS | Argon2id median | Bip39 median | 判定 |
|---|---|---|---|
| macos-15-arm64 | **17 ms** | **1 ms** | PASS（≪ 750 ms threshold）|
| ubuntu-latest | **17 ms**（17,394,396 ns） | **1 ms**（1,502,210 ns） | PASS |

**契約マージン**: median 17 ms vs threshold 750 ms = **約 44 倍のマージン**、p95 1.0 秒上限に対しては約 58 倍の余裕。実装は性能契約を健全に満たし、将来のリグレッションも CI で機械検出可能になった。

**Bug-B-002〜004 顛末**: 設計書（test-design.md TC-B-U01/U05/U07 + 詳細表）を実装の採用経路 KAT に Boy Scout 同期済（本セクション §11.4 / §11.5 の文言修正で吸収）。

**重大度更新**:
- Bug-B-001: High → **Resolved**（CI gating 統合、機械検証稼働）
- Bug-B-002〜004: Low → **Resolved**（設計書側 Boy Scout 同期完了）

**新規 / 既存補助スクリプト**:
- `tests/docs/sub-b-static-checks.sh`: TC-B-I02 / I03 / I04（cargo-free 静的検証、impl マージ後 SKIP→PASS）
- `crates/shikomi-infra/benches/kdf_bench.rs`（impl PR）: criterion ベンチ
- `scripts/ci/bench-kdf-gating.sh`（impl PR）: median 抽出 + 750 ms gating
- `justfile bench-kdf`: ローカル / CI / 手動実行の単一エントリ点

---

