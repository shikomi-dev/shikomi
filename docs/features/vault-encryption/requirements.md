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
| 概要 | VEK / KEK / WrappedVek / MasterPassword / RecoveryMnemonic / KdfSalt / NonceCounter の `secrecy` + `zeroize` 型定義、`Clone` 禁止、`Debug` 秘匿、`Drop` 連鎖 |
| 関連脅威 ID | L1（型レベル fail-secure で改竄検証の bypass 禁止）／ L2（`SecretBox` + `zeroize` で過去メモリ抽出耐性）／ L3（`KdfSalt::generate()` 単一コンストラクタで salt の OsRng 由来契約） |
| 入力 | TBD by Sub-A |
| 処理 | TBD by Sub-A |
| 出力 | TBD by Sub-A |
| エラー時 | TBD by Sub-A（Fail-Secure 必須: `Verified<Plaintext>` newtype、`NonceCounter::increment` の `Result` 返却、`MasterPassword::new` の構築時強度検証） |

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
| 担当 Sub | Sub-D (#42) — `feat(shikomi-infra)` |
| 概要 | `vault encrypt` 入口で zxcvbn 強度 ≥ 3 を Fail Fast チェック、強度不足時は `Feedback`（warning + suggestions）を CLI/GUI に提示（Fail Kindly） |
| 関連脅威 ID | L3（弱パスワード時の Argon2id offline 突破を入口で禁止）／ L1（同上） |
| 入力 | TBD by Sub-D |
| 処理 | TBD by Sub-D（`MasterPassword::new` 構築時に zxcvbn 強度判定、強度 < 3 で `WeakPassword { feedback }` を返す） |
| 出力 | TBD by Sub-D |
| エラー時 | TBD by Sub-D（Fail-Secure + Fail Kindly: 拒否は早期、`feedback.warning` / `feedback.suggestions` をユーザに渡す。MSG-* 文言は Sub-D で確定） |

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

### REQ-S14: nonce overflow 検知 → rekey 強制

| 項目 | 内容 |
|------|------|
| 担当 Sub | Sub-C (#41) + Sub-F (#44) |
| 概要 | 上限 $2^{32}$ 到達時 `NonceLimitExceeded` を返し、Sub-F の `vault rekey` で VEK 再生成 + 全レコード再暗号化 |
| 関連脅威 ID | L1（random nonce 衝突確率を $\le 2^{-32}$ に維持） |
| 入力 | TBD by Sub-C / Sub-F |
| 処理 | TBD by Sub-C / Sub-F（`NonceCounter::increment` で上限到達時 `Result::Err`、CLI 側は rekey フローへ誘導） |
| 出力 | TBD by Sub-C / Sub-F |
| エラー時 | TBD by Sub-C / Sub-F（Fail-Secure 必須: 上限到達後の暗号化試行は型レベルで拒否） |

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
| 担当 Sub | Sub-A〜F 全 Sub 共通 |
| 概要 | `Verified<Plaintext>` newtype、`MasterPassword::new` の強度検証、`NonceCounter::increment` の `Result` 返却、`match` 暗号アーム第一パターン、`Drop` 連鎖 |
| 関連脅威 ID | L1 / L2 / L3（型レベルで fail-secure を強制し、実装ミスによる脆弱性経路を構造的に閉じる） |
| 入力 | TBD by Sub-A 起点 |
| 処理 | TBD by Sub-A〜F 各 Sub（Sub-A で newtype と契約を確定、Sub-B〜F は本文を破らない） |
| 出力 | TBD |
| エラー時 | TBD（型システムによる強制であり、違反はコンパイルエラーまたは clippy lint 失敗） |

## 画面・CLI仕様

該当なし — 理由: 本 Sub-0 は脅威モデル文書化と REQ-S* 採番のみがスコープ。CLI 仕様（`shikomi vault {encrypt/decrypt/unlock/lock/change-password/recovery-show/rekey}` の引数・出力形式・終了コード）は **Sub-F (#44)** の `requirements.md` 拡張で確定する。本ファイルの本セクションは Sub-F 設計時に **READ → EDIT** で表に書き起こす。

## API仕様

該当なし — 理由: 本 Sub-0 段階では API 確定をしない。IPC V2 拡張（`IpcRequest::{Unlock, Lock, ChangePassword, RotateRecovery, Rekey}` / レスポンス型 / エラー variant）の確定は **Sub-E (#43)** で行い、`daemon-ipc/basic-design/ipc-protocol.md` および `daemon-ipc/detailed-design/protocol-types.md` を主たる正本とする。本ファイルの本セクションは Sub-E 設計時に外部リンクのみ書き戻す。

## データモデル

本 Sub-0 段階では暗号メタデータのデータモデルを骨格のみ採番する。属性詳細・型・SQLite カラム制約・関連は **Sub-A (#39)** および **Sub-D (#42)** の設計工程で本ファイルを READ → EDIT して埋める。

| エンティティ | 属性 | 型 | 制約 | 関連 |
|-------------|------|---|------|------|
| `VaultEncryptedHeader` | TBD | TBD by Sub-A | ヘッダ独立 AEAD タグで保護、`vault_format_version` で互換管理 | `Vault` ↔ 1:1 |
| `WrappedVek` | TBD | TBD by Sub-A | AEAD ciphertext + nonce + tag、`wrapped_VEK_by_pw` / `wrapped_VEK_by_recovery` の 2 バリアント | `VaultEncryptedHeader` ↔ N:1 |
| `KdfSalt` | TBD | TBD by Sub-A | 16B、`KdfSalt::generate()` 単一コンストラクタで OsRng 由来契約 | `VaultEncryptedHeader` ↔ 1:1 |
| `KdfParams` | TBD | TBD by Sub-A | Argon2id `m, t, p`（凍結値: `m=19456, t=2, p=1`） | `VaultEncryptedHeader` ↔ 1:1 |
| `NonceCounter` | TBD | TBD by Sub-A | u64、上限 $2^{32}$ 到達で `NonceLimitExceeded` | `VaultEncryptedHeader` ↔ 1:1 |
| `EncryptedRecord` | TBD | TBD by Sub-C / Sub-D | per-record AEAD ciphertext + AAD（record_id ‖ version ‖ created_at、26B）+ nonce 12B + tag 16B | `Vault` ↔ N:1 |
| `MasterPassword`（揮発のみ） | TBD | TBD by Sub-A | `SecretBytes`、構築時 zxcvbn 強度 ≥ 3 検証、`Drop` 時 zeroize | 永続化しない |
| `RecoveryMnemonic`（揮発のみ） | TBD | TBD by Sub-A | BIP-39 24 語、`Drop` 時 zeroize、再表示不可 | 永続化しない |
| `Vek`（揮発のみ） | TBD | TBD by Sub-A | `secrecy::SecretBox<[u8;32]>`、daemon プロセス内のみ | キャッシュ寿命: アイドル 15min / スクリーンロック / サスペンドで zeroize |

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
| MSG-S09 | エラー | TBD by Sub-E | アンロック失敗（連続失敗回数 / 次の試行可能までの待機秒数を含む、内部詳細は含めない） |
| MSG-S10 | エラー | TBD by Sub-C / Sub-D | AEAD 認証タグ不一致（vault.db 改竄の可能性、ユーザにバックアップから復元を案内） |
| MSG-S11 | エラー | TBD by Sub-C / Sub-F | nonce 上限到達（`vault rekey` への誘導） |
| MSG-S12 | エラー | TBD by Sub-D | リカバリニーモニック検証失敗（チェックサム不一致 / 単語数不正） |
| MSG-S13 | エラー | TBD by Sub-D | 平文 ⇄ 暗号化マイグレーション失敗（atomic write 失敗、原状復帰済みを明示） |
| MSG-S14 | 確認 | TBD by Sub-F | `vault decrypt` 実行前（暗号保護を外すリスクを明示） |
| MSG-S15 | エラー | TBD by Sub-E | IPC V2 非対応クライアント（V1 クライアントへの guidance） |

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
