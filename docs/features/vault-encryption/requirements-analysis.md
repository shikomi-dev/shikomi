# 要求分析書

<!-- feature: vault-encryption / Epic #37 / Sub-0 (#38) -->
<!-- 配置先: docs/features/vault-encryption/requirements-analysis.md -->
<!-- 本書は Epic #37（vault 暗号化モード実装）の「設計の前段」として、後続 Sub-A〜F が共通参照する脅威モデルを凍結する -->

## 人間の要求

> 【決定】分割案v2を採用する。脅威モデル先行、DAG: 0 → A → {B, C 並列} → D → E → F。
> Sub-0: 脅威モデル文書化（攻撃者能力L1〜L4凍結）
> — キャプテン・アメリカ（Epic #37 配下、Sub-issue #38）

> （Issue #4 / #7 / #10 / #26 / #30 / #33 完了の流れを受けて）`vault-persistence` の REQ-P11「暗号化モード即時拒否」を解禁したい。`tech-stack.md` §2.4 と `threat-model.md` §7 で凍結済みの暗号スイート（Argon2id + AES-256-GCM + BIP-39 24語 + Envelope Encryption）を、後続 Sub-A〜F で実装に落とす。
> — 依頼主（まこちゃん）／ Epic #37

## 背景・目的

- `docs/architecture/context/threat-model.md` §7 / §8、`docs/architecture/tech-stack.md` §2.4 / §4.7 で**暗号スイートと crate version pin は確定済み**だが、**「どの攻撃者に対して何をどこまで守るのか」は STRIDE の文章中に分散**しており、後続 Sub-A〜F の **受入条件・テスト基準・残存リスクの一本化に直接使える形で凍結されていない**
- 後続 Sub-issue（A: 暗号ドメイン型 / B: KDF / C: AEAD / D: 暗号化リポジトリ + マイグレ / E: VEK キャッシュ + IPC V2 / F: vault 管理 CLI）はいずれも「**この攻撃者に対してこの守りで十分**」を判定根拠として書かれる必要がある。脅威 ID（L1〜L4）が共通語彙として無いと、各 Sub 設計者・実装者・レビュアー・テスト担当が **再発明と不整合**を起こす（例: Sub-C 設計者は「メモリスナップショット攻撃を考慮」と書き、Sub-E 設計者は「考慮不要」と書く）
- `tech-stack.md` 工程0レビューで「random nonce 採用根拠は**並行性モデル**から導く」「`zxcvbn` の Fail Kindly」「`getrandom` 二重防護」など**設計判断の根拠**が攻撃者モデルに紐づき始めている。本書はその根拠の**唯一の根拠源**として L1〜L4 を凍結する
- ビジネス価値: ペルソナ A（田中、Windows 営業）/ C（佐々木、総務）が暗号化モードに切替えた時、**「どんなマルウェアまで守れるのか」を SECURITY.md / `vault encrypt --help` で正確に説明**できる根拠を与える。曖昧な「安全です」を防ぎ、過信による事故と過小評価による離脱の両方を防ぐ
- Vモデル対応: 本書は Epic #37 の「要求分析」階層であり、対応するテストレベルは **E2E テスト**（攻撃シナリオ E2E と Fail-Secure E2E）

## 議論結果

Epic #37 の Sub-issue 分割案 v2 確定時に以下を合意（PR #45 工程0 の議論経過を集約）:

- 設計担当（セル）の見解: 脅威モデル文書化を Sub-0 として **A 着手前に分離凍結**。L1〜L4 capability matrix を**唯一の真実源**にし、`threat-model.md` §7 STRIDE は引用元として残し L1〜L4 への参照を加える方向
- 防衛線（ペテルギウス）の指摘: 攻撃者モデル未凍結で Sub-A〜F に着手すると、**各 Sub の受入条件が脅威 ID 不在のまま増殖**し、後続レビューで「この対策はどの脅威向け？」の議論が反復する
- ペガサスの指摘: ユーザ向け文言（SECURITY.md / `vault encrypt --help`）が脅威モデルから**逆引き**できる構造でないと、Fail Kindly が「弱いです」だけになる
- 服部の指摘: `tech-stack.md` で random nonce / `subtle` v2.5+ floor / `getrandom` 独立扱いの**根拠**が攻撃者モデルに帰着するため、L1〜L4 をテストベクトル選定根拠にも使う
- キャプテン決定: Sub-0 は **2 ファイル**（本書 + `requirements.md` の REQ-S* id 採番）に絞る。`threat-model.md` §7 への L1〜L4 語彙の差し戻し（精緻化）は、Sub-A 以降の設計過程で参照側から段階的に拡張する（YAGNI、Boy Scout Rule の漸進適用）

## ペルソナ

エンドユーザーペルソナ（田中 / 山田 / 佐々木）は `docs/architecture/context/overview.md` §3.2 を参照。本 feature は**暗号化モードに自分でオプトインしたユーザ**が対象であり、上記 3 ペルソナのうち**全員が対象になりうる**（プライマリは「機密性が高いものを保管したい A / B」、セカンダリは「会社規定で暗号化を求められた C」）。本 Sub-0 は実装ではなく**設計合意文書**のため、開発者ペルソナを直接の読者として置く。

| ペルソナ名 | 役割 | 技術レベル | 利用文脈 | 達成したいゴール |
|-----------|------|-----------|---------|----------------|
| 野木 拓海 | 後続 Sub-A〜F の Rust 実装担当 | Rust 中級（暗号 crate / `tokio` / IPC 実務経験） | 各 Sub の詳細設計を読み実装に落とす | **どの攻撃者を想定して**この型 / KDF パラメータ / nonce 戦略 / キャッシュ寿命 を選んだのか、**ID（L1〜L4）で逆引きできる**。実装中に「ここまでやれば十分か？」の判定根拠が単一文書で確認できる |
| 涅 マユリ | テスト担当（後続 Sub-A〜F のテスト設計） | テスト戦略立案 | 攻撃シナリオ E2E と Fail-Secure ユニットを設計 | 各 REQ-S* が**どの脅威 ID に紐づくか**を見て、テスト観点（攻撃者シミュレーション・改竄注入・nonce overflow 強制等）を網羅的に設計できる |
| 服部 平次 | セキュリティ防衛側（外部レビュアー） | 脅威モデル / OWASP / RFC | 後続 Sub-A〜F の PR を縦串で監査する | **スコープ外として何を受容したか**が L4 / 同特権デバッガ / 物理攻撃 / 量子攻撃の各カテゴリで明示されており、レビュー指摘の判断軸がブレない |
| 田中 俊介（間接） | Windows 営業職（エンドユーザー） | OS 操作可、CLI 不可 | `shikomi vault encrypt` を選んだ後、SECURITY.md と `--help` を読む | 「**どんなマルウェアまで守れるのか／何は守れないのか**」が暗号化モード切替時に正確に伝わる（過信防止 + 過小評価による離脱防止） |

## 前提条件・制約

- **Epic #37 のスコープ凍結値はそのまま**: 鍵階層（VEK 32B / KEK_pw / KEK_recovery）、AEAD（AES-256-GCM、per-record、nonce 12B、AAD 26B、上限 $2^{32}$）、KDF（Argon2id `m=19456, t=2, p=1`）、リカバリ（BIP-39 24 語）、マスターパスワード変更（VEK 不変、`wrapped_VEK_by_pw` のみ更新で O(1)）、VEK キャッシュ（daemon プロセス内 `secrecy::SecretBox<[u8;32]>`、アイドル 15min / スクリーンロック / サスペンドで `zeroize`）。本書はこれらの**選定根拠を脅威 ID 別に整理する**ものであり、凍結値そのものを変更しない
- **本書のスコープは「凍結」**: 実装可能な KDF パラメータ・nonce 戦略・暗号 crate の version pin はすべて確定済（`tech-stack.md` §2.4 / §4.7 / §4.3.2、PR #45 マージ済）。本書は**それらが脅威 L1〜L4 に対してどう機能するか**を凍結し、後続 Sub の実装判断（性能 / API 形 / エラー設計 / テスト観点）の参照点となる
- **本 Sub-0 の成果物範囲**: 本書 + `requirements.md` の REQ-S* id 採番骨格の 2 ファイル。`docs/architecture/context/threat-model.md` §7 への L1〜L4 語彙の差し戻し（精緻化）は **Sub-A 以降の設計過程で参照側から段階的に拡張**する方針（キャプテン決定）。本 Sub-0 では `threat-model.md` §7 を**変更しない**
- **既存設計書との関係**: `vault-persistence/requirements-analysis.md` / `vault/requirements-analysis.md` が**部分的に**脅威への言及を持つ（OS パーミッション・atomic write 等）が、本書はそれらを「L1 同ユーザ別プロセス」の対策として**統合参照**する。重複再記載は避ける
- **依存 Sub なし**: 本 Sub-0 は Sub-A の前段。本書を確定しないと Sub-A 以降の受入条件が定義不能
- **暗号アルゴリズムの再選定は対象外**: Argon2id / AES-256-GCM / BIP-39 / Envelope Encryption の選定根拠は `tech-stack.md` §2.4 / §4.7 で確定済。本書は「**選定済暗号スイートが L1〜L4 にどう機能するか**」を凍結するのみで、別暗号への切替議論は再開しない（YAGNI、`tech-stack.md` 4 年再評価サイクルに従う）

## 脅威モデル（Sub-0 の主成果物 — 凍結対象）

### 1. システム範囲（System Under Analysis）

本脅威モデルが対象とするシステム要素:

- **shikomi-daemon プロセス**: vault データの真実源、VEK キャッシュ保持、IPC サーバ、ホットキー購読
- **vault.db**（暗号化モード時の SQLite ファイル）: ヘッダ + records テーブル、AEAD 暗号化済み payload + AAD（record_id ‖ version ‖ created_at）+ per-record nonce 平文
- **vault ヘッダの暗号メタデータ**: `wrapped_VEK_by_pw`、`wrapped_VEK_by_recovery`、`kdf_params`（Argon2id `m, t, p`）、`kdf_salt`（16B 平文）、`nonce_counter`、`vault_format_version`
- **マスターパスワード入力経路**: CLI prompt（`shikomi vault unlock`）、GUI パスワード入力欄（Tauri WebView）→ daemon IPC（UDS/Named Pipe）
- **リカバリ表示・入力経路**: 初回 1 度きりの BIP-39 24 語表示（`shikomi vault recovery-show` 直後の永続無記録）、リカバリ復号入力（24 語 → KEK_recovery → VEK）
- **shikomi-cli / shikomi-gui プロセス**: vault 真実源は持たず、daemon に IPC で委譲

対象外:

- OS 自体（カーネル・ブートローダ・ファームウェア）
- ハードウェア（CPU・RAM・ディスク・周辺機器）
- 同居する他アプリケーション（ブラウザ・メーラ・IME 等）
- ネットワーク経路（shikomi はネットワーク I/O を持たない、`tauri-plugin-updater` の更新チェックを除く）

### 2. 信頼境界（Trust Boundaries）

| 境界 | 内側（信頼） | 外側（非信頼） | 横断ポイント |
|-----|-----------|-------------|------------|
| **プロセス境界** | shikomi-daemon の単一プロセス内（VEK / 平文レコード） | 同ユーザ内の他プロセス（CLI / GUI 含む） | IPC（UDS `0700` / Named Pipe SDDL）、ピア UID 検証、セッショントークン（後続 Issue 予定） |
| **ユーザ境界** | 現在の OS ユーザ | 他 OS ユーザ / root | OS パーミッション `0600` / `0700`、Windows DACL 所有者 SID 単独 ACE |
| **永続化境界** | プロセス内 RAM | ディスク（vault.db） | AEAD 暗号化（記録時）/ AEAD 復号 + MAC 検証（読出時）、`secrecy::SecretBox` + `zeroize`（RAM 側） |
| **表示境界** | shikomi 描画ピクセル | 画面共有 / リモートデスクトップ / 肩越し閲覧 | リカバリ表示は明示確認後 1 度きり、クリップボード通知に秘密を含めない（`threat-model.md` §7.2.1） |
| **入力境界** | shikomi のパスワード入力欄 | OS キーロガー / 注入アプリ | macOS Secure Event Input（仕様）、その他 OS は防御不能（受容、L4 相当） |

### 3. 保護資産インベントリ（Protected Assets）

機密レベルは **Tier-1〜3** で分類する。Tier-1 は漏洩 = 全敗。

| 資産 | Tier | 所在 | 寿命 | 漏洩時の被害 |
|-----|------|------|-----|------------|
| **マスターパスワード**（ユーザ入力） | 1 | プロセス内 RAM（`SecretBytes`、入力直後 → KDF 完了で `zeroize`） | 入力〜KDF 完了（< 1 秒） | KEK_pw 再導出可能 → `wrapped_VEK_by_pw` を offline 復号 → VEK 復元 → 全レコード平文化 |
| **リカバリニーモニック**（BIP-39 24 語） | 1 | 初回 1 度のみ画面表示、ユーザ手書き保管。プロセス内には**保持しない**（生成直後に zeroize、再表示不可） | 表示中 + ユーザ手書き保管期間 | KEK_recovery 再導出可能 → `wrapped_VEK_by_recovery` を offline 復号 → VEK 復元 → 全レコード平文化（**完全敗北、回復不能**） |
| **VEK**（Vault Encryption Key、32B） | 1 | プロセス内 RAM（`secrecy::SecretBox<[u8;32]>`、unlock 後〜アイドル 15min / スクリーンロック / サスペンドで `zeroize`） | unlock〜lock（最大 15min） | 全レコード平文化（即時） |
| **KEK_pw**（パスワード由来 KEK、32B） | 1 | プロセス内 RAM（`SecretBytes`、Argon2id 完了 → wrap/unwrap 完了で `zeroize`） | 数百ms〜1 秒 | VEK unwrap 可能 → 全レコード平文化 |
| **KEK_recovery**（リカバリ由来 KEK、32B） | 1 | プロセス内 RAM（同上） | 数百ms〜1 秒 | 同上 |
| **平文レコード**（unlock 後の records） | 1 | プロセス内 RAM（`SecretBytes`、表示完了 → 30 秒クリップボードクリア後 `zeroize`） | レコード単位、表示〜30 秒 | 該当レコード即時漏洩 |
| **`wrapped_VEK_by_pw`**（AEAD ciphertext + nonce + tag） | 2 | vault.db ヘッダ（ディスク平文保管） | 永続 | 強パスワード（Argon2id KDF 強度依存）であれば offline brute force 困難。弱パスワード時は L3 で全敗 |
| **`wrapped_VEK_by_recovery`**（同上） | 2 | vault.db ヘッダ（ディスク平文保管） | 永続 | BIP-39 24 語の暗号エントロピー 256 bit + PBKDF2 2048iter で offline brute force は事実上不可能（24 語盗難時のみ L4 相当の完全敗北） |
| **records.ciphertext**（AEAD per-record） | 2 | vault.db `records` テーブル（ディスク平文保管） | 永続 | VEK 不在では復号不可。MAC 改竄は AEAD タグで検出 → fail fast |
| **`kdf_salt`**（16B、Argon2id 入力 salt） | 3 | vault.db ヘッダ（ディスク平文保管） | 永続 | 公開前提（salt の秘匿は不要、§2.4 KDF ソルト行）。**改竄**されれば KDF 出力が変わり unlock が永久に失敗（DoS）→ atomic write + 起動時整合チェックで検出 |
| **per-record `nonce`**（12B） | 3 | vault.db `records` テーブル（ディスク平文保管） | 永続 | 公開前提。**衝突**が起きると同一 VEK 下で平文同値漏洩 → random nonce + $2^{32}$ rekey で衝突確率 $\le 2^{-32}$ |
| **AAD**（record_id ‖ version ‖ created_at、26B） | 3 | vault.db `records` 各カラム（ディスク平文保管） | 永続 | 改竄されれば AEAD 検証失敗 → fail fast。version 改竄でロールバック攻撃を企図しても AAD に含まれているため検出 |
| **`kdf_params`**（Argon2id `m, t, p`） | 3 | vault.db ヘッダ（ディスク平文保管） | 永続 | ダウングレード攻撃（弱パラメータへの差替）→ ヘッダ全体に独立 AEAD タグを付与する設計で検出（Sub-A / Sub-D で詳細化） |

### 4. 攻撃者能力 L1〜L4（凍結）

**本セクションは Sub-A〜F の受入条件・テスト基準が参照する唯一の脅威 ID 定義である。**

#### L1: 同ユーザ別プロセス（Same-User Other-Process）

| 項目 | 内容 |
|-----|------|
| **能力** | 現在の OS ユーザ権限で動作する別プロセス（マルウェア / 誤実行スクリプト / 同居アプリの脆弱性経由）。`~/.config/shikomi/vault.db` を OS パーミッション内で **read / write 可能**。**daemon プロセスのメモリ空間には到達不可**（OS のプロセス分離による） |
| **想定具体例** | ブラウザ拡張のマルウェア化、ダウンロードした不審スクリプト、サプライチェーン攻撃された開発ツール、メーラ添付の自動実行 |
| **対応する STRIDE** | Tampering（vault.db 書換）、Information Disclosure（vault.db 直接読取）、DoS（vault.db 削除 / 破損注入） |
| **対策（暗号化モード）** | (a) **AES-256-GCM の認証タグ**で改竄検出 → `CryptoError::AeadTagMismatch` で fail fast、(b) AAD（record_id ‖ version ‖ created_at）でレコード入替・ロールバック検出、(c) ヘッダ全体に独立 AEAD タグを付与し `kdf_params` / `wrapped_VEK_*` の差替検出（Sub-D）、(d) random nonce + 衝突上限 $2^{32}$ で同一 VEK 下の平文同値漏洩を確率的に排除、(e) `wrapped_VEK_*` を offline brute force するには Argon2id を超える必要あり（強パスワード前提） |
| **残存リスク（受容）** | 強パスワード前提：弱パスワード（zxcvbn 強度 < 3）は本書では受容しない（Sub-D の `vault encrypt` 入口で Fail Fast 拒否）。`wrapped_VEK_*` のヘッダ部分は**読取できる**ため、強パスワードでも理論上 offline 試行は走らせられる（Argon2id `m=19456, t=2, p=1` 1 回当たり数百ms〜1 秒の作業証明が要件） |
| **平文モード時の扱い** | **本書スコープ外**（`threat-model.md` §7.0 で受容済）。L1 攻撃で全レコード即時漏洩 |
| **テスト観点（Sub 受入の参照点）** | (a) 改竄注入 → `AeadTagMismatch` fail fast を確認（Sub-C / Sub-D）、(b) ヘッダ書換 → ヘッダ AEAD タグ検証で fail fast（Sub-D）、(c) random nonce 衝突確率の理論計算とベンチ（Sub-C） |

#### L2: メモリスナップショット（Memory Snapshot）

| 項目 | 内容 |
|-----|------|
| **能力** | コアダンプファイル（`/var/lib/systemd/coredump/`、`%LocalAppData%\CrashDumps\`、`/cores/`）、ハイバネーションファイル（`hiberfil.sys`、`/var/vm/sleepimage`、`/swap`）、ページングスワップから daemon プロセスの**過去**メモリを抽出。**daemon が稼働中の active メモリへの直接アクセスは不可**（同特権デバッガは L4） |
| **想定具体例** | クラッシュダンプ自動アップロード設定、サスペンド中のラップトップ盗難、SSD のセキュア消去なしでの破棄 |
| **対応する STRIDE** | Information Disclosure（VEK / KEK_* / 平文レコードの過去メモリからの抽出） |
| **対策（暗号化モード）** | (a) **`secrecy::SecretBox<[u8;32]>` で `Drop` 時 `zeroize`**（VEK / KEK_* / 平文レコード全て）、(b) **VEK キャッシュのアイドル 15min / スクリーンロック / サスペンドで明示 `zeroize`**（Sub-E）、(c) **best-effort `mlock(2)` / `VirtualLock`**（スワップへの書き出し抑制、ただし RAM 不足時は OS が無視可能）、(d) KDF 中間値（Argon2id 内部メモリ）も `zeroize` 連携 feature を有効化（`tech-stack.md` §4.7 各 crate 行）、(e) 平文レコードはクリップボード投入後 30 秒で `zeroize`（`threat-model.md` §7.2） |
| **残存リスク（受容）** | (a) `mlock` は OS の RAM 不足時には強制スワップ可能（`mlock(2)` man-page）、(b) ハイバネーションは OS 機構で RAM 全体がディスクに書き出される、(c) JIT / コンパイラ最適化で秘密値が一時レジスタ / スタックフレームに残る可能性は完全には排除不能、(d) `zeroize` 完了前の coredump は防げない |
| **平文モード時の扱い** | 該当なし（暗号化モードのみで秘密鍵キャッシュが存在） |
| **テスト観点（Sub 受入の参照点）** | (a) `Drop` 後のメモリパターン検証（Sub-A、`SecretBox` の zeroize 観測）、(b) アイドル 15min タイマで VEK が zeroize されることを観測（Sub-E）、(c) サスペンド signal で zeroize を観測（Sub-E、OS 統合テスト） |

#### L3: 物理ディスク奪取（Offline Disk Acquisition）

| 項目 | 内容 |
|-----|------|
| **能力** | OS が起動していない状態でディスクを直接マウント / イメージ取得。vault.db 全データ（ヘッダ + records ciphertext + AAD + nonce + `wrapped_VEK_*`）を**任意回数オフライン解析可能**。RAM 上の VEK / KEK / 平文には到達不可（電源断時に揮発） |
| **想定具体例** | ノートパソコン紛失・盗難（電源 off 状態）、廃棄時のディスク引き抜き、データセンター物理侵入 |
| **対応する STRIDE** | Information Disclosure（vault.db 全件の offline 解析） |
| **対策（暗号化モード）** | (a) **AES-256-GCM** により VEK 不在では平文不可、(b) **Argon2id `m=19456, t=2, p=1`** で `wrapped_VEK_by_pw` の offline brute force に作業証明を強制（`threat-model.md` §8、OWASP 推奨値）、(c) **`wrapped_VEK_by_recovery` は BIP-39 24 語 256 bit エントロピー + PBKDF2-HMAC-SHA512 2048iter** により事実上 brute force 不可能、(d) ヘッダ AEAD タグでヘッダ改竄検出 |
| **残存リスク（受容）** | (a) **弱パスワード時の Argon2id 総当たり**は本書では受容しない（zxcvbn 強度 < 3 を入口で拒否、Sub-D）、(b) **強パスワードであっても Argon2id は決定論的**であり、攻撃者が望むだけ並列化して試行できる。`m=19456 KiB`（≈19 MiB）のメモリコストはコモディティ GPU/ASIC を制限する設計、(c) **BIP-39 24 語が漏洩した瞬間に完全敗北**（リカバリは「マスターパスワード失念対策」の保険であり、24 語自体の保管はユーザ責任）、(d) 暗号アルゴリズムの将来的な破綻（量子計算機による Grover アルゴリズム適用 → AES-256 の実効強度は 128 bit に低下するが、本書執筆時点では実装可能な脅威ではない、4 年再評価サイクル `tech-stack.md`） |
| **平文モード時の扱い** | **本書スコープ外**（`threat-model.md` §7.0 で受容、ディスク暗号化（BitLocker / FileVault / LUKS）併用が推奨対策） |
| **テスト観点（Sub 受入の参照点）** | (a) RFC 9106 Appendix A の Argon2id KAT（Sub-B）、(b) NIST CAVP の AES-GCM テストベクトル（Sub-C）、(c) BIP-39 trezor 公式ベクトル（Sub-B）、(d) `wrapped_VEK_*` 復号路の正常系・改竄系・パスワード違い系の網羅（Sub-D） |

#### L4: 同ユーザ・root（Same-User Root / OS Compromise）

| 項目 | 内容 |
|-----|------|
| **能力** | 現在の OS ユーザの管理者権限取得、または root 権限取得、またはカーネル / ブートローダ / ファームウェア侵害。**全ての OS 信頼境界が崩壊**。daemon プロセスへの ptrace / debugger アタッチ、kernel keylogger、LD_PRELOAD / DLL injection、`/proc/<pid>/mem` 読取、Hypervisor からのメモリ読取、Cold Boot 攻撃、Evil Maid 攻撃 |
| **想定具体例** | 侵害された企業端末、root 権限取得済みのマルウェア、共有端末（複数ユーザが root 持ち）、悪意あるシステム管理者、サプライチェーン攻撃された OS イメージ |
| **対応する STRIDE** | 全カテゴリ（S/T/R/I/D/E すべて成立可能） |
| **対策** | **対象外**（`threat-model.md` §7.0「侵害された端末では使わない」既定方針）。本書では shikomi 側で**いかなる対策も追加しない** |
| **残存リスク** | 全敗（受容） |
| **ユーザ向け約束** | SECURITY.md / `vault encrypt --help` / 初回 unlock 時の説明文で「**侵害された端末での使用は想定外**」「**root 権限を持つマルウェアからは保護できない**」を明示。過信防止が UX 責務（Fail Visible） |
| **テスト観点（Sub 受入の参照点）** | テストしない（防御不能の受容）。ただし **L4 を装ったテストで「対策が無いこと」を間接的に確認**（例: `gdb` attach で VEK が読めることを確認し、それが**設計通り**であることをドキュメント化、Sub-A〜E のセキュリティドキュメントで明示） |

### 5. スコープ外（明示的に守らないもの）

以下は shikomi の暗号化モードでも**意図的に守らない**領域。受容根拠を明示しておくことで、後続レビューで「対策漏れ」と誤認されないようにする。

| カテゴリ | 内容 | 受容根拠 |
|--------|-----|---------|
| **L4 全般**（同ユーザ root / OS 侵害） | ptrace / kernel keylogger / LD_PRELOAD / `/proc/<pid>/mem` 等 | 防御不能。`threat-model.md` §7.0「侵害された端末では使わない」既定 |
| **稼働中 daemon への同特権デバッガアタッチ** | gdb / lldb / WinDbg / cdb での active メモリ読取 | OS 信頼境界の問題。`PR_SET_DUMPABLE(0)` / `PROCESS_VM_READ` 拒否は OS により無視可能 / バイパス可能 |
| **物理ハードウェア攻撃** | Cold Boot、JTAG、DMA 攻撃（Thunderbolt PCIe）、サイドチャネル（電力解析・電磁波） | 物理アクセス前提の攻撃は OS / ハードウェアの責務 |
| **量子計算機攻撃** | Shor / Grover アルゴリズム | 本書執筆時点（2026-04）で実装可能な脅威ではない。AES-256 は Grover 適用後も実効 128 bit、4 年再評価サイクル（`tech-stack.md` §4.7）で post-quantum AEAD への移行を再検討 |
| **共有端末・侵害 OS** | 複数ユーザが root 持ちの端末、root kit 感染済み端末 | `threat-model.md` §7.0 既定、SECURITY.md で「使わない」を明記 |
| **BIP-39 24 語の盗難** | ユーザが手書きメモを撮影される / クラウドにアップロードする | 24 語の保管はユーザ責任。SECURITY.md / `recovery-show` 出力で「金庫保管」「写真撮影禁止」「クラウド保管禁止」を明示 |
| **画面共有 / リモートデスクトップ中の表示漏洩** | Zoom / Meet / TeamViewer 等 | OS / 配信ツール側の責務。クリップボード通知に秘密を含めない（`threat-model.md` §7.2.1）対策のみ実施 |
| **OS キーロガー** | macOS Secure Event Input が効かないアプリ、Windows / Linux の入力フック | 本質的に L4 相当。macOS のみ部分対策、他 OS は受容 |
| **マスターパスワード失念**（リカバリ 24 語も紛失） | ユーザがパスワードを忘れ、24 語も保管していない | **回復不能**を仕様として明示（`threat-model.md` §8 A07）。`wrapped_VEK_*` は強い KDF で守られているため、ベンダ側に復号バックドアを置かない |
| **vault.db のバックアップ媒体での復号** | クラウド同期ツールが `~/.config/shikomi/` を同期 | バックアップ先でも暗号化モードならば AEAD で保護される（強み）。ただしユーザが平文 export を別場所に置けば対象外（`threat-model.md` §7.0 推奨対処） |

### 6. Fail-Secure 哲学（凍結）

Issue #33 の `(_, Ipc) => Secret` パターン（IPC 経路は無条件で秘密扱い）と同じ思想で、本 feature の暗号化境界も **fail-secure を型レベルで強制**する。

**原則**: 鍵到達失敗・MAC 検証失敗・nonce 上限到達・KDF 失敗・パスワード強度不足は**例外なく拒否**する。中途半端な状態（部分復号済み・VEK が一部だけキャッシュ済み・パスワード変更が wrap だけ更新失敗）を引きずらない。

**型レベル強制パターン**（Sub-A 設計時に詳細化）:

| パターン | 適用 | 効果 |
|--------|-----|------|
| `match` の暗号アーム第一 | `match (mode, outcome)` で暗号化モード × 検証結果の組合せを網羅、未検証ケースの fall-through を構造的に禁止 | 部分検証で先に進む実装ミスを排除（Issue #33 同型） |
| `Verified<Plaintext>` newtype | 復号成功は `Verified` でラップされた型でしか得られない。**生 `Plaintext` を作る経路は復号関数のみ** | 「未検証 ciphertext を平文として扱う」事故を型レベルで禁止 |
| `NonceCounter::increment` の `Result` 返却 | 上限到達時 `NonceLimitExceeded` を返す。`unwrap()` は禁止（clippy lint） | 上限到達後の暗号化を構造的に禁止 |
| `MasterPassword::new(s)` 構築時 zxcvbn 検証 | 強度 < 3 で `Err(WeakPassword { feedback })` を返す。**生 `String` から VEK 経路に入れない** | 弱パスワードでの暗号化を入口で禁止 |
| `Drop` 連鎖 | `secrecy::SecretBox` / `Zeroizing` 型から派生する全集約に `Drop` 経路を保証、忘却型による zeroize 漏れを禁止 | L2 メモリスナップショット対策の型レベル担保 |

## 機能一覧

本 Epic #37 の Sub-issue を機能粒度で列挙する。Sub-issue 番号は GitHub Issue 番号と一致。各 REQ-S* の本文（入力 / 処理 / 出力 / エラー時）は `requirements.md` に枠組みのみ採番、詳細は後続 Sub-A〜F の設計工程で拡張。

| 機能ID | 機能名 | 概要 | 担当 Sub | 優先度 |
|--------|-------|------|---------|--------|
| REQ-S01 | 脅威モデル準拠 | 本書 §脅威モデル の L1〜L4 凍結を後続 Sub の受入条件・テスト基準の唯一根拠とする | Sub-0（本書） | 必須 |
| REQ-S02 | 暗号ドメイン型 | VEK / KEK / WrappedVek / MasterPassword / RecoveryMnemonic / KdfSalt / NonceCounter の `secrecy` + `zeroize` 型定義、Clone 禁止、`Debug` 秘匿 | Sub-A (#39) | 必須 |
| REQ-S03 | KDF（Argon2id） | パスワード → KEK_pw 導出（`m=19456, t=2, p=1`）、RFC 9106 KAT、CI criterion ベンチで p95 1 秒継続検証 | Sub-B (#40) | 必須 |
| REQ-S04 | KDF（BIP-39 + PBKDF2 + HKDF） | 24 語 → seed → KEK_recovery 導出（PBKDF2-HMAC-SHA512 2048iter + HKDF-SHA256 `info='shikomi-kek-v1'`）、trezor 公式ベクトル + RFC 5869 KAT | Sub-B (#40) | 必須 |
| REQ-S05 | AEAD（AES-256-GCM） | per-record 暗号化、AAD = record_id ‖ version ‖ created_at（26B）、random nonce 12B、上限 $2^{32}$ で `NonceLimitExceeded`、NIST CAVP テストベクトル | Sub-C (#41) | 必須 |
| REQ-S06 | 暗号化 Vault リポジトリ | `EncryptedSqliteVaultRepository` 実装、`VaultRepository` trait の暗号化モード経路、平文⇄暗号化双方向マイグレーション | Sub-D (#42) | 必須 |
| REQ-S07 | REQ-P11 解禁 | `vault-persistence/requirements.md` REQ-P11「暗号化モード即時拒否」を改訂、暗号化モード vault の load/save を解禁 | Sub-D (#42) | 必須 |
| REQ-S08 | パスワード強度ゲート | `vault encrypt` 入口で zxcvbn 強度 ≥ 3 を Fail Fast チェック、強度不足時は `Feedback`（warning + suggestions）を CLI/GUI に提示（Fail Kindly） | Sub-D (#42) | 必須 |
| REQ-S09 | VEK キャッシュ | daemon プロセス内 `secrecy::SecretBox<[u8;32]>`、アイドル 15min / スクリーンロック / サスペンドで `zeroize` | Sub-E (#43) | 必須 |
| REQ-S10 | マスターパスワード変更 O(1) | VEK 不変、`wrapped_VEK_by_pw` のみ再生成・置換、全レコード再暗号化なし | Sub-E (#43) | 必須 |
| REQ-S11 | アンロック失敗バックオフ | 連続失敗 5 回で `tokio::time::sleep` 指数バックオフ、ホットキー購読 blocking なし（プロセス全体は応答継続） | Sub-E (#43) | 必須 |
| REQ-S12 | IPC V2 拡張 | `IpcRequest::{Unlock, Lock, ChangePassword, RotateRecovery, Rekey}` 追加、`IpcProtocolVersion::V2` 非破壊昇格 | Sub-E (#43) | 必須 |
| REQ-S13 | リカバリ初回 1 度表示 | BIP-39 24 語の生成・表示は初回のみ、再表示不可、永続化しない（メモリゼロ化のみ）、ユーザ手書き保管前提 | Sub-D / Sub-E | 必須 |
| REQ-S14 | nonce overflow 検知 → rekey 強制 | 上限 $2^{32}$ 到達時 `NonceLimitExceeded` を返し、Sub-F の `vault rekey` で VEK 再生成 + 全レコード再暗号化 | Sub-C / Sub-F | 必須 |
| REQ-S15 | vault 管理サブコマンド | `shikomi vault {encrypt, decrypt, unlock, lock, change-password, recovery-show, rekey}` の CLI 実装、IPC V2 経由 | Sub-F (#44) | 必須 |
| REQ-S16 | 保護モード可視化 | `shikomi list` 出力ヘッダで `[plaintext]` / `[encrypted]` を常時表示（Fail Visible、`threat-model.md` §7.0 既定踏襲） | Sub-F (#44) | 必須 |
| REQ-S17 | Fail-Secure 型レベル強制 | `Verified<Plaintext>` newtype、`MasterPassword::new` の強度検証、`NonceCounter::increment` の `Result` 返却、`match` 暗号アーム第一パターン | Sub-A〜F 全 Sub 共通 | 必須 |

## Sub-issue分割計画

Epic #37 で **DAG: 0 → A → {B, C 並列} → D → E → F** として既に確定済み。本書（Sub-0）はその DAG の起点。

| Sub-issue | スコープ | 依存関係 |
|----------|---------|---------|
| **Sub-0 #38**（本書） | 脅威モデル文書化（L1〜L4 凍結 + REQ-S* id 採番） | なし（DAG 起点） |
| Sub-A #39 | shikomi-core 暗号ドメイン型（VEK/KEK/WrappedVek/MasterPassword/RecoveryMnemonic/KdfSalt/NonceCounter）+ ゼロ化契約 | Sub-0 |
| Sub-B #40 | shikomi-infra KDF アダプタ（Argon2id + BIP-39/PBKDF2/HKDF + KAT） | Sub-A |
| Sub-C #41 | shikomi-infra AEAD アダプタ（AES-256-GCM per-record + AAD + nonce 上限 + CAVP） | Sub-A |
| Sub-D #42 | shikomi-infra 暗号化 Vault リポジトリ + 平文⇄暗号化双方向マイグレーション + REQ-P11 解禁 | Sub-B, Sub-C |
| Sub-E #43 | shikomi-daemon VEK キャッシュ + IPC V2 拡張 | Sub-D |
| Sub-F #44 | shikomi-cli vault 管理サブコマンド | Sub-E |

## 非機能要求

要件詳細は `requirements.md` の REQ-S* 表で id 採番のみ確定し、本文は後続 Sub-A〜F の設計時に拡張する（本 Sub-0 のスコープは脅威モデルと骨格まで）。

| 区分 | 指標 | 目標 | 凍結根拠 |
|-----|------|------|---------|
| **機密性** | L1（同ユーザ別プロセス）に対する vault.db 平文化耐性 | AES-256-GCM 認証タグで改竄検出、強パスワード時 offline 復元不可 | 本書 §脅威モデル L1 |
| **機密性** | L3（物理ディスク奪取）に対する offline brute force 耐性 | Argon2id `m=19456, t=2, p=1` で 1 試行あたり数百ms〜1 秒のメモリハードコスト強制 | 本書 §脅威モデル L3、OWASP Password Storage Cheat Sheet、`tech-stack.md` §4.7 |
| **機密性** | L2（メモリスナップショット）に対する VEK 滞留時間 | unlock〜アイドル 15min 上限、`secrecy` + `zeroize` + best-effort `mlock` | 本書 §脅威モデル L2、`threat-model.md` §7 |
| **完全性** | vault.db 改竄検出 | 全レコードに AEAD 認証タグ、AAD で record_id/version/created_at 結合、ヘッダ独立 AEAD タグ | 本書 §脅威モデル L1 |
| **可用性** | unlock レイテンシ（p95） | 1 秒以下（3 OS 最低スペック相当、CI criterion ベンチで継続検証、逸脱はリリースブロッカ） | `tech-stack.md` §4.7 `argon2` 行 |
| **可用性** | nonce 衝突確率 | $\le 2^{-32}$（random nonce 96 bit + 上限 $2^{32}$ rekey、NIST SP 800-38D §8.3） | 本書 §脅威モデル L1、`tech-stack.md` §4.7 `aes-gcm` 行 |
| **可用性** | unlock 連続失敗時の応答性 | プロセス全体は blocking sleep しない、該当 IPC リクエストにのみ指数バックオフ（ホットキー購読継続） | `tech-stack.md` §2.4 / `threat-model.md` §8 A07 |
| **保守性** | 暗号 crate のサプライチェーン監査 | `cargo-deny` `unmaintained = "all"`、暗号クリティカル crate の `[advisories].ignore` 禁止、unmaintained 化したら代替移行 Issue 即発行 | `tech-stack.md` §4.3.2 / §4.7 末尾 |
| **保守性** | KDF パラメータ再評価サイクル | 4 年ごと、または CI ベンチが p95 1 秒を超えた時 | `tech-stack.md` §4.7 `argon2` 行、OWASP 推奨更新サイクル |
| **検証可能性** | 暗号アルゴリズム実装の正しさ | RFC 9106 Argon2id KAT、NIST CAVP AES-GCM、RFC 5869 HKDF、BIP-39 trezor 公式ベクトルを CI で実行 | `tech-stack.md` §4.7 各 crate 行 |

## 受入基準

本 Sub-0 (#38) の受入基準は**ドキュメント完成性**に閉じる。Epic #37 全体の受入は Sub-F 完了時に判定。

| # | 基準 | 検証方法 |
|---|------|---------|
| 1 | 本書 §脅威モデル に L1〜L4 capability matrix が完全に記載されている（能力 / 想定具体例 / STRIDE / 対策 / 残存リスク / 平文モード扱い / テスト観点 の 7 列を全 L1〜L4 で埋める） | レビュアー（ペテルギウス / ペガサス / 服部）の合格判定 |
| 2 | 保護資産が Tier-1〜3 で分類され、所在 / 寿命 / 漏洩時被害が明示されている | 同上 |
| 3 | 信頼境界が「内側 / 外側 / 横断ポイント」の 3 列で明示されている | 同上 |
| 4 | スコープ外（L4、同特権デバッガ、物理ハードウェア攻撃、量子攻撃、共有端末、侵害 OS、BIP-39 漏洩、画面共有、OS キーロガー、パスワード失念、バックアップ媒体）が**明示的に列挙**されている | 同上 |
| 5 | Fail-Secure 哲学が型レベル強制パターン（5 種以上）で言語化されている | 同上 |
| 6 | `requirements.md` に REQ-S01〜REQ-S17 の id が採番されており、各 REQ-S* がどの Sub に属するかが明示されている | レビュアーが requirements.md を読んで Sub マッピングを確認 |
| 7 | 後続 Sub-A〜F の設計者が「**この対策はどの脅威 ID 向け？**」を本書のみで逆引きできる | レビュアー（特に涅マユリ / 服部）の検証 |
| 8 | 既存 `threat-model.md` §7 / `tech-stack.md` §2.4 / §4.7 / §4.3.2 と内容が**矛盾しない**（本書は凍結値の整理であって変更ではない） | レビュアーの差分確認 |
| 9 | 外部レビュー（人間、まこちゃん）で「侵害された端末での挙動」「強パスワードでも brute force リスクが残る前提」「BIP-39 漏洩時の完全敗北」が**過信なく / 過小評価なく**伝わる | 外部レビュー結果 |

## 扱うデータと機密レベル

本書 §脅威モデル §3「保護資産インベントリ」を**正本**とする。本セクションでは要約のみ示す。

| 機密レベル | 含まれるデータ | 保護方針 |
|----------|------------|---------|
| **Tier-1（漏洩 = 全敗）** | マスターパスワード入力、リカバリニーモニック（24 語）、VEK、KEK_pw、KEK_recovery、平文レコード | プロセス内 RAM のみ、`secrecy` + `zeroize`、滞留時間最小化、ディスク永続化禁止 |
| **Tier-2（offline 解析の対象）** | `wrapped_VEK_by_pw`、`wrapped_VEK_by_recovery`、records.ciphertext | AEAD で保護、KDF（Argon2id / BIP-39+PBKDF2）の作業証明で offline brute force コストを強制 |
| **Tier-3（公開前提、改竄検出のみ）** | kdf_salt、per-record nonce、AAD（record_id/version/created_at）、kdf_params、vault_format_version | 平文保管、改竄は AEAD タグ / ヘッダ独立 AEAD タグで検出 → fail fast |
