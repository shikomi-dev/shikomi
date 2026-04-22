# System Context — Problems, Scope & NFR（shikomi）

> **本書の位置づけ**: `docs/architecture/context/` 配下の**課題・スコープ・非機能要件編**。システム概要・ペルソナは `overview.md`、プロセスモデル / IPC / vault 保護モードは `process-model.md`、脅威モデル / OWASP は `threat-model.md` を参照。

## 5. 解決する課題

1. Clibor が Windows 専用で、macOS / Ubuntu に同等機能の軽量ツールがない
2. 既存の代替（AutoKey / Espanso 等）は設定が複雑、または GUI が前世紀風
3. パスワードマネージャの「Auto-Type」は特定アプリ内限定で、任意アプリでは動作しない
4. クリップボード管理系は履歴保持が既定で、機密情報がクリップボード履歴や Cloud Clipboard に流出する

## 6. スコープ

### In Scope（MVP）
- ホットキー → クリップボードへの平文投入（`Ctrl/Cmd+V` は呼ばない。貼り付け操作はユーザ）
- 自動クリア（既定 30 秒、設定可能）
- クリップボードセンシティブヒントメタデータ付与
- **平文 vault**（OS ファイルパーミッション `0600` のみで保護、デフォルト）
- **暗号化 vault（オプトイン）**: マスターパスワード方式（Argon2id + AES-256-GCM + BIP-39 24 語リカバリ）
- vault モード切替 CLI: `shikomi vault encrypt` / `shikomi vault decrypt`
- CLI（`add` / `list` / `rm` / `export` / `import` / `daemon` / `vault`）
- Tauri v2 GUI（一覧・編集・ホットキー設定・テーマ・vault 保護切替）
- 署名済みインストーラ配布（Win: NSIS / macOS: DMG+Notarization / Linux: deb+rpm+AppImage）

### Out of Scope（MVPでは対象外、将来拡張）
- キーストローク注入によるアプリ直接入力（macOS Secure Event Input で失敗する、UX 不安定）
- クラウド同期（設計上の単一障害点。ローカル export/import のみ提供）
- ブラウザ拡張連携
- TOTP / パスワード生成
- モバイル（iOS/Android）

## 9. 非機能要件（概要、詳細は各 feature の requirements.md に展開）

| 区分 | 指標 | 目標 |
|-----|------|------|
| パフォーマンス（平文モード） | ホットキー押下 → クリップボード書込完了 | p95 50 ms 以下（KDF を走らせないため余裕） |
| パフォーマンス（暗号化モード・アンロック済） | ホットキー押下 → クリップボード書込完了（VEK キャッシュ保持中） | p95 100 ms 以下 |
| 認証処理（暗号化モードのみ） | マスターパスワード入力 → アンロック完了（Argon2id 実行） | p95 1 秒以下（OWASP 推奨パラメータ前提） |
| バイナリサイズ | GUI インストーラ展開後 | 30 MB 以下（Tauri 実績 ~10 MB に安全マージン） |
| メモリ常駐 | アイドル時 | 50 MB 以下 |
| 起動時間 | コールドスタート（GUI） | 1 秒以下 |
| 対応 OS | 最低ライン | Windows 10+、macOS 12+（Monterey）、Ubuntu 22.04+ / Fedora 40+ |
| ライセンス | — | MIT（OSS 公開・貢献容易性優先） |
| 署名 | 全 OS | 製品リリースでは全プラットフォーム署名必須 |
