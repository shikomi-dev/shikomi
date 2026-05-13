# 基本設計書 — vault-encryption（インデックス）

<!-- 詳細設計書（detailed-design/ ディレクトリ）とは別ディレクトリ。統合禁止 -->
<!-- feature: vault-encryption / Epic #37 -->
<!-- 配置先: docs/features/vault-encryption/basic-design/index.md -->
<!-- 本ディレクトリは Sub-D (#42) 工程5 ペガサス指摘で `basic-design.md`（565 行、500 行ルール違反）を分割した結果。
     Sub-E〜F の本文は各 Sub の設計工程で本ディレクトリ内の対応ファイルを READ → EDIT で追記する。 -->

## 分冊構成

| 分冊 | 主担当範囲 |
|---|---|
| [`architecture.md`](./architecture.md) | モジュール構成 / クラス設計 / 依存方向（Clean Arch） / アーキテクチャへの影響 / ER 図 |
| [`processing-flows.md`](./processing-flows.md) | F-A1〜F-A5 / F-B1〜F-B4 / F-C1〜F-C4 / F-D1〜F-D5 / F-E1〜F-E5 / **F-F1〜F-F9（Sub-F 新規、CLI vault サブコマンド + 既存 CRUD ロック時挙動）** 等の処理フロー + シーケンス図 |
| [`security.md`](./security.md) | セキュリティ設計（脅威モデル整合 / Fail-Secure 型レベル強制 / OWASP Top 10）+ エラーハンドリング方針 |
| [`ux-and-msg.md`](./ux-and-msg.md) | 外部連携（該当なし）+ UX 設計（Sub-A `WeakPasswordFeedback` / Sub-D MSG 文言設計指針概要 / Sub-E IPC V2 経路 UX + cache_relocked: false 仕様 + 田中ペルソナ ジャーニー + 不変条件 C-30/C-31/C-32 / **Sub-F CLI 文言索引 + i18n 翻訳キー命名 + 終了コード割当**） |

## 記述ルール（必ず守ること）

基本設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。ソースコードと二重管理になりメンテナンスコストしか生まない。

## 分割方針

- 1 分冊あたり原則 **400 行以内**を目標とし、超過したら更に分割を検討（Sub-E〜F 拡張への余地を確保）
- **サフィックス分割禁止**（`architecture-sub-d.md` のような形は不可）。Sub 単位ではなく**責務領域**でファイル分け（detailed-design 同方針）
- 各分冊は冒頭に「親ディレクトリ参照 / 主担当範囲」を明示し、独立して読めるようにする
