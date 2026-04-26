# テスト設計書 — Sub-A (#:crypto）:39) 暗号ドメイン型（shikomi-core

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-a-crypto-types.md -->
<!-- 共通方針（テストレベル読み替え / 受入基準 AC-* / E2E ペルソナ等）は sub-0-threat-model.md §1〜§9 を正本とする。 -->

## 10. Sub-A (#39) テスト設計 — 暗号ドメイン型（`shikomi-core::crypto`）

| 項目 | 内容 |
|------|------|
| 対象 Sub-issue | [#39](https://github.com/shikomi-dev/shikomi/issues/39) |
| 対象 PR | #48（`d28b87f`） |
| 対象成果物 | `basic-design.md` / `detailed-design/{index,crypto-types,password,nonce-and-aead,errors-and-contracts}.md`（Rev1 で 4 分冊化）/ `requirements.md`（REQ-S02 / REQ-S08 trait 部分 / REQ-S14 / REQ-S17 EDIT） |
| 設計根拠 | `detailed-design/index.md` §不変条件・契約サマリ C-1〜C-13、各分冊の §クラス・関数仕様 |
| 対象 crate | `shikomi-core` — pure Rust / no-I/O、暗号アルゴリズム実装は持たず**型と trait のみ** |
| **Sub-A TC 総数** | **22 件**（ユニット 18 + 結合 3 + E2E 1） |

### 10.1 Sub-A テストレベルの読み替え（Rust crate 用）

`shikomi-core` は CLI/UI を持たないライブラリ crate のため、伝統的 E2E は適用不能。Vモデル対応を以下に固定：

| テストレベル | 通常の対応 | Sub-A での読み替え | 検証手段 |
|------------|-----------|------------------|---------|
| **ユニット**（詳細設計に対応） | メソッド単位、ホワイトボックス | **inline `#[cfg(test)] mod tests` + `compile_fail` doc test** で各型の不変条件・契約・禁止トレイト検証 | `cargo test -p shikomi-core` + `cargo test --doc -p shikomi-core` |
| **結合**（基本設計に対応） | モジュール連携、契約検証 | **`tests/` 配下の integration test** で shikomi-core の pub API のみを使い、契約 C-13（rename）や Sub-B〜F が想定する利用パスをラウンドトリップ検証 | `cargo test --test crypto_contracts -p shikomi-core` |
| **E2E**（要件定義に対応） | 完全ブラックボックス、ペルソナシナリオ | **後続 Sub 実装者ペルソナ（野木拓海）による「`Vek` を Sub-B/C/D/E が使う際にコンパイルが通る経路と通らない経路」検証** | 人手レビュー + サンプル呼出コード（doc コメント内）が `cargo test --doc` で実行される |

### 10.2 外部I/O依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---------|------------|---------|---------------------|
| **該当なし** | — | — | — |

**理由**: `shikomi-core::crypto` は pure Rust / no-I/O。CSPRNG 由来の値（VEK 32B / kdf_salt 16B / nonce 12B）は呼出側（`shikomi-infra::crypto::Rng`）から `[u8; N]` で受け取る型レベル契約のため、本層では時刻・ファイル・ネットワーク・乱数のいずれも触らない。characterization fixture は Sub-B (#40) 以降で必要に応じて起票（KDF / AEAD 実装が外部 crate に委譲する箇所）。

### 10.3 Sub-A 受入基準（13 契約 + REQ 4 件）

`detailed-design/index.md` §不変条件・契約サマリの **C-1〜C-13** を**テスト設計の起点**とする。各契約を 1 つ以上の TC で検証。さらに REQ-S02 / REQ-S08 trait 部分 / REQ-S14 / REQ-S17 の機能要件もマトリクスに紐付ける。

| 受入基準ID | 内容 | 検証レベル |
|-----------|------|----------|
| C-1 | Tier-1 揮発型は `Drop` 時 zeroize | ユニット（runtime） |
| C-2 | Tier-1 揮発型は `Clone` 不可 | ユニット（compile_fail） |
| C-3 | Tier-1 揮発型は `Debug` で秘密値を出さない（`[REDACTED ...]`） | ユニット（runtime） |
| C-4 | Tier-1 揮発型は `Display` 不可 | ユニット（compile_fail） |
| C-5 | Tier-1 揮発型は `serde::Serialize` 不可 | ユニット（compile_fail） |
| C-6 | `Kek<KekKindPw>` と `Kek<KekKindRecovery>` は混合不可（phantom-typed + Sealed） | ユニット（compile_fail） |
| C-7 | `Verified<T>` は AEAD 復号関数からのみ構築可（`pub(crate)` 可視性） | ユニット（compile_fail） |
| C-8 | `MasterPassword::new` は `PasswordStrengthGate::validate` 通過必須 | ユニット（runtime） |
| C-9 | `NonceCounter::increment` は上限到達で `Err(NonceLimitExceeded)` | ユニット（runtime、境界値） |
| C-10 | `NonceBytes::from_random([u8; 12])` は失敗しない（型レベル長さ強制） | ユニット（runtime + 回帰） |
| C-11 | `WrappedVek::new` は ciphertext 空 / 短すぎを拒否 | ユニット（runtime、境界値） |
| C-12 | `RecoveryMnemonic::from_words` は 24 語固定（`[String; 24]`） | ユニット（コンパイラ強制） |
| C-13 | 既存 `DomainError::NonceOverflow` は `NonceLimitExceeded` に rename | 結合（grep + cargo） |

### 10.4 Sub-A テストマトリクス（トレーサビリティ）

| テストID | 受入基準 / REQ | 検証内容 | レベル | 種別 |
|---------|--------------|---------|------|------|
| TC-A-U01 | C-1 / REQ-S02 / L2 | `Vek` / `Kek<Pw>` / `Kek<Recovery>` / `MasterPassword` / `RecoveryMnemonic` / `Plaintext` の `Drop` 後にメモリ領域がゼロ化されている | ユニット | 振る舞い検証 |
| TC-A-U02 | C-2 / REQ-S17 | Tier-1 6 型に `Clone` 実装が存在しない（`let _ = vek.clone();` が compile_fail） | ユニット | compile_fail doc test |
| TC-A-U03 | C-3 / REQ-S02 | `format!("{:?}", vek)` 等の `Debug` 出力に秘密値が出ず `[REDACTED Vek]` 等の固定文字列のみ | ユニット | 振る舞い検証 |
| TC-A-U04 | C-4 / REQ-S02 | Tier-1 6 型に `Display` 実装が存在しない（`format!("{}", vek)` が compile_fail） | ユニット | compile_fail doc test |
| TC-A-U05 | C-5 / REQ-S02 | Tier-1 6 型に `serde::Serialize` 実装が存在しない（`serde_json::to_string(&vek)` が compile_fail） | ユニット | compile_fail doc test |
| TC-A-U06 | C-6 / REQ-S02 / L1 | `let kek_pw: Kek<KekKindPw> = ...; kek_recovery_aead(&kek_pw)` のような型混合が compile_fail。Sealed trait で外部 crate からの追加バリアント定義も compile_fail | ユニット | compile_fail doc test |
| TC-A-U07 | C-7 / REQ-S17 | 外部 crate（`shikomi-infra` / `tests/`）から `Verified::new(plaintext)` 直接呼出が compile_fail（`pub(crate)` 可視性） | ユニット | compile_fail doc test |
| TC-A-U08 | C-8 / REQ-S08 trait | (a) `AlwaysAcceptGate` で `MasterPassword::new(s, gate)` が `Ok` を返す、(b) `AlwaysRejectGate` で `Err(WeakPassword { feedback })` を返す、(c) feedback の `warning` / `suggestions` 両方が空でない | ユニット | 正常/異常/構造 |
| TC-A-U09 | C-9 / REQ-S14 / L1 | `NonceCounter::increment` 上限境界: (a) 0 → 1, ..., $2^{32} - 1$ → $2^{32}$ で `Ok`、(b) $2^{32}$ で `Err(NonceLimitExceeded)`、(c) `Err` 後も状態を進めない（再呼出も同じ `Err`） | ユニット | 境界値 |
| TC-A-U10 | C-10 / REQ-S05 | `NonceBytes::from_random([0u8; 12])` 構築可能、`from_random([0u8; 11])` / `from_random([0u8; 13])` は compile_fail | ユニット | コンパイラ強制 + 回帰 |
| TC-A-U11 | C-11 / REQ-S05 | `WrappedVek::new(ciphertext, nonce, tag)` 境界: (a) ciphertext 空で `Err`、(b) ciphertext 32B 未満で `Err`、(c) 32B 以上で `Ok` | ユニット | 境界値 |
| TC-A-U12 | C-12 / REQ-S13 | `RecoveryMnemonic::from_words([String; 24])` 構築可、`[String; 23]` / `[String; 25]` は compile_fail | ユニット | コンパイラ強制 |
| TC-A-U13 | REQ-S02 / Sub-0 凍結 | `KdfSalt::try_new(&[u8])` が **16B** 入力でのみ `Ok`、それ以外は `Err`。`shikomi-core` 側に `generate()` メソッドが**存在しない**（grep で確認） | ユニット | 境界値 + Clean Arch 整合 |
| TC-A-U14 | REQ-S02 / Sub-0 凍結 | `Vek::from_bytes([u8; 32])` のみコンパイル可、`[u8; 31]` / `[u8; 33]` は compile_fail | ユニット | コンパイラ強制 |
| TC-A-U15 | REQ-S02 / Sub-0 凍結 | `HeaderAeadKey::from_kek_pw(&Kek<KekKindPw>)` のみコンパイル可、`Kek<KekKindRecovery>` 渡しは compile_fail（chicken-and-egg 回避の鍵経路凍結） | ユニット | 型レベル設計判断 |
| TC-A-U16 | REQ-S08 trait | `PasswordStrengthGate::validate(&self, raw: &str) -> Result<(), WeakPasswordFeedback>` シグネチャが固定。trait は `dyn`-safe（オブジェクト安全） | ユニット | trait 契約 |
| TC-A-U17 | REQ-S02 | `WeakPasswordFeedback { warning: Option<String>, suggestions: Vec<String> }` 構造、空 `warning` / 空 `suggestions` の両方が許容（zxcvbn 仕様準拠） | ユニット | データ構造 |
| TC-A-U18 | REQ-S17 | `CryptoOutcome<T>` enum バリアント **5 件**（`detailed-design/errors-and-contracts.md` §CryptoOutcome と完全一致）: `TagMismatch` / `NonceLimit` / `KdfFailed(KdfErrorKind)` / `WeakPassword(WeakPasswordFeedback)` / `Verified(Verified<T>)`。`match` 強制で fall-through なし、`#[non_exhaustive]` で外部 crate からの追加に破壊的変更耐性 | ユニット | enum 網羅 |
| TC-A-I01 | C-13 / REQ-S14 | `grep -nE "NonceOverflow" --include='*.rs' .` で 0 件、`NonceLimitExceeded` のみ存在。`cargo check -p shikomi-core` でコンパイル成功 | 結合 | rename 整合性 |
| TC-A-I02 | REQ-S02 / Clean Arch | `shikomi-core` 内に `rand::` / `getrandom::` / `OsRng` 参照が**存在しない**（grep）。CSPRNG 経路は `shikomi-infra::crypto::Rng` の単一エントリ点のみ | 結合 | 依存方向検証 |
| TC-A-I03 | REQ-S17 | `cargo test -p shikomi-core --doc` で全 doc test（compile_fail 含む）が pass、`cargo clippy -p shikomi-core -- -D warnings` で警告 0 件 | 結合 | CI ゲート |
| TC-A-E01 | 全契約 / Sub-B〜F 利用視点 | 後続 Sub 実装者ペルソナ（野木拓海）が `Vek` / `Kek<Pw>` / `Verified<Plaintext>` / `NonceCounter` / `WrappedVek` を**サンプル呼出（doc コメント内 ```rust ブロック）**経由で「正常系がコンパイル通り、禁止系が compile_fail になる」体験ができる | E2E | 逆引き可能性（人手 + doc test） |

### 10.5 Sub-A ユニットテストケース（詳細）

#### Tier-1 揮発型の振る舞い検証（C-1 / C-3）

| テストID | クラス/メソッド | 種別 | 入力（factory） | 期待結果 |
|---------|---------------|------|---------------|---------|
| TC-A-U01a | `Vek::from_bytes([u8;32]).drop()` | 振る舞い | `[0xAB; 32]` 固定値 | `Drop` 後、内部 `SecretBox` のメモリ領域が `[0u8; 32]` に置換（`std::mem::transmute` で内部表現を覗き、zeroize 観測） |
| TC-A-U01b | `MasterPassword::new(s, gate).drop()` | 振る舞い | `"correct horse battery staple".to_string()` + `AlwaysAcceptGate` | `Drop` 後、`SecretBytes` の内部バッファに元文字列が残らない（パターン残存検出） |
| TC-A-U01c | `RecoveryMnemonic::from_words([String;24]).drop()` | 振る舞い | BIP-39 trezor 公式テストベクトル先頭 24 語 | `Drop` 後、各 `String` 領域が 0 埋めされている |
| TC-A-U03a | `format!("{:?}", &vek)` | 振る舞い | `Vek::from_bytes([0xCAFE_BABE_u32 ..; 32])` | 出力に `CAFE_BABE` のような 16 進パターンが**含まれず**、`Vek([REDACTED 32 bytes])` 等の固定文字列のみ |
| TC-A-U03b | `format!("{:?}", &mp)` | 振る舞い | `MasterPassword` 構築 | 出力に元パスワード文字列が含まれず、`MasterPassword([REDACTED])` 等のみ |
| TC-A-U03c | `format!("{:#?}", &recovery_mnemonic)` | 振る舞い | 24 語ニーモニック | 各単語が含まれず、`RecoveryMnemonic([REDACTED 24 words])` 等のみ |

#### コンパイル時禁止検証（C-2 / C-4 / C-5 / C-6 / C-7）

`compile_fail` doc test で実装。各テストは doc コメント内に `/// ```compile_fail` ブロックを置き、`cargo test --doc` で「これがコンパイルできてしまったら fail」を機械検証する。

| テストID | コード片 | 期待結果 |
|---------|---------|---------|
| TC-A-U02 | `let v: Vek = ...; let v2 = v.clone();` | compile_fail（`Clone` 未実装） |
| TC-A-U04 | `let v: Vek = ...; format!("{}", v);` | compile_fail（`Display` 未実装） |
| TC-A-U05 | `let v: Vek = ...; serde_json::to_string(&v).unwrap();` | compile_fail（`Serialize` 未実装） |
| TC-A-U06a | `let kp: Kek<KekKindPw> = ...; let kr: Kek<KekKindRecovery> = ...; if kp == kr { }` | compile_fail（型不整合、`PartialEq` 跨ぎなし） |
| TC-A-U06b | `pub struct EvilKind; impl KekKind for EvilKind {}` （外部 crate にて） | compile_fail（Sealed trait） |
| TC-A-U07 | `let p = Plaintext(...); let v = Verified::new(p);` （外部 crate `shikomi-infra` にて） | compile_fail（`Verified::new` は `pub(crate)`） |

#### ランタイム検証（C-8 / C-9 / C-10 / C-11）

| テストID | クラス/メソッド | 種別 | 入力 | 期待結果 |
|---------|---------------|------|------|---------|
| TC-A-U08a | `MasterPassword::new(s, &AlwaysAcceptGate)` | 正常系 | `"any string".to_string()` | `Ok(MasterPassword)` |
| TC-A-U08b | `MasterPassword::new(s, &AlwaysRejectGate { warning: Some("weak").into(), suggestions: vec!["use longer".into()] })` | 異常系 | 同上 | `Err(WeakPassword { feedback })`、`feedback.warning` / `feedback.suggestions` が gate の入力と一致 |
| TC-A-U09a | `NonceCounter::default().increment()` × `LIMIT - 1` 回 | 正常系 | `LIMIT = 2^{32}` | 全回 `Ok(())`、内部 count が線形に増加 |
| TC-A-U09b | 上記の続きで `LIMIT` 回目 | 異常系（境界） | — | `Err(NonceLimitExceeded { count: 2^{32} })`、内部 count は変更しない |
| TC-A-U09c | `Err` 後にもう一度 `increment()` | 異常系（再帰） | — | 再度 `Err(NonceLimitExceeded)`（state 無破損） |
| TC-A-U10 | `NonceBytes::from_random([0u8; 12])` | 正常系 | 12B 配列 | `NonceBytes`（`Result` ではなく直接構築可） |
| TC-A-U11a | `WrappedVek::new(vec![], nonce, tag)` | 異常系 | 空 ciphertext | `Err(DomainError::EmptyCiphertext)` |
| TC-A-U11b | `WrappedVek::new(vec![0u8; 31], nonce, tag)` | 異常系（境界） | 32B 未満 | `Err(DomainError::CiphertextTooShort)` |
| TC-A-U11c | `WrappedVek::new(vec![0u8; 48], nonce, tag)` | 正常系 | 48B = VEK 32B + tag 16B 想定 | `Ok(WrappedVek)` |
| TC-A-U12 | `RecoveryMnemonic::from_words(["word".to_string(); 24])` | コンパイラ強制 | 24 語 | `RecoveryMnemonic`（直接構築） |
| TC-A-U13 | `KdfSalt::try_new(&[u8])` | 境界値 | 15B / 16B / 17B | 16B のみ `Ok`、他は `Err(InvalidSaltLength)`。さらに `grep "fn generate" shikomi-core/src/crypto/` 0 件 |
| TC-A-U14 | `Vek::from_bytes` 固定長 | コンパイラ強制 | 32B のみ可 | `Vek`、その他長は型エラー |
| TC-A-U15 | `HeaderAeadKey::from_kek_pw(&Kek<KekKindPw>)` | 型レベル | KEK_pw 渡し / KEK_recovery 渡し | 前者のみ `Ok`、後者は compile_fail |
| TC-A-U16 | `PasswordStrengthGate` trait | trait 契約 | `&dyn PasswordStrengthGate` | dyn-safe、`validate(&self, raw: &str) -> Result<(), WeakPasswordFeedback>` シグネチャ固定 |
| TC-A-U17 | `WeakPasswordFeedback` 構造 | データ構造 | `{ warning: None, suggestions: vec![] }` / `{ warning: Some, suggestions: non-empty }` | 両方ともコンパイル可、`Debug` で内容透過（feedback 自体は秘密値ではない） |
| TC-A-U18 | `CryptoOutcome<T>` `match` 網羅 | enum 網羅 | **5 バリアント全列挙**: `TagMismatch` / `NonceLimit` / `KdfFailed(KdfErrorKind)` / `WeakPassword(WeakPasswordFeedback)` / `Verified(Verified<T>)`（`detailed-design/errors-and-contracts.md` §CryptoOutcome と完全一致） | `match` で 5 アーム全網羅、いずれか省略すると `non_exhaustive_patterns` 警告。`#[non_exhaustive]` 属性により外部 crate 側の `match` は wildcard `_` 必須で破壊的変更耐性 |

### 10.6 Sub-A 結合テストケース

| テストID | 対象連携 | 使用 fixture | 前提条件 | 操作 | 期待結果 |
|---------|---------|-------------|---------|------|---------|
| TC-A-I01 | `DomainError::NonceOverflow` → `NonceLimitExceeded` rename 整合性 | なし（grep + cargo） | `feature/issue-39-crypto-domain-types` チェックアウト | (1) `grep -rn "NonceOverflow" --include='*.rs' .` (2) `grep -rn "NonceLimitExceeded" --include='*.rs' .` (3) `cargo check -p shikomi-core` | (1) 0 件、(2) 1 件以上、(3) 成功 |
| TC-A-I02 | Clean Architecture 依存方向（`shikomi-core` の I/O 不在） | なし | 同上 | `grep -rn "rand::\|getrandom::\|OsRng\|SystemTime\|std::fs" crates/shikomi-core/src/ --include='*.rs'` | 0 件（pure Rust no-I/O） |
| TC-A-I03 | CI ゲート全 pass | なし | 同上 | `cargo test -p shikomi-core` + `cargo test --doc -p shikomi-core` + `cargo clippy -p shikomi-core --all-targets -- -D warnings` + `cargo fmt --check` | 全コマンドが exit 0 |

### 10.7 Sub-A E2Eテストケース

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---------|---------|---------|---------|---------|
| TC-A-E01 | 野木 拓海（Sub-B〜F 実装者） | Sub-A 公開 API のサンプル呼出経路（`cargo doc` 出力 + doc test）で正常系・禁止系の両方を**コンパイラに対話的に教わる** | (1) `cargo doc --open -p shikomi-core` で API リファレンスを開く (2) 各型の rustdoc に貼られた ```rust ブロック（正常系）と ```compile_fail ブロック（禁止系）を読む (3) `cargo test --doc -p shikomi-core` を実行 | 正常系 doc test 全 pass、`compile_fail` doc test 全件「コンパイルに失敗することを確認」して pass。野木が Sub-B 着手前に「Sub-A 契約で何が許され何が禁止されるか」を 30 分以内に把握 |

### 10.8 Sub-A テスト実行手順

```bash
# Rust unit test (#[cfg(test)] inline)
cargo test -p shikomi-core

# Rust doc test (compile_fail 含む)
cargo test --doc -p shikomi-core

# integration test (tests/ 配下)
cargo test --test crypto_contracts -p shikomi-core

# CI ゲート（lefthook / GitHub Actions）
cargo clippy -p shikomi-core --all-targets -- -D warnings
cargo fmt --all -- --check

# Boy Scout Rule 整合性チェック
grep -rn "NonceOverflow" --include='*.rs' .  # 0 件期待
grep -rn "rand::\|getrandom::\|OsRng" crates/shikomi-core/src/ --include='*.rs'  # 0 件期待
```

### 10.9 Sub-A テスト証跡

- `cargo test -p shikomi-core` の stdout（pass 件数 + テスト名一覧）
- `cargo test --doc -p shikomi-core` の stdout（compile_fail テストの結果含む）
- Boy Scout Rule grep の結果ログ
- 全て `/app/shared/attachments/マユリ/sub-a-*.txt` に保存し Discord 添付

### 10.10 後続 Sub-B〜F への引継ぎ（Sub-A から派生）

| Sub | 本ファイル §10 拡張時の追加内容 |
|-----|----------------------------|
| Sub-B (#40) | `Vek` / `Kek<Pw>` / `Kek<Recovery>` のラウンドトリップ（KDF 出力 → newtype 変換）の結合テスト、Argon2id KAT、BIP-39 trezor ベクトル、HKDF KAT、`PasswordStrengthGate` 実装（`shikomi-infra::crypto::ZxcvbnGate`）の C-8 契約適合確認 |
| Sub-C (#41) | `WrappedVek::new` 境界値の AEAD 実装側統合、`Verified<Plaintext>` 構築経路（`pub(crate)` 越境のみ許可）の C-7 契約適合確認、`NonceCounter::increment` 上限到達 → `vault rekey` 誘導フロー |
| Sub-D (#42) | `MasterPassword::new` × `ZxcvbnGate` の Fail Kindly E2E、`WeakPasswordFeedback` の MSG-S08 表示テスト、Sub-0 で凍結した MSG-S16/S18 + REQ-S13 アクセシビリティ |
| Sub-E (#43) | `Vek` キャッシュ寿命管理（アイドル 15min / サスペンドで `Drop` 強制）、`SecretBox::expose_secret` 呼出箇所の grep 監査 |
| Sub-F (#44) | CLI / GUI からの `MasterPassword::new` 経路、`RecoveryMnemonic` 初回 1 度表示の Drop 強制、MSG-S16/S17/S18 文言確定 |

### 10.11 Sub-A 工程4 実施実績（2026-04-25、PR #49 / `5373043`）

| 区分 | TC 数 | pass | 検証手段 |
|---|---|---|---|
| ユニット（runtime） | 12 | 12 | CI `cargo test -p shikomi-core` で 159 unit pass |
| ユニット（compile_fail doctest） | 6 | 8 doctest pass（C-2/4/5/6/7 + Plaintext/Mnemonic/HeaderAeadKey 越境） | CI `Doc-tests shikomi_core: 8 passed` + Docker `rust:1.95-slim` 再現一致 |
| 結合 | 3 | 3 | `tests/docs/sub-a-static-checks.sh` (TC-A-I01 NonceOverflow rename / TC-A-I02 no-I/O purity / TC-A-U13 KdfSalt single-entry) + CI 8 ジョブ全 SUCCESS |
| E2E | 1 | 1 | compile_fail doctest 8 件全 pass で野木ペルソナの `cargo doc` 対話を間接担保 |
| **合計** | **22** | **22** | **CI + Docker + 静的 grep の三系で交叉確認** |

**Bug-A-001 顛末（自己反省として残す）**: 本工程開始時、CI ログを `grep -E "Doc-tests|test result"` で抽出した範囲が狭く、Doc-tests セクション直後の `1 passed` を見て「9 個書いた compile_fail doctest が 1 件しか走っていない」と誤認した。Docker `rust:1.95-slim` で `cargo test --doc -p shikomi-core` を再現したところ **8 件全 pass**、CI ログを `grep -cE "test crates/.*compile fail \.\.\. ok"` で件数取得したところ同じく 8 件 pass を確認。**Bug-A-001 は誤認で撤回**。教訓: テストレポート作成時は **CI ログ依存だけでなく Docker で答え合わせ**を必須化し、静的検証スクリプトに件数 assert を組み込む（justfile に `test-doc-core` レシピを Boy Scout で追加済、CI 統合は Sub-B 以降の任意拡張に委ねる）。

**新規補助スクリプト**:
- `tests/docs/sub-a-static-checks.sh`: TC-A-I01 / I02 / U13 の grep ベース静的検証（cargo 不要、ローカルで即実行可）
- `justfile test-doc-core`: compile_fail doctest 件数を独立に観測する recipe

---

