# 基本設計書 — error（エラーハンドリング方針 / 禁止事項）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/basic-design/error.md -->
<!-- 兄弟: ./index.md（モジュール / クラス / フロー）, ./security.md（セキュリティ設計） -->

## 記述ルール（必ず守ること）

基本設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

## エラーハンドリング方針

| 例外種別 | 処理方針 | ユーザーへの通知 |
|---------|---------|----------------|
| ファイル I/O 失敗（NotFound 以外） | `PersistenceError::Io` にラップ、`#[source]` で下位保持、即 return | 開発者向けエラー文面（UI は別 Issue で i18n 写像） |
| SQLite エラー（BUSY / LOCKED 含む） | `PersistenceError::Sqlite` にラップ、即 return | 同上 |
| ドメイン整合性違反（復元時） | `PersistenceError::Corrupted { table, row_key, reason: CorruptedReason, source: Option<DomainError> }` にラップ | 同上。`row_key` で何番目の行が壊れているか特定可能、`reason` で分類、`#[source]` で下位 error を追跡 |
| パーミッション異常（Unix mode / Windows DACL 不変条件違反） | `PersistenceError::InvalidPermission { path, expected, actual }` で即 return。**自動修復しない**（ユーザ明示操作を要求、Fail Secure）。`actual` 先頭ラベル（Windows: `inherited/ace_count/trustee_mismatch/mask_mismatch`）で 4 不変条件のどれが壊れたかを識別（`../detailed-design/flows.md` §Windows `verify_*`） | 同上 |
| `.new` 残存 | `PersistenceError::OrphanNewFile` で即 return。**自動削除しない** | 同上。リカバリ UI で案内（別 Issue） |
| atomic write 失敗 | 発生 stage を `AtomicWriteStage` 列挙で区別、`.new` はベストエフォートで削除、`PersistenceError::AtomicWriteFailed` を返す | 同上 |
| Windows rename PermissionDenied (`code: 5`) | 根本対策（DB ハンドル明示クローズ + WAL checkpoint/journal_mode=DELETE によるサイドカー解放、`./index.md` §設計判断メモ §atomic write の不変条件）を実施した上で、なおも残存する Win Indexer / Defender 等のスキャン由来の一過性ロックを補強する目的で **`cfg(windows)` 限定 rename リトライ**（指数バックオフ `50ms × 2^(n-1)` ± `25ms` jitter × 最大 5 回、**最悪 ~1675ms / 平均 ~1550ms**、`./security.md` §jitter）を `AtomicWriteStage::Rename` 段に挿入。リトライ失敗で `AtomicWriteFailed { stage: Rename, source }` を返す | 同上。Issue #65（Bug-G-001 で CI Defender 250ms+ ハンドル保持を吸収可能な指数バックオフへ拡張）|
| スキーマ不一致（`application_id` / `user_version`） | `PersistenceError::SchemaMismatch` または `Corrupted`（前者は「別アプリの DB」、後者は「バージョン未知」）で区別 | 同上 |
| 暗号化モード vault | `PersistenceError::UnsupportedYet { feature, tracking_issue }` で即 return（Fail Fast） | 同上。別 Issue 進捗を tracking_issue で明示 |
| vault ディレクトリ解決失敗 | `PersistenceError::CannotResolveVaultDir` で即 return | 同上 |
| `SHIKOMI_VAULT_DIR` バリデーション違反 | `PersistenceError::InvalidVaultDir { path, reason: VaultDirReason }` で即 return（`./security.md` §vault ディレクトリ検証）。**自動で安全な別パスに fallback しない**（ユーザ明示修正を要求、Fail Secure） | 同上 |
| プロセス間 advisory lock 競合 | `PersistenceError::Locked { path, holder_hint }` で即 return（`VaultLock::acquire_{shared,exclusive}` 失敗時）。**待機・再試行しない**（ユーザに別プロセス終了を促す、Fail Fast） | 同上。CLI は holder_hint を表示して「`shikomi-daemon` などが稼働していないか確認してください」とガイド |
| 内部バグ（不変条件違反） | `debug_assert!` で検出、production では `tracing::error!` 後 panic | daemon がキャプチャ（別 Issue） |

**本 Issue での禁止事項**: `Result<T, String>` / `Result<T, Box<dyn Error>>` 等のエラー情報を失う型の公開 API 使用／本番コードパスの `unwrap()` `expect()`／エラー握り潰し（`let _ = ...` 等の無言通過）。`AtomicWriter::cleanup_new` のみベストエフォート（`tracing::warn!` でログ、元のエラーは必ず呼出側に伝播）。`permission/windows.rs` の unsafe ブロック内での早期 return は RAII ガード（`SecurityDescriptorGuard` / `LocalFreeAclGuard` / `SidStringGuard`）の `Drop` で `LocalFree` が走ることに依存して良いが、`Drop` 内の `LocalFree` 失敗は**元のエラーを上書きしない**（`tracing::warn!` のみ、`./security.md` §Windows owner-only DACL の適用戦略）。

**Windows rename retry の盲目採用は禁止**（Issue #65 由来の防衛線）: `cfg(windows)` rename リトライは「DB ハンドル明示クローズ + WAL/journal サイドカー解放」の根本対策が**先**に施されている前提でのみ補強として許可される。根本原因（rusqlite Drop 順序遅延 / サイドカー残存 / Win file-handle semantics）の articulate を省いて retry のみで CI green を装う PR は却下対象。同じく `#[cfg(windows)] #[ignore]` でテストを回避する PR は問答無用で却下（CI スコープ錯覚の温床）。リトライ採用の根拠は `./index.md` §設計判断メモ §atomic write の不変条件 と本ファイル §エラーハンドリング方針 §Windows rename PermissionDenied 行の双方に記録する（SSoT 二重露出による忘却防止）。
