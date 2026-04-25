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
| 本テスト設計のスコープ | **Sub-0 (#38) + Sub-A (#39)** — Sub-0: 脅威モデル文書（§1〜§9）、Sub-A: 暗号ドメイン型 `shikomi-core::crypto`（§10〜§16）。Sub-B〜F は本ファイルを順次 READ → EDIT で拡張 |
| 対象 Issue | [#38](https://github.com/shikomi-dev/shikomi/issues/38) / [#39](https://github.com/shikomi-dev/shikomi/issues/39) |
| 対象ブランチ | `feature/issue-38-threat-model`（Sub-0、マージ済）/ `feature/issue-39-crypto-domain-types`（Sub-A） |
| 対象成果物 | Sub-0: `requirements-analysis.md` / `requirements.md`。Sub-A: `basic-design.md` / `detailed-design.md` / `requirements.md`（REQ-S02 / REQ-S08 trait 部分 / REQ-S14 / REQ-S17 確定） |
| 設計根拠 | Sub-0: §受入基準 1〜9、REQ-S* id 採番。Sub-A: 詳細設計書 §不変条件・契約サマリ C-1〜C-13、REQ-S02/S08 trait/S14/S17 |
| テスト実行タイミング | Sub-0 は外部レビュー承認前ゲートでマージ済（PR #46/47）。Sub-A は `feature/issue-39-crypto-domain-types` → `develop` マージ前 |
| **本テスト設計の TC 総数** | **Sub-0: 26 件**（TC-DOC-U01〜U10 / I01〜I08 / E01〜E08）+ **Sub-A: 22 件**（TC-A-U01〜U18 / I01〜I03 / E01）= **合計 48 件**。各 Sub のローカル META チェックは独立に管理（ID prefix で分離） |

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
| 対象成果物 | `basic-design.md` / `detailed-design.md`（新規）/ `requirements.md`（REQ-S02 / REQ-S08 trait 部分 / REQ-S14 / REQ-S17 EDIT） |
| 設計根拠 | `detailed-design.md` §不変条件・契約サマリ C-1〜C-13、§Sub-A クラス・関数仕様 |
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

`detailed-design.md` §不変条件・契約サマリの **C-1〜C-13** を**テスト設計の起点**とする。各契約を 1 つ以上の TC で検証。さらに REQ-S02 / REQ-S08 trait 部分 / REQ-S14 / REQ-S17 の機能要件もマトリクスに紐付ける。

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
| TC-A-U18 | REQ-S17 | `CryptoOutcome<T>` enum バリアント **5 件**（`detailed-design.md` §CryptoOutcome と完全一致）: `TagMismatch` / `NonceLimit` / `KdfFailed(KdfErrorKind)` / `WeakPassword(WeakPasswordFeedback)` / `Verified(Verified<T>)`。`match` 強制で fall-through なし、`#[non_exhaustive]` で外部 crate からの追加に破壊的変更耐性 | ユニット | enum 網羅 |
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
| TC-A-U18 | `CryptoOutcome<T>` `match` 網羅 | enum 網羅 | **5 バリアント全列挙**: `TagMismatch` / `NonceLimit` / `KdfFailed(KdfErrorKind)` / `WeakPassword(WeakPasswordFeedback)` / `Verified(Verified<T>)`（`detailed-design.md` §CryptoOutcome と完全一致） | `match` で 5 アーム全網羅、いずれか省略すると `non_exhaustive_patterns` 警告。`#[non_exhaustive]` 属性により外部 crate 側の `match` は wildcard `_` 必須で破壊的変更耐性 |

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
