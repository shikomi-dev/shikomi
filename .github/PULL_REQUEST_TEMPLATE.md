## Summary

<!-- 変更内容を箇条書きで記述してください -->
-
-

## 種別

<!-- 該当するものにチェックを入れてください -->
- [ ] `feature/*` → `develop`（新機能・改善）
- [ ] `release/*` → `main`（リリース）
- [ ] `release/*` → `develop`（back-merge）
- [ ] `hotfix/*` → `main`（緊急修正）
- [ ] `hotfix/*` → `develop`（back-merge）

## 関連 Issue

<!-- Closes #XXX または Refs #XXX -->
Closes #

## チェックリスト

- [ ] PR タイトルが Conventional Commits に従っている（例: `feat(vault): add encryption opt-in`）
- [ ] `cargo fmt --check` が通る
- [ ] `cargo clippy -- -D warnings` が通る
- [ ] `Cargo.lock` のみが変更されている場合は `deps-lockfile-only` ラベルを付与し、意図的な更新である理由を本文に記載している

---

<!-- ▼ release/* または hotfix/* → main PR の場合のみ記入 ▼ -->

## リリース PR 責任確認（release/* / hotfix/* → main のみ）

> このセクションは `release/*` / `hotfix/*` → `main` PR の場合のみ記入してください。
> feature → develop PR では削除または空欄で構いません。

- [ ] バージョン bump（`Cargo.toml` / `tauri.conf.json` / `package.json`）が完了している
- [ ] `CHANGELOG.md` の内容を確認・校正済み
- [ ] 署名・公証（macOS Notarization / Windows Azure Trusted Signing）の動作確認済み
- [ ] **本 PR のマージから 24h 以内に `develop` への back-merge PR を作成する**（`back-merge-check` CI が監視します）

back-merge PR 予定: <!-- 作成済みの場合は PR リンクを貼ってください -->
