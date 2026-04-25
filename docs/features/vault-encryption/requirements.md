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
| 処理 | 本 feature 配下の全設計書（basic-design.md / detailed-design.md / test-design.md）が脅威 ID L1〜L4 を参照可能な状態を維持する。各 Sub の PR レビューで「対策が脅威 ID と紐付いているか」を必須チェック項目とする |
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
| 概要 | パスワード → KEK_pw 導出（`m=19456, t=2, p=1`）、RFC 9106 KAT、CI criterion ベンチで p95 1 秒継続検証、4 年再評価 |
| 関連脅威 ID | L3（offline brute force に作業証明を強制）／ L1（弱パスワード時の wrapped_VEK_by_pw 復号耐性） |
| 入力 | TBD by Sub-B |
| 処理 | TBD by Sub-B |
| 出力 | TBD by Sub-B |
| エラー時 | TBD by Sub-B（Fail-Secure 必須: KDF 失敗で `KdfError` を返し、unwrap 経路を作らない） |

### REQ-S04: KDF（BIP-39 + PBKDF2 + HKDF）

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-B (#40) — `feat(shikomi-infra)` |
| 概要 | 24 語 → seed → KEK_recovery 導出（PBKDF2-HMAC-SHA512 2048iter + HKDF-SHA256 `info='shikomi-kek-v1'`）、trezor 公式ベクトル + RFC 5869 KAT |
| 関連脅威 ID | L3（リカバリ経路の brute force 耐性: 256 bit エントロピー）／ L4（24 語盗難時の完全敗北は受容、ユーザ責任で保管） |
| 入力 | TBD by Sub-B |
| 処理 | TBD by Sub-B |
| 出力 | TBD by Sub-B |
| エラー時 | TBD by Sub-B（Fail-Secure 必須: ニーモニック検証失敗 / チェックサム不一致は即拒否、再試行回数を制限しない＝サイドチャネル排除） |

### REQ-S05: AEAD（AES-256-GCM）

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-C (#41) — `feat(shikomi-infra)` |
| 概要 | per-record 暗号化、AAD = record_id ‖ version ‖ created_at（26B）、random nonce 12B、上限 $2^{32}$ で `NonceLimitExceeded`、NIST CAVP テストベクトル |
| 関連脅威 ID | L1（AEAD 認証タグで改竄検出、AAD でロールバック検出、random nonce で並行書込時の衝突確率制約）／ L3（VEK 不在時の平文化阻止） |
| 入力 | TBD by Sub-C |
| 処理 | TBD by Sub-C |
| 出力 | TBD by Sub-C（必ず `Verified<Plaintext>` newtype で返す） |
| エラー時 | TBD by Sub-C（Fail-Secure 必須: タグ不一致は `AeadTagMismatch` で fail fast、nonce 上限到達は `NonceLimitExceeded` で rekey 強制） |

### REQ-S06: 暗号化 Vault リポジトリ

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-D (#42) — `feat(shikomi-infra)` |
| 概要 | `EncryptedSqliteVaultRepository` 実装、`VaultRepository` trait の暗号化モード経路、平文⇄暗号化双方向マイグレーション、ヘッダ独立 AEAD タグ |
| 関連脅威 ID | L1（ヘッダ AEAD タグで `kdf_params` / `wrapped_VEK_*` 差替検出）／ L3（vault.db 全件の AEAD 保護） |
| 入力 | TBD by Sub-D |
| 処理 | TBD by Sub-D（マイグレーションは atomic write + 失敗時ロールバック必須、`vault-persistence` REQ-P04 / REQ-P05 を踏襲） |
| 出力 | TBD by Sub-D |
| エラー時 | TBD by Sub-D（Fail-Secure 必須: 部分マイグレーションを残さない、ヘッダ AEAD 検証失敗は即拒否） |

### REQ-S07: REQ-P11 解禁

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-D (#42) — `feat(shikomi-infra)` |
| 概要 | `vault-persistence/requirements.md` REQ-P11「暗号化モード即時拒否」を改訂、暗号化モード vault の load/save を解禁 |
| 関連脅威 ID | L1 / L3（暗号化モードを実運用可能にすることで両脅威への対策が初めて実効化する） |
| 入力 | TBD by Sub-D |
| 処理 | TBD by Sub-D（`PersistenceError::UnsupportedYet` の発火経路を削除、暗号化モード正常系を解禁） |
| 出力 | TBD by Sub-D |
| エラー時 | TBD by Sub-D（既存の `PersistenceError` 各バリアントは維持、新規エラーは Sub-D 設計で確定） |

### REQ-S08: パスワード強度ゲート

| 項目 | 内容 |
|------|------|
| 担当 Sub | **Sub-A (#39) で trait のみ確定** + Sub-D (#42) で実装 — `feat(shikomi-core)` + `feat(shikomi-infra)` |
| 概要 | shikomi-core 側で `PasswordStrengthGate` trait と `WeakPasswordFeedback` 型を定義（Sub-A 担当）、shikomi-infra 側で zxcvbn 強度 ≥ 3 の `ZxcvbnGate` 実装と `vault encrypt` 入口の Fail Fast 経路（Sub-D 担当）。強度不足時は `Feedback`（warning + suggestions）を CLI/GUI に提示（Fail Kindly） |
| 関連脅威 ID | L3（弱パスワード時の Argon2id offline 突破を入口で禁止）／ L1（同上） |
| 入力 | **Sub-A**: trait シグネチャ `validate(&self, password: &str) -> Result<(), WeakPasswordFeedback>` のみ。**Sub-D**: 構造体 `ZxcvbnGate` のコンフィグ（最小強度値、辞書データ） |
| 処理 | **Sub-A**: `MasterPassword::new(s, gate)` が `gate.validate(&s)` を呼び、`Ok(())` なら `MasterPassword` 構築、`Err(WeakPasswordFeedback)` なら `Err(CryptoError::WeakPassword(_))` で構築失敗。trait 自体は強度基準を持たない（実装が決定）。**Sub-D**: `ZxcvbnGate` が `zxcvbn::zxcvbn(&password, &[]).score() >= 3` を判定し、未達なら `feedback.warning` / `feedback.suggestions` を `WeakPasswordFeedback` に詰めて返す |
| 出力 | **Sub-A**: trait `Result<(), WeakPasswordFeedback>`。**Sub-D**: 同 trait の具象実装、CLI/GUI への MSG-S08 渡しまで |
| エラー時 | Fail-Secure + Fail Kindly: 拒否は早期（`MasterPassword` 構築自体が失敗、後続経路に弱鍵を渡さない）、`feedback.warning` / `feedback.suggestions` をユーザにそのまま提示。MSG-S08 文言は Sub-D で確定 |

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
| 入力 | TBD by Sub-D / Sub-E |
| 処理 | TBD by Sub-D / Sub-E（生成 → 表示完了確認 → zeroize の単一フロー、再表示 API を提供しない） |
| 出力 | TBD by Sub-D / Sub-E |
| エラー時 | TBD by Sub-D / Sub-E（Fail-Secure 必須: 表示完了前のクラッシュ時は新規 vault 作成を巻戻し、未確認の `wrapped_VEK_by_recovery` を残さない） |
| **アクセシビリティ** | **「初回 1 度きり表示」は視覚障害ユーザにとって完全敗北リスクに直結**（24 語を視認できない → 手書き不能 → 再表示不可 → マスターパスワード失念時に L4 相当の永久損失）。以下の代替経路を Sub-D / Sub-F が提供する: (1) **スクリーンリーダー対応**: 24 語表示要素に明示的な ARIA ロール / `aria-live="assertive"` / 連続読み上げ可能なテキスト構造を付与、(2) **OS 読み上げ拒否環境への代替**: `vault recovery-show --print` で**ハイコントラスト印刷可能 PDF**（黒地白文字、最大 36pt フォント、各語に番号付与）を出力、(3) **点字対応**: `vault recovery-show --braille` で `.brf`（Braille Ready Format）出力、(4) **音声プレイヤー優先順位ガイド**: `--audio` オプションで OS 標準 TTS を呼ぶ際、録音禁止プレイヤー（macOS VoiceOver / Windows ナレーター / Linux Orca 直接呼出し）の優先順位をドキュメント化。ユーザ向け案内は MSG-S18 で確定。**WCAG 2.1 AA 準拠**を非機能要件として固定し、Sub-D / Sub-F の受入条件に組み込む |

### REQ-S14: nonce overflow 検知 → rekey 強制

| 項目 | 内容 |
|------|------|
| 担当 Sub | **Sub-A (#39) で型契約確定**（`NonceCounter::increment` の `Result` 返却 + Boy Scout Rule で責務再定義） + Sub-C (#41) で AEAD 経路統合 + Sub-F (#44) で `vault rekey` フロー |
| 概要 | shikomi-core 側で `NonceCounter` の責務を「VEK ごとの暗号化回数監視」に再定義し、`increment(&mut self) -> Result<(), DomainError>` が上限 $2^{32}$ 到達時 `NonceLimitExceeded` を返す型契約（Sub-A）。AEAD 経路統合（Sub-C）と `vault rekey` 起動フロー（Sub-F）は後続 |
| 関連脅威 ID | L1（random nonce 衝突確率を $\le 2^{-32}$ に維持、上限到達後の暗号化を**型レベルで構造禁止**） |
| 入力 | **Sub-A**: vault ヘッダから読み込む `nonce_counter: u64`（`NonceCounter::resume(count)` 経由）、または新規 vault `NonceCounter::new()` で `count=0`。**Sub-C**: AEAD 暗号化のたびに `NonceCounter::increment` 呼出。**Sub-F**: `NonceLimitExceeded` 検知時の `vault rekey` 起動 |
| 処理 | **Sub-A**: `count < (1u64 << 32)` なら `count += 1; Ok(())`、上限到達なら `Err(DomainError::NonceLimitExceeded)`。`#[must_use]` 属性で結果無視を clippy lint で検出。既存「8B prefix + 4B counter」設計を**完全廃止**（Boy Scout Rule、per-record nonce は `NonceBytes::from_random([u8;12])` で完全 random 12B に変更）。**Sub-C / Sub-F**: TBD |
| 出力 | **Sub-A**: `Result<(), DomainError>`（成功時 unit 値、失敗時 variant）、`current(&self) -> u64`（永続化用）。**Sub-C / Sub-F**: TBD |
| エラー時 | Fail-Secure 必須: (a) 上限到達後の暗号化試行は **`NonceCounter::increment` が `Err` を返すことで構造的に禁止**、(b) `unwrap()` は禁止（`#[must_use]` + clippy lint）、(c) Sub-F の `vault rekey` 完了まで以後のレコード暗号化を全面拒否（Sub-D / Sub-F で詳細化） |

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
| 処理 | **Sub-A**: detailed-design.md §クラス設計（詳細）参照。`shikomi-core::crypto::verified` モジュールに `Verified<T>` / `Plaintext` / `CryptoOutcome<T>` を実装。`shikomi-core::crypto::password` に `PasswordStrengthGate` trait と `MasterPassword::new`。`shikomi-core::vault::nonce` の `NonceCounter::increment` に `#[must_use]` 付与。**Sub-B〜F**: 契約破りは PR レビューで却下（Boy Scout Rule） |
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
| MSG-S01 | 成功 | TBD by Sub-D | `vault encrypt` 完了時 |
| MSG-S02 | 成功 | TBD by Sub-D | `vault decrypt` 完了時 |
| MSG-S03 | 成功 | TBD by Sub-E | `vault unlock` 成功時 |
| MSG-S04 | 成功 | TBD by Sub-E | `vault lock` 完了時 |
| MSG-S05 | 成功 | TBD by Sub-E | `vault change-password` 完了時 |
| MSG-S06 | 警告 | TBD by Sub-D / Sub-F | `vault recovery-show` 表示直前（「この画面を 1 度しか表示しない」「写真撮影禁止」「金庫保管」） |
| MSG-S07 | 成功 | TBD by Sub-F | `vault rekey` 完了時（再暗号化レコード数を表示） |
| MSG-S08 | エラー | TBD by Sub-D | パスワード強度不足（zxcvbn `feedback.warning` + `feedback.suggestions` を埋め込む、Fail Kindly） |
| MSG-S09 | エラー | TBD by Sub-E（**カテゴリ別ヒント方針**: 単一文言で「失敗しました」と返さず、原因カテゴリ別に異なる Fail Kindly メッセージを出す。最低 3 カテゴリ — (a)「パスワード違い」: 連続失敗回数 / 次の試行可能までの待機秒数 + 「リカバリニーモニックでの復号 (`vault unlock --recovery`) も可能」案内、(b)「IPC 接続不能」: daemon 起動状態 (`shikomi daemon status`) の確認案内、(c)「キャッシュ揮発タイムアウト / 自動 lock」: アイドル 15min / スクリーンロック / サスペンドで lock した旨と再 unlock 案内。**内部詳細**（KDF パラメータ・nonce カウンタ・スタックトレース）は含めない。MSG 文言は Sub-E で確定） | アンロック失敗（IPC 経由全般） |
| MSG-S10 | エラー | TBD by Sub-C / Sub-D | AEAD 認証タグ不一致（vault.db 改竄の可能性、ユーザにバックアップから復元を案内） |
| MSG-S11 | エラー | TBD by Sub-C / Sub-F | nonce 上限到達（`vault rekey` への誘導） |
| MSG-S12 | エラー | TBD by Sub-D | リカバリニーモニック検証失敗（チェックサム不一致 / 単語数不正） |
| MSG-S13 | エラー | TBD by Sub-D | 平文 ⇄ 暗号化マイグレーション失敗（atomic write 失敗、原状復帰済みを明示） |
| MSG-S14 | 確認 | TBD by Sub-F | `vault decrypt` 実行前（暗号保護を外すリスクを明示） |
| MSG-S15 | エラー | TBD by Sub-E | IPC V2 非対応クライアント（V1 クライアントへの guidance） |
| MSG-S16 | 警告 | TBD by Sub-D / Sub-F（**暗号化モード初回切替時の限界説明**: 受入基準#9「過信なく / 過小評価なく伝わる」を担保する MSG。`vault encrypt` 確認モーダル / プロンプトで以下 3 点を必ず提示する — (1)「**侵害された端末（root マルウェア / 同特権デバッガ / kernel keylogger）からは保護できません**」、(2)「**BIP-39 24 語が漏洩した場合は完全敗北です。手書きメモを写真撮影 / クラウド同期しないでください**」、(3)「**画面共有・リモートデスクトップ中は秘密情報の表示を避けてください**」。CLI / GUI 両経路で同等表示、ユーザの明示的合意（`--accept-limits` フラグ / モーダル「理解しました」ボタン）なしに次工程へ進ませない） | `vault encrypt` 初回実行直前 / GUI 「暗号化モードに切替」ボタン押下直後 |
| MSG-S17 | 警告 | TBD by Sub-F + 後続 GUI feature（**GUI 暗号化モード可視化（田中ペルソナ対応）**: ペルソナ A 田中は CLI を読めない。Tauri WebView の常駐表示要素（タイトルバー / トレイアイコンツールチップ / レコード一覧画面ヘッダ）に **`[encrypted]` / `[plaintext]` バッジを常時表示**する。色（緑/灰）と文字（暗号化中/平文）の二重符号化（色覚多様性対応）。CLI 側 `shikomi list` の `[plaintext]` / `[encrypted]` バナー（REQ-S16）と文言を統一し、ユーザがどちらの UI を使ってもモードが視覚的に同期する。Sub-F の CLI 実装と将来の GUI feature 設計で同 MSG ID を共有 | GUI 起動時 / モード切替直後 / レコード一覧画面常時 |
| MSG-S18 | 警告 | TBD by Sub-D / Sub-F（**アクセシビリティ代替経路**: スクリーンリーダー利用ユーザ / 視覚障害ユーザがリカバリニーモニック（24 語）を取り扱う際の代替手段案内。OS 読み上げ拒否環境では「**録音禁止の音声プレイヤー優先順位ガイド**」「**印刷可能なハイコントラスト PDF 出力経路（`vault recovery-show --print`）**」「**点字対応プリンタ向け .brf 出力**」のいずれかを選べる旨を明示。Sub-D / Sub-F のアクセシビリティ要件（REQ-S13 末尾）と整合 | `vault recovery-show` 実行時にアクセシビリティモードが OS / shikomi 設定で検出された場合 |

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
| `zxcvbn` | crate | major ピン v3 系（同上） | REQ-S08 パスワード強度ゲート（Sub-D） |
| `secrecy` | crate | minor ピン（同上） | REQ-S02 / REQ-S09 秘密値ラッパ（Sub-A / Sub-E） |
| `zeroize` | crate | minor ピン（同上） | REQ-S02 / REQ-S09 / REQ-S13 メモリ消去（Sub-A〜F 全般） |
| `shikomi-core::Vault` 集約 | 既存 | Issue #7 完了済 | 暗号化モード経路で同一集約を再利用（Sub-A 拡張、Sub-D 利用） |
| `shikomi-infra::SqliteVaultRepository` | 既存 | Issue #10 完了済 | `EncryptedSqliteVaultRepository` 実装の参照元（Sub-D） |
| `shikomi-daemon` IPC 基盤 | 既存 | Issue #26 / #30 / #33 完了済 | IPC V1 → V2 非破壊拡張（Sub-E） |
| `shikomi-cli vault コマンド` | 既存 | `cli-vault-commands` feature | サブコマンド追加点（Sub-F） |
| `tech-stack.md` §2.4 / §4.7 / §4.3.2 | アーキ | PR #45 マージ済 | 暗号スイート凍結値・crate version pin・サプライチェーン契約 |
| `threat-model.md` §7 / §8 / §7.0 / §7.1 / §7.2 | アーキ | 既存 | 既存 STRIDE / OWASP 対応表との整合参照 |
