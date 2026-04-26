# 基本設計書

<!-- 詳細設計書（detailed-design/ ディレクトリ）とは別ファイル。統合禁止 -->
<!-- 詳細設計は Sub-A Rev1 で 4 分冊化: detailed-design/{index,crypto-types,password,nonce-and-aead,errors-and-contracts}.md -->
<!-- feature: vault-encryption / Epic #37 -->
<!-- 配置先: docs/features/vault-encryption/basic-design.md -->
<!-- 本書は Sub-A (#39) 着手時に新規作成。Sub-A スコープ（shikomi-core 暗号ドメイン型 + ゼロ化契約）の基本設計を確定する。
     Sub-B〜F の本文は各 Sub の設計工程で本ファイルを READ → EDIT で追記する。 -->
## セキュリティ設計

### 脅威モデル

`requirements-analysis.md` §脅威モデル §4 攻撃者能力 L1〜L4 を**正本**とする。本セクションは**Sub-A スコープに閉じた対応**を整理。

| 想定攻撃者 | 攻撃経路 | 保護資産 | Sub-A 型レベル対策 |
|-----------|---------|---------|------------------|
| **L1**: 同ユーザ別プロセス | vault.db 改竄、IPC スプーフィング（Sub-E 担当） | `wrapped_VEK_*` / `kdf_params` / records ciphertext | `Verified<T>` newtype で「未検証 ciphertext を `Plaintext` として扱う」事故を**三段防御で構造封鎖**: (1) `Verified::new_from_aead_decrypt` が `pub(crate)` 可視性で外部 crate から構築不可、(2) `Plaintext::new_within_module` が `pub(in crate::crypto::verified)` 可視性で `Verified` を実装する同一モジュール内からのみ構築可、(3) Sub-C PR レビューで `verify_aead_decrypt(\|\| ...)` クロージャ内が AEAD 検証を実装しているか必須確認。**型レベル完全保証ではなく caller-asserted マーカー契約**（`detailed-design/nonce-and-aead.md` §`verify_aead_decrypt` ラッパ関数の契約 参照）。**Sub-C 追加対策**: (a) `AesGcmAeadAdapter::decrypt_record` / `unwrap_vek` で AEAD 検証失敗時に `Plaintext` を構築しない（C-14 構造禁止）、(b) `Aad::to_canonical_bytes()` 26B（record_id + version + created_at）を AAD として GMAC 計算に組み込み、AAD 入れ替え攻撃を tag 不一致で検出、(c) random nonce 12B（Sub-B `Rng::generate_nonce_bytes`）+ `NonceCounter::increment` 上限 $2^{32}$ で衝突確率 ≤ $2^{-32}$ 維持、上限到達時 `vault rekey` 強制（Sub-F、`detailed-design/nonce-and-aead.md` §nonce_counter 統合契約） |
| **L2**: メモリスナップショット | コアダンプ / ハイバネーションファイル / スワップから VEK / KEK / MasterPassword / 平文抽出 | `Vek` / `Kek` / `MasterPassword` / `RecoveryMnemonic` / `Plaintext` | 全て `secrecy::SecretBox` ベース、`Drop` 連鎖で**派生集約も連動消去**。`Clone` を**意図的に未実装**（誤コピーで滞留時間延長を構造禁止）。`Debug` は `[REDACTED ...]` 固定、`Display` 未実装、`serde::Serialize` 未実装（コンパイル時に誤シリアライズを排除） |
| **L3**: 物理ディスク奪取 | offline brute force | `wrapped_VEK_*`（KDF 作業証明依存） | Sub-A 型レベル対策**なし**（KDF 計算は Sub-B、AEAD 計算は Sub-C 担当）。ただし `MasterPassword::new` で `PasswordStrengthGate` 通過を**型コンストラクタ要件**として強制し、弱パスワードを構造的に Sub-D の Argon2id 入力から排除（**KDF 強度の前提条件を型で担保**） |
| **L4**: 同ユーザ root / OS 侵害 | ptrace / kernel keylogger / `/proc/<pid>/mem` 等 | 全て | **対象外**（`requirements-analysis.md` §脅威モデル §4 L4 / §5 スコープ外）。型レベルで防御不能、Sub-A は対策追加せず |

### Fail-Secure 型レベル強制（REQ-S17 主担当）

`requirements-analysis.md` §脅威モデル §6 Fail-Secure 哲学の 5 種類を Sub-A で**型システムに焼き付ける**:

| パターン | Sub-A 実装 | 効果 |
|--------|----------|------|
| **`Verified<T>` newtype** | `Verified::new_from_aead_decrypt(t: T) -> Verified<T>` を `pub(crate)` 可視性で実装 + `Plaintext::new_within_module` を `pub(in crate::crypto::verified)` 可視性で同一モジュール内に閉じる二段防御 | AEAD 復号成功経路でのみ `Verified<Plaintext>` が得られる。「未検証 ciphertext を平文として扱う」事故を**三段防御で構造封鎖**（型レベル可視性 + モジュール内可視性 + Sub-C PR レビュー）。**caller-asserted マーカーであり、AEAD 検証 bypass の完全な型レベル保証ではない**点に注意（`detailed-design/nonce-and-aead.md` §設計判断の補足 参照） |
| **`MasterPassword::new` の構築時強度検証** | 構築時に `&dyn PasswordStrengthGate` を要求、Sub-D の zxcvbn 実装が `validate(&s) -> Result<(), WeakPasswordFeedback>` を返す | 弱パスワードでの `MasterPassword` 構築を**入口で禁止**、Sub-B Argon2id 入力に到達させない |
| **`NonceCounter::increment` の `Result` 返却** | 上限 $2^{32}$ 到達時 `Err(DomainError::NonceLimitExceeded)`、`#[must_use]` で結果無視を clippy lint で検出 | 上限到達後の暗号化を**構造的に禁止**、rekey 強制経路（Sub-F）へ誘導 |
| **`match` 暗号アーム第一パターン** | Sub-A 提供型 `enum CryptoOutcome { TagMismatch, NonceLimit, KdfFailed, Verified(Plaintext) }` で**未検証ケース第一**の網羅 match を Sub-C / Sub-D 実装で強制 | 部分検証で先に進む実装ミスを排除（Issue #33 `(_, Ipc) => Secret` パターン同型） |
| **`Drop` 連鎖** | `Vek` / `Kek<_>` / `MasterPassword` / `RecoveryMnemonic` / `Plaintext` / `HeaderAeadKey` 全てに `Drop` 経路、内包する `SecretBox` の zeroize が transitive に発火 | L2 メモリスナップショット対策の**型レベル担保**、忘却型による zeroize 漏れを禁止 |

### OWASP Top 10 対応

| # | カテゴリ | 対応状況 |
|---|---------|---------|
| A01 | Broken Access Control | 該当なし — 理由: Sub-A はドメイン型ライブラリで、認可境界は持たない。アクセス制御は IPC（Sub-E）/ OS パーミッション（既存 `vault-persistence`）担当 |
| A02 | Cryptographic Failures | **主担当**。`Verified<T>` newtype + `Plaintext::new_within_module` の二段可視性 + Sub-C PR レビューで AEAD 検証 bypass を**三段防御で構造封鎖**（caller-asserted マーカー契約）、`Vek` / `Kek<_>` / `MasterPassword` / `RecoveryMnemonic` / `Plaintext` / `HeaderAeadKey` を `secrecy` + `zeroize` で滞留時間最小化、`Clone` 禁止で誤コピー排除、`Debug` 秘匿で誤ログ漏洩排除、`PasswordStrengthGate` で弱鍵禁止。**Sub-C 追加**: `AeadKey` trait（クロージャインジェクション）で鍵バイトを shikomi-infra に**借用越境のみ**で渡し、所有権は shikomi-core 側に保持（Sub-B Rev2 可視性ポリシー差別化との整合）、AEAD 中間バッファを `Zeroizing<Vec<u8>>` で囲み Drop 時 zeroize（C-16）、`subtle` v2.5+ の constant-time 比較に委譲（自前 `==` 禁止） |
| A03 | Injection | 該当なし — 理由: shikomi-core は SQL / shell / HTML を扱わない |
| A04 | Insecure Design | **主担当**。Fail-Secure を**型システムで強制**する設計（`Verified<T>` / `pub(crate)` 可視性 / phantom-typed `Kek<Kind>` 取り違え禁止 / `#[must_use]` 結果無視検出）。Issue #33 の `(_, Ipc) => Secret` 思想を継承し、暗号化境界も型で fail-secure |
| A05 | Security Misconfiguration | 該当なし — 理由: 設定値は Sub-B（KDF パラメータ）/ Sub-C（nonce 上限）担当 |
| A06 | Vulnerable Components | **Sub-C 追加**: `aes-gcm`（RustCrypto、minor pin、`tech-stack.md` §4.7 凍結）+ `subtle` v2.5+（major pin、constant-time 比較）+ `rand_core` minor pin（既存導入済）。すべて §4.3.2 暗号クリティカル ignore 禁止リスト対象。NIST CAVP テストベクトルで Sub-C 工程4 KAT を CI 必須実行 |
| A07 | Auth Failures | 部分担当。`MasterPassword` の強度検証契約のみ確定（実装は Sub-D zxcvbn）、リトライ回数管理は Sub-E |
| A08 | Data Integrity Failures | 該当なし — 理由: ヘッダ AEAD タグの実検証は Sub-C / Sub-D 担当、Sub-A は `HeaderAeadKey` 型と `Verified<T>` 契約のみ提供 |
| A09 | Logging Failures | **主担当**。`Debug` を `[REDACTED VEK]` / `[REDACTED KEK]` / `[REDACTED MASTER PASSWORD]` / `[REDACTED MNEMONIC]` / `[REDACTED PLAINTEXT]` の固定文字列に統一、`tracing` で誤った構造化ログを出さない契約。`Display` / `serde::Serialize` 未実装で誤シリアライズを**コンパイル時禁止** |
| A10 | SSRF | 該当なし — 理由: shikomi-core はネットワーク I/O を持たない |

## エラーハンドリング方針

Sub-A で **`DomainError` の拡張**として暗号特化エラーを追加（`shikomi_core::error::DomainError` の variant 追加、または独立 `CryptoError` 型を `DomainError::Crypto(...)` で内包）。詳細な variant 仕様は `detailed-design/errors-and-contracts.md` を参照。

| 例外種別 | 処理方針 | ユーザーへの通知 |
|---------|---------|----------------|
| `CryptoError::WeakPassword(WeakPasswordFeedback)` | `MasterPassword::new` の構築失敗。呼び出し側（Sub-D）が `Feedback` をそのまま MSG-S08 に変換 | MSG-S08「パスワード強度不足」+ zxcvbn の `warning` / `suggestions`（Fail Kindly） |
| `CryptoError::AeadTagMismatch` | AEAD 復号失敗。**`Verified<Plaintext>` を構築せず**、Sub-D が即拒否 → vault.db 改竄の可能性をユーザに通知。**Sub-C 発火経路**: `AesGcmAeadAdapter::{decrypt_record, unwrap_vek}` 内の `aes_gcm::aead::AeadInPlace::decrypt_in_place_detached` が `Err(aes_gcm::Error)` を返した時に変換。タグ不一致 / AAD 不一致 / nonce-key 取り違え / ciphertext 改竄を**全て本 variant に統一**（内部詳細秘匿、攻撃者へのオラクル排除） | MSG-S10「vault.db 改竄の可能性、バックアップから復元を案内」 |
| `CryptoError::NonceLimitExceeded` | `NonceCounter::increment` の上限到達。Sub-D が即 `vault rekey` フロー（Sub-F）へ誘導 | MSG-S11「nonce 上限到達、`vault rekey` 実行を案内」 |
| `CryptoError::KdfFailed { kind, source }` | Argon2id / HKDF / PBKDF2 計算失敗（メモリ不足 / 入力長不正等）。Sub-B が即拒否、リトライしない（KDF 失敗は決定論的バグまたはリソース枯渇のため） | MSG-S09 カテゴリ「(c) キャッシュ揮発タイムアウト」隣接の「KDF 失敗」カテゴリ（Sub-B / Sub-E で文言確定） |
| `CryptoError::VerifyRequired` | `Plaintext` を `Verified` 経由なしで直接構築しようとした（`pub(crate)` 可視性で**コンパイルエラーになる経路**だが、テストでの構築シナリオに限り runtime 検出の余地） | 開発者向けエラー、ユーザ通知なし（`tracing` で audit log のみ） |
| 既存 `DomainError::NonceOverflow` | **Sub-A で `NonceLimitExceeded` に名称統一**（Boy Scout Rule、責務再定義に整合）。後方互換は Issue #7 時点で本 variant を呼ぶ箇所なし、安全に rename | 同上 |

**Fail-Secure 哲学の徹底**: 上記いずれのエラーも **「中途半端な状態を呼び出し側に渡さない」**（`Result::Err` のみで返す、`Option::None` で曖昧化しない、panic で巻戻さない）。Issue #33 の `(_, Ipc) => Secret` パターン継承。
