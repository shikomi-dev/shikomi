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
| 本テスト設計のスコープ | **Sub-0 (#38) のみ** — 脅威モデル文書化（L1〜L4 凍結 + REQ-S* id 採番） |
| 対象 Issue | [#38](https://github.com/shikomi-dev/shikomi/issues/38) |
| 対象ブランチ | `feature/issue-38-threat-model` |
| 対象成果物 | `docs/features/vault-encryption/requirements-analysis.md` / `requirements.md` の 2 ファイル |
| 設計根拠 | `requirements-analysis.md` §受入基準 1〜9、`requirements.md` 機能要件 REQ-S01〜S17 採番 |
| テスト実行タイミング | `feature/issue-38-threat-model` → `develop` へのマージ前（外部レビュー承認前ゲート） |
| **本テスト設計の TC 総数** | **26 件**（ユニット相当 TC-DOC-U01〜U10: 10 件 + 結合相当 TC-DOC-I01〜I08: 8 件 + E2E 相当 TC-DOC-E01〜E08: 8 件） |

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
