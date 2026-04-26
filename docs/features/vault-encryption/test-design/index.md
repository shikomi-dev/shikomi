# テスト設計書 — vault-encryption（インデックス）

<!-- feature: vault-encryption / Epic #37 -->
<!-- 配置先: docs/features/vault-encryption/test-design/index.md -->
<!-- 本ディレクトリは Sub-D (#42) 工程5 ペガサス指摘で `test-design.md`（1005 行、500 行ルール大幅違反）を分割した結果。
     Sub-E〜F のテスト設計は、各 Sub の工程2 でこのディレクトリ内の対応ファイルを新規作成または既存ファイルを READ → EDIT する。 -->

## 分冊構成

| 分冊 | 対象 | TC ID prefix |
|---|---|---|
| [`sub-0-threat-model.md`](./sub-0-threat-model.md) | Sub-0 (#38) 脅威モデル文書化 + 共通受入基準 §1〜§9 | `TC-DOC-{U,I,E}*` |
| [`sub-a-crypto-types.md`](./sub-a-crypto-types.md) | Sub-A (#39) 暗号ドメイン型（`shikomi-core::crypto`） | `TC-A-{U,I,E}*` |
| [`sub-b-kdf-rng-zxcvbn.md`](./sub-b-kdf-rng-zxcvbn.md) | Sub-B (#40) KDF + Rng + ZxcvbnGate | `TC-B-{U,I,E}*` |
| [`sub-c-aead.md`](./sub-c-aead.md) | Sub-C (#41) AEAD アダプタ + AeadKey trait | `TC-C-{U,I,P,E}*` |
| [`sub-d-repository-migration.md`](./sub-d-repository-migration.md) | Sub-D (#42) 暗号化 Vault リポジトリ + マイグレーション + 横断 REQ-P11 改訂 | `TC-D-{U,I,P,E,S}*` |
| [`sub-e-vek-cache-ipc.md`](./sub-e-vek-cache-ipc.md) | Sub-E (#43) VEK キャッシュ + IPC V2 拡張 + 横断 daemon-ipc V2 ラウンドトリップ | `TC-E-{U,I,P,E,S}*` |
| [`sub-f-cli-subcommands.md`](./sub-f-cli-subcommands.md)（Sub-F 新規、テスト担当が工程2 後半で作成）| Sub-F (#44) shikomi-cli vault サブコマンド + 既存 CRUD ロック時挙動 + 保護モードバナー + アクセシビリティ + TC-E-E01 田中ペルソナ E2E（Sub-E 凍結シナリオの実機完走）| `TC-F-{U,I,E,S}*` |

## 共通方針

- **Sub-0 範囲（共通受入基準・テストレベル読み替え・E2E ペルソナ等）**は `sub-0-threat-model.md` を**正本**とする
- 後続 Sub-A〜D はそれぞれの分冊で「テストレベル読み替え（Sub 固有）」「受入基準（Sub 固有 BC-* / CC-* / DC-*）」「TC マトリクス」「実行手順」「証跡」「後続引継ぎ」を完備する
- TC ID prefix は **Sub 単位で物理分離**（`TC-A-` / `TC-B-` / `TC-C-` / `TC-D-` / `TC-DOC-`）。ID 重複ゼロ
- META チェック（TC 件数 assert）は各分冊が独立に管理（lint スクリプトもファイル単位で参照）

## TC 総数（Sub-F 工程2 追加後、テスト担当が確定）

| Sub | TC 数 | 分冊 |
|---|---|---|
| Sub-0 | 26 | `sub-0-threat-model.md` |
| Sub-A | 22 | `sub-a-crypto-types.md` |
| Sub-B | 25 | `sub-b-kdf-rng-zxcvbn.md` |
| Sub-C | 26 | `sub-c-aead.md` |
| Sub-D | 26 | `sub-d-repository-migration.md` |
| Sub-E | 27 | `sub-e-vek-cache-ipc.md` |
| Sub-F | TBD by テスト担当（工程2 後半）| `sub-f-cli-subcommands.md`（新規、テスト担当が REQ-S15/S16 + `detailed-design/cli-subcommands.md` を入力に作成）|
| **合計** | **TBD** | — |

**静的検査（grep gate）**: Sub-D 7 件（TC-D-S01..S07）+ Sub-E 8 件（TC-E-S01..S08、Bug-E-001 解決経路 + cache_relocked seam 含む）+ **Sub-F TBD**（C-37 隠蔽オプション禁止 grep gate / clap 派生型 8 variant 網羅 grep / `--no-mode-banner` 不在 grep 等、テスト担当が確定）。Sub-D Rev3〜Rev4 で凍結した「実装直読 + grep gate」原則を Sub-F に継承し、5 度目以降の同型ドリフトを構造封鎖。

## 関連スクリプト

`tests/docs/` 配下の lint / cross-ref / static-checks スクリプトの参照パスは本分割と同期更新済（Sub-D 工程5 Rev1）:

- `sub-0-structure-lint.py`: `test-design/sub-0-threat-model.md` を対象
- `sub-0-cross-ref.sh`: `test-design/` 配下全ファイルを参照
- `sub-{a,b,c,d,e}-static-checks.sh`: 各 Sub の対応分冊を参照
- `sub-f-static-checks.sh`: Sub-F 新規（テスト担当が工程2 後半で作成）。clap 派生型 8 variant 網羅 / 隠蔽オプション禁止（`--no-mode-banner` 不在）/ `presenter::recovery_disclosure` 1 度表示の構造防衛 / i18n 辞書キー命名規則 grep gate
