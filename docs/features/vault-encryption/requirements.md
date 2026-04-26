# 要件定義書

<!-- feature: vault-encryption / Epic #37 / Sub-0 (#38) -->
<!-- 配置先: docs/features/vault-encryption/requirements.md -->
<!-- 本書は Sub-0 段階では REQ-S* の id 採番と骨格のみを確定する。
     入力 / 処理 / 出力 / エラー時の本文詳細は、各 REQ-S* を担当する
     Sub-A〜F の設計工程で **READ → EDIT** で拡張する（新規ファイル作成禁止）。 -->

## 機能要件

本 Sub-0 段階では各 REQ-S* について「**Sub 担当 / 概要 / 関連脅威 ID**」を確定し、入力 / 処理 / 出力 / エラー時の本文は `TBD by Sub-X` のプレースホルダで残す。後続 Sub の設計工程で各担当者が本ファイルを READ → EDIT して埋める。

> **記入規約**（後続 Sub-A〜F 共通）:
> - 「処理」欄には**必ず関連脅威 ID（L1〜L4）への対応関係を 1 行明記**する。例: 「L1: AEAD タグ検証で改竄検出」「L2: `Drop` 時 zeroize で過去メモリ抽出を最小化」
> - 「エラー時」欄は **Fail-Secure**（fail fast / 中途半端状態を残さない）を必ず満たす設計とする
> - REQ-S* と Sub-issue 設計書の章節は**双方向リンク**で参照する

### REQ-S01: 脅威モデル準拠

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-0 (#38) — 本 Issue で完了 |
| 概要 | `requirements-analysis.md` §脅威モデル の L1〜L4 凍結を後続 Sub の受入条件・テスト基準の唯一根拠とする |
| 関連脅威 ID | L1 / L2 / L3 / L4（凍結文書として全脅威を扱う） |
| 入力 | — |
| 処理 | 本 feature 配下の全設計書（basic-design.md / detailed-design/*.md / test-design.md）が脅威 ID L1〜L4 を参照可能な状態を維持する。各 Sub の PR レビューで「対策が脅威 ID と紐付いているか」を必須チェック項目とする |
| 出力 | 本 feature 配下の設計書群が L1〜L4 を共通語彙として使用 |
| エラー時 | 設計書中の「対策」記述に L1〜L4 の参照が無い場合、レビュー却下（Boy Scout Rule） |

### REQ-S02: 暗号ドメイン型

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-A (#39) — `feat(shikomi-core)` |
| 概要 | 鍵階層上位型 `Vek` / `Kek<KekKindPw>` / `Kek<KekKindRecovery>` / `HeaderAeadKey` / `MasterPassword` / `RecoveryMnemonic` / `Plaintext` / `Verified<T>` を新規追加、既存 `WrappedVek` / `NonceCounter` の Sub-0 凍結整合改訂、`Clone` 禁止、`Debug` 秘匿（`[REDACTED ...]` 固定）、`Display` 未実装、`serde::Serialize` 未実装、`Drop` 連鎖 |
| 関連脅威 ID | L1（`Verified<T>` newtype で改竄検証 bypass を**型レベル禁止**、`pub(crate)` コンストラクタで構造的封鎖）／ L2（`SecretBox<Zeroizing<...>>` + `zeroize` で過去メモリ抽出耐性、`Clone` 禁止で誤コピー滞留禁止）／ L3（`MasterPassword::new` で `PasswordStrengthGate` 通過を**型コンストラクタ要件**として強制し弱パスワードを KDF 入力から構造排除、`KdfSalt::generate()` は **`shikomi-infra::crypto::Rng::generate_kdf_salt()` 単一エントリ点**として再解釈し shikomi-core の no-I/O 制約と整合） |
| 入力 | (a) `[u8; 32]` バイト配列（`Vek::from_array` / `Kek::from_array`）、(b) ユーザ入力 `String` + `&dyn PasswordStrengthGate`（`MasterPassword::new`）、(c) `[String; 24]`（`RecoveryMnemonic::from_words`）、(d) `[u8; 12]`（`NonceBytes::from_random`、CSPRNG 由来）、(e) `(Vec<u8>, NonceBytes, AuthTag)`（`WrappedVek::new`） |
| 処理 | basic-design.md §処理フロー F-A1〜F-A5 に詳述。各型は構築時に長さ / 強度 / wordlist 検証を行い、内部に `SecretBox<Zeroizing<...>>` / `SecretBytes` で秘密値を保護する。`Kek<Kind>` は phantom-typed + Sealed trait で `KekPw` / `KekRecovery` 取り違えをコンパイルエラー化。`HeaderAeadKey::from_kek_pw(&Kek<KekKindPw>)` で Sub-0 §脅威モデル §4 L1 §対策(c) のヘッダ AEAD 鍵経路（KEK_pw 流用）を型表現 |
| 出力 | 各 newtype（成功時）/ `Result<T, CryptoError>`（失敗可能経路）。`Debug` 出力は秘密を含まない `[REDACTED ...]` 固定、`Display` / `serde::Serialize` は実装しない |
| エラー時 | Fail-Secure 必須: (a) `MasterPassword::new` 失敗 → `Err(CryptoError::WeakPassword(WeakPasswordFeedback))`、(b) `WrappedVek::new` 失敗 → `Err(DomainError::InvalidVaultHeader(WrappedVekEmpty / WrappedVekTooShort))`、(c) `NonceBytes::try_new` 失敗 → `Err(DomainError::InvalidRecordPayload(NonceLength))`、(d) `Verified<T>` を `pub(crate)` 経路外から構築しようとする → コンパイルエラー（型レベル禁止）。中途半端な構築（部分初期化型）は型システム上存在しない |

### REQ-S03: KDF（Argon2id）

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-B (#40) — `feat(shikomi-infra)` |
| 概要 | shikomi-infra に `crypto::kdf::Argon2idAdapter` を実装。`MasterPassword` + `KdfSalt` → `Kek<KekKindPw>` を導出（凍結値 `m=19456 KiB, t=2, p=1`）。RFC 9106 KAT を CI で実行、`criterion` ベンチで p95 1 秒上限を継続検証、4 年または逸脱時に再評価（`tech-stack.md` §4.7 `argon2` 行 / OWASP Password Storage Cheat Sheet） |
| 関連脅威 ID | L3（offline brute force に作業証明を強制、`m=19456 KiB` ≈ 19 MiB のメモリコストで GPU/ASIC を制限）／ L1（弱パスワード時の `wrapped_VEK_by_pw` 復号耐性は REQ-S08 zxcvbn ゲートとの併用で担保） |
| 入力 | (a) `&MasterPassword`（Sub-A の `expose_secret_bytes` 経由、`pub` 可視性正規経路、`detailed-design/password.md` §可視性ポリシー差別化）、(b) `&KdfSalt`（Sub-A 既存型 + Sub-B `Rng::generate_kdf_salt()` 由来、16B）、(c) `Argon2idParams { m: 19456, t: 2, p: 1 }`（const として shikomi-infra に固定、Sub-D でヘッダ永続化用に流用） |
| 処理 | `argon2::Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(19456, 2, 1, Some(32))) → hash_password_into(&mut [u8;32], password, salt)` で 32B 出力、即 `Kek::<KekKindPw>::from_array(out)` でラップ。中間 `[u8;32]` バッファは `Zeroizing<[u8;32]>` で囲む。`password-hash` PHC 文字列形式は使わない（DRY 違反、`tech-stack.md` §4.7 凍結） |
| 出力 | `Result<Kek<KekKindPw>, CryptoError>`（成功時） / `CryptoError::KdfFailed { kind: KdfErrorKind::Argon2id, source }`（失敗時） |
| エラー時 | Fail-Secure 必須: (a) `argon2` crate の `Error` を `KdfErrorKind::Argon2id` に包んで返す、(b) `unwrap()` / `expect()` 禁止、(c) リトライしない（KDF 失敗は決定論的バグまたはリソース枯渇のため、サイレントリトライで隠蔽しない）、(d) 中間バッファは Drop 時 zeroize（`Zeroizing` で型レベル保証） |

### REQ-S04: KDF（BIP-39 + PBKDF2 + HKDF）

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-B (#40) — `feat(shikomi-infra)` |
| 概要 | shikomi-infra に `crypto::kdf::Bip39Pbkdf2Hkdf` を実装。`RecoveryMnemonic`（24 語）→ BIP-39 標準 PBKDF2-HMAC-SHA512 2048iter で 64B seed 生成 → HKDF-SHA256 (`salt=None, ikm=seed, info=b"shikomi-kek-v1"`) で 32B `Kek<KekKindRecovery>` 導出。**`bip39` crate** の wordlist + checksum 検証通過を `RecoveryMnemonic::from_words` 構築の前提として強制。trezor 公式 `vectors.json` + RFC 5869 Appendix A KAT を CI で実行（`tech-stack.md` §4.7 `bip39` / `pbkdf2` / `hkdf` 行） |
| 関連脅威 ID | L3（リカバリ経路の brute force 耐性: 256 bit エントロピー × PBKDF2 2048iter で事実上不可能）／ L4（24 語盗難時の完全敗北は受容、ユーザ手書き保管責務、Sub-0 §脅威モデル §5 スコープ外） |
| 入力 | (a) `&RecoveryMnemonic`（Sub-A 既存型 `shikomi_core::crypto::recovery::RecoveryMnemonic`、**`expose_words()` 経由**で 24 語の `&[String; 24]` 取得、`pub` 可視性正規経路、`detailed-design/password.md` §可視性ポリシー差別化）、(b) BIP-39 passphrase は **Sub-B では未使用**（空文字列固定、`tech-stack.md` §2.4 凍結値 `salt='mnemonic'+''`）、(c) HKDF info 定数 `b"shikomi-kek-v1"`（const として shikomi-infra に固定） |
| 処理 | (1) **wordlist + checksum 検証**: `bip39::Mnemonic::parse_in(Language::English, words.join(" "))` で BIP-39 v2 系の `Mnemonic` を構築。失敗時 `CryptoError::InvalidMnemonic`（新規 variant、§errors-and-contracts.md `KdfErrorKind` 拡張）、(2) **seed 導出**: `mnemonic.to_seed("")` で 64B seed（内部で PBKDF2-HMAC-SHA512 2048iter `salt="mnemonic"+""`）、(3) **KEK_recovery 導出**: `hkdf::Hkdf::<Sha256>::new(None, &seed).expand(b"shikomi-kek-v1", &mut [u8;32])` で 32B 出力、即 `Kek::<KekKindRecovery>::from_array(out)` でラップ。中間 seed 64B / KEK 32B は `Zeroizing` で囲む |
| 出力 | `Result<Kek<KekKindRecovery>, CryptoError>`（成功時） / `CryptoError::InvalidMnemonic` または `CryptoError::KdfFailed { kind: KdfErrorKind::{Pbkdf2 \| Hkdf} }`（失敗時） |
| エラー時 | Fail-Secure 必須: (a) wordlist 不一致 / checksum 不一致は `bip39` crate が `Mnemonic::parse_in` で `Err` を返す → そのまま `CryptoError::InvalidMnemonic` で即拒否、(b) **再試行回数制限なし**（サイドチャネル排除: リトライ計測攻撃に Argon2id とは異なり PBKDF2 2048iter は計算時間が短いため、回数制限を設けると別経路の timing leak になる）、(c) 中間 seed / KEK は Drop 時 zeroize |

### REQ-S05: AEAD（AES-256-GCM）

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-C (#41) — `feat(shikomi-infra)` |
| 概要 | shikomi-infra に `crypto::aead::AesGcmAeadAdapter` を実装。per-record 暗号化（`encrypt_record` / `decrypt_record`）+ VEK wrap（`wrap_vek` / `unwrap_vek`）の 4 メソッド構成。AAD = `Aad::to_canonical_bytes()` の 26B（record_id 16B + vault_version 2B BE + created_at_micros 8B BE、既存 `shikomi_core::vault::crypto_data::Aad` 再利用）、random nonce 12B（Sub-B `Rng::generate_nonce_bytes`）、上限 $2^{32}$ で `NonceLimitExceeded`（Sub-D 呼出側責務）、NIST CAVP "GCM Test Vectors" KAT を CI で実行（`tech-stack.md` §4.7 `aes-gcm` 行）。**鍵バイト経路**: Sub-C で `crypto::aead_key::AeadKey` trait（クロージャインジェクション、`with_secret_bytes`）を新規追加、`Vek` / `Kek<_>` に impl、shikomi-core 側の `pub(crate)` 可視性ポリシー（Sub-B Rev2 凍結）を破壊せず外部 crate に借用越境（`detailed-design/nonce-and-aead.md` §`AeadKey` trait） |
| 関連脅威 ID | L1（AEAD 認証タグで改竄検出、AAD でロールバック検出、random nonce で並行書込時の衝突確率制約 ≤ $2^{-32}$）／ L3（VEK 不在時の平文化阻止、AEAD 検証失敗で `Verified<Plaintext>` を構築しない C-14 契約） |
| 入力 | (a) `&impl AeadKey`（`Vek` for per-record / `Kek<_>` for VEK wrap、Sub-A 凍結型 + Sub-C trait 経由）、(b) `&NonceBytes`（Sub-B `Rng::generate_nonce_bytes()` 由来、12B）、(c) `&Aad`（per-record のみ、`Aad::to_canonical_bytes() -> [u8;26]` で正規化）、(d) `&[u8]` plaintext / `&[u8]` ciphertext + `&AuthTag`（`encrypt_record` / `decrypt_record`） / `&Vek` / `&WrappedVek`（`wrap_vek` / `unwrap_vek`） |
| 処理 | (1) `key.with_secret_bytes(\|bytes\| ...)` クロージャ内で `Aes256Gcm::new(GenericArray::from_slice(bytes))` 構築（`aes-gcm` crate `aes_gcm::Aes256Gcm`、feature `aes` `alloc` + `zeroize`）、(2) **暗号化**: `aead::AeadInPlace::encrypt_in_place_detached(nonce, aad_bytes, &mut Zeroizing<Vec<u8>>::new(plaintext.to_vec()))` で **tag 分離方式**で AES-256-GCM 暗号化、(3) **復号 + AEAD 検証**: `decrypt_in_place_detached(nonce, aad_bytes, &mut buf, tag.as_array())` でタグ検証、成功時のみ `verify_aead_decrypt(\|\| Ok(Plaintext::new_within_module(buf)))` 経由で `Verified<Plaintext>` 構築（Sub-A caller-asserted マーカー契約）、(4) 中間バッファは `Zeroizing<Vec<u8>>` で囲み Drop 時 zeroize（C-16）、(5) **nonce_counter 統合**: adapter は `NonceCounter::increment` を呼ばない、Sub-D の vault リポジトリ層が encrypt 前に必須呼出（`detailed-design/nonce-and-aead.md` §nonce_counter 統合契約） |
| 出力 | `encrypt_record` → `Result<(Vec<u8>, AuthTag), CryptoError>`、`decrypt_record` → `Result<Verified<Plaintext>, CryptoError>`、`wrap_vek` → `Result<WrappedVek, CryptoError>`、`unwrap_vek` → `Result<Verified<Plaintext>, CryptoError>`（Sub-D で 32B 長さ検証して `Vek::from_array` 復元） |
| エラー時 | Fail-Secure 必須: (a) タグ不一致 / AAD 不一致 / nonce-key 取り違え / ciphertext 改竄は全て `CryptoError::AeadTagMismatch` に統一（内部詳細秘匿、MSG-S10）、(b) nonce 上限到達は **Sub-D の `NonceCounter::increment()?` で先行検出**、`CryptoError::NonceLimitExceeded` で `vault rekey` フローへ誘導（MSG-S11）、(c) `unwrap` / `expect` 禁止、`subtle` 経由でない自前 `==` tag 比較禁止（`tech-stack.md` §4.7 `subtle` 行）、(d) AEAD 検証失敗時に `Plaintext::new_within_module` を呼ばない（C-14 構造禁止）、(e) AEAD 中間バッファの `Zeroizing` 包囲を grep で機械検証（C-16） |

### REQ-S06: 暗号化 Vault リポジトリ

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-D (#42) — `feat(shikomi-infra)` |
| 概要 | shikomi-infra に `persistence::vault_migration::VaultMigration` を実装。**`SqliteVaultRepository` は暗号化に「無知」のまま据え置き**（Issue #42 凍結の責務境界、暗号文は不透明 BLOB として永続化）、`VaultMigration` service が `Argon2idHkdfVekProvider` + `AesGcmAeadAdapter` を組合せて平文⇄暗号化双方向マイグレーション + ヘッダ独立 AEAD タグ検証を担う。**4 メソッド構成**: `encrypt_vault` / `decrypt_vault` / `unlock_with_password` (`unlock_with_recovery`) / `rekey` / `change_password`（`detailed-design/repository-and-migration.md` §`VaultMigration` service） |
| 関連脅威 ID | L1（ヘッダ AEAD タグで `kdf_params` / `wrapped_VEK_*` / `nonce_counter` 差替検出、AAD にヘッダ全フィールド正規化バイト列を含める C-17/C-18）／ L3（vault.db 全件の AEAD 保護 + 平文⇄暗号化マイグレーション原子性で部分書込ゼロ C-21） |
| 入力 | (a) **`vault encrypt`**: `&str` 平文パスワード（`MasterPassword::new` で強度ゲート通過必須）、(b) **`vault decrypt`**: `&str` パスワード + `DecryptConfirmation`（型レベル二段確認証跡、`"DECRYPT"` キーワード + パスワード再入力で構築）、(c) **`vault unlock`**: `&str` パスワード または `RecoveryMnemonic`、(d) **`rekey`**: 現在のパスワード（VEK 入替契機: nonce overflow 自動 / ユーザ明示）、(e) **`change-password`**: 現パスワード + 新パスワード |
| 処理 | (1) `MasterPassword::new` 強度ゲート（Sub-A/B、強度 ≥ 3）、(2) Sub-B `Argon2idAdapter::derive_kek_pw` / `Bip39Pbkdf2Hkdf::derive_kek_recovery` で KEK 階層構築、(3) Sub-B `Rng::generate_vek/kdf_salt/nonce_bytes/mnemonic_entropy` で CSPRNG、(4) Sub-C `AesGcmAeadAdapter::wrap_vek/unwrap_vek/encrypt_record/decrypt_record` で AEAD、(5) ヘッダ AEAD タグ envelope を `HeaderAeadKey` + `Aad::HeaderEnvelope(canonical_bytes)` で構築・検証（C-17）、(6) `vault-persistence` の atomic write（`.new` → fsync → rename）で REQ-P04/P05 継承、(7) マイグレーション失敗時は `.new` cleanup で原状復帰（C-21）、(8) `vault encrypt` 完了時 `RecoveryDisclosure` を返却（**呼出側 = Sub-E daemon / Sub-F CLI が 1 度だけ `disclose` してユーザに表示、再表示禁止を型レベル強制 C-19**） |
| 出力 | `Result<RecoveryDisclosure, MigrationError>`（`encrypt_vault`） / `Result<usize, MigrationError>`（`decrypt_vault` / `rekey`、復号 / 再暗号化レコード件数） / `Result<(Vault, Vek), MigrationError>`（`unlock_with_*`、Sub-E が `Vek` を daemon RAM キャッシュ） / `Result<(), MigrationError>`（`change_password`） |
| エラー時 | Fail-Secure 必須: (a) 弱パスワード `Err(MigrationError::WeakPassword)` → MSG-S08（`detailed-design/password.md` §i18n 戦略責務分離 で英語raw → 翻訳）、(b) AEAD タグ検証失敗 `Err(MigrationError::Crypto(AeadTagMismatch))` → MSG-S10（**過信防止 = 断定禁止、改竄シナリオ最低1件明示、バックアップ復元案内**、Sub-C Rev1 凍結指針）、(c) nonce 上限到達 `Err(MigrationError::Crypto(NonceLimitExceeded))` → MSG-S11（`vault rekey` 誘導、残操作猶予数値非表示）、(d) atomic write 失敗 `Err(MigrationError::AtomicWriteFailed)` → MSG-S13（原状復帰済み明示）、(e) マイグレーション中の SIGKILL / 電源断は `vault-persistence` の `.new` 残存検出で次回 load 時にクリーンアップ（部分書込ゼロ） |

### REQ-S07: REQ-P11 解禁

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-D (#42) — `feat(shikomi-infra)` + `vault-persistence` 横断改訂 |
| 概要 | `vault-persistence/requirements.md` REQ-P11 を「暗号化モード**全般**を即時拒否」→「**未対応バージョン**の暗号化スキーマを拒否」に意味論変更（Issue #42 §REQ-P11 改訂のトレーサビリティ凍結文言）。現行 v1 は受入、v999 等の未来バージョンは `UnsupportedYet { feature: "vault schema version", supported_range, actual }` で拒否 |
| 関連脅威 ID | L1 / L3（暗号化モードを実運用可能にすることで両脅威への対策が初めて実効化する） |
| 入力 | (a) `vault.protection_mode() == Encrypted` の `Vault`（save 側）、(b) `vault_header.protection_mode='encrypted'` 行（load 側）、(c) `vault_header.vault_version` の値（バージョン判定の入力） |
| 処理 | (1) `vault-persistence/detailed-design/flows.md` load step 12 / save step 2 の **`UnsupportedYet` 即 return を削除**、暗号化モード正常経路（`Mapping::row_to_vault_header` で `VaultEncryptedHeader` 構築 → `Vault::new_encrypted` 集約構築）を解禁、(2) **`PRAGMA user_version` 範囲外**判定を新規追加（既存 step 9 のスキーマバージョン検証を拡張）、範囲外なら `UnsupportedYet { feature: "vault schema version", supported_range, actual }`、(3) `vault-persistence` は引き続き暗号化に「無知」（暗号文は不透明 BLOB、AEAD 計算は呼出側 Sub-D `VaultMigration` 責務）、(4) DDL 拡張（`kdf_params` / `header_aead_*` カラム追加）は `PRAGMA user_version` bump + `ALTER TABLE` で既存 plaintext vault に影響なし |
| 出力 | (a) 既存 v1 暗号化 vault: `Ok(Vault)` / `Ok(())`（解禁）、(b) v999 等の未来バージョン: `Err(PersistenceError::UnsupportedYet { feature: "vault schema version", supported_range: (V_MIN, V_MAX), actual })` |
| エラー時 | 既存の `PersistenceError` 各バリアントは維持、`UnsupportedYet` の意味論のみ変更。新規エラー variant は追加せず、既存 11 variants で完結（最小差分原則） |

### REQ-S08: パスワード強度ゲート

| 項目 | 内容 |
|------|------|
| 担当 Sub | **Sub-A (#39) で trait + WeakPasswordFeedback 確定** + **Sub-B (#40) で `ZxcvbnGate` 具象実装** + Sub-D (#42) で `vault encrypt` 入口統合・MSG-S08 文言 — `feat(shikomi-core)` + `feat(shikomi-infra)` （**Boy Scout Rule で旧 Sub-D 担当を Sub-B に再分配**: `ZxcvbnGate` は shikomi-infra::crypto::password 配下の暗号アダプタ層、Sub-D の vault リポジトリ責務とは独立、Clean Arch 境界整合のため Sub-B 担当が正） |
| 概要 | `vault encrypt` 入口で zxcvbn 強度 ≥ 3 を Fail Fast チェック、強度不足時は `Feedback`（warning + suggestions）を CLI/GUI に提示（Fail Kindly）。Sub-A は shikomi-core の `PasswordStrengthGate` trait + `WeakPasswordFeedback` 型のみを公開し、Sub-B が `shikomi-infra::crypto::password::ZxcvbnGate` で zxcvbn 呼出の具象実装、Sub-D が `vault encrypt` 入口で呼出 + i18n 翻訳して MSG-S08 を提示する三段責務分担 |
| 関連脅威 ID | L3（弱パスワード時の Argon2id offline 突破を入口で禁止、KDF 強度の前提条件を型コンストラクタで担保）／ L1（同上、`wrapped_VEK_by_pw` の offline brute force 耐性確保） |
| 入力 | **Sub-A**: trait シグネチャ `validate(&self, password: &str) -> Result<(), WeakPasswordFeedback>`、`WeakPasswordFeedback { warning: Option<String>, suggestions: Vec<String> }`。**Sub-B**: `ZxcvbnGate` 構造体のコンフィグ（最小強度 const = 3、`zxcvbn::Score::Three` 以上）、user_inputs リスト（空 `&[]` で初期実装、Sub-D で username / vault path 等の文脈追加検討）。**Sub-D**: `vault encrypt` サブコマンドからの呼出経路 |
| 処理 | **Sub-A**: `MasterPassword::new(s, gate)` が `gate.validate(&s)` を呼び、`Ok(())` なら `MasterPassword` 構築、`Err(WeakPasswordFeedback)` なら `Err(CryptoError::WeakPassword(Box::new(_)))` で構築失敗。**Sub-B**: `impl PasswordStrengthGate for ZxcvbnGate { fn validate(&self, p: &str) -> Result<(), WeakPasswordFeedback> { let r = zxcvbn::zxcvbn(p, &[]); if (r.score() as u8) >= 3 { Ok(()) } else { Err(WeakPasswordFeedback { warning: r.feedback().and_then(\|f\| f.warning().map(\|w\| w.to_string())), suggestions: r.feedback().map(\|f\| f.suggestions().iter().map(\|s\| s.to_string()).collect()).unwrap_or_default() }) } } }` 相当の責務（疑似コード禁止のため概要記述）。`tech-stack.md` §4.7 凍結の英語 raw `Feedback` をそのまま運ぶ（i18n 翻訳は Sub-D / Sub-F 責務、`detailed-design/password.md` §i18n 戦略責務分離）。**Sub-D**: 受領した `WeakPasswordFeedback` を MSG-S08 / MSG-S18 に変換、`warning=None` 時のフォールバック文言提示 |
| 出力 | **Sub-A**: trait `Result<(), WeakPasswordFeedback>`。**Sub-B**: 同 trait の具象実装 `ZxcvbnGate`、無状態 struct（`#[derive(Default)]` で構築、内部に最小強度 const 値のみ）。**Sub-D**: ユーザ向け MSG-S08 メッセージ |
| エラー時 | Fail-Secure + Fail Kindly: (a) 拒否は早期（`MasterPassword` 構築自体が失敗、後続 KDF / AEAD 経路に弱鍵を渡さない）、(b) `feedback.warning` / `feedback.suggestions` をユーザにそのまま提示（Sub-A → Sub-B → Sub-D の責務鎖）、(c) `ZxcvbnGate::validate` 内で **panic 禁止**（zxcvbn の内部例外は `WeakPasswordFeedback` または `Ok(())` のいずれかに収束させる、`detailed-design/password.md` §Sub-D が遵守すべき契約 第 2 項）、(d) MSG-S08 文言と `warning=None` フォールバック / i18n 戦略は Sub-D で確定（Sub-B は trait 契約を守る責務に閉じる） |

### REQ-S09: VEK キャッシュ

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-E (#43) — `feat(shikomi-daemon)` |
| 概要 | daemon プロセス内 `secrecy::SecretBox<[u8;32]>`、アイドル 15min / スクリーンロック / サスペンドで `zeroize` |
| 関連脅威 ID | L2（VEK 滞留時間の上限化、サスペンド時の `zeroize` で過去メモリ抽出を最小化）／ L4（同特権デバッガには無力＝受容） |
| 入力 | TBD by Sub-E |
| 処理 | TBD by Sub-E（OS 別のスクリーンロック検出 / サスペンド signal 購読は `shikomi-infra` のアダプタ経由） |
| 出力 | TBD by Sub-E |
| エラー時 | TBD by Sub-E（Fail-Secure 必須: ロック失敗時は VEK を必ず zeroize して再 unlock 強制） |

### REQ-S10: マスターパスワード変更 O(1)

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-E (#43) — `feat(shikomi-daemon)` |
| 概要 | VEK 不変、`wrapped_VEK_by_pw` のみ再生成・置換、全レコード再暗号化なし |
| 関連脅威 ID | L1（変更操作中もレコード AEAD タグは不変、改竄検出能力を維持）／ L2（VEK は変更されないため再 unlock 不要、滞留時間の追加リスクなし） |
| 入力 | TBD by Sub-E |
| 処理 | TBD by Sub-E（atomic write 必須、`wrapped_VEK_by_pw` 更新と新 KDF パラメータ反映を 1 トランザクションで） |
| 出力 | TBD by Sub-E |
| エラー時 | TBD by Sub-E（Fail-Secure 必須: 旧 wrap が消えて新 wrap が書込失敗する状態を作らない＝ atomic write） |

### REQ-S11: アンロック失敗バックオフ

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-E (#43) — `feat(shikomi-daemon)` |
| 概要 | 連続失敗 5 回で `tokio::time::sleep` 指数バックオフ、ホットキー購読 blocking なし（プロセス全体は応答継続） |
| 関連脅威 ID | L1（同ユーザ別プロセスからの IPC 経由 brute force レート制限）／ L4（root 権限取得攻撃には無効＝受容） |
| 入力 | TBD by Sub-E |
| 処理 | TBD by Sub-E（バックオフは該当 IPC リクエスト hop に閉じ、daemon 全体の `Future` を blocking しない＝ホットキー応答継続） |
| 出力 | TBD by Sub-E |
| エラー時 | TBD by Sub-E（Fail-Secure 必須: 失敗カウンタの永続化方針を Sub-E で確定、再起動で失敗履歴をリセットしない） |

### REQ-S12: IPC V2 拡張

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-E (#43) — `feat(shikomi-daemon)` + `feat(shikomi-cli/gui)` |
| 概要 | `IpcRequest::{Unlock, Lock, ChangePassword, RotateRecovery, Rekey}` 追加、`IpcProtocolVersion::V2` 非破壊昇格、`daemon-ipc/basic-design/ipc-protocol.md` を更新 |
| 関連脅威 ID | L1（IPC 経路は `(_, Ipc) => Secret` パターン継承、Issue #33 の fail-secure を踏襲） |
| 入力 | TBD by Sub-E |
| 処理 | TBD by Sub-E（V1 互換維持、V2 専用 variant は V1 クライアントには `UnsupportedRequest` で拒否） |
| 出力 | TBD by Sub-E |
| エラー時 | TBD by Sub-E（Fail-Secure 必須: バージョン不整合は接続拒否、中途半端な V1.5 状態を作らない） |

### REQ-S13: リカバリ初回 1 度表示

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-D (#42) + Sub-E (#43) |
| 概要 | BIP-39 24 語の生成・表示は初回のみ、再表示不可、永続化しない（メモリゼロ化のみ）、ユーザ手書き保管前提 |
| 関連脅威 ID | L1 / L2 / L3（24 語をディスクに残さない、メモリ滞留を最小化、ヘッダには wrap 後の `wrapped_VEK_by_recovery` のみ保管）／ L4（24 語自体の盗難は受容、ユーザ責任） |
| 入力 | (a) `vault encrypt` 完了直後の `RecoveryDisclosure` 値（Sub-D `VaultMigration::encrypt_vault` の戻り値）、(b) ユーザ画面 / プリンタ / 点字プリンタ等の出力デバイス（Sub-F CLI / Sub-E daemon の経路選択） |
| 処理 | (1) **Sub-D**: `VaultMigration::encrypt_vault` 完了時に `RecoveryMnemonic` から `RecoveryDisclosure` を構築（`pub(crate)` コンストラクタ、shikomi-infra 内のみ）、戻り値として呼出側に渡す、(2) **Sub-E (daemon) または Sub-F (CLI)**: `disclosure.disclose()` を**1 回だけ呼出**して `RecoveryWords` を取得（**`self` 消費で 2 度呼出は compile_fail = C-19**）、ユーザに表示、表示完了後 `RecoveryWords` を即 Drop（zeroize）、(3) クラッシュ・キャンセル経路では `disclosure.drop_without_disclose()` で「ユーザに見せずに即破棄」する Fail Secure 経路、(4) **再表示 API は提供しない**（`RecoveryDisclosure::disclose` の所有権消費 + `RecoveryWords::Serialize` 未実装で永続化禁止 + `Display` 未実装で誤フォーマット出力禁止の三層型レベル封鎖） |
| 出力 | (a) 通常経路: ユーザ画面に番号付き 24 語表示（MSG-S06 警告同伴）、(b) アクセシビリティ経路: `vault recovery-show --print` ハイコントラスト PDF / `--braille` .brf 出力 / `--audio` OS TTS（MSG-S18 案内同伴） |
| エラー時 | Fail-Secure 必須: (a) 表示完了前のクラッシュ時は `RecoveryDisclosure::Drop` で `RecoveryMnemonic` zeroize（**ユーザに見せていない & 永続化されていない 24 語が残らない**、`vault encrypt` 自体は atomic write 完了済のため新規 vault は残るが、ユーザは MSG-S11 / MSG-S12 経路で「リカバリ表示失敗、`vault recovery-show` 再実行を……」とは案内**しない**: 再表示 API が存在しないため、Sub-D は「**最初からやり直し（`vault decrypt` で平文に戻して再 encrypt**）」または「**マスターパスワードのみで運用、リカバリ放棄**」の 2 択をユーザに提示する、(b) `RecoveryDisclosure::disclose` 2 回目呼出は所有権消費で compile_fail（型レベル禁止）、runtime での `RecoveryAlreadyConsumed` は basic dev mistake 検出用 |
| **アクセシビリティ** | **「初回 1 度きり表示」は視覚障害ユーザにとって完全敗北リスクに直結**（24 語を視認できない → 手書き不能 → 再表示不可 → マスターパスワード失念時に L4 相当の永久損失）。以下の代替経路を Sub-D / Sub-F が提供する: (1) **スクリーンリーダー対応**: 24 語表示要素に明示的な ARIA ロール / `aria-live="assertive"` / 連続読み上げ可能なテキスト構造を付与、(2) **OS 読み上げ拒否環境への代替**: `vault recovery-show --print` で**ハイコントラスト印刷可能 PDF**（黒地白文字、最大 36pt フォント、各語に番号付与）を出力、(3) **点字対応**: `vault recovery-show --braille` で `.brf`（Braille Ready Format）出力、(4) **音声プレイヤー優先順位ガイド**: `--audio` オプションで OS 標準 TTS を呼ぶ際、録音禁止プレイヤー（macOS VoiceOver / Windows ナレーター / Linux Orca 直接呼出し）の優先順位をドキュメント化。ユーザ向け案内は MSG-S18 で確定。**WCAG 2.1 AA 準拠**を非機能要件として固定し、Sub-D / Sub-F の受入条件に組み込む |

### REQ-S14: nonce overflow 検知 → rekey 強制

| 項目 | 内容 |
|------|------|
| 担当 Sub | **Sub-A (#39) で型契約確定**（`NonceCounter::increment` の `Result` 返却 + Boy Scout Rule で責務再定義） + Sub-C (#41) で AEAD 経路統合 + Sub-F (#44) で `vault rekey` フロー |
| 概要 | shikomi-core 側で `NonceCounter` の責務を「VEK ごとの暗号化回数監視」に再定義し、`increment(&mut self) -> Result<(), DomainError>` が上限 $2^{32}$ 到達時 `NonceLimitExceeded` を返す型契約（Sub-A）。AEAD 経路統合（Sub-C）と `vault rekey` 起動フロー（Sub-F）は後続 |
| 関連脅威 ID | L1（random nonce 衝突確率を $\le 2^{-32}$ に維持、上限到達後の暗号化を**型レベルで構造禁止**） |
| 入力 | **Sub-A**: vault ヘッダから読み込む `nonce_counter: u64`（`NonceCounter::resume(count)` 経由）、または新規 vault `NonceCounter::new()` で `count=0`。**Sub-C**: `AesGcmAeadAdapter::encrypt_record` 自体は increment を呼ばない（責務分離 SRP）。`NonceLimitExceeded` の発火は **Sub-D の vault リポジトリ層**が `encrypt_record` 呼出前に `NonceCounter::increment()?` を実行する設計（`detailed-design/nonce-and-aead.md` §nonce_counter 統合契約）。**Sub-F**: `NonceLimitExceeded` 検知時の `vault rekey` 起動 |
| 処理 | **Sub-A**: `count < (1u64 << 32)` なら `count += 1; Ok(())`、上限到達なら `Err(DomainError::NonceLimitExceeded)`。`#[must_use]` 属性で結果無視を clippy lint で検出。既存「8B prefix + 4B counter」設計を**完全廃止**（Boy Scout Rule、per-record nonce は `NonceBytes::from_random([u8;12])` で完全 random 12B に変更）。**Sub-C**: AEAD adapter は nonce 上限管理を持たない（Sub-D 側に委譲、SRP）。**Sub-F**: `vault rekey` フローで新 VEK 生成 + 全レコード再暗号化、`NonceCounter::new()` で count をリセット |
| 出力 | **Sub-A**: `Result<(), DomainError>`（成功時 unit 値、失敗時 variant）、`current(&self) -> u64`（永続化用）。**Sub-C**: 出力なし（adapter 経由）。**Sub-F**: TBD |
| エラー時 | Fail-Secure 必須: (a) 上限到達後の暗号化試行は **`NonceCounter::increment` が `Err` を返すことで構造的に禁止**、(b) `unwrap()` は禁止（`#[must_use]` + clippy lint）、(c) Sub-F の `vault rekey` 完了まで以後のレコード暗号化を全面拒否（Sub-D / Sub-F で詳細化）、(d) **Sub-C adapter 単体の `encrypt_record` を nonce_counter 経由なく呼び出した場合は契約違反**（adapter は責務分離で increment を持たないため、呼出側 = Sub-D が increment を忘れた場合に nonce 上限を超えても adapter は検出不能）。**呼出側責務**として `detailed-design/nonce-and-aead.md` §nonce_counter 統合契約 + Sub-D `repository-and-migration.md` PR レビューチェックリストで担保 |

### REQ-S15: vault 管理サブコマンド

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-F (#44) — `feat(shikomi-cli)` |
| 概要 | `shikomi vault {encrypt, decrypt, unlock, lock, change-password, recovery-show, rekey}` の CLI 実装、IPC V2 経由 |
| 関連脅威 ID | L1（CLI → daemon は IPC、CLI 自身は VEK を持たない）／ L2（CLI プロセスは短命、メモリ滞留時間が daemon より小さい） |
| 入力 | TBD by Sub-F |
| 処理 | TBD by Sub-F（各サブコマンドは IPC V2 リクエストへ 1:1 マップ、Phase 2 規定通り CLI は vault に直接触らない） |
| 出力 | TBD by Sub-F |
| エラー時 | TBD by Sub-F（Fail-Secure 必須: IPC エラーをユーザに正確に伝達、内部詳細は audit log にのみ） |

### REQ-S16: 保護モード可視化

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-F (#44) — `feat(shikomi-cli)` |
| 概要 | `shikomi list` 出力ヘッダで `[plaintext]` / `[encrypted]` を常時表示（Fail Visible、`threat-model.md` §7.0 既定踏襲） |
| 関連脅威 ID | L1 / L3（モード誤認による平文モードでの長期運用事故防止） |
| 入力 | TBD by Sub-F |
| 処理 | TBD by Sub-F（ヘッダ常時可視化、`--no-mode-banner` のような隠蔽オプションは提供しない） |
| 出力 | TBD by Sub-F |
| エラー時 | TBD by Sub-F（Fail-Secure 必須: モード判定不能時は `[unknown]` を表示し、レコード一覧表示自体を停止） |

### REQ-S17: Fail-Secure 型レベル強制

| 項目 | 内容 |
|------|------|
| 担当 Sub | **Sub-A (#39) で 5 パターン全契約確定** + Sub-B〜F で契約遵守（破ったら PR レビュー却下） |
| 概要 | (1) `Verified<T>` newtype（`pub(crate)` コンストラクタ可視性）、(2) `MasterPassword::new` の `&dyn PasswordStrengthGate` 構築時要求、(3) `NonceCounter::increment` の `Result<(), DomainError>` 返却 + `#[must_use]`、(4) `CryptoOutcome<T>` enum で `match` 暗号アーム第一パターン強制（失敗バリアント先頭並び）、(5) `Drop` 連鎖（`Vek` / `Kek<_>` / `MasterPassword` / `RecoveryMnemonic` / `Plaintext` / `HeaderAeadKey` 全てに `Drop` 経路、内包する `SecretBox<Zeroizing<...>>` の zeroize が transitive 発火） |
| 関連脅威 ID | L1（`Verified<T>` で AEAD 検証 bypass 構造禁止、`Kek<Kind>` phantom-typed で鍵経路取り違え禁止）／ L2（`Drop` 連鎖と `Clone` 禁止で滞留時間最小化）／ L3（`MasterPassword` 強度ゲートで弱鍵を KDF 入口排除）— 実装ミスによる脆弱性経路を**型システムで構造封鎖** |
| 入力 | **Sub-A**: なし（型契約のみ）。**Sub-B〜F**: 各 Sub 本文の入力を本契約の枠内に収める |
| 処理 | **Sub-A**: `detailed-design/index.md` §クラス設計（詳細） + 各分冊（`crypto-types.md` / `password.md` / `nonce-and-aead.md` / `errors-and-contracts.md`）参照。`shikomi-core::crypto::verified` モジュールに `Verified<T>` / `Plaintext` / `CryptoOutcome<T>` を実装。`shikomi-core::crypto::password` に `PasswordStrengthGate` trait と `MasterPassword::new`。`shikomi-core::vault::nonce` の `NonceCounter::increment` に `#[must_use]` 付与。**Sub-B〜F**: 契約破りは PR レビューで却下（Boy Scout Rule） |
| 出力 | **Sub-A**: 上記 5 種の型・trait・enum 定義。**Sub-B〜F**: 契約遵守の実装 |
| エラー時 | 型システムによる強制（違反はコンパイルエラーまたは clippy lint 失敗）。runtime 検出は `CryptoError::VerifyRequired`（テスト経路でのみ発生想定） |

## 画面・CLI仕様

該当なし — 理由: 本 Sub-0 は脅威モデル文書化と REQ-S* 採番のみがスコープ。CLI 仕様（`shikomi vault {encrypt/decrypt/unlock/lock/change-password/recovery-show/rekey}` の引数・出力形式・終了コード）は **Sub-F (#44)** の `requirements.md` 拡張で確定する。本ファイルの本セクションは Sub-F 設計時に **READ → EDIT** で表に書き起こす。

## API仕様

該当なし — 理由: 本 Sub-0 段階では API 確定をしない。IPC V2 拡張（`IpcRequest::{Unlock, Lock, ChangePassword, RotateRecovery, Rekey}` / レスポンス型 / エラー variant）の確定は **Sub-E (#43)** で行い、`daemon-ipc/basic-design/ipc-protocol.md` および `daemon-ipc/detailed-design/protocol-types.md` を主たる正本とする。本ファイルの本セクションは Sub-E 設計時に外部リンクのみ書き戻す。

## データモデル

Sub-A (#39) で **shikomi-core 側の型定義を確定**。SQLite カラム制約 / 永続化フォーマット詳細は Sub-D (#42) で本ファイルを READ → EDIT して追記する。

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| `VaultEncryptedHeader` | version / created_at / kdf_salt / wrapped_vek_by_pw / wrapped_vek_by_recovery / nonce_counter / kdf_params | `VaultVersion` / `OffsetDateTime` / `KdfSalt` / `WrappedVek` / `WrappedVek` / `NonceCounter` / `KdfParams`（Sub-D で型確定） | ヘッダ独立 AEAD タグで保護（鍵 = `HeaderAeadKey`、Sub-D 詳細）、`vault_format_version` で互換管理 | `Vault` ↔ 1:1 |
| `WrappedVek` | ciphertext / nonce / tag | `Vec<u8>` / `NonceBytes` / `AuthTag` | Sub-A で**内部構造分離型化**（Boy Scout Rule）、`new(ct, nonce, tag) -> Result<Self, DomainError>`、ciphertext 空 / 32B 未満は拒否、`wrapped_VEK_by_pw` / `wrapped_VEK_by_recovery` の 2 バリアント | `VaultEncryptedHeader` ↔ N:1 |
| `KdfSalt` | inner | `[u8; 16]` 固定長 | 16B、shikomi-core 側は `try_new(&[u8])` のみ、**`shikomi-infra::crypto::Rng::generate_kdf_salt() -> KdfSalt`** が単一エントリ点（Sub-0 凍結文言を Clean Architecture 整合的に再解釈） | `VaultEncryptedHeader` ↔ 1:1 |
| `KdfParams` | m / t / p | TBD by Sub-D（`Argon2idParams` struct 想定） | Argon2id `m=19456, t=2, p=1`（`tech-stack.md` §4.7 凍結値）、ヘッダ AEAD タグで改竄検出（直接ではなく KDF 出力変化での間接検出、basic-design.md §セキュリティ設計 §脅威モデル L1 §対策(c)） | `VaultEncryptedHeader` ↔ 1:1 |
| `NonceCounter` | count | `u64` | Sub-A で**責務再定義**: 既存「8B prefix + 4B counter」設計廃止、新責務は「VEK ごとの暗号化回数監視のみ」。上限 `1u64 << 32` (= $2^{32}$) で `NonceLimitExceeded` | `VaultEncryptedHeader` ↔ 1:1 |
| `NonceBytes` | inner | `[u8; 12]` 固定長 | per-record AEAD nonce、`from_random([u8;12])`（CSPRNG 由来）と `try_new(&[u8])`（永続化復元）の 2 経路 | `WrappedVek` / `EncryptedRecord` から参照 |
| `AuthTag` | inner | `[u8; 16]` 固定長 | AES-GCM 認証タグ、`try_new(&[u8])` で長さ検証 | `WrappedVek` / `EncryptedRecord` から参照 |
| `EncryptedRecord` | ciphertext / nonce / aad / tag | TBD by Sub-C / Sub-D | per-record AEAD ciphertext + AAD（record_id ‖ version ‖ created_at、26B）+ nonce 12B + tag 16B | `Vault` ↔ N:1 |
| `Vek`（揮発のみ、Sub-A 新規） | inner | `SecretBox<Zeroizing<[u8; 32]>>` | 32B、`from_array([u8;32])`、`Clone` 禁止、`Debug='[REDACTED VEK]'`、`Display`/`Serialize` 未実装、`expose_within_crate` は `pub(crate)` のみ。daemon プロセス内のみ滞留（unlock〜lock、最大アイドル 15min） | キャッシュ寿命: Sub-E |
| `Kek<KekKindPw>`（揮発のみ、Sub-A 新規） | inner / kind | `(SecretBox<Zeroizing<[u8;32]>>, PhantomData<KekKindPw>)` | 32B、Argon2id 出力をラップ、phantom-typed で `KekKindRecovery` と取り違え不可 | KekPw 由来鍵階層 |
| `Kek<KekKindRecovery>`（揮発のみ、Sub-A 新規） | inner / kind | `(SecretBox<Zeroizing<[u8;32]>>, PhantomData<KekKindRecovery>)` | 32B、HKDF 出力をラップ | KekRecovery 由来鍵階層 |
| `HeaderAeadKey`（揮発のみ、Sub-A 新規） | inner | `SecretBox<Zeroizing<[u8;32]>>` | `from_kek_pw(&Kek<KekKindPw>) -> HeaderAeadKey`、Sub-0 凍結のヘッダ AEAD 鍵 = KEK_pw 流用契約を型表現 | ヘッダ AEAD 検証専用 |
| `MasterPassword`（揮発のみ、Sub-A 新規） | inner | `SecretBytes` | `new(s, &dyn PasswordStrengthGate) -> Result<MasterPassword, CryptoError>`、強度ゲート通過後のみ構築、`Drop` 時 zeroize | 永続化しない |
| `RecoveryMnemonic`（揮発のみ、Sub-A 新規） | words | `SecretBox<Zeroizing<[String; 24]>>` | BIP-39 24 語、`from_words([String;24])`、`Drop` 時各語 zeroize、再表示不可（Sub-0 REQ-S13）。BIP-39 wordlist 検証は Sub-B 連携 | 永続化しない |
| `Plaintext`（揮発のみ、Sub-A 新規） | inner | `SecretBytes` | `new_within_crate(Vec<u8>)` で `pub(crate)` 構築、`Verified<Plaintext>::into_inner` 経由でのみ取り出し可 | レコード復号後の平文 |
| `Verified<T>`（揮発のみ、Sub-A 新規） | inner | `T`（ジェネリクス） | `new_from_aead_decrypt(t: T) -> Verified<T>` を `pub(crate)` 可視性で実装、AEAD 復号成功経路でのみ構築可 | Fail-Secure 型レベル強制 |
| `WeakPasswordFeedback`（公開構造体、Sub-A 新規） | warning / suggestions | `Option<String>` / `Vec<String>` | zxcvbn の `feedback` 構造をそのまま運ぶ、`Debug`/`Clone`/`Serialize` 派生（フィードバック自体は秘密でない） | `PasswordStrengthGate::validate` の Err |
| `CryptoOutcome<T>`（Sub-A 新規 enum） | TagMismatch / NonceLimit / KdfFailed / WeakPassword / Verified | enum バリアント | 失敗バリアント先頭並び（`match` 暗号アーム第一強制）、`#[non_exhaustive]` で将来追加に備える | Sub-C / Sub-D 実装で使用 |

## ユーザー向けメッセージ一覧

本 Sub-0 段階では MSG-* の文言を確定しない。各 MSG-* の文言・表示条件は担当 Sub の設計工程で本ファイルを READ → EDIT して埋める。**Fail Kindly 原則**（拒否は早期、しかし「なぜ・どう」をユーザに渡す）を全 MSG-* で守ること。

| ID | 種別 | メッセージ | 表示条件 |
|----|------|----------|---------|
| MSG-S01 | 成功 | **「暗号化が完了しました。リカバリ用の 24 語をこの後表示します。**写真撮影は禁止**、紙に書き写し金庫等で保管してください**」（後続で MSG-S06 + 24 語表示に連結、Sub-D 確定） | `vault encrypt` 完了時 |
| MSG-S02 | 成功 | 「**暗号保護を解除しました**。vault.db は平文モードに戻っています。物理ディスク奪取で全レコードが平文化されるリスクを踏まえ、必要に応じて `vault encrypt` で再度暗号化してください」（Sub-D 確定） | `vault decrypt` 完了時 |
| MSG-S03 | 成功 | TBD by Sub-E | `vault unlock` 成功時 |
| MSG-S04 | 成功 | TBD by Sub-E | `vault lock` 完了時 |
| MSG-S05 | 成功 | **Sub-D 部分確定**: 「マスターパスワードを変更しました。**VEK は不変のため再 unlock は不要、レコード再暗号化も発生しません**」（O(1) change-password、REQ-S10 / `repository-and-migration.md` §F-D5）。Sub-E が IPC V2 経路で daemon 側のキャッシュ無効化メッセージと統合確定 | `vault change-password` 完了時 |
| MSG-S06 | 警告 | **Sub-D 確定**: 「**【最重要】この 24 語は今しか表示できません。再表示は永久に不可能です。**」+ 3 点遵守事項 — (1)「**紙に書き写し**、photoや画面録画は絶対に行わないでください」、(2)「クラウドストレージ・メール・SMS・パスワードマネージャ等の同期可能領域に**保存しないでください**」、(3)「金庫等の物理保管推奨。マスターパスワードと**別の場所**に保管してください」+ 「書き写し完了」確認ボタン押下まで次画面に進めない（**MSG-S16 同型の明示合意取得**）。Sub-F は CLI 経路でも同等文言を確定（`--accept-recovery-shown` のような bypass フラグは**提供しない**） | `vault recovery-show` 表示直前 |
| MSG-S07 | 成功 | TBD by Sub-F | `vault rekey` 完了時（再暗号化レコード数を表示） |
| MSG-S08 | エラー | **Sub-D 確定**: 「**パスワードが弱すぎます**」+ zxcvbn の `feedback.warning` 翻訳（i18n 翻訳辞書経由、`detailed-design/password.md` §i18n 戦略責務分離）+ `feedback.suggestions` を bullet list 表示。`warning=None` 時のフォールバック: (a) 既定文言「強度が不足しています、下記の改善提案を参照してください」+ (b) `suggestions` 先頭文 + (c) 強度スコア「現在 N/4」表示の 3 経路から Sub-D が判定（`detailed-design/password.md` §`warning=None` 時の代替警告文契約）。**内部詳細**（zxcvbn raw score / KDF パラメータ）は含めない | パスワード強度不足（`vault encrypt` / `change-password` 入口） |
| MSG-S09 | エラー | TBD by Sub-E（**カテゴリ別ヒント方針**: 単一文言で「失敗しました」と返さず、原因カテゴリ別に異なる Fail Kindly メッセージを出す。最低 3 カテゴリ — (a)「パスワード違い」: 連続失敗回数 / 次の試行可能までの待機秒数 + 「リカバリニーモニックでの復号 (`vault unlock --recovery`) も可能」案内、(b)「IPC 接続不能」: daemon 起動状態 (`shikomi daemon status`) の確認案内、(c)「キャッシュ揮発タイムアウト / 自動 lock」: アイドル 15min / スクリーンロック / サスペンドで lock した旨と再 unlock 案内。**内部詳細**（KDF パラメータ・nonce カウンタ・スタックトレース）は含めない。MSG 文言は Sub-E で確定） | アンロック失敗（IPC 経由全般） |
| MSG-S10 | エラー | **Sub-D 確定**（Sub-C Rev1 で文言設計指針凍結）: 「**vault.db の AEAD 検証に失敗しました**」+ 過信防止「これは vault.db の**改竄の可能性**を示しますが、ディスク破損 / 実装バグでも発生します（**断定はできません**）」+ 過小評価回避「**いずれにせよ vault.db を信頼してはなりません**」+ 次の一手「**バックアップから復元してください**」+ 田中ペルソナ向け GUI モーダル経路（CLI を読めないユーザのため `shikomi-gui` 常駐表示要素 + モーダルで同等文言、MSG-S16 同型レイアウト）。**内部詳細**（GMAC 値 / AAD 内容 / nonce / どの record か）は含めない（攻撃者へのオラクル経路排除） | AEAD 認証タグ不一致（`unlock` / `decrypt` / `unwrap_vek` / ヘッダ AEAD タグ検証の全経路） |
| MSG-S11 | エラー | **Sub-D 部分確定**（Sub-C Rev1 で文言設計指針凍結）: 「**nonce 上限に到達しました**」+ ユーザ誘導「**`shikomi vault rekey` を実行**して新しい鍵で再暗号化してください」+ GUI 経路では「**鍵を再生成する**」ボタンで Sub-F の rekey フローを起動（田中ペルソナ）+ rekey の所要時間目安提示可（「全レコード件数に応じた時間がかかります」）。**残操作猶予の数値非表示**（「あと N 回」のような `NonceCounter::current()` 由来の数値を**ユーザに見せない**、攻撃面情報漏洩回避）。Sub-F が CLI 経路の最終文言確定 + 終了コード割当 | nonce 上限到達（`encrypt_record` 前の `NonceCounter::increment` で fail fast、Sub-D `MigrationError::Crypto(NonceLimitExceeded)`） |
| MSG-S12 | エラー | **Sub-D 確定**: 「**リカバリニーモニックが認識できません**」+ 原因カテゴリ別ヒント（過信防止）: (a)「**24 語の単語数を確認**してください（現在: N 語）」、(b)「**BIP-39 wordlist に存在しない単語**が含まれます」（具体的にどの語かは表示しない、攻撃者向けオラクル排除）、(c)「**チェックサム不一致**: 単語の打ち間違い / 順序入れ替えの可能性」+ 「`shikomi vault unlock --recovery` を再実行してください」誘導 | `vault unlock --recovery` でリカバリニーモニック検証失敗（`bip39::Mnemonic::parse_in` の `Err` → `CryptoError::InvalidMnemonic`） |
| MSG-S13 | エラー | **Sub-D 確定**: 「**マイグレーションに失敗しました**」+ 原状復帰明示「**vault.db は変更前の状態に戻っています**（`.new` ファイルは自動削除されました）」+ 段階情報（過信防止）「失敗段階: WriteTemp / FsyncTemp / FsyncDir / Rename のいずれか」（具体段階はログにのみ記録、ユーザには概要のみ）+ 次の一手「ディスク容量・パーミッション・別プロセスの干渉を確認後、再試行してください」 | 平文⇄暗号化マイグレーション失敗（`MigrationError::AtomicWriteFailed` 発火、`vault-persistence` の `.new` cleanup 経路で原状復帰済み） |
| MSG-S14 | 確認 | **Sub-D 部分確定**（Sub-F が CLI 経路実装）: 「**警告: 暗号保護を解除しようとしています**」+ 3 点リスク明示 — (1)「**vault.db が平文モードに戻り、物理ディスク奪取で全レコードが平文化されます**」、(2)「BIP-39 24 語と Argon2id KDF による保護が**全件失われます**」、(3)「この操作は**取り消せません**（再暗号化には再度 `vault encrypt` 実行 + 新規 24 語生成が必要）」+ 二段確認「**`DECRYPT` と入力してください**」+ 「**マスターパスワードを再入力してください**」（**両方一致しないと進まない、`--force` でも省略不可**、`DecryptConfirmation::confirm` の型レベル強制 C-20）| `vault decrypt` 実行前 |
| MSG-S15 | エラー | TBD by Sub-E | IPC V2 非対応クライアント（V1 クライアントへの guidance） |
| MSG-S16 | 警告 | **Sub-D 確定**（Sub-F が CLI 経路実装）: **暗号化モード初回切替時の限界説明**（受入基準#9「過信なく / 過小評価なく伝わる」担保）。`vault encrypt` 確認モーダル / プロンプトで以下 3 点を必ず提示 — (1)「**侵害された端末（root マルウェア / 同特権デバッガ / kernel keylogger）からは保護できません**」、(2)「**BIP-39 24 語が漏洩した場合は完全敗北です。手書きメモを写真撮影 / クラウド同期しないでください**」、(3)「**画面共有・リモートデスクトップ中は秘密情報の表示を避けてください**」。CLI / GUI 両経路で同等表示、ユーザの明示的合意（`--accept-limits` フラグ / モーダル「理解しました」ボタン）なしに次工程へ進ませない | `vault encrypt` 初回実行直前 / GUI 「暗号化モードに切替」ボタン押下直後 |
| MSG-S17 | 警告 | TBD by Sub-F + 後続 GUI feature（**GUI 暗号化モード可視化（田中ペルソナ対応）**: ペルソナ A 田中は CLI を読めない。Tauri WebView の常駐表示要素（タイトルバー / トレイアイコンツールチップ / レコード一覧画面ヘッダ）に **`[encrypted]` / `[plaintext]` バッジを常時表示**する。色（緑/灰）と文字（暗号化中/平文）の二重符号化（色覚多様性対応）。CLI 側 `shikomi list` の `[plaintext]` / `[encrypted]` バナー（REQ-S16）と文言を統一し、ユーザがどちらの UI を使ってもモードが視覚的に同期する。Sub-F の CLI 実装と将来の GUI feature 設計で同 MSG ID を共有 | GUI 起動時 / モード切替直後 / レコード一覧画面常時 |
| MSG-S18 | 警告 | **Sub-D 確定**（Sub-F が CLI 出力実装）: **アクセシビリティ代替経路**（スクリーンリーダー利用ユーザ / 視覚障害ユーザがリカバリニーモニック 24 語を取り扱う際の代替手段案内）。OS 読み上げ拒否環境では以下の 3 経路から選択可能: (1)「**印刷可能なハイコントラスト PDF**（黒地白文字、最大 36pt、各語に番号付与）」: `vault recovery-show --print` で出力、(2)「**点字対応 `.brf`（Braille Ready Format）**」: `vault recovery-show --braille` で出力、(3)「**録音禁止の音声プレイヤー優先順位ガイド**」: `vault recovery-show --audio` で OS 標準 TTS（macOS VoiceOver / Windows ナレーター / Linux Orca）を直接呼び録音可能アプリ経由の漏洩を回避。**WCAG 2.1 AA 準拠**を非機能要件として固定（REQ-S13 末尾）。Sub-D / Sub-F のアクセシビリティ要件と整合 | `vault recovery-show` 実行時にアクセシビリティモードが OS / shikomi 設定で検出された場合 |

## 依存関係

| 依存先 | 種別 | バージョン / 参照 | 用途 |
|-------|------|----------------|------|
| `aes-gcm` | crate | minor ピン（`tech-stack.md` §4.7） | REQ-S05 AEAD 実装（Sub-C） |
| `argon2` | crate | minor ピン（同上） | REQ-S03 KDF 実装（Sub-B） |
| `hkdf` | crate | minor ピン（同上） | REQ-S04 KDF（HKDF 経路、Sub-B） |
| `pbkdf2` | crate | minor ピン（同上） | REQ-S04 KDF（PBKDF2 経路、Sub-B） |
| `bip39` | crate | major ピン v2 系（同上） | REQ-S04 / REQ-S13 ニーモニック（Sub-B） |
| `rand_core` | crate | minor ピン（同上） | CSPRNG（Sub-A〜F 全般、`shikomi-infra::crypto::Rng` 単一エントリ点経由） |
| `getrandom` | crate | minor ピン（同上） | CSPRNG OS syscall ゲートウェイ（Sub-A〜F 全般） |
| `subtle` | crate | major ピン v2.5+（同上） | constant-time 比較（Sub-A〜D 必要箇所） |
| `zxcvbn` | crate | major ピン v3 系（同上） | REQ-S08 パスワード強度ゲート（**Sub-B で `ZxcvbnGate` 具象実装、Sub-D で `vault encrypt` 入口統合 + MSG-S08 文言**） |
| `secrecy` | crate | minor ピン（同上） | REQ-S02 / REQ-S09 秘密値ラッパ（Sub-A / Sub-E） |
| `zeroize` | crate | minor ピン（同上） | REQ-S02 / REQ-S09 / REQ-S13 メモリ消去（Sub-A〜F 全般） |
| `shikomi-core::Vault` 集約 | 既存 | Issue #7 完了済 | 暗号化モード経路で同一集約を再利用（Sub-A 拡張、Sub-D 利用） |
| `shikomi-infra::SqliteVaultRepository` | 既存 | Issue #10 完了済 | `EncryptedSqliteVaultRepository` 実装の参照元（Sub-D） |
| `shikomi-daemon` IPC 基盤 | 既存 | Issue #26 / #30 / #33 完了済 | IPC V1 → V2 非破壊拡張（Sub-E） |
| `shikomi-cli vault コマンド` | 既存 | `cli-vault-commands` feature | サブコマンド追加点（Sub-F） |
| `tech-stack.md` §2.4 / §4.7 / §4.3.2 | アーキ | PR #45 マージ済 | 暗号スイート凍結値・crate version pin・サプライチェーン契約 |
| `threat-model.md` §7 / §8 / §7.0 / §7.1 / §7.2 | アーキ | 既存 | 既存 STRIDE / OWASP 対応表との整合参照 |
