# 基本設計書

<!-- 詳細設計書（detailed-design/ ディレクトリ）とは別ファイル。統合禁止 -->
<!-- 詳細設計は Sub-A Rev1 で 4 分冊化: detailed-design/{index,crypto-types,password,nonce-and-aead,errors-and-contracts}.md -->
<!-- feature: vault-encryption / Epic #37 -->
<!-- 配置先: docs/features/vault-encryption/basic-design.md -->
<!-- 本書は Sub-A (#39) 着手時に新規作成。Sub-A スコープ（shikomi-core 暗号ドメイン型 + ゼロ化契約）の基本設計を確定する。
     Sub-B〜F の本文は各 Sub の設計工程で本ファイルを READ → EDIT で追記する。 -->
## 処理フロー

### REQ-S02 / REQ-S17: 暗号ドメイン型の構築・破棄ライフサイクル（Sub-A 主機能）

本 Sub-A 自体は I/O を持たない型ライブラリのため、ユースケース処理フローは**型の構築〜破棄経路**として記述する。実暗号操作のフロー（`unwrap_with_password` 等）は Sub-B〜D 設計時に追記。

#### F-A1: マスターパスワード構築フロー（呼び出し側 = Sub-D / Sub-E）

1. CLI / GUI からユーザ入力された生 `String` を受け取る（呼び出し側）
2. **`MasterPassword::new(s, &gate)` を呼ぶ** — `gate` は `PasswordStrengthGate` 実装（Sub-D の zxcvbn 実装）
3. `gate.validate(&s)` が `Ok(())` を返すなら `MasterPassword` を構築（内部は `SecretBytes`）
4. `gate.validate(&s)` が `Err(WeakPasswordFeedback { warning, suggestions })` を返すなら `MasterPassword` 構築失敗、呼び出し側はそのまま MSG-S08 に変換してユーザに提示（Fail Kindly）
5. `MasterPassword` の `Drop` 時に `zeroize` で内部秘密値を消去

#### F-A2: VEK 構築フロー（呼び出し側 = Sub-B / Sub-D）

1. **新規 vault 作成時**: `shikomi-infra::crypto::Rng::generate_vek()` が `[u8;32]` を CSPRNG から生成 → `Vek::from_array([u8;32])` で構築
2. **既存 vault 読込時**: `wrap` 状態の `WrappedVek` を `unwrap_with_password` 経路で復号 → 結果を `Vek::from_array([u8;32])` で構築
3. `Vek` は `Clone` 不可（一度構築したら同じ実体しか存在しない）、`Debug` は `[REDACTED VEK]` 固定
4. `Vek` の `Drop` 時に `zeroize` で内部 32B を消去

#### F-A3: KEK 構築フロー（呼び出し側 = Sub-B）

1. **`KekPw` の場合**: KDF（Argon2id）の出力 `[u8;32]` を `Kek::<KekKindPw>::from_array([u8;32])` で構築
2. **`KekRecovery` の場合**: HKDF-SHA256 の出力 `[u8;32]` を `Kek::<KekKindRecovery>::from_array([u8;32])` で構築
3. **phantom-typed**: `KekPw` と `KekRecovery` は **型レベルで区別**され、取り違えがコンパイルエラーになる
4. KEK の `Drop` 時に `zeroize` で内部 32B を消去
5. wrap/unwrap 完了後即座に `Drop` させる（呼び出し側責務、Sub-B 詳細設計で明記）

#### F-A4: AEAD 復号成功時の `Verified<Plaintext>` 構築フロー（呼び出し側 = Sub-C）

1. AEAD 復号関数が ciphertext + nonce + AAD + tag を受け取る
2. AES-256-GCM 復号 + GMAC タグ検証
3. **タグ検証成功時のみ** `Verified::<Plaintext>::new_from_aead_decrypt(...)` で構築（コンストラクタは `pub(crate)` 限定 = `shikomi-infra` の AEAD 実装からのみ呼出可能）
4. **タグ検証失敗時** `CryptoError::AeadTagMismatch` を返し、`Plaintext` 自体を構築しない（型レベルで「未検証 ciphertext を平文として扱う事故」を禁止）
5. 呼び出し側は `Verified<Plaintext>::into_inner()` で `Plaintext` を取り出せる（取り出し後の使用は呼び出し側責任、`Plaintext` 自身も `SecretBytes` ベースで `Drop` 時 zeroize）

#### F-A5: NonceCounter 暗号化回数監視フロー（Boy Scout Rule で責務再定義）

1. **vault unlock 時**: vault ヘッダから `nonce_counter: u64` を読み込み `NonceCounter::resume(count)` で構築
2. **レコード暗号化のたびに**: `NonceCounter::increment()` を呼ぶ → 上限 $2^{32}$ 未満なら `Ok(())`、上限到達なら `Err(DomainError::NonceLimitExceeded)`
3. **per-record nonce 値そのものは別経路**: `shikomi-infra::crypto::Rng::generate_nonce_bytes() -> NonceBytes` で完全 random 12B を取得（`NonceBytes::from_random([u8;12])` 受取コンストラクタ）。`NonceCounter` は nonce 値生成に**関与しない**
4. **vault save 時**: `NonceCounter::current()` で現在のカウント値を取り出しヘッダに保存
5. **上限到達時**: 呼び出し側（Sub-D / Sub-F）は `vault rekey` フローを起動（Vault::rekey_with(VekProvider) 既存メソッド経路、Issue #7 完了済）

### REQ-S03 / REQ-S04 / REQ-S08（実装）: KDF + Rng + ZxcvbnGate（Sub-B 主機能）

#### F-B1: パスワード経路 KEK_pw 導出フロー（呼び出し側 = Sub-D の `vault unlock` / `vault encrypt`）

1. CLI / GUI からユーザ入力された生 `String` を受け取る（Sub-D / Sub-F 責務）
2. `let gate = ZxcvbnGate::default();`（`min_score = 3` 凍結値、shikomi-infra）
3. `MasterPassword::new(s, &gate)?` で構築（Sub-A `crypto::password`、内部で `gate.validate(&s)` を呼び `zxcvbn::zxcvbn(s, &[]).score() >= 3` を判定、未達なら `Err(CryptoError::WeakPassword(Box::new(WeakPasswordFeedback)))`）
4. **vault encrypt 初回**: `let salt = rng.generate_kdf_salt();`（shikomi-infra `Rng`、`OsRng::fill_bytes` で 16B を `KdfSalt` にラップ）。**vault unlock**: vault ヘッダから既存 `KdfSalt::try_new(stored_bytes)?` で復元
5. `let kek_pw = Argon2idAdapter::default().derive_kek_pw(&master_password, &salt)?;`（shikomi-infra `kdf::argon2id`、`Argon2id m=19456 t=2 p=1 → [u8;32]` を `Kek<KekKindPw>` にラップ）
6. `kek_pw` で AEAD wrap/unwrap（Sub-C で結合）。`master_password` / `kek_pw` / 中間バッファ全て scope 抜けで `Drop` 連鎖 zeroize

#### F-B2: リカバリ経路 KEK_recovery 導出フロー（呼び出し側 = Sub-D の `vault unlock --recovery`）

1. CLI / GUI からユーザ入力 24 語 `[String; 24]` を受け取る（Sub-D / Sub-F 責務）
2. `RecoveryMnemonic::from_words(words)?` で軽量検証（Sub-A、長さ + ASCII 性のみ）
3. `let kek_recovery = Bip39Pbkdf2Hkdf.derive_kek_recovery(&mnemonic)?;`（shikomi-infra `kdf::bip39_pbkdf2_hkdf`）
   - 内部: `bip39::Mnemonic::parse_in(English, joined)` で wordlist + checksum 検証 → 失敗時 `Err(CryptoError::InvalidMnemonic)`
   - 内部: `mnemonic.to_seed("")` で 64B seed 生成（PBKDF2-HMAC-SHA512 2048iter）
   - 内部: `Hkdf::<Sha256>::new(None, &seed).expand(b"shikomi-kek-v1", &mut [u8;32])` で KEK_recovery 導出
4. `kek_recovery` で AEAD unwrap（Sub-C で結合）。中間 seed 64B + KEK 32B 全て `Zeroizing` で囲み `Drop` 時 zeroize

#### F-B3: VEK / Mnemonic Entropy 生成フロー（呼び出し側 = Sub-D の `vault encrypt` 初回）

1. `let rng = Rng::default();`（無状態 struct、構築コストゼロ）
2. `let vek = rng.generate_vek();`（`OsRng` で 32B 生成 → `Vek::from_array` でラップ）
3. `let entropy = rng.generate_mnemonic_entropy();`（`OsRng` で 32B `Zeroizing<[u8;32]>` を生成）
4. `let mnemonic = bip39::Mnemonic::from_entropy(&entropy[..])?;` → `RecoveryMnemonic::from_words(mnemonic.words())?` で型化（Sub-D で結合、初回 1 度きり表示）
5. 後続: VEK で `wrap_with_kek_pw` / `wrap_with_kek_recovery` の wrap 経路（Sub-C 結合）

#### F-B4: ZxcvbnGate `warning=None` 経路（Sub-D へのフォールバック責務移譲）

1. ユーザ入力パスワードが zxcvbn 強度 < 3 だが、`zxcvbn::Feedback::warning()` が `None` を返した
2. `ZxcvbnGate::validate` が `Err(WeakPasswordFeedback { warning: None, suggestions: vec![...] })` を返す（Sub-B、英語 raw のまま）
3. **Sub-D の MSG-S08 文言層**で `warning.is_none()` を検出 → フォールバック警告文（既定文言 / `suggestions[0]` / 強度スコア値のいずれか）を提示（`detailed-design/password.md` §`warning=None` 時の代替警告文契約）
4. `WeakPasswordFeedback` を IPC 経由で daemon → CLI / GUI に渡し、ユーザ提示まで Fail Kindly 維持

### REQ-S05 / REQ-S14（実装結合）: AEAD 経路（Sub-C 主機能）

#### F-C1: per-record 暗号化フロー（呼び出し側 = Sub-D の `vault encrypt` レコード追加 / 更新）

1. Sub-D の vault リポジトリ層で **`header.increment_nonce_counter()?`** を実行（`VaultEncryptedHeader::increment_nonce_counter(&mut self) -> Result<(), CryptoError>`、内部で `self.nonce_counter.increment()` を呼出し上限 $2^{32}$ チェック、`Err(NonceLimitExceeded)` なら fail fast → MSG-S11。**`Rng` への依存を持たない**、nonce 生成は次手順の呼出側責務）
2. `let nonce = rng.generate_nonce_bytes();`（Sub-B `Rng`、12B random、衝突確率 ≤ $2^{-32}$ で運用範囲内）
3. `let aad = Aad::new(record_id, vault_version, record_created_at)?;`（既存 `shikomi_core::vault::crypto_data::Aad`、26B 正規化）
4. `let aead = AesGcmAeadAdapter::default();`（Sub-C、無状態 unit struct）
5. `let (ciphertext, tag) = aead.encrypt_record(&vek, &nonce, &aad, plaintext)?;`（Sub-C、`vek.with_secret_bytes(|bytes| Aes256Gcm::new(bytes).encrypt_in_place_detached(...))` 経由）
6. `EncryptedRecord { ciphertext, nonce, aad, tag }` を構築（Sub-D の永続化型、本ファイル §データモデルで Sub-D が確定）
7. **AEAD 中間バッファ**: 手順 5 内部の `Zeroizing<Vec<u8>>` は scope 抜けで Drop 連鎖 zeroize（C-16 / L2 対策）
8. **Vek の Drop**: vault unlock セッションが続く限り `Vek` は daemon RAM に滞留（Sub-E `tokio::sync::RwLock<Option<Vek>>`、最大アイドル 15min）

#### F-C2: per-record 復号フロー（呼び出し側 = Sub-D の `vault unlock` 後のレコード読出）

1. SQLite から `EncryptedRecord { ciphertext, nonce, aad_components, tag }` を読み出し（Sub-D `vault-persistence`）
2. `let aad = Aad::new(record_id, vault_version, record_created_at)?;`（永続化された components から再構築）
3. `let aead = AesGcmAeadAdapter::default();`
4. `let verified = aead.decrypt_record(&vek, &nonce, &aad, &ciphertext, &tag)?;`（Sub-C）
   - 内部: `vek.with_secret_bytes(|bytes| Aes256Gcm::new(bytes).decrypt_in_place_detached(...))` でタグ検証、成功時のみ `verify_aead_decrypt(|| Ok(Plaintext::new_within_module(buf)))` で `Verified<Plaintext>` 構築
   - **タグ検証失敗時**: `Err(CryptoError::AeadTagMismatch)` → MSG-S10「vault.db 改竄の可能性」、`Plaintext` は構築されない（C-14）
5. `let plaintext = verified.into_inner();`（Sub-A `Verified::into_inner`）
6. plaintext を呼出側に渡す（Sub-D / Sub-E でクリップボード投入 30 秒タイマー、L2 対策）

#### F-C3: VEK wrap 経路（呼び出し側 = Sub-D の `vault encrypt` 初回 / `change-password` / `rekey`）

1. `let vek = rng.generate_vek();`（初回のみ、Sub-B `Rng`）または既存 `vek`（change-password 時）
2. `let kek_pw = argon2.derive_kek_pw(&master_password, &salt)?;`（Sub-B `Argon2idAdapter`）
3. `let nonce = rng.generate_nonce_bytes();`（Sub-B `Rng`）
4. `let aead = AesGcmAeadAdapter::default();`
5. `let wrapped = aead.wrap_vek(&kek_pw, &nonce, &vek)?;`（Sub-C、AAD は空 `&[]`、ciphertext 32B + tag 16B = `WrappedVek { ciphertext, nonce, tag }`）
6. **KEK_pw の Drop**: 手順 5 完了で scope 抜け、`SecretBox<Zeroizing<[u8;32]>>` が Drop 連鎖 zeroize（滞留 < 1 秒、L2 対策）
7. recovery 経路は同形（`Bip39Pbkdf2Hkdf::derive_kek_recovery` で `Kek<KekKindRecovery>` を導出 → `wrap_vek` の `key` 引数に渡す、phantom-typed の C-6 契約は `WrappedVek` 受け側の関数シグネチャで担保）

#### F-C4: VEK unwrap 経路（呼び出し側 = Sub-D の `vault unlock`）

1. SQLite から `wrapped_VEK_by_pw: WrappedVek` を読み出し（Sub-D）
2. `let kek_pw = argon2.derive_kek_pw(&master_password, &salt)?;`（Sub-B）
3. `let aead = AesGcmAeadAdapter::default();`
4. `let verified = aead.unwrap_vek(&kek_pw, &wrapped_vek_by_pw)?;`（Sub-C、AAD は空、戻り値は `Verified<Plaintext>`）
5. `let bytes: [u8;32] = verified.into_inner().expose_secret().try_into().map_err(|_| CryptoError::AeadTagMismatch)?;`（Sub-D 側で 32B 長さ検証 Fail Fast）
6. `let vek = Vek::from_array(bytes);`（Sub-A）
7. **Drop 連鎖**: `Plaintext` / `bytes` / `kek_pw` は scope 抜けで全 zeroize（L2 対策）
8. `vek` を Sub-E の VEK キャッシュに格納（`tokio::sync::RwLock<Option<Vek>>`、unlock〜lock 間滞留）

### REQ-S06 / REQ-S07 / REQ-S13: 暗号化 Vault リポジトリ + マイグレーション（Sub-D 主機能）

詳細は `detailed-design/repository-and-migration.md` 参照。本書では概要フローのみ。

#### F-D1: `vault encrypt`（平文 → 暗号化、片方向昇格）

1. `MasterPassword::new(plaintext_password, &gate)?` 強度ゲート（Sub-A/B、強度 ≥ 3、失敗時 MSG-S08）
2. `KdfSalt` / VEK / mnemonic entropy / nonce を CSPRNG（Sub-B `Rng`）で生成
3. KEK_pw（`Argon2idAdapter::derive_kek_pw`）/ KEK_recovery（`Bip39Pbkdf2Hkdf::derive_kek_recovery`）導出（Sub-B）
4. `wrapped_VEK_by_pw` / `wrapped_VEK_by_recovery` を `AesGcmAeadAdapter::wrap_vek` で構築（Sub-C）
5. 既存平文 vault 読込 → 各 record を `encrypt_record` で AEAD 暗号化（Sub-C、AAD = `Aad::Record { record_id, vault_version, created_at }`）
6. ヘッダ AEAD タグ envelope 構築（`HeaderAeadKey::from_kek_pw` + `Aad::HeaderEnvelope(canonical_bytes)`、ヘッダ全フィールド改竄を 1 variant 検出、契約 C-17/C-18）
7. `SqliteVaultRepository::save(&encrypted_vault)?`（vault-persistence の atomic write、`.new` → fsync → rename）
8. `RecoveryDisclosure` 返却（呼出側 = Sub-E daemon / Sub-F CLI が**1 度だけ** `disclose` してユーザに表示、再表示禁止を型レベル強制 C-19）
9. KEK / VEK / MasterPassword / mnemonic は scope 抜けで Drop 連鎖 zeroize（L2 対策）

#### F-D2: `vault unlock`（暗号化 vault の復号・メモリロード）

1. `MasterPassword::new` 強度ゲート（再入力、再構築失敗で MSG-S08 経路）
2. `SqliteVaultRepository::load(&self)?` で `EncryptedVault` 読込
3. **ヘッダ AEAD タグ検証**: `HeaderAeadKey::from_kek_pw(&kek_pw)` で AEAD 鍵派生 → `decrypt_record` で AAD = `Aad::HeaderEnvelope(canonical_bytes)` のタグ検証、失敗時 MSG-S10
4. `wrapped_VEK_by_pw` を `unwrap_vek` で復号 → 32B 長さ検証 → `Vek::from_array` 復元（Sub-C `unwrap_vek_with_password` 同型）
5. `(Vault, Vek)` を Sub-E daemon に返却 → daemon は `Vek` を `tokio::sync::RwLock<Option<Vek>>` でキャッシュ（Sub-E 詳細）

#### F-D3: `vault decrypt`（暗号化 → 平文、片方向降格、リスク方向）

1. CLI / GUI で MSG-S14 確認モーダル（暗号保護除去のリスク 3 点明示）
2. ユーザに `"DECRYPT"` キーワード入力 + パスワード再入力を要求
3. **Sub-F CLI / GUI 内で**: `subtle::ConstantTimeEq` で `"DECRYPT"` キーワード + パスワード再入力の両方一致を判定 → 通過後 `let confirmation = DecryptConfirmation::confirm();` で型レベル証跡構築（C-20、`--force` でも省略不可、Sub-D Rev2 で確認ロジック自体を Sub-F 責務に移譲、shikomi-infra は通過証跡型のみ提供 / Clean Arch 維持）
4. `unlock_with_password` で復号 + VEK 取得
5. 全 `EncryptedRecord` を `decrypt_record` で復号（タグ失敗時 MSG-S10）→ `PlaintextRecord` 構築
6. `SqliteVaultRepository::save(&plaintext_vault)?`（atomic write、`protection_mode='plaintext'` に切替）
7. **`save` 失敗時**: `.new` cleanup で原状（暗号化 vault）復帰、MSG-S13

#### F-D4: `rekey`（VEK 入替、nonce overflow / 明示 rekey）

1. **トリガ**: `NonceCounter::increment` が `Err(NonceLimitExceeded)` → MSG-S11 で `vault rekey` 案内 → ユーザ実行（自動）、または `shikomi vault rekey` 明示実行（手動）
2. `unlock_with_password` で旧 VEK 取得
3. 新 VEK / 新 nonce 生成 → 全 record を旧 VEK で復号 → 新 VEK で再暗号化
4. `wrapped_VEK_by_pw` / `wrapped_VEK_by_recovery` を新 VEK で wrap し直し
5. `nonce_counter` を `NonceCounter::new()` でリセット
6. ヘッダ AEAD envelope を新 wrapped + 新 nonce_counter で再構築
7. `SqliteVaultRepository::save` で atomic write、MSG-S07 で再暗号化レコード数表示

#### F-D5: `change-password`（マスターパスワード変更、O(1)、VEK 不変）

1. `unlock_with_password(current)` で旧パスワードで復号、VEK 保持
2. `MasterPassword::new(new, &gate)?` 新パスワード強度ゲート
3. **新 `KdfSalt` 生成**（旧 salt の流用禁止、salt-password ペア更新で旧 brute force 進捗を無効化）
4. 新 KEK_pw を Argon2id 導出 → `wrapped_VEK_by_pw` のみ新 KEK_pw で wrap し直し
5. **`wrapped_VEK_by_recovery` / `nonce_counter` は変更しない**（VEK 不変、リカバリ経路維持、record AEAD nonce 衝突確率変化なし）
6. ヘッダ AEAD envelope を**新 kdf_salt / 新 wrapped_VEK_by_pw**で再構築
7. `SqliteVaultRepository::save` で atomic write、MSG-S05 で完了通知

### REQ-S09 / REQ-S10 / REQ-S11 / REQ-S12: VEK キャッシュ + IPC V2 拡張（Sub-E 主機能）

詳細は `detailed-design/vek-cache-and-ipc.md` 参照。本書では概要フローのみ。

#### F-E1: `vault unlock`（IPC `Unlock` 受信）

1. クライアントから `IpcRequest::Unlock { master_password, recovery: None }` 受信
2. `backoff.check()?` でバックオフ中なら `Err(IpcError::BackoffActive { wait_secs })` で即拒否（MSG-S09 (a) + 待機時間）
3. `cache.state()` を確認、`Unlocked` なら `Err(IpcErrorCode::Internal { reason: "already-unlocked" })` で拒否
4. Sub-D `vault_migration.unlock_with_password(&master_password)?` を呼出
   - **失敗時** `MigrationError::RecoveryRequired` → `IpcError::RecoveryRequired` 透過 → MSG-S09 (a) リカバリ経路案内（Sub-D Rev5 ペガサス指摘契約の実装、契約 C-27）
   - **失敗時** `MigrationError::Crypto(_)` → `backoff.record_failure()` → 5 回連続なら指数バックオフ発動（契約 C-26）
5. 戻り値 `(Vault, Vek)` の `Vek` を `cache.unlock(vek).await?` で `VaultUnlockState::Unlocked` に遷移
6. `backoff.record_success()` で失敗カウンタリセット
7. `IpcResponse::Unlocked {}` 応答 + MSG-S03 表示

#### F-E2: `vault lock`（IPC `Lock` / アイドル / OS シグナル）

1. **明示 `Lock` IPC**: `cache.lock().await` 呼出 → 旧 `Vek` Drop 連鎖 zeroize → `IpcResponse::Locked {}` 応答 + MSG-S04
2. **アイドル 15min**: `IdleTimer` バックグラウンド task が 60 秒ポーリングで `now - last_used >= 15min` 検出 → `cache.lock()` 呼出（IPC 応答なし、契約 C-24）
3. **OS スクリーンロック / サスペンド**: `OsLockSignal::next_lock_event().await` で `LockEvent::ScreenLocked` / `SystemSuspended` 受信 → `cache.lock()`（契約 C-25、100ms 以内）

#### F-E3: `change_password`（REQ-S10 O(1)、IPC `ChangePassword` 受信）

1. `cache.state()` を確認、`Locked` なら `Err(IpcError::VaultLocked)` で拒否（契約 C-22）
2. Sub-D `vault_migration.change_password(&old, &new)?`（§F-D5）
   - VEK 不変、`wrapped_VEK_by_pw` のみ新 KEK で再 wrap、新 `kdf_salt` 生成、`wrapped_VEK_by_recovery` / `nonce_counter` は変更なし
3. **キャッシュ無効化は不要**（VEK 不変、再 unlock 不要）
4. `IpcResponse::PasswordChanged {}` 応答 + MSG-S05

#### F-E4: `rotate_recovery`（IPC `RotateRecovery` 受信）

1. `cache.state()` を確認、`Locked` なら `Err(IpcError::VaultLocked)` で拒否
2. パスワード再認証（戻り値 `Vek` は破棄、cache 既に保持）
3. 新 mnemonic entropy 生成 → 新 `RecoveryMnemonic` → 新 `kek_recovery` 導出
4. 既存 VEK を新 kek_recovery で wrap → `wrapped_vek_by_recovery` のみ更新（`wrapped_vek_by_pw` / `nonce_counter` / `kdf_params` は維持）
5. ヘッダ AEAD envelope 再構築（C-17/C-18 通り）→ atomic write
6. **再キャッシュ試行**: `cache.lock().await` で旧 VEK 破棄 → `unlock_with_password()` で新パスワードを使い再キャッシュを試行
7. `IpcResponse::RecoveryRotated { words: RecoveryWords, cache_relocked: bool }` で**新 24 語を初回 1 度のみ返却**（C-19 所有権消費を IPC 経路で表現、daemon ログには記録しない）。**`cache_relocked` フィールド**: step 6 の再キャッシュが成功したかを示す（C-30/C-31/C-32、`basic-design/ux-and-msg.md` §cache_relocked: false の UX 設計判断 / `detailed-design/vek-cache-and-ipc.md` §不変条件参照）。`false` 時は MSG-S20 を連結表示するため Sub-F が応答を解釈、`true` 時は通常通り次操作可能

#### F-E5: `rekey`（IPC `Rekey` 受信、nonce overflow / 明示 rekey）

1. `cache.state()` を確認、`Locked` なら `Err(IpcError::VaultLocked)` で拒否
2. Sub-D `vault_migration.rekey_with_recovery_rotation(&master_password)?`（`detailed-design/vek-cache-and-ipc.md` §F-E5 atomic 化採用案）— 旧 VEK で全レコード復号 → 新 VEK で全件再暗号化、`wrapped_vek_by_pw` 再 wrap、新 mnemonic 生成 + `wrapped_vek_by_recovery` 再 wrap、`nonce_counter` リセット、atomic write 1 回
3. **キャッシュ更新**: `cache.lock().await` で旧 VEK を破棄 → `unlock_with_password()` で新パスワードを使い再キャッシュを試行
4. `IpcResponse::Rekeyed { records_count, words: RecoveryWords, cache_relocked: bool }` 応答 + MSG-S07。**`cache_relocked` フィールド**: step 3 の再キャッシュが成功したかを示す（C-30/C-31/C-32、`basic-design/ux-and-msg.md` §cache_relocked: false の UX 設計判断参照）。`false` 時は MSG-S20 を連結表示し Sub-F が再 unlock 経路を能動的に提示する責務

### REQ-S15 / REQ-S16: vault 管理サブコマンド + 保護モード可視化（Sub-F 主機能）

詳細は `detailed-design/cli-subcommands.md` 参照。本書では概要フローのみ。

#### F-F1: `vault encrypt`（CLI → daemon）

1. clap で `Subcommand::Vault(VaultSubcommand::Encrypt(EncryptArgs { accept_limits }))` を受領
2. `--accept-limits` フラグなしなら MSG-S16「暗号化モード初回切替時の限界説明」を stderr に表示 + 「`理解しました [y/N]` を入力してください」プロンプト → ユーザが `y` 回答しない場合は終了コード 1 で fail fast
3. `input::password::prompt` でマスターパスワード入力（TTY 非エコー読取）+ 確認入力一致を `subtle::ConstantTimeEq` で判定
4. `IpcClient::connect` → handshake V2 → `IpcRequest::Encrypt { master_password, accept_limits }` 送信（Sub-E 経由 Sub-D `encrypt_vault`）
5. **失敗時** `IpcResponse::Error(IpcErrorCode::Crypto { reason: "weak-password" })` → MSG-S08 + 終了コード 1
6. **成功時** `IpcResponse::Encrypted { disclosure: RecoveryWords }` 受領 → MSG-S06 警告連結 → `presenter::recovery_disclosure::display(disclosure)` で 24 語表示 + Drop zeroize 連鎖（C-19）→ MSG-S01 完了通知 → 終了コード 0

#### F-F2: `vault decrypt`（CLI 二段確認 → daemon）

1. clap で `Subcommand::Vault(VaultSubcommand::Decrypt)` を受領
2. **MSG-S14 二段確認**: `input::decrypt_confirmation::prompt` で `DECRYPT` 文字列入力 + マスターパスワード再入力 → `subtle::ConstantTimeEq` で両方一致を判定 + paste 抑制（30ms 以内連続入力拒否、C-34）+ 大文字検証 → 通過時に `DecryptConfirmation::confirm()` 呼出（C-20、`--force` でも省略不可）
3. `IpcClient::connect` → handshake V2 → `IpcRequest::Decrypt { master_password, confirmation }` 送信
4. **成功時** `IpcResponse::Decrypted` → MSG-S02 完了 → 終了コード 0
5. **失敗時**（パスワード違い / AEAD 改竄等）→ MSG-S09(a) または MSG-S10 → 終了コード 1 / 2

#### F-F3: `vault unlock`（CLI → daemon、password / recovery 二経路）

1. clap で `Subcommand::Vault(VaultSubcommand::Unlock(UnlockArgs { recovery }))` を受領
2. `recovery == false` なら `input::password::prompt` でマスターパスワード入力、`recovery == true` なら `input::mnemonic::prompt` で 24 語入力 + `bip39::Mnemonic::parse_in` で検証
3. `IpcClient::connect` → handshake V2 → `IpcRequest::Unlock { master_password, recovery: Option<RecoveryMnemonic> }` 送信
4. **成功時** `IpcResponse::Unlocked` → MSG-S03 → 終了コード 0
5. **失敗時** `IpcError::BackoffActive { wait_secs }` → MSG-S09(a) + 待機時間案内 → 終了コード 2 / `IpcError::RecoveryRequired` → MSG-S09(a) リカバリ経路案内 → 終了コード 5 / `MigrationError::Crypto(InvalidMnemonic)` → MSG-S12 → 終了コード 1

#### F-F4: `vault lock`（CLI → daemon）

1. clap で `Subcommand::Vault(VaultSubcommand::Lock)` を受領
2. `IpcClient::connect` → handshake V2 → `IpcRequest::Lock` 送信（フィールドなし）
3. `IpcResponse::Locked` 受領 → MSG-S04 → 終了コード 0

#### F-F5: `vault change-password`（CLI → daemon、O(1)）

1. clap で `Subcommand::Vault(VaultSubcommand::ChangePassword)` を受領
2. `input::password::prompt` で旧パスワード + 新パスワード + 新確認の 3 段入力
3. **新パスワードの強度ゲート前段確認**（Sub-A `MasterPassword::new` 経路、CLI 段で zxcvbn 確認しても良い、Sub-F PR で UX 確定）
4. `IpcClient::connect` → handshake V2 → `IpcRequest::ChangePassword { old, new }` 送信
5. **成功時** `IpcResponse::PasswordChanged` → MSG-S05「VEK 不変、再 unlock 不要」明示 → 終了コード 0
6. **失敗時** MSG-S08 弱パスワード / MSG-S09(a) 旧パスワード違い → 終了コード 1

#### F-F6: `vault recovery-show`（CLI 内 + アクセシビリティ分岐）

1. clap で `Subcommand::Vault(VaultSubcommand::RecoveryShow(RecoveryShowArgs { print, braille, audio }))` を受領
2. **本フローは IPC を呼ばない**: `encrypt` 直後（F-F1）に取得した `disclosure` を**プロセス内のみ**で消費する経路（C-19、daemon 側で `RecoveryDisclosure` 構築済フラグを持ち、2 度目以降は `IpcErrorCode::Internal { reason: "recovery-already-disclosed" }` で拒否、C-35）
3. **アクセシビリティモード判定**: `SHIKOMI_ACCESSIBILITY=1` env / OS スクリーンリーダー検出 / `--print` / `--braille` / `--audio` フラグのいずれか → `accessibility::print_pdf::output` / `braille_brf::output` / `audio_tts::output` 分岐
4. **通常経路**: `presenter::recovery_disclosure::display` で 24 語表示 + MSG-S06 警告連結 + MSG-S18 アクセシビリティ案内 → Drop zeroize 連鎖
5. **2 度目以降の呼出**は MSG-S09 系で fail fast、終了コード 1（C-19 / C-35）

#### F-F7: `vault rekey`（CLI → daemon、cache_relocked 分岐）

1. clap で `Subcommand::Vault(VaultSubcommand::Rekey)` を受領
2. `input::password::prompt` でマスターパスワード入力
3. `IpcClient::connect` → handshake V2 → `IpcRequest::Rekey { master_password }` 送信（Sub-E §F-E5 atomic 化）
4. `IpcResponse::Rekeyed { records_count, words, cache_relocked }` 受領
5. `presenter::recovery_disclosure::display(words)` で**新 24 語を先に表示**（rekey の主目的、ux-and-msg.md §文言の不変条件 (c)）+ MSG-S06 警告連結
6. MSG-S07 完了通知（再暗号化レコード数 = `records_count`）
7. **`cache_relocked == false` 時**: `presenter::cache_relocked_warning::display` で MSG-S20 連結 + 「次の操作前に `shikomi vault unlock`」案内 + （オプション）TTY 検出時に再 unlock プロンプト自動起動（C-32 能動的提示、Sub-F 工程5 UX レビュー後確定）→ **終了コード 0**（C-31 / C-36）
8. **`cache_relocked == true` 時**: 終了コード 0（通常経路）

#### F-F8: `vault rotate-recovery`（CLI → daemon、cache_relocked 分岐）

1. clap で `Subcommand::Vault(VaultSubcommand::RotateRecovery)` を受領
2. `input::password::prompt` でマスターパスワード再認証入力
3. `IpcClient::connect` → handshake V2 → `IpcRequest::RotateRecovery { master_password }` 送信（Sub-E §F-E4）
4. `IpcResponse::RecoveryRotated { words, cache_relocked }` 受領
5. `presenter::recovery_disclosure::display(words)` で**新 24 語を先に表示**（rotate-recovery の主目的）+ MSG-S06 警告連結
6. MSG-S19 完了通知
7. **`cache_relocked == false` 時**: F-F7 と同経路で MSG-S20 連結 + 再 unlock 案内 → 終了コード 0
8. **`cache_relocked == true` 時**: 終了コード 0

#### F-F9: 既存 `add` / `list` / `edit` / `remove` のロック時挙動（REQ-S16 整合）

1. `usecase::{add,list,edit,remove}::execute` 実行
2. `IpcClient::send_request(IpcRequest::ListRecords / AddRecord / ...)` 送信
3. **`IpcResponse::Error(IpcErrorCode::VaultLocked)` 受領時**: MSG-S09(c)「アイドル 15min / スクリーンロック / サスペンドで自動 lock しました、再度 `vault unlock` を実行してください」+ 終了コード 3 で fail fast、レコード内容は応答に含まれず情報漏洩なし
4. **`IpcResponse::Records { records, protection_mode }` 受領時** (`usecase::list`): `presenter::mode_banner::display(protection_mode)` でヘッダバナー（`[plaintext]` / `[encrypted, locked]` / `[encrypted, unlocked]` / `[unknown]`、ANSI カラー + 文字二重符号化、`NO_COLOR` env 尊重）→ レコード一覧出力 → 終了コード 0
5. **`protection_mode == Unknown` 時**: バナー `[unknown]` 表示 + 終了コード 3 で fail-secure（一覧表示停止、REQ-S16 整合）

## シーケンス図

Sub-A スコープは型ライブラリで I/O 不在のため、メイン処理シーケンスは Sub-B〜D で初めて成立する。本書では **Sub-A 型の使用パターン（呼び出し側との境界）** のみ示す。

```mermaid
sequenceDiagram
    participant CLI as shikomi-cli (Sub-F)
    participant Daemon as shikomi-daemon (Sub-E)
    participant Infra as shikomi-infra (Sub-B〜D)
    participant Core as shikomi-core::crypto (Sub-A)

    Note over CLI,Core: 暗号化モード unlock の代表シナリオ

    CLI->>Daemon: IPC Unlock { master_password: SecretBytes }
    Daemon->>Infra: unwrap_vek(master_password, kdf_salt, wrapped_vek_by_pw)
    Infra->>Core: MasterPassword::new(s, &gate)
    Core-->>Infra: Ok(MasterPassword) or Err(WeakPasswordFeedback)
    Infra->>Infra: Argon2id(master_password, kdf_salt) -> [u8;32]
    Infra->>Core: Kek::<KekKindPw>::from_array([u8;32])
    Core-->>Infra: Kek<KekKindPw>
    Infra->>Infra: AES-GCM unwrap(wrapped_vek_by_pw, kek_pw)
    alt タグ検証成功
        Infra->>Core: Verified::<Plaintext>::new_from_aead_decrypt(vek_bytes)
        Core-->>Infra: Verified<Plaintext>
        Infra->>Core: Vek::from_array(verified.into_inner().as_bytes())
        Core-->>Infra: Vek
        Infra-->>Daemon: Ok(Vek)
        Note over Core: KekPw / Verified / Plaintext は<br/>スコープ抜けで全て zeroize
    else タグ検証失敗
        Infra-->>Daemon: Err(CryptoError::AeadTagMismatch)
        Note over Core: KekPw / MasterPassword は<br/>スコープ抜けで zeroize
    end
    Daemon->>Daemon: VEK キャッシュへ保存（Sub-E 責務）
    Daemon-->>CLI: IPC Response（成功 or MSG-S09 カテゴリ別ヒント）
```

