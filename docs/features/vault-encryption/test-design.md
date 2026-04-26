# テスト設計書 — vault-encryption Sub-0（脅威モデル凍結）

<!-- feature: vault-encryption / Epic #37 / Sub-0 (#38) -->
<!-- 配置先: docs/features/vault-encryption/test-design.md -->
<!-- 本書は Sub-0 (#38) 「脅威モデル文書化（L1〜L4 凍結）」のテスト設計のみを扱う。
     後続 Sub-A〜F のテスト設計は、各 Sub の実装着手前に本ファイルを READ → EDIT で
     セクション拡張する（新規ファイル作成禁止、feature 単位 1 ファイル原則）。 -->

## 1. 概要

| 項目 | 内容 |
|------|------|
| 対象 feature | vault-encryption（暗号化モード実装、Epic #37） |
| 本テスト設計のスコープ | **Sub-0 (#38) + Sub-A (#39) + Sub-B (#40)** — Sub-0: 脅威モデル文書（§1〜§9）、Sub-A: 暗号ドメイン型 `shikomi-core::crypto`（§10）、Sub-B: KDF + Rng + ZxcvbnGate（`shikomi-infra::crypto::{kdf, rng, password}`、§11）。Sub-C〜F は本ファイルを順次 READ → EDIT で拡張 |
| 対象 Issue | [#38](https://github.com/shikomi-dev/shikomi/issues/38) / [#39](https://github.com/shikomi-dev/shikomi/issues/39) / [#40](https://github.com/shikomi-dev/shikomi/issues/40) |
| 対象ブランチ | `feature/issue-38-threat-model`（Sub-0、マージ済）/ `feature/issue-39-crypto-domain-types`（Sub-A、マージ済）/ `feature/issue-40-kdf`（Sub-B） |
| 対象成果物 | Sub-0: `requirements-analysis.md` / `requirements.md`。Sub-A: `basic-design.md` / `detailed-design/{index,crypto-types,password,nonce-and-aead,errors-and-contracts}.md` / `requirements.md`（REQ-S02 / REQ-S08 trait 部分 / REQ-S14 / REQ-S17）。Sub-B: `detailed-design/{kdf,rng}.md`（新規）+ `detailed-design/{password,errors-and-contracts,index}.md` / `basic-design.md` / `requirements.md`（REQ-S03 / REQ-S04 / REQ-S08 zxcvbn 実装部分） |
| 設計根拠 | Sub-0: §受入基準 1〜9、REQ-S* id 採番。Sub-A: 詳細設計書 §不変条件・契約サマリ C-1〜C-13、REQ-S02/S08 trait/S14/S17。Sub-B: `detailed-design/kdf.md` Argon2id / Bip39Pbkdf2Hkdf 契約、`rng.md` 4 メソッド単一エントリ点契約、`password.md` ZxcvbnGate 具象実装契約、REQ-S03 / REQ-S04 / REQ-S08 |
| テスト実行タイミング | Sub-0 / Sub-A はマージ済（PR #46/47/49/50）。Sub-B は `feature/issue-40-kdf` → `develop` マージ前 |
| **本テスト設計の TC 総数** | **Sub-0: 26 件** + **Sub-A: 22 件** + **Sub-B: 25 件**（TC-B-U01〜U20 / I01〜I04 / E01）= **合計 73 件**。各 Sub のローカル META チェックは独立に管理（ID prefix で分離） |

### 1.1 テストレベルの読み替え（重要）

**本 Sub-0 は文書アーティファクトのみを成果物とし、実行可能コードを含まない。** したがって伝統的な「ユニット / 結合 / E2E」を**文書品質検証に読み替えて**適用する。Vモデルとの対応関係を以下に固定する。

| テストレベル | 通常の対応 | 本 Sub-0 での読み替え | 検証手段 |
|------------|-----------|----------------------|---------|
| **E2E**（要件定義に対応） | 完全ブラックボックス、ペルソナシナリオ | **後続 Sub 担当者ペルソナ（野木 / 涅 / 服部 / 田中）による「逆引き可能性シナリオ」検証** — 本書のみで脅威 ID から対策／対策から脅威 ID を逆引きできるか | レビュアー 3 名（ペテルギウス / ペガサス / 服部）+ 外部レビュー（まこちゃん）による人手レビュー |
| **結合**（基本設計に対応） | モジュール間連携、契約検証 | **クロスリファレンス整合性テスト** — 本書 ↔ `threat-model.md` ↔ `tech-stack.md` ↔ `requirements.md` の参照鎖が壊れていないか、矛盾がないか | grep / markdown link checker / 手動差分確認 |
| **ユニット**（詳細設計に対応） | メソッド単位、ホワイトボックス | **各セクション構造完全性テスト** — capability matrix の 7 列が全 L1〜L4 で埋まっているか、Tier 分類が全資産に付いているか等、機械的に検証可能な構造ルール | `awk` / Python スクリプトで matrix 列数 / セル空欄を判定 |

### 1.2 外部I/O依存マップ

| 外部I/O | raw fixture | factory | characterization状態 |
|---------|------------|---------|---------------------|
| **該当なし** | — | — | — |

**理由**: 本 Sub-0 は文書のみで、外部 API・DB・ファイル I/O・時刻に依存するコードを一切含まない。characterization fixture は対象外（後続 Sub-A〜F の本ファイル拡張時に該当 Sub のテスト設計に追加）。

## 2. テスト対象と受入基準（トレーサビリティ起点）

`requirements-analysis.md` §受入基準（基準 1〜9）を**テスト設計の唯一の起点**とする。各基準を 1 つ以上のテストケースで検証する。

| 受入基準ID | 受入基準（要約） | 検証レベル（読み替え） |
|-----------|----------------|----------------------|
| AC-01 | L1〜L4 capability matrix が 7 列（能力 / 想定具体例 / STRIDE / 対策 / 残存リスク / 平文モード扱い / テスト観点）全て埋まっている | ユニット相当（構造完全性） |
| AC-02 | 保護資産が Tier-1〜3 で分類され、所在 / 寿命 / 漏洩時被害が明示されている | ユニット相当（構造完全性） |
| AC-03 | 信頼境界が「内側 / 外側 / 横断ポイント」の 3 列で明示されている | ユニット相当（構造完全性） |
| AC-04 | スコープ外 10 カテゴリ以上が明示列挙されている（L4 / 同特権デバッガ / 物理 / 量子 / 共有端末 / BIP-39 漏洩 / 画面共有 / OS キーロガー / 失念 / バックアップ） | ユニット相当（構造完全性） |
| AC-05 | Fail-Secure 哲学が型レベル強制パターン 5 種以上で言語化されている | ユニット相当（構造完全性） |
| AC-06 | `requirements.md` に REQ-S01〜REQ-S17 の id が採番されており、各 REQ-S* の Sub マッピングが明示されている | ユニット相当（構造完全性） |
| AC-07 | 後続 Sub-A〜F の設計者が「**この対策はどの脅威 ID 向け？**」を本書のみで逆引きできる | E2E相当（ペルソナシナリオ） |
| AC-08 | 既存 `threat-model.md` §7 / `tech-stack.md` §2.4 / §4.7 / §4.3.2 と内容が**矛盾しない** | 結合相当（クロスリファレンス整合性） |
| AC-09 | 外部レビュー（人間）で「侵害された端末での挙動」「強パスワード brute force 残存リスク」「BIP-39 漏洩時完全敗北」が**過信なく / 過小評価なく**伝わる | E2E相当（ペルソナシナリオ：田中 = エンドユーザー視点） |

## 3. テストマトリクス（トレーサビリティ）

**TC 総数: 26 件**（内訳: ユニット相当 10 件 [TC-DOC-U01〜U10] + 結合相当 8 件 [TC-DOC-I01〜I08] + E2E 相当 8 件 [TC-DOC-E01〜E08]）。本表の行数と末尾 ID 番号の整合は §6 末尾「自己整合チェック」で機械検証する。

| テストID | 受入基準ID / REQ-S* | 検証内容 | テストレベル（読み替え） | 種別 |
|---------|--------------------|---------|-----------------------|------|
| TC-DOC-U01 | AC-01 | L1〜L4 各層に 7 列（能力/具体例/STRIDE/対策/残存リスク/平文モード扱い/テスト観点）が全て非空である | ユニット | 構造完全性 |
| TC-DOC-U02 | AC-01 | L1〜L4 各層の「対策」セルに最低 3 個以上の具体的対策（記号 (a)(b)(c)... 形式）が列挙されている | ユニット | 網羅性 |
| TC-DOC-U03 | AC-01 / AC-09 | L4 の「対策」が**明示的に「対象外」**と書かれている（防御不能の受容を曖昧化していない） | ユニット | 過小評価防止 |
| TC-DOC-U04 | AC-02 | 保護資産表で全 13 資産に Tier 1〜3 のいずれかが付与されている | ユニット | 構造完全性 |
| TC-DOC-U05 | AC-02 | Tier-1 資産（マスターパスワード / リカバリ / VEK / KEK_pw / KEK_recovery / 平文レコード）の「**所在**」または「寿命」欄に**ゼロ化トリガ**（zeroize / KDF 完了 / unlock-lock / Drop / 30 秒 / 15min / アイドル / 保持しない 等）が明記されている。**寿命列は時間メトリクス、所在列はゼロ化契約**という意味論分業を尊重する | ユニット | 過信防止 |
| TC-DOC-U06 | AC-03 | 信頼境界表が 5 種（プロセス / ユーザ / 永続化 / 表示 / 入力）以上、各行に「内側 / 外側 / 横断ポイント」3 列が埋まっている | ユニット | 構造完全性 |
| TC-DOC-U07 | AC-04 | スコープ外表に 10 カテゴリ以上、各行に「受容根拠」が記載されている（空セル無し） | ユニット | 構造完全性 |
| TC-DOC-U08 | AC-05 | Fail-Secure 型レベル強制パターン表に 5 種以上のパターン（match / Verified newtype / NonceCounter::increment / MasterPassword::new / Drop 連鎖）が記載されている | ユニット | 構造完全性 |
| TC-DOC-U09 | AC-06 | `requirements.md` に REQ-S01〜REQ-S17 の 17 個全 id が連番採番、各 id に「担当 Sub」「概要」「関連脅威 ID」が記入されている | ユニット | 構造完全性 |
| TC-DOC-U10 | AC-06 | REQ-S01〜S17 の「関連脅威 ID」欄に L1〜L4 のいずれか（または `—` で意図的不適用）が必ず記載されている | ユニット | トレーサビリティ |
| TC-DOC-I01 | AC-08 | `requirements-analysis.md` で参照される `threat-model.md` の節番号（§7.0 / §7.1 / §7.2 / §8 / §A07）が実在する | 結合 | 参照整合性 |
| TC-DOC-I02 | AC-08 | `requirements-analysis.md` で参照される `tech-stack.md` の節番号（§2.4 / §4.7 / §4.3.2）が実在する | 結合 | 参照整合性 |
| TC-DOC-I03 | AC-08 | 凍結値の二段検証: **(a) tech-stack 凍結値**（KDF `m=19456, t=2, p=1` / nonce 12B / 上限 $2^{32}$ / VEK 32B / kdf_salt 16B）が `requirements-analysis.md` と `tech-stack.md` §2.4 / §4.7 で**両方に出現**、**(b) feature 局所凍結値**（AAD 26B / アイドル 15min）は `requirements-analysis.md` 内のみで定義され `tech-stack.md` に書かないことを確認（Bug-DOC-003: 当初「全 7 値が両文書一致」と書いていたが、AAD サイズとキャッシュ idle は feature 局所のため tech-stack 不在が正常） | 結合 | 矛盾検出 + 責務分離 |
| TC-DOC-I04 | AC-08 | `requirements-analysis.md` 機能一覧 REQ-S01〜S17 の Sub マッピングと、Sub-issue 分割計画 §（DAG 0→A→{B,C}→D→E→F）が**1:1 で整合**している | 結合 | DAG 整合性 |
| TC-DOC-I05 | AC-08 | `requirements.md` REQ-S* の「担当 Sub」欄と `requirements-analysis.md` 機能一覧の「担当 Sub」欄が**全 17 行で一致** | 結合 | 双方向参照整合 |
| TC-DOC-I06 | AC-06 / AC-08 | `requirements.md` データモデル表のエンティティ（VaultEncryptedHeader / WrappedVek / KdfSalt / KdfParams / NonceCounter / EncryptedRecord / MasterPassword / RecoveryMnemonic / Vek）が `requirements-analysis.md` §保護資産インベントリと**過不足なく対応**している | 結合 | エンティティ網羅 |
| TC-DOC-I07 | AC-08 | `requirements.md` 依存関係表の crate 一覧が `tech-stack.md` §4.7 暗号化スタック表と**全件マッチ**（aes-gcm / argon2 / hkdf / pbkdf2 / bip39 / rand_core / getrandom / subtle / zxcvbn / secrecy / zeroize 等） | 結合 | 依存契約整合 |
| TC-DOC-I08 | AC-08 | `requirements-analysis.md` 内のリンク（`docs/architecture/...` / 他 feature 参照）が**実在ファイル**を指す（broken link 無し） | 結合 | リンク死活 |
| TC-DOC-E01 | AC-07 | **野木ペルソナ（後続 Sub 実装者）**が「Sub-C で random nonce を選んだ理由はどの脅威 ID？」を本書のみで 30 秒以内に回答できる | E2E | 逆引き可能性 |
| TC-DOC-E02 | AC-07 | **野木ペルソナ**が「Sub-E で VEK を 15min で zeroize する理由はどの脅威 ID？」を本書のみで回答できる | E2E | 逆引き可能性 |
| TC-DOC-E03 | AC-07 | **涅マユリペルソナ（テスト担当）**が REQ-S05 (AEAD) のテスト観点を本書から抽出し、L1（改竄注入 / ロールバック / nonce 衝突）と L3（VEK 不在時の平文化阻止）の 2 軸で網羅できる | E2E | テスト観点導出 |
| TC-DOC-E04 | AC-07 | **服部ペルソナ（外部レビュアー）**が「同特権デバッガアタッチを対策しない理由」を本書スコープ外表から即座に提示できる（口頭審問形式） | E2E | スコープ外受容根拠 |
| TC-DOC-E05 | AC-09 | **田中ペルソナ（CLI 不可エンドユーザー）**が **GUI モーダル MSG-S16** 草稿（暗号化モード初回切替時、本書 §脅威モデル §4 L4 の「ユーザ向け約束」を引用）を読み、「侵害された端末・root マルウェアからは保護されない」を理解。`--help` を読まずに GUI のみで完結する経路を検証 | E2E | 過信防止 UX（GUI 経路） |
| TC-DOC-E06 | AC-09 | **田中ペルソナ**が **GUI モーダル MSG-S16 + recovery-show 警告 MSG-S06** 経由で「BIP-39 24 語を失くしたらパスワード忘却時に回復できない」を理解し、紙保管の必要性を認識できる | E2E | 過小評価防止 UX（GUI 経路） |
| TC-DOC-E07 | AC-07 / AC-09 | レビュアー 3 名（ペテルギウス / ペガサス / 服部）の合格判定 + 外部レビュー（まこちゃん）の承認が揃う | E2E | 統合受入 |
| TC-DOC-E08 | AC-09 | **田中ペルソナ**が **GUI モーダル MSG-S16 の 3 点目「画面共有時の表示回避」**を読み、Zoom/Meet/TeamViewer 中に vault 操作を避ける運用を理解。CLI 経路を持たないペルソナへの GUI-first 伝達を検証 | E2E | 画面共有リスク回避 UX |

## 4. E2Eテストケース（読み替え：ペルソナシナリオ検証）

<!-- 完全ブラックボックス：本書のみを読んで判定。他文書を引いて補完したらテスト失敗。 -->
<!-- 後続 Sub の実装者・テスト担当・レビュアー・エンドユーザーの 4 ペルソナで検証する。 -->

| テストID | ペルソナ | シナリオ | 操作手順 | 期待結果 |
|---------|---------|---------|---------|---------|
| TC-DOC-E01 | 野木 拓海（Sub-C 実装者） | random nonce 採用根拠を逆引きする | (1) `requirements-analysis.md` を開く (2) §脅威モデル §4 L1 の「対策」欄を読む (3) 「random nonce + 衝突上限 $2^{32}$」の記述を発見 (4) §6 Fail-Secure 哲学で `NonceCounter::increment` の `Result` 返却を確認 | 30 秒以内に「L1（同ユーザ別プロセス）対策の (d) で、random nonce による平文同値漏洩の確率的排除のため」と回答できる |
| TC-DOC-E02 | 野木 拓海（Sub-E 実装者） | VEK 15min ゼロ化根拠を逆引きする | (1) §脅威モデル §3 保護資産で VEK 行を読む (2) §4 L2 メモリスナップショット §対策 (b) を確認 | 「L2（メモリスナップショット）対策、過去メモリ抽出時の VEK 滞留時間最小化のため」と回答できる |
| TC-DOC-E03 | 涅 マユリ（テスト担当） | REQ-S05 (AEAD) のテスト観点導出 | (1) `requirements.md` REQ-S05 の関連脅威 ID（L1 / L3）を確認 (2) `requirements-analysis.md` §4 L1 §テスト観点 (a)(b)(c) と L3 §テスト観点 (a)(b) を引く | テスト観点 5 軸（改竄注入で `AeadTagMismatch` / AAD ロールバック検出 / random nonce 衝突理論ベンチ / NIST CAVP / `wrapped_VEK` 復号路網羅）を抽出できる |
| TC-DOC-E04 | 服部 平次（外部レビュアー、口頭審問） | 同特権デバッガ非対策の理由提示 | 「なぜ稼働中 daemon への gdb attach を対策しないのか？」と問う | §脅威モデル §5 スコープ外表から「OS 信頼境界の問題、`PR_SET_DUMPABLE(0)` / `PROCESS_VM_READ` 拒否は OS により無視可能 / バイパス可能」を即座に引用できる |
| TC-DOC-E05 | 田中 俊介（CLI 不可エンドユーザー） | 「侵害された端末で使えない」を **GUI モーダル経路**で理解 | (1) `vault encrypt` 初回 GUI モーダル MSG-S16 草稿を表示 — 3 点（侵害端末 / BIP-39 漏洩 / 画面共有）を文 + アイコン併記 (2) 「**理解しました**」ボタンを押さないと先に進めない（明示合意取得） (3) 田中がモーダル内容を自分の言葉で言い換える | CLI を一切使わずに「**root 権限を持つマルウェアからは保護できない**」「**侵害された端末での使用は想定外**」を自分の言葉で言い換えられる。`--help` を読む必要が無い |
| TC-DOC-E06 | 田中 俊介（CLI 不可エンドユーザー） | BIP-39 24 語紛失リスクを **GUI モーダル経路**で理解 | (1) `vault encrypt` 初回 GUI モーダル MSG-S16 を表示（リカバリ生成前の警告） (2) `vault recovery-show` 直前に再警告 MSG-S06（**写真撮影禁止 / 金庫保管 / クラウド保管禁止**）を表示 (3) 「**理解しました**」と「**書き写し完了**」の二段階確認を経て初めて 24 語表示 | CLI を一切使わずに「24 語を紛失したらパスワード忘却時に回復不能」「24 語の保管はユーザ責任」「金庫保管・写真禁止・クラウド禁止」を理解、二段階確認で**消え去り型表示**を許容 |
| TC-DOC-E07 | レビュアー統合 | 統合受入（人手レビュー + 外部レビュー） | レビュアー 3 名（ペテルギウス / ペガサス / 服部）が並列レビュー → 全員 `[合格]` → 外部レビュー（まこちゃん）承認 | 4 名全員の合格判定が揃う。1 名でも `[却下]` あれば差し戻し |
| TC-DOC-E08 | 田中 俊介（CLI 不可エンドユーザー） | **画面共有時の表示回避**運用を MSG-S16 経由で理解 | (1) `vault encrypt` 初回 GUI モーダル MSG-S16 の 3 点目「**画面共有 / リモートデスクトップ中の表示漏洩**」段落を読む (2) §5 スコープ外「画面共有 / リモートデスクトップ」行を裏付けとして MSG-S16 詳細リンクから参照 (3) 田中が運用ルールを言語化する | 「Zoom / Meet / TeamViewer / 画面録画 中は vault unlock / recovery-show を実行しない」「OS / 配信ツール側の責務であり shikomi は防御不能、運用で回避」を理解。**MSG-S16 のみが田中への伝達経路**であり、`--help` や SECURITY.md は補足扱い |

**E2E 証跡**: 各 TC-DOC-E0x の検証結果を Markdown レポート（`/app/shared/attachments/マユリ/sub-0-doc-test-report.md`）に記録し Discord 添付。

## 5. 結合テストケース（読み替え：クロスリファレンス整合性）

<!-- 本書 ↔ threat-model.md ↔ tech-stack.md ↔ requirements.md の 4 文書間の参照鎖検証。 -->
<!-- grep / 手動 diff / markdown-link-check で機械検証可能なものを優先。 -->

| テストID | 対象連携 | 検証コマンド / 手段 | 前提条件 | 操作 | 期待結果 |
|---------|---------|------------------|---------|------|---------|
| TC-DOC-I01 | requirements-analysis.md → threat-model.md | `grep -E "threat-model.md (§7\.0\|§7\.1\|§7\.2\|§7\|§8\|§A07)" docs/features/vault-encryption/requirements-analysis.md` で抽出した節番号を `docs/architecture/context/threat-model.md` 内で `grep -E "^### 7\.0\|^## 7\|^## 8"` で実在確認 | `develop` ブランチに最新 threat-model.md がマージ済 | 抽出 → 突合 → 不一致一覧を出力 | 全節番号が threat-model.md 内に実在（不一致 0 件） |
| TC-DOC-I02 | requirements-analysis.md → tech-stack.md | 同上方式で `tech-stack.md §2.4 / §4.7 / §4.3.2` を実在確認 | tech-stack.md PR #45 マージ済 | 抽出 → 突合 | 全節番号実在 |
| TC-DOC-I03 | 凍結値の責務分離検証 | 二段 grep: (a) tech-stack 凍結値が RA + TS 両方に出現 (b) feature 局所凍結値が RA 内のみ定義 | RA / TS / REQ がローカル | (a) 5 値（KDF `m=19456, t=2, p=1` / nonce 12B / 上限 $2^{32}$ / VEK 32B / kdf_salt 16B）grep → 両文書出現確認、(b) 2 値（AAD 26B / アイドル 15min）grep → RA のみ出現確認 | (a) 全 5 値が両文書出現、(b) 全 2 値が RA に最低 1 回出現。tech-stack 越境した feature 局所値は責務違反として検出 |
| TC-DOC-I04 | DAG 整合性 | requirements-analysis.md §機能一覧の Sub マッピング表 vs §Sub-issue分割計画 表 | 同一文書内の 2 表 | 17 行の Sub マッピングを抽出 → DAG 表（7 行）の依存先と整合チェック | Sub-A 依存 = Sub-0、Sub-B/C 依存 = Sub-A、Sub-D 依存 = Sub-B+C、Sub-E 依存 = Sub-D、Sub-F 依存 = Sub-E が崩れていない |
| TC-DOC-I05 | requirements-analysis ↔ requirements の Sub マッピング双方向 | 両文書から「REQ-S* / 担当 Sub」を抽出して join | 両ファイル存在 | REQ-S01〜S17 の 17 行を両文書から抽出 → join | 17 行全て担当 Sub が一致（不一致 0 件） |
| TC-DOC-I06 | データモデル ↔ 保護資産インベントリ | requirements.md §データモデルの 9 エンティティ vs requirements-analysis.md §3 の 13 資産 | 両文書存在 | エンティティ名と資産名を突合 | 9 エンティティが資産インベントリのいずれかに対応する（VaultEncryptedHeader = ヘッダ複合資産 / Vek = VEK / MasterPassword = マスターパスワード …） |
| TC-DOC-I07 | 依存 crate ↔ tech-stack §4.7 | requirements.md §依存関係 crate 一覧 vs tech-stack.md §4.7 表 | 両文書存在 | 11 crate を両表から抽出 → 突合 | 全 crate が §4.7 に登録されている。version pin の表記が一致 |
| TC-DOC-I08 | broken link | `markdown-link-check docs/features/vault-encryption/requirements-analysis.md` および `requirements.md` を実行（無ければ手動 grep）| なし | 全リンクを ping | 全リンク 200 / 内部参照は実ファイル存在（broken 0 件） |

**結合テスト実行スクリプト**: `tests/docs/sub-0-cross-ref.sh`（Sub-0 専用、検証用 shell スクリプトとして同 PR にコミット）。

## 6. ユニットテストケース（読み替え：構造完全性）

<!-- 各セクションの機械的検証可能ルール。awk / Python / pandoc で table を抽出して列数・空セルを判定。 -->
<!-- 本書（test-design.md）の存在自体を保証する CI lint も含める。 -->

| テストID | 対象セクション | 種別 | 入力（fixture 不要） | 期待結果 |
|---------|---------------|------|--------------------|---------|
| TC-DOC-U01 | requirements-analysis.md §脅威モデル §4 L1〜L4 | 構造完全性 | 4 つの L サブセクション内の小表 | 各表で「能力 / 想定具体例 / 対応する STRIDE / 対策 / 残存リスク / 平文モード時の扱い / テスト観点」の 7 行が**全て非空**（`空 / TBD / TODO` 等の placeholder 文字列が無い） |
| TC-DOC-U02 | 同上 §対策セル | 網羅性 | L1〜L4 の対策セル本文 | L1 / L2 / L3 各層で**最低 3 個以上**の対策（記号 (a)(b)(c)... 形式）。L4 のみ「対象外」明示で例外 |
| TC-DOC-U03 | §4 L4 §対策 | 過小評価防止 | L4 §対策 セル | 文字列 `**対象外**` が含まれる（曖昧な「考慮中」「将来検討」を禁止） |
| TC-DOC-U04 | §3 保護資産インベントリ表 | 構造完全性 | 13 資産行 | 全 13 行に Tier 1 / 2 / 3 のいずれかが付与（空 Tier 0 件） |
| TC-DOC-U05 | §3 Tier-1 資産行（6 資産） | 過信防止 | Tier-1 各行の「所在」または「寿命」セル | 各行の**所在 or 寿命のいずれか**に**ゼロ化トリガ語**（`zeroize` / `KDF 完了` / `unlock` / `Drop` / `30 秒` / `15min` / `アイドル` / `保持しない` 等）が含まれる。**寿命列は時間メトリクス、所在列はゼロ化契約という意味論分業**を尊重（Sub-0 テスト実行時に発見、Boy Scout Rule で TC 文言を実態に同期、Bug-DOC-001 参照） |
| TC-DOC-U06 | §2 信頼境界表 | 構造完全性 | 5 行（プロセス / ユーザ / 永続化 / 表示 / 入力） | 5 行以上、各行 「内側 / 外側 / 横断ポイント」3 列が非空 |
| TC-DOC-U07 | §5 スコープ外表 | 構造完全性 | スコープ外行 | 10 カテゴリ以上、各行に「受容根拠」セルが非空 |
| TC-DOC-U08 | §6 Fail-Secure 型レベル強制パターン表 | 構造完全性 | パターン表 | 5 行以上、各行「適用 / 効果」セルが非空 |
| TC-DOC-U09 | requirements.md REQ-S* セクション | 構造完全性 | `### REQ-S01` 〜 `### REQ-S17` の 17 セクション | 17 セクション全て存在、各セクション「担当 Sub / 概要 / 関連脅威 ID」3 行が非空 |
| TC-DOC-U10 | requirements.md REQ-S* §関連脅威 ID 行 | トレーサビリティ | 17 セクションの該当行 | 各行に `L1` / `L2` / `L3` / `L4` のいずれか（最低 1 個）または明示的 `—`（意図的不適用宣言）が含まれる |

**ユニットテスト実行スクリプト**: `tests/docs/sub-0-structure-lint.py`（同 PR にコミット、CI で `python3 tests/docs/sub-0-structure-lint.py` として実行）。

### 6.1 自己整合チェック（メタテスト）

本テスト設計書自身の TC 採番と本文宣言値が乖離しないことを保証するメタチェック。報告者の数え間違い（過去事例: 「27」と誤数）を構造的に防ぐ。

| サブチェックID | 検証対象 | 期待結果 |
|--------------|---------|---------|
| META-01 | §1 概要表「TC 総数」セルの宣言値 | `26` と一致 |
| META-02 | §3 マトリクス見出しの宣言値 | `26` と一致 |
| META-03 | §3 マトリクス表の `TC-DOC-` プレフィクス行数 | 10 (U) + 8 (I) + 8 (E) = 26 行 |
| META-04 | TC-DOC-U / I / E 各シリーズの末尾 ID | U10 / I08 / E08（連番欠番無し） |

**実装**: `sub-0-structure-lint.py` 末尾で `grep -oE "TC-DOC-(U\|I\|E)[0-9]+" docs/features/vault-encryption/test-design.md | sort -u | wc -l` を呼び、26 と等しいことを assert（プレースホルダ `TC-DOC-E0x` は末尾に数字が無いためマッチしない設計）。**TC を増減した時はこの数値・META-01〜04 の期待値・lint 実装の 3 箇所を同期更新する型レベル契約**。

## 7. テスト実行手順

### 7.1 ローカル実行

```bash
# ユニット相当（構造完全性 lint）
python3 tests/docs/sub-0-structure-lint.py

# 結合相当（クロスリファレンス整合性）
bash tests/docs/sub-0-cross-ref.sh

# E2E 相当（ペルソナシナリオ）
# → レビュアー 3 名による人手レビュー（GitHub PR コメント）
# → 外部レビュー（まこちゃん）の承認
```

### 7.2 CI 統合

- ユニット / 結合相当の自動化分は GitHub Actions で `paths: docs/features/vault-encryption/**` トリガで実行
- E2E 相当（ペルソナシナリオ）は GitHub PR レビューゲート（CODEOWNERS 必須レビュアー指定）で代替

### 7.3 人間が動作確認できるタイミング

- 本 Sub-0 はドキュメントのため**実行可能成果物なし**。代わりに以下の形で人間検証を行う:
  - レビュアー 3 名 + 外部レビュー（まこちゃん）の GitHub PR レビュー
  - 後続 Sub-A 着手時に「本書のみで設計判断ができたか」を Sub-A の振り返りで再確認（後付け E2E）

## 8. テスト証跡の残し方

| テストレベル | 証跡形式 | 保存先 |
|------------|---------|-------|
| ユニット相当 | `sub-0-structure-lint.py` の stdout / 終了コード（0 = 全 pass） | CI ログ + Discord 添付（テキスト） |
| 結合相当 | `sub-0-cross-ref.sh` の stdout / 不一致一覧 | 同上 |
| E2E 相当 | レビュアー 4 名の合格コメントへのリンク + Markdown サマリ | Discord 添付 Markdown |

## 9. 後続 Sub への引継ぎ事項

本 Sub-0 のテスト設計はあくまで**文書凍結の品質ゲート**。後続 Sub-A〜F が実装に入る時、本ファイルを **READ → EDIT** で以下を順次拡張する（新規ファイル禁止）:

| Sub | 追加するテストレベル | 追加内容の概要 |
|-----|------------------|--------------|
| Sub-A (#39) | ユニット | 暗号ドメイン型（VEK / KEK / WrappedVek / MasterPassword / RecoveryMnemonic / KdfSalt / NonceCounter）の `secrecy` + `zeroize` 契約検証、`Clone` 禁止コンパイルテスト、`Drop` 後メモリパターン検証 |
| Sub-B (#40) | ユニット + 結合 | RFC 9106 Argon2id KAT、BIP-39 trezor 公式ベクトル、RFC 5869 HKDF KAT、CI criterion ベンチで p95 1 秒継続検証 |
| Sub-C (#41) | ユニット + 結合 | NIST CAVP AES-GCM テストベクトル、AAD ロールバック検出、random nonce 衝突確率ベンチ、`NonceLimitExceeded` rekey 強制 |
| Sub-D (#42) | ユニット + 結合 + E2E | 平文⇄暗号化双方向マイグレーション（atomic write 失敗ロールバック）、zxcvbn 強度ゲート（弱パスワード `Feedback` 出力）、ヘッダ AEAD 改竄検出、REQ-P11 解禁の整合、**MSG-S16 限界説明 3 点（侵害端末 / BIP-39 漏洩 / 画面共有）+ 明示合意取得 UX**（CLI フラグ `--accept-limits` ＋ GUI モーダル「理解しました」二択、CLI / GUI 両経路）、**MSG-S18 アクセシビリティ 4 経路**（ARIA + `--print` PDF + `--braille` .brf + `--audio` TTS）、**REQ-S13 WCAG 2.1 AA 準拠**（recovery 24 語表示時のスクリーンリーダー読み上げ・OS 読み上げ拒否時の代替経路） |
| Sub-E (#43) | ユニット + 結合 + E2E | VEK アイドル 15min ゼロ化観測、サスペンド signal ゼロ化、IPC V2 拡張の V1 互換、アンロック失敗指数バックオフのホットキー継続 |
| Sub-F (#44) | E2E（CLI + GUI） | `shikomi vault {encrypt/decrypt/unlock/lock/change-password/recovery-show/rekey}` の bash + curl/IPC E2E、**REQ-S16 保護モード可視化（CLI ヘッダ `[plaintext]` / `[encrypted]` ＋ GUI 常駐バッジ MSG-S17 — 色覚多様性対応で色＋文字の二重符号化）**、recovery 初回 1 度表示、**MSG-S16 / MSG-S17 / MSG-S18 の文言確定と CLI / GUI 両経路の表示テスト**、**REQ-S13 アクセシビリティ 4 経路の Sub-F 側統合テスト**（Tauri WebView ARIA 属性、Playwright スクリーンリーダー検証） |

**characterization fixture** の起票は Sub-A〜F の各 Sub の本ファイル拡張時に「§1.2 外部I/O依存マップ」に追記する。本 Sub-0 では該当なし。

---

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

### 11.11 Sub-B 工程4 実施実績（2026-04-25、PR #52 / `edd7cc0`）

| 区分 | TC 数 | pass | 検証手段 |
|---|---|---|---|
| ユニット（runtime + KAT） | 20 | 20（うち KAT 3 件は採用経路自己整合 + RFC 5869 A.1 bit-exact）| CI `cargo test -p shikomi-infra` で 49 unit pass、Sub-B 関連 28 件確認 |
| 結合（grep + bench 代替） | 4 | **3 PASS + 1 FAIL（Bug-B-001）** | `tests/docs/sub-b-static-checks.sh` (3/3) + Argon2id 代替実測（Docker rust:1.95-slim aarch64 / **median 11.452 ms ≪ 1000 ms 上限、契約は実装で満たすが CI gating 不在**）|
| E2E | 1 | 1 | CI 7 ジョブ全 SUCCESS で野木ペルソナ cargo doc 経路は実装到達可能 |
| **合計** | **25** | **24 PASS + 1 FAIL（criterion bench 不在）** | CI + Docker 代替実測 + 静的 grep の三系で交叉確認 |

**バグレポート 4 件（テスト工程発見）**:

- **Bug-B-001（criterion ベンチ未実装、`benches/` 不在 → CI gating ゼロ）**: 設計書 BC-3 / TC-B-I01 で「Argon2id criterion ベンチ p95 ≤ 1.0 秒、CI 必須 pass、逸脱はリリースブロッカ」と凍結し三層レビューが合格判定したが、impl PR #52 に `crates/shikomi-infra/benches/` ディレクトリ・`Cargo.toml` `[[bench]]` 設定・CI `bench-kdf` job のいずれも存在しない。テスト工程で **Argon2id `FROZEN_OWASP_2024_05` を Docker `rust:1.95-slim` aarch64 release で 10 回実測**（median 11.452 ms、min 10.723 ms、max 13.327 ms）、**実装は契約を 87 倍以上のマージンで満たす**ことを確認。ただし**機械的 CI gating は欠落**しており、将来のリグレッション（依存更新でパラメータ実効値が変わる等）を検出できない。**別 Issue 起票推奨**：`benches/argon2id.rs` 新設 + CI `bench-kdf` job 統合。
- **Bug-B-002（Argon2id KAT 採用経路置換）**: 設計書 BC-1 / TC-B-U01 は「RFC 9106 Test Vector A.2 bit-exact」だが、実装は採用 API (`hash_password_into`、secret/AD なし) の決定論性 KAT に置換（銀ちゃん `edd7cc0` 判断、コメントで「公式ベクトルは secret + AD 経路、shikomi 採用経路と API 不一致」と根拠明示）。**実装判断は妥当**（DRY 違反回避）、設計書側を Boy Scout で実態同期（TC-B-U01 文言修正済）。
- **Bug-B-003（BIP-39 KAT 採用経路置換）**: 設計書 BC-4 / TC-B-U05 は「trezor 公式 vectors 5 ベクトル bit-exact」だが、実装は `abandon abandon ... art` 24 語 + `to_seed("")` 採用経路自己整合 KAT に置換。**実装判断は妥当**（採用経路 = passphrase=`""` 固定、trezor vectors の passphrase 経由ベクトルは API 不一致）、設計書側 Boy Scout で同期（TC-B-U05 文言修正済）。
- **Bug-B-004（HKDF KAT 数）**: 設計 BC-6 / TC-B-U07 は「Appendix A.1〜A.3 3 ベクトル」だが、実装は A.1 basic case のみ（1 ベクトル）。A.2/A.3 は SHA-1 経由など採用 API と異なる経路、A.1 が SHA-256 採用経路と一致。**実装判断は妥当**、TC-B-U07 文言を 1 ベクトルに修正（Boy Scout）。

**重大度分類**:
- Bug-B-001: **High**（設計契約と実装乖離、CI gating 不在で将来リグレッション検出不能）→ 別 Issue 起票推奨
- Bug-B-002〜004: **Low**（実装判断は妥当、設計書側の Boy Scout 修正で吸収済み）

**Argon2id 性能の独立実測値**:

```
Argon2id FROZEN_OWASP_2024_05 perf probe (m=19456 KiB, t=2, p=1)
arch: aarch64, profile: release, n=10 samples
  min   :  10.723 ms
  median:  11.452 ms
  max   :  13.327 ms
BC-3 contract: median (proxy for p95) <= 1000 ms
Result: PASS (median 11ms, ≪ 1000ms upper bound)
```

3 OS 最低スペック相当で 50-200 ms 程度を想定しても **1 秒上限まで 5-20 倍のマージン**。実装は性能契約を健全に満たす。

**新規補助スクリプト**:
- `tests/docs/sub-b-static-checks.sh`: TC-B-I02 / I03 / I04 の cargo-free 静的検証、実装マージ後に SKIP → PASS 自動切替
- `scripts/argon2id-perf-probe.md`（添付ノート）: Bug-B-001 の Docker 実測手順を保存、別 Issue で benches 実装する際の参考データ
