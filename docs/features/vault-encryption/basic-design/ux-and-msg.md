# 基本設計書

<!-- 詳細設計書（detailed-design/ ディレクトリ）とは別ファイル。統合禁止 -->
<!-- 詳細設計は Sub-A Rev1 で 4 分冊化: detailed-design/{index,crypto-types,password,nonce-and-aead,errors-and-contracts}.md -->
<!-- feature: vault-encryption / Epic #37 -->
<!-- 配置先: docs/features/vault-encryption/basic-design.md -->
<!-- 本書は Sub-A (#39) 着手時に新規作成。Sub-A スコープ（shikomi-core 暗号ドメイン型 + ゼロ化契約）の基本設計を確定する。
     Sub-B〜F の本文は各 Sub の設計工程で本ファイルを READ → EDIT で追記する。 -->
## 外部連携

該当なし — 理由: Sub-A は shikomi-core の暗号ドメイン型ライブラリで、外部 API / OS / DB / network への発信は一切行わない（pure Rust / no-I/O 制約継承）。

## UX設計

該当なし — 理由: Sub-A は内部型ライブラリで UI 不在。ただし `MasterPassword::new` の構築失敗時に返す `WeakPasswordFeedback { warning, suggestions }` は **Sub-D で MSG-S08 ユーザ提示（Fail Kindly）に直接渡される構造データ**として設計する。**`warning=None` 時の代替警告文契約 + i18n 戦略責務分離（Sub-A は英語 raw のみ運ぶ、Sub-D が i18n 層を挟む）** は `detailed-design/password.md` §`warning=None` 契約 / §i18n 戦略責務分離 を参照。

